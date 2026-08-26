use std::io;

use thiserror::Error;

use crate::config::NetworkConfigError;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error(transparent)]
    Configuration(#[from] NetworkConfigError),

    #[error("network I/O failed: {0}")]
    Io(#[from] io::Error),

    #[error("connection is closed")]
    ConnectionClosed,

    #[error("connection closed by Ardosia backpressure policy")]
    Backpressure,

    #[error("network backend stopped")]
    BackendStopped,

    #[error("network backend failed: {message}")]
    BackendFailure { message: String },
}
