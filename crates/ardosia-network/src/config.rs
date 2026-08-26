use std::net::SocketAddr;

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

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub bind_addr: SocketAddr,
    pub raknet_protocols: Vec<u8>,
    pub max_connections: usize,
    /// Opaque unconnected-pong advertisement interpreted by the application.
    pub advertisement: String,
    /// Whether the underlying RakNet handshake advertises and requires cookies.
    pub send_cookie: bool,
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
            advertisement: self.advertisement.clone(),
            send_cookie: self.send_cookie,
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
            advertisement: "ardosia-network-test".into(),
            send_cookie: true,
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

    #[test]
    fn legacy_transport_options_reach_vendor_config() {
        let config = NetworkConfig {
            bind_addr: SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 19132),
            raknet_protocols: vec![8],
            max_connections: 20,
            advertisement: "MCPE;Ardosia;84;0.15.10;0;20".into(),
            send_cookie: false,
            runtime: NetworkRuntimeConfig::default(),
        };

        let vendor = config.to_vendor_transport_config().unwrap();

        assert_eq!(vendor.supported_protocols, vec![8]);
        assert_eq!(vendor.advertisement, "MCPE;Ardosia;84;0.15.10;0;20");
        assert!(!vendor.send_cookie);
    }
}
