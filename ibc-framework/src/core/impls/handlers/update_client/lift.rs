use core::marker::PhantomData;

use crate::core::aliases::client::{AnyClientHeader, AnyClientState, AnyConsensusState};
use crate::core::traits::client::{ClientTypes, HasAnyClient, HasClient};
use crate::core::traits::error::HasError;
use crate::core::traits::handlers::update_client::{AnyUpdateClientHandler, UpdateClientHandler};
use crate::core::traits::ibc::HasIbcTypes;

pub struct MismatchClientHeaderFormat<ClientType> {
    pub expected_client_type: ClientType,
}

pub struct LiftClientUpdateHandler<Handler>(pub PhantomData<Handler>);

impl<Context, Handler, Client, AnyClient> AnyUpdateClientHandler<Context>
    for LiftClientUpdateHandler<Handler>
where
    Context: HasError + HasIbcTypes,
    Context: HasAnyClient<AnyClient = AnyClient>,
    AnyClient: HasClient<Client>,
    Client: ClientTypes,
    Handler: UpdateClientHandler<Context, Client = Client>,
    Context::Error: From<MismatchClientHeaderFormat<AnyClient::ClientType>>,
{
    fn check_header_and_update_state(
        context: &Context,
        client_id: &Context::ClientId,
        client_state: &AnyClientState<Context::AnyClient>,
        new_client_header: &AnyClientHeader<Context::AnyClient>,
    ) -> Result<
        (
            AnyClientState<Context::AnyClient>,
            AnyConsensusState<Context::AnyClient>,
        ),
        Context::Error,
    > {
        let client_state = AnyClient::try_from_any_client_state(client_state).ok_or_else(|| {
            MismatchClientHeaderFormat {
                expected_client_type: AnyClient::CLIENT_TYPE,
            }
        })?;

        let client_header =
            AnyClient::try_from_any_client_header(new_client_header).ok_or_else(|| {
                MismatchClientHeaderFormat {
                    expected_client_type: AnyClient::CLIENT_TYPE,
                }
            })?;

        let (new_client_state, new_consensus_state) = Handler::check_header_and_update_state(
            context,
            client_id,
            client_state,
            client_header,
        )?;

        Ok((
            AnyClient::to_any_client_state(new_client_state),
            AnyClient::to_any_consensus_state(new_consensus_state),
        ))
    }
}
