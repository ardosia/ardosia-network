use bytes::Bytes;
use thiserror::Error;

pub const FRAME_MAGIC: [u8; 4] = *b"ARDS";
pub const FRAME_VERSION: u8 = 1;
const HEADER_LEN: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameKind {
    UnreliableData = 1,
    ReliableOrderedData = 2,
    FragmentedReliableOrderedData = 3,
    EchoRequest = 4,
    EchoResponse = 5,
}

impl TryFrom<u8> for FrameKind {
    type Error = FrameError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::UnreliableData),
            2 => Ok(Self::ReliableOrderedData),
            3 => Ok(Self::FragmentedReliableOrderedData),
            4 => Ok(Self::EchoRequest),
            5 => Ok(Self::EchoResponse),
            other => Err(FrameError::UnknownKind(other)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkFrame {
    pub kind: FrameKind,
    pub client_id: u64,
    pub sequence: u64,
    pub probe_id: u64,
    pub payload: Bytes,
}

impl BenchmarkFrame {
    pub fn encode(&self) -> Bytes {
        let mut encoded = Vec::with_capacity(HEADER_LEN.saturating_add(self.payload.len()));
        encoded.extend_from_slice(&FRAME_MAGIC);
        encoded.push(FRAME_VERSION);
        encoded.push(self.kind as u8);
        encoded.extend_from_slice(&self.client_id.to_be_bytes());
        encoded.extend_from_slice(&self.sequence.to_be_bytes());
        encoded.extend_from_slice(&self.probe_id.to_be_bytes());
        encoded.extend_from_slice(&self.payload);
        Bytes::from(encoded)
    }

    pub fn decode(input: &[u8]) -> Result<Self, FrameError> {
        if input.len() < HEADER_LEN {
            return Err(FrameError::TooShort {
                actual: input.len(),
                minimum: HEADER_LEN,
            });
        }
        if input[..FRAME_MAGIC.len()] != FRAME_MAGIC {
            return Err(FrameError::BadMagic);
        }
        if input[FRAME_MAGIC.len()] != FRAME_VERSION {
            return Err(FrameError::UnsupportedVersion(input[FRAME_MAGIC.len()]));
        }

        let kind = FrameKind::try_from(input[FRAME_MAGIC.len() + 1])?;
        let client_id = read_u64(input, 6)?;
        let sequence = read_u64(input, 14)?;
        let probe_id = read_u64(input, 22)?;

        Ok(Self {
            kind,
            client_id,
            sequence,
            probe_id,
            payload: Bytes::copy_from_slice(&input[HEADER_LEN..]),
        })
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FrameError {
    #[error("benchmark frame too short: {actual} bytes, need at least {minimum}")]
    TooShort { actual: usize, minimum: usize },

    #[error("invalid benchmark frame magic")]
    BadMagic,

    #[error("unsupported benchmark frame version {0}")]
    UnsupportedVersion(u8),

    #[error("unknown benchmark frame kind {0}")]
    UnknownKind(u8),
}

fn read_u64(input: &[u8], offset: usize) -> Result<u64, FrameError> {
    let bytes: [u8; 8] = input
        .get(offset..offset + 8)
        .ok_or(FrameError::TooShort {
            actual: input.len(),
            minimum: offset + 8,
        })?
        .try_into()
        .map_err(|_| FrameError::TooShort {
            actual: input.len(),
            minimum: offset + 8,
        })?;
    Ok(u64::from_be_bytes(bytes))
}
