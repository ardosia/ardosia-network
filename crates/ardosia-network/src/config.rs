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
