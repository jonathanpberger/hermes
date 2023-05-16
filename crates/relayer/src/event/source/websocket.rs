pub mod extract;

use alloc::sync::Arc;
use core::cmp::Ordering;

use crossbeam_channel as channel;
use futures::{
    pin_mut,
    stream::{self, select_all, StreamExt},
    Stream, TryStreamExt,
};
use tokio::task::JoinHandle;
use tokio::{runtime::Runtime as TokioRuntime, sync::mpsc};
use tracing::{debug, error, info, instrument, trace};

use tendermint_rpc::{
    client::CompatMode, event::Event as RpcEvent, query::Query, SubscriptionClient,
    WebSocketClient, WebSocketClientDriver, WebSocketClientUrl,
};

use ibc_relayer_types::{core::ics24_host::identifier::ChainId, events::IbcEvent};

use crate::{
    chain::tracking::TrackingId,
    event::{bus::EventBus, error::*, IbcEventWithHeight},
    telemetry,
    util::{
        retry::{retry_with_index, RetryResult},
        stream::try_group_while,
    },
};

use super::{EventBatch, EventSourceCmd, Result, SubscriptionStream, TxEventSourceCmd};

use self::extract::extract_events;

mod retry_strategy {
    use crate::util::retry::clamp_total;
    use core::time::Duration;
    use retry::delay::Fibonacci;

    // Default parameters for the retrying mechanism
    const MAX_DELAY: Duration = Duration::from_secs(60); // 1 minute
    const MAX_TOTAL_DELAY: Duration = Duration::from_secs(10 * 60); // 10 minutes
    const INITIAL_DELAY: Duration = Duration::from_secs(1); // 1 second

    pub fn default() -> impl Iterator<Item = Duration> {
        clamp_total(Fibonacci::from(INITIAL_DELAY), MAX_DELAY, MAX_TOTAL_DELAY)
    }
}

/// A batch of events received from a WebSocket endpoint from a
/// chain at a specific height.
///
/// Connect to a Tendermint node, subscribe to a set of queries,
/// receive push events over a websocket, and filter them for the
/// event handler.
///
/// The default events that are queried are:
/// - [`EventType::NewBlock`](tendermint_rpc::query::EventType::NewBlock)
/// - [`EventType::Tx`](tendermint_rpc::query::EventType::Tx)
pub struct EventSource {
    chain_id: ChainId,
    /// WebSocket to collect events from
    client: WebSocketClient,
    /// Async task handle for the WebSocket client's driver
    driver_handle: JoinHandle<()>,
    /// Event bus for broadcasting events
    event_bus: EventBus<Arc<Result<EventBatch>>>,
    /// Channel where to receive client driver errors
    rx_err: mpsc::UnboundedReceiver<tendermint_rpc::Error>,
    /// Channel where to send client driver errors
    tx_err: mpsc::UnboundedSender<tendermint_rpc::Error>,
    /// Channel where to receive commands
    rx_cmd: channel::Receiver<EventSourceCmd>,
    /// Node Address
    ws_url: WebSocketClientUrl,
    /// RPC compatibility mode
    rpc_compat: CompatMode,
    /// Queries
    event_queries: Vec<Query>,
    /// All subscriptions combined in a single stream
    subscriptions: Box<SubscriptionStream>,
    /// Tokio runtime
    rt: Arc<TokioRuntime>,
}

