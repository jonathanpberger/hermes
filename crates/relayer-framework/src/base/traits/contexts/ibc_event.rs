use crate::base::core::traits::sync::Async;
use crate::base::traits::contexts::chain::{ChainContext, IbcChainContext};

pub trait HasIbcEvents<Counterparty>: IbcChainContext<Counterparty>
where
    Counterparty: ChainContext,
{
    type WriteAcknowledgementEvent: Async;

    fn try_extract_write_acknowledgement_event(
        event: Self::Event,
    ) -> Option<Self::WriteAcknowledgementEvent>;
}