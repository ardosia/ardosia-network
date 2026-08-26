#![forbid(unsafe_code)]

//! Game-agnostic asynchronous payload transport for Ardosia.
//!
//! The crate owns listener and connection lifecycle over the pinned RakNet
//! transport. It deliberately has no MCPE packet, player, or world knowledge.

mod backend;
mod config;
mod connection;
mod error;
mod metrics;
mod reliability;
mod server;

pub use config::{NetworkConfig, NetworkRuntimeConfig};
pub use connection::Connection;
pub use error::NetworkError;
pub use metrics::{
    NetworkMetrics, NetworkShardMetrics, TransportMetrics, TransportOrderingMetrics,
    TransportQueueMetrics, TransportReliabilityMetrics, TransportSessionMetrics,
    TransportTimingMetrics, TransportTrafficMetrics,
};
pub use reliability::Reliability;
pub use server::NetworkServer;

pub(crate) use metrics::MetricsState;
