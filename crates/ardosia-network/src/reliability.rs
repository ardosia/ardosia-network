/// Delivery semantics requested for one outbound connected payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reliability {
    /// Best-effort delivery without retransmission or ordering.
    Unreliable,
    /// Best-effort delivery where only the newest sequenced payload is useful.
    UnreliableSequenced,
    /// Retransmit until acknowledged without imposing an ordering channel.
    Reliable,
    /// Retransmit and deliver in order within the transport ordering channel.
    ReliableOrdered,
    /// Retransmit while allowing newer sequenced payloads to supersede older ones.
    ReliableSequenced,
}

impl Reliability {
    pub(crate) fn into_vendor(self) -> raknet_rust::low_level::protocol::Reliability {
        use raknet_rust::low_level::protocol::Reliability as Vendor;

        match self {
            Self::Unreliable => Vendor::Unreliable,
            Self::UnreliableSequenced => Vendor::UnreliableSequenced,
            Self::Reliable => Vendor::Reliable,
            Self::ReliableOrdered => Vendor::ReliableOrdered,
            Self::ReliableSequenced => Vendor::ReliableSequenced,
        }
    }
}
