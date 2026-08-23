use raknet_rust::low_level::protocol::constants::RAKNET_PROTOCOL_VERSION;
use raknet_rust::low_level::transport::TransportConfig;

#[test]
fn pinned_hardfork_has_expected_protocol_configuration_surface() {
    let config = TransportConfig::default();
    assert_eq!(RAKNET_PROTOCOL_VERSION, 11);
    assert_eq!(config.supported_protocols, vec![11]);
}
