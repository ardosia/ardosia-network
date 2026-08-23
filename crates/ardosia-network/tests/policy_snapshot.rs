use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use ardosia_network::{NetworkConfig, NetworkRuntimeConfig, NetworkServer};

fn allocate_loopback_addr() -> SocketAddr {
    let socket = std::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    socket.local_addr().unwrap()
}

#[tokio::test]
async fn bound_server_exposes_effective_abuse_control_policy() {
    let mut server = NetworkServer::bind(NetworkConfig {
        bind_addr: allocate_loopback_addr(),
        raknet_protocols: vec![8],
        max_connections: 32,
        runtime: NetworkRuntimeConfig::default(),
    })
    .await
    .unwrap();

    let policy = server.policy_snapshot();

    assert_eq!(policy.packet_window.per_ip_packet_limit, 120);
    assert_eq!(policy.packet_window.global_packet_limit, 100_000);
    assert_eq!(policy.packet_window.window, Duration::from_millis(10));
    assert_eq!(policy.packet_window.block_duration, Duration::from_secs(10));

    assert!(policy.processing_budget.enabled);
    assert_eq!(
        policy.processing_budget.per_ip_refill_units_per_sec,
        3_000_000
    );
    assert_eq!(policy.processing_budget.per_ip_burst_units, 1_500_000);
    assert_eq!(
        policy.processing_budget.global_refill_units_per_sec,
        128_000_000
    );
    assert_eq!(
        policy.processing_budget.global_burst_units,
        32_000_000
    );
    assert_eq!(
        policy.processing_budget.bucket_idle_ttl,
        Duration::from_secs(30)
    );

    server.shutdown().await.unwrap();
}
