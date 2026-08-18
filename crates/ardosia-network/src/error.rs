use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("invalid network configuration field {field}: {message}")]
    InvalidConfig {
        field: &'static str,
        message: String,
    },

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
