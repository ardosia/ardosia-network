use std::net::SocketAddr;
use std::time::Duration;

use raknet_rust::low_level::transport::{ShardedRuntimeConfig, TransportConfig};

use crate::NetworkError;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NetworkRuntimeConfig {
    /// Exact RakNet transport worker count.
    ///
    /// `None` preserves the vendor runtime default.
    pub worker_shards: Option<usize>,
}

impl NetworkRuntimeConfig {
    pub fn effective_worker_shards(self) -> usize {
        self.worker_shards
            .unwrap_or_else(|| ShardedRuntimeConfig::default().shard_count)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkPacketWindowPolicy {
    pub per_ip_packet_limit: usize,
    pub global_packet_limit: usize,
    pub window: Duration,
    pub block_duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkProcessingBudgetPolicy {
    pub enabled: bool,
    pub per_ip_refill_units_per_sec: u32,
    pub per_ip_burst_units: u32,
    pub global_refill_units_per_sec: u32,
    pub global_burst_units: u32,
    pub bucket_idle_ttl: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkPolicySnapshot {
    pub packet_window: NetworkPacketWindowPolicy,
    pub processing_budget: NetworkProcessingBudgetPolicy,
}

impl NetworkPolicySnapshot {
    pub(crate) fn from_transport_config(config: &TransportConfig) -> Self {
        let processing_budget = config.processing_budget;
        Self {
            packet_window: NetworkPacketWindowPolicy {
                per_ip_packet_limit: config.per_ip_packet_limit,
                global_packet_limit: config.global_packet_limit,
                window: config.rate_window,
                block_duration: config.block_duration,
            },
            processing_budget: NetworkProcessingBudgetPolicy {
                enabled: processing_budget.enabled,
                per_ip_refill_units_per_sec: processing_budget.per_ip_refill_units_per_sec,
                per_ip_burst_units: processing_budget.per_ip_burst_units,
                global_refill_units_per_sec: processing_budget.global_refill_units_per_sec,
                global_burst_units: processing_budget.global_burst_units,
                bucket_idle_ttl: processing_budget.bucket_idle_ttl,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub bind_addr: SocketAddr,
    pub raknet_protocols: Vec<u8>,
    pub max_connections: usize,
    pub runtime: NetworkRuntimeConfig,
}

impl NetworkConfig {
    pub fn validate(&self) -> Result<(), NetworkError> {
        if self.raknet_protocols.is_empty() {
            return Err(NetworkError::InvalidConfig {
                field: "raknet_protocols",
                message: "must contain at least one RakNet protocol version".into(),
            });
        }

        if self.max_connections == 0 {
            return Err(NetworkError::InvalidConfig {
                field: "max_connections",
                message: "must be at least 1".into(),
            });
        }

        if self.runtime.worker_shards == Some(0) {
            return Err(NetworkError::InvalidConfig {
                field: "runtime.worker_shards",
                message: "must be at least 1 when specified".into(),
            });
        }

        Ok(())
    }

    pub(crate) fn to_vendor_transport_config(&self) -> Result<TransportConfig, NetworkError> {
        self.validate()?;

        let vendor = TransportConfig {
            bind_addr: self.bind_addr,
            supported_protocols: self.raknet_protocols.clone(),
            max_sessions: self.max_connections,
            ..TransportConfig::default()
        };
        vendor
            .validate()
            .map_err(|error| NetworkError::InvalidConfig {
                field: "vendor_transport",
                message: error.to_string(),
            })?;

        Ok(vendor)
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddr};

    use super::{NetworkConfig, NetworkRuntimeConfig};
    use crate::NetworkError;

    fn config_with_runtime(runtime: NetworkRuntimeConfig) -> NetworkConfig {
        NetworkConfig {
            bind_addr: SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0),
            raknet_protocols: vec![8],
            max_connections: 500,
            runtime,
        }
    }

    #[test]
    fn runtime_defaults_to_automatic_worker_shards() {
        let runtime = NetworkRuntimeConfig::default();

        assert_eq!(runtime.worker_shards, None);
    }

    #[test]
    fn automatic_worker_shards_are_valid() {
        let config = config_with_runtime(NetworkRuntimeConfig {
            worker_shards: None,
        });

        assert!(config.validate().is_ok());
    }

    #[test]
    fn explicit_worker_shards_are_valid() {
        let config = config_with_runtime(NetworkRuntimeConfig {
            worker_shards: Some(4),
        });

        assert!(config.validate().is_ok());
    }

    #[test]
    fn zero_worker_shards_are_rejected() {
        let config = config_with_runtime(NetworkRuntimeConfig {
            worker_shards: Some(0),
        });

        let error = config.validate().unwrap_err();

        match error {
            NetworkError::InvalidConfig { field, message } => {
                assert_eq!(field, "runtime.worker_shards");
                assert_eq!(message, "must be at least 1 when specified");
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