impl EventSource {
    /// Create an event monitor, and connect to a node
    #[instrument(
        name = "event_source.create",
        level = "error",
        skip_all,
        fields(chain = %chain_id, url = %ws_url)
    )]
    pub fn new(
        chain_id: ChainId,
        ws_url: WebSocketClientUrl,
        rpc_compat: CompatMode,
        rt: Arc<TokioRuntime>,
    ) -> Result<(Self, TxEventSourceCmd)> {
        let event_bus = EventBus::new();
        let (tx_cmd, rx_cmd) = channel::unbounded();

        let builder = WebSocketClient::builder(ws_url.clone()).compat_mode(rpc_compat);

        let (client, driver) = rt
            .block_on(builder.build())
            .map_err(|_| Error::client_creation_failed(chain_id.clone(), ws_url.clone()))?;

        let (tx_err, rx_err) = mpsc::unbounded_channel();
        let driver_handle = rt.spawn(run_driver(driver, tx_err.clone()));

        // TODO: move them to config file(?)
        let event_queries = super::queries::all();

        let monitor = Self {
            rt,
            chain_id,
            client,
            driver_handle,
            event_queries,
            event_bus,
            rx_err,
            tx_err,
            rx_cmd,
            ws_url,
            rpc_compat,
            subscriptions: Box::new(futures::stream::empty()),
        };

        Ok((monitor, TxEventSourceCmd(tx_cmd)))
    }

    /// The list of [`Query`] that this event monitor is subscribing for.
    pub fn queries(&self) -> &[Query] {
        &self.event_queries
    }

    /// Clear the current subscriptions, and subscribe again to all queries.
    #[instrument(name = "event_source.init_subscriptions", skip_all, fields(chain = %self.chain_id))]
    pub fn init_subscriptions(&mut self) -> Result<()> {
        let mut subscriptions = vec![];

        for query in &self.event_queries {
            trace!("subscribing to query: {}", query);

            let subscription = self
                .rt
                .block_on(self.client.subscribe(query.clone()))
                .map_err(Error::client_subscription_failed)?;

            subscriptions.push(subscription);
        }

        self.subscriptions = Box::new(select_all(subscriptions));

        trace!("subscribed to all queries");

        Ok(())
    }

    #[instrument(
        name = "event_source.try_reconnect",
        level = "error",
        skip_all,
        fields(chain = %self.chain_id)
    )]
    fn try_reconnect(&mut self) -> Result<()> {
        trace!("trying to reconnect to WebSocket endpoint {}", self.ws_url);

        // Try to reconnect
        let builder = WebSocketClient::builder(self.ws_url.clone()).compat_mode(self.rpc_compat);

        let (mut client, driver) = self.rt.block_on(builder.build()).map_err(|_| {
            Error::client_creation_failed(self.chain_id.clone(), self.ws_url.clone())
        })?;

        let mut driver_handle = self.rt.spawn(run_driver(driver, self.tx_err.clone()));

        // Swap the new client with the previous one which failed,
        // so that we can shut the latter down gracefully.
        core::mem::swap(&mut self.client, &mut client);
        core::mem::swap(&mut self.driver_handle, &mut driver_handle);

        trace!("reconnected to WebSocket endpoint {}", self.ws_url);

        // Shut down previous client
        trace!("gracefully shutting down previous client",);

        let _ = client.close();

        self.rt
            .block_on(driver_handle)
            .map_err(Error::client_termination_failed)?;

        trace!("previous client successfully shutdown");

        Ok(())
    }

    /// Try to resubscribe to events
    #[instrument(
        name = "event_source.try_resubscribe",
        level = "error",
        skip_all,
        fields(chain = %self.chain_id)
    )]
    fn try_resubscribe(&mut self) -> Result<()> {
        trace!("trying to resubscribe to events");
        self.init_subscriptions()
    }

    /// Attempt to reconnect the WebSocket client using the given retry strategy.
    ///
    /// See the [`retry`](https://docs.rs/retry) crate and the
    /// [`crate::util::retry`] module for more information.
    #[instrument(
        name = "event_source.reconnect",
        level = "error",
        skip_all,
        fields(chain = %self.chain_id)
    )]
    fn reconnect(&mut self) {
        let result = retry_with_index(retry_strategy::default(), |_| {
            // Try to reconnect
            if let Err(e) = self.try_reconnect() {
                trace!("error when reconnecting: {}", e);
                return RetryResult::Retry(());
            }

            // Try to resubscribe
            if let Err(e) = self.try_resubscribe() {
                trace!("error when resubscribing: {}", e);
                return RetryResult::Retry(());
            }

            RetryResult::Ok(())
        });

        match result {
            Ok(()) => info!(
                "successfully reconnected to WebSocket endpoint {}",
                self.ws_url
            ),
            Err(e) => error!(
                "failed to reconnect to {} after {} retries",
                self.ws_url, e.tries
            ),
        }
    }

    /// Event monitor loop
    #[allow(clippy::while_let_loop)]
    #[instrument(
        name = "event_source",
        level = "error",
        skip_all,
        fields(chain = %self.chain_id)
    )]
    pub fn run(mut self) {
        debug!("starting event monitor");

        // Continuously run the event loop, so that when it aborts
        // because of WebSocket client restart, we pick up the work again.
        loop {
            match self.run_loop() {
                Next::Continue => continue,
                Next::Abort => break,
            }
        }

        debug!("event monitor is shutting down");

        // Close the WebSocket connection
        let _ = self.client.close();

        // Wait for the WebSocket driver to finish
        let _ = self.rt.block_on(self.driver_handle);

        trace!("event monitor has successfully shut down");
    }

    fn run_loop(&mut self) -> Next {
        // Take ownership of the subscriptions
        let subscriptions =
            core::mem::replace(&mut self.subscriptions, Box::new(futures::stream::empty()));

        // Convert the stream of RPC events into a stream of event batches.
        let batches = stream_batches(subscriptions, self.chain_id.clone());

        // Needed to be able to poll the stream
        pin_mut!(batches);

        // Work around double borrow
        let rt = self.rt.clone();

        loop {
            // Process any shutdown or subscription commands
            if let Ok(cmd) = self.rx_cmd.try_recv() {
                match cmd {
                    EventSourceCmd::Shutdown => return Next::Abort,
                    EventSourceCmd::Subscribe(tx) => {
                        if let Err(e) = tx.send(self.event_bus.subscribe()) {
                            error!("failed to send back subscription: {e}");
                        }
                    }
                }
            }

            let result = rt.block_on(async {
                tokio::select! {
                    Some(batch) = batches.next() => batch,
                    Some(e) = self.rx_err.recv() => Err(Error::web_socket_driver(e)),
                }
            });

            // Before handling the batch, check if there are any pending shutdown or subscribe commands.
            if let Ok(cmd) = self.rx_cmd.try_recv() {
                match cmd {
                    EventSourceCmd::Shutdown => return Next::Abort,
                    EventSourceCmd::Subscribe(tx) => {
                        if let Err(e) = tx.send(self.event_bus.subscribe()) {
                            error!("failed to send back subscription: {e}");
                        }
                    }
                }
            }

            match result {
                Ok(batch) => self.process_batch(batch),
                Err(e) => {
                    if let ErrorDetail::SubscriptionCancelled(reason) = e.detail() {
                        error!("subscription cancelled, reason: {}", reason);

                        self.propagate_error(e);

                        telemetry!(ws_reconnect, &self.chain_id);

                        // Reconnect to the WebSocket endpoint, and subscribe again to the queries.
                        self.reconnect();

                        // Abort this event loop, the `run` method will start a new one.
                        // We can't just write `return self.run()` here because Rust
                        // does not perform tail call optimization, and we would
                        // thus potentially blow up the stack after many restarts.
                        return Next::Continue;
                    } else {
                        error!("failed to collect events: {}", e);

                        telemetry!(ws_reconnect, &self.chain_id);

                        // Reconnect to the WebSocket endpoint, and subscribe again to the queries.
                        self.reconnect();

                        // Abort this event loop, the `run` method will start a new one.
                        // We can't just write `return self.run()` here because Rust
                        // does not perform tail call optimization, and we would
                        // thus potentially blow up the stack after many restarts.
                        return Next::Continue;
                    };
                }
            }
        }
    }

    /// Propagate error to subscribers.
    ///
    /// The main use case for propagating RPC errors is for the [`Supervisor`]
    /// to notice that the WebSocket connection or subscription has been closed,
    /// and to trigger a clearing of packets, as this typically means that we have
    /// missed a bunch of events which were emitted after the subscription was closed.
    /// In that case, this error will be handled in [`Supervisor::handle_batch`].
    fn propagate_error(&mut self, error: Error) {
        self.event_bus.broadcast(Arc::new(Err(error)));
    }

    /// Collect the IBC events from the subscriptions
    fn process_batch(&mut self, batch: EventBatch) {
        telemetry!(ws_events, &batch.chain_id, batch.events.len() as u64);

        debug!(chain = %batch.chain_id, len = %batch.events.len(), "emitting batch");

        self.event_bus.broadcast(Arc::new(Ok(batch)));
    }
}

