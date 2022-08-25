use ibc_relayer::chain::handle::ChainHandle;
use ibc_relayer_cosmos::cosmos::batch::new_relay_context_with_batch;
use ibc_relayer_cosmos::cosmos::context::chain::CosmosChainImpl;
use ibc_relayer_cosmos::cosmos::context::relay::CosmosRelayImpl;
use ibc_relayer_cosmos::cosmos::types::relay::CosmosRelayContext;
use ibc_relayer_framework::one_for_all::traits::relay::OfaRelayContext;
use ibc_relayer_runtime::tokio::context::TokioRuntimeContext;
use ibc_test_framework::types::binary::chains::ConnectedChains;
use std::sync::Arc;

pub fn build_cosmos_relay_context<ChainA, ChainB>(
    chains: &ConnectedChains<ChainA, ChainB>,
) -> OfaRelayContext<
    CosmosRelayContext<CosmosRelayImpl<CosmosChainImpl<ChainA>, CosmosChainImpl<ChainB>>>,
>
where
    ChainA: ChainHandle,
    ChainB: ChainHandle,
{
    let runtime = TokioRuntimeContext::new(chains.node_a.value().chain_driver.runtime.clone());

    let (chain_a, receiver_a) = CosmosChainImpl::new(
        chains.handle_a.clone(),
        chains
            .node_a
            .value()
            .wallets
            .relayer
            .address
            .0
            .parse()
            .unwrap(),
        chains.node_a.value().chain_driver.tx_config.clone(),
        chains.node_a.value().wallets.relayer.key.clone(),
    );

    let (chain_b, receiver_b) = CosmosChainImpl::new(
        chains.handle_b.clone(),
        chains
            .node_b
            .value()
            .wallets
            .relayer
            .address
            .0
            .parse()
            .unwrap(),
        chains.node_b.value().chain_driver.tx_config.clone(),
        chains.node_b.value().wallets.relayer.key.clone(),
    );

    let relay = new_relay_context_with_batch(
        runtime.clone(),
        Arc::new(chain_a),
        Arc::new(chain_b),
        chains.foreign_clients.client_a_to_b.clone(),
        chains.foreign_clients.client_b_to_a.clone(),
        Default::default(),
        receiver_a,
        receiver_b,
    );

    relay
}
