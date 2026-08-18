use std::net::SocketAddr;
use std::time::Duration;

use ardosia_network::{NetworkConfig, NetworkServer};
use bytes::Bytes;
use raknet_rust::client::{ClientSendOptions, RaknetClient, RaknetClientConfig};
use raknet_rust::low_level::protocol::Reliability as VendorReliability;
use tokio::time::timeout;

fn allocate_loopback_addr() -> SocketAddr {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap()
}

fn network_config(addr: SocketAddr) -> NetworkConfig {
    NetworkConfig {
        bind_addr: addr,
        raknet_protocols: vec![8],
        max_connections: 32,
    }
}

fn protocol8_client_config() -> RaknetClientConfig {
    RaknetClientConfig {
        protocol_version: 8,
        ..RaknetClientConfig::default()
    }
}

#[tokio::test]
async fn fragmented_reliable_ordered_payload_reassembles() {
    let addr = allocate_loopback_addr();
    let mut server = NetworkServer::bind(network_config(addr)).await.unwrap();
    let mut client = RaknetClient::connect_with_config(addr, protocol8_client_config())
        .await
        .unwrap();
    let mut connection = timeout(Duration::from_secs(2), server.accept())
        .await
        .unwrap()
        .unwrap();

    let payload = Bytes::from(vec![0x5a; 4096]);
    client
        .send_with_options(
            payload.clone(),
            ClientSendOptions {
                reliability: VendorReliability::ReliableOrdered,
                ..ClientSendOptions::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(
        timeout(Duration::from_secs(3), connection.recv())
            .await
            .unwrap()
            .unwrap(),
        payload
    );

    client.disconnect(None).await.unwrap();
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn clean_disconnect_releases_connected_session_metric() {
    let addr = allocate_loopback_addr();
    let mut server = NetworkServer::bind(network_config(addr)).await.unwrap();
    let mut client = RaknetClient::connect_with_config(addr, protocol8_client_config())
        .await
        .unwrap();
    let _connection = timeout(Duration::from_secs(2), server.accept())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(server.metrics().connected_current, 1);
    client.disconnect(None).await.unwrap();

    timeout(Duration::from_secs(2), async {
        loop {
            if server.metrics().connected_current == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    assert_eq!(server.metrics().disconnected_total, 1);
    server.shutdown().await.unwrap();
}
