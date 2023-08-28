use crate::chain::traits::types::packet::{HasIbcPacketFields, HasIbcPacketTypes};
use crate::chain::types::aliases::{ChannelId, Height, PortId, Sequence, Timestamp};
use crate::core::traits::sync::Async;
use crate::relay::traits::chains::HasRelayChains;

pub trait HasRelayPacket:
    HasRelayChains<SrcChain = Self::SrcChainWithPacket, DstChain = Self::DstChainWithPacket>
{
    type Packet: Async;

    type SrcChainWithPacket: HasIbcPacketFields<Self::DstChain, OutgoingPacket = Self::Packet>;

    type DstChainWithPacket: HasIbcPacketFields<Self::SrcChain, IncomingPacket = Self::Packet>;
}

impl<Relay> HasRelayPacket for Relay
where
    Relay: HasRelayChains,
{
    type Packet = <Relay::SrcChain as HasIbcPacketTypes<Relay::DstChain>>::OutgoingPacket;

    type SrcChainWithPacket = Relay::SrcChain;

    type DstChainWithPacket = Relay::DstChain;
}

pub trait HasRelayPacketFields: HasRelayPacket {
    /**
        The source port of a packet, which is a port ID on the source chain
        that corresponds to the destination chain.
    */
    fn packet_src_port(packet: &Self::Packet) -> &PortId<Self::SrcChain, Self::DstChain>;

    /**
        The source channel ID of a packet, which is a channel ID on the source chain
        that corresponds to the destination chain.
    */
    fn packet_src_channel_id(packet: &Self::Packet) -> &ChannelId<Self::SrcChain, Self::DstChain>;

    /**
        The destination port of a packet, which is a port ID on the destination chain
        that corresponds to the source chain.
    */
    fn packet_dst_port(packet: &Self::Packet) -> &PortId<Self::DstChain, Self::SrcChain>;

    /**
        The destination channel ID of a packet, which is a channel ID on the destination chain
        that corresponds to the source chain.
    */
    fn packet_dst_channel_id(packet: &Self::Packet) -> &ChannelId<Self::DstChain, Self::SrcChain>;

    /**
        The sequence a packet, which is a sequence stored on the source chain
        that corresponds to the destination chain.
    */
    fn packet_sequence(packet: &Self::Packet) -> &Sequence<Self::SrcChain, Self::DstChain>;

    /**
        The optional timeout height of a packet, which is a height on the destination chain.
    */
    fn packet_timeout_height(packet: &Self::Packet) -> Option<&Height<Self::DstChain>>;

    /**
        The timeout timestamp of a packet, which is a timestamp on the destination chain.
    */
    fn packet_timeout_timestamp(packet: &Self::Packet) -> &Timestamp<Self::DstChain>;
}

impl<Relay> HasRelayPacketFields for Relay
where
    Relay: HasRelayChains,
{
    fn packet_src_port(packet: &Self::Packet) -> &PortId<Self::SrcChain, Self::DstChain> {
        Self::SrcChain::outgoing_packet_src_port(packet)
    }

    fn packet_src_channel_id(packet: &Self::Packet) -> &ChannelId<Self::SrcChain, Self::DstChain> {
        Self::SrcChain::outgoing_packet_src_channel_id(packet)
    }

    fn packet_dst_port(packet: &Self::Packet) -> &PortId<Self::DstChain, Self::SrcChain> {
        Self::SrcChain::outgoing_packet_dst_port(packet)
    }

    fn packet_dst_channel_id(packet: &Self::Packet) -> &ChannelId<Self::DstChain, Self::SrcChain> {
        Self::SrcChain::outgoing_packet_dst_channel_id(packet)
    }

    fn packet_sequence(packet: &Self::Packet) -> &Sequence<Self::SrcChain, Self::DstChain> {
        Self::SrcChain::outgoing_packet_sequence(packet)
    }

    fn packet_timeout_height(packet: &Self::Packet) -> Option<&Height<Self::DstChain>> {
        Self::SrcChain::outgoing_packet_timeout_height(packet)
    }

    fn packet_timeout_timestamp(packet: &Self::Packet) -> &Timestamp<Self::DstChain> {
        Self::SrcChain::outgoing_packet_timeout_timestamp(packet)
    }
}