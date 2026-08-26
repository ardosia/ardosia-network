#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! Game-agnostic asynchronous payload transport for Ardosia.
//!
//! The crate owns listener and connection lifecycle over the pinned RakNet
//! transport. It deliberately has no MCPE packet, player, or world knowledge.
//!
//! # Example
//!
//! ```no_run
//! use std::net::{Ipv4Addr, SocketAddr};
//! use std::num::NonZeroUsize;
//!
//! use ardosia_network::{CookieMode, NetworkConfig, NetworkServer, Reliability};
//! use bytes::Bytes;
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let config = NetworkConfig::new(
//!     SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 19132),
//!     [8],
//!     NonZeroUsize::new(20).unwrap(),
//!     "ardosia",
//!     CookieMode::Enabled,
//! )?;
//! let mut server = NetworkServer::bind(config).await?;
//! let connection = server.accept().await?;
//! connection
//!     .send(Bytes::from_static(b"payload"), Reliability::ReliableOrdered)
//!     .await?;
//! server.shutdown().await?;
//! # Ok(())
//! # }
//! ```

mod backend;
mod config;
mod connection;
mod error;
mod reliability;
mod server;

pub use config::{CookieMode, NetworkConfig, NetworkConfigError};
pub use connection::Connection;
pub use error::NetworkError;
pub use reliability::Reliability;
pub use server::NetworkServer;