/// Collect the IBC events from an RPC event
fn collect_events(
    chain_id: &ChainId,
    event: RpcEvent,
) -> impl Stream<Item = Result<IbcEventWithHeight>> {
    let events = extract_events(chain_id, event).unwrap_or_default();
    stream::iter(events).map(Ok)
}

/// Convert a stream of RPC event into a stream of event batches
fn stream_batches(
    subscriptions: Box<SubscriptionStream>,
    chain_id: ChainId,
) -> impl Stream<Item = Result<EventBatch>> {
    let id = chain_id.clone();

    // Collect IBC events from each RPC event
    let events = subscriptions
        .map_ok(move |rpc_event| {
            debug!(chain = %id, "received an RPC event: {}", rpc_event.query);
            collect_events(&id, rpc_event)
        })
        .map_err(Error::canceled_or_generic)
        .try_flatten();

    // Group events by height
    let grouped = try_group_while(events, |ev0, ev1| ev0.height == ev1.height);

    // Convert each group to a batch
    grouped.map_ok(move |mut events_with_heights| {
        let height = events_with_heights
            .first()
            .map(|ev_with_height| ev_with_height.height)
            .expect("internal error: found empty group"); // SAFETY: upheld by `group_while`

        sort_events(&mut events_with_heights);

        debug!(chain = %chain_id, len = %events_with_heights.len(), "assembled batch");

        EventBatch {
            height,
            events: events_with_heights,
            chain_id: chain_id.clone(),
            tracking_id: TrackingId::new_uuid(),
        }
    })
}

/// Sort the given events by putting the NewBlock event first,
/// and leaving the other events as is.
fn sort_events(events: &mut [IbcEventWithHeight]) {
    events.sort_by(|a, b| match (&a.event, &b.event) {
        (IbcEvent::NewBlock(_), _) => Ordering::Less,
        _ => Ordering::Equal,
    })
}

async fn run_driver(
    driver: WebSocketClientDriver,
    tx: mpsc::UnboundedSender<tendermint_rpc::Error>,
) {
    if let Err(e) = driver.run().await {
        if tx.send(e).is_err() {
            error!("failed to relay driver error to event monitor");
        }
    }
}

pub enum Next {
    Abort,
    Continue,
}