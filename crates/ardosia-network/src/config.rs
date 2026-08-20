use std::net::SocketAddr;

use raknet_rust::low_level::transport::TransportConfig;

use crate::NetworkError;

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub bind_addr: SocketAddr,
    pub raknet_protocols: Vec<u8>,
    pub max_connections: usize,
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
