use std::collections::HashSet;
use std::net::SocketAddr;
use std::num::NonZeroUsize;

use raknet_rust::low_level::transport::TransportConfig;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NetworkConfigError {
    #[error("at least one RakNet protocol must be configured")]
    NoProtocols,
    #[error("RakNet protocol {protocol} is configured more than once")]
    DuplicateProtocol { protocol: u8 },
    #[error("RakNet rejected the transport configuration: {message}")]
    TransportRejected { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieMode {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    bind_addr: SocketAddr,
    raknet_protocols: Vec<u8>,
    max_connections: NonZeroUsize,
    advertisement: String,
    cookie_mode: CookieMode,
    worker_shards: Option<NonZeroUsize>,
}

impl NetworkConfig {
    pub fn new(
        bind_addr: SocketAddr,
        raknet_protocols: impl IntoIterator<Item = u8>,
        max_connections: NonZeroUsize,
        advertisement: impl Into<String>,
        cookie_mode: CookieMode,
    ) -> Result<Self, NetworkConfigError> {
        let mut seen = HashSet::new();
        let mut protocols = Vec::new();

        for protocol in raknet_protocols {
            if !seen.insert(protocol) {
                return Err(NetworkConfigError::DuplicateProtocol { protocol });
            }
            protocols.push(protocol);
        }

        if protocols.is_empty() {
            return Err(NetworkConfigError::NoProtocols);
        }

        Ok(Self {
            bind_addr,
            raknet_protocols: protocols,
            max_connections,
            advertisement: advertisement.into(),
            cookie_mode,
            worker_shards: None,
        })
    }

    pub fn with_worker_shards(mut self, worker_shards: NonZeroUsize) -> Self {
        self.worker_shards = Some(worker_shards);
        self
    }

    pub(crate) fn max_connections(&self) -> NonZeroUsize {
        self.max_connections
    }

    pub(crate) fn worker_shards(&self) -> Option<NonZeroUsize> {
        self.worker_shards
    }

    pub(crate) fn to_vendor_transport_config(
        &self,
    ) -> Result<TransportConfig, NetworkConfigError> {
        let vendor = TransportConfig {
            bind_addr: self.bind_addr,
            supported_protocols: self.raknet_protocols.clone(),
            max_sessions: self.max_connections.get(),
            advertisement: self.advertisement.clone(),
            send_cookie: matches!(self.cookie_mode, CookieMode::Enabled),
            ..TransportConfig::default()
        };

        vendor
            .validate()
            .map_err(|error| NetworkConfigError::TransportRejected {
                message: error.to_string(),
            })?;

        Ok(vendor)
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddr};
    use std::num::NonZeroUsize;

    use super::{CookieMode, NetworkConfig};

    #[test]
    fn legacy_transport_options_reach_vendor_config() {
        let config = NetworkConfig::new(
            SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 19132),
            [8],
            NonZeroUsize::new(20).unwrap(),
            "MCPE;Ardosia;84;0.15.10;0;20",
            CookieMode::Disabled,
        )
        .unwrap();

        let vendor = config.to_vendor_transport_config().unwrap();

        assert_eq!(vendor.supported_protocols, vec![8]);
        assert_eq!(vendor.advertisement, "MCPE;Ardosia;84;0.15.10;0;20");
        assert!(!vendor.send_cookie);
    }

    #[test]
    fn enabled_cookie_mode_reaches_vendor_config() {
        let config = NetworkConfig::new(
            SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 19132),
            [8],
            NonZeroUsize::new(20).unwrap(),
            "ardosia-network-test",
            CookieMode::Enabled,
        )
        .unwrap();

        let vendor = config.to_vendor_transport_config().unwrap();

        assert!(vendor.send_cookie);
    }

    #[test]
    fn explicit_worker_shards_are_retained() {
        let config = NetworkConfig::new(
            SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 19132),
            [8],
            NonZeroUsize::new(20).unwrap(),
            "ardosia-network-test",
            CookieMode::Enabled,
        )
        .unwrap()
        .with_worker_shards(NonZeroUsize::new(4).unwrap());

        assert_eq!(config.worker_shards().map(NonZeroUsize::get), Some(4));
    }
}
