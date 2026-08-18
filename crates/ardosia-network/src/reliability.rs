#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reliability {
    Unreliable,
    UnreliableSequenced,
    Reliable,
    ReliableOrdered,
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
