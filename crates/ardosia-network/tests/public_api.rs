use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::time::Duration;

use ardosia_network::{CookieMode, NetworkConfig, NetworkConfigError, NetworkServer, Reliability};
use bytes::Bytes;
use raknet_rust::client::{ClientSendOptions, RaknetClient, RaknetClientConfig, RaknetClientEvent};
use raknet_rust::low_level::protocol::Reliability as RaknetReliability;
use tokio::time::timeout;

fn allocate_loopback_addr() -> SocketAddr {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap()
}

fn network_config(addr: SocketAddr) -> NetworkConfig {
    NetworkConfig::new(
        addr,
        [8],
        NonZeroUsize::new(32).unwrap(),
        "ardosia-network-test",
        CookieMode::Enabled,
    )
    .unwrap()
}

#[test]
fn rejects_empty_protocol_set_during_construction() {
    let error = NetworkConfig::new(
        allocate_loopback_addr(),
        [],
        NonZeroUsize::new(32).unwrap(),
        "ardosia-network-test",
        CookieMode::Enabled,
    )
    .unwrap_err();

    assert_eq!(error, NetworkConfigError::NoProtocols);
}

#[test]
fn rejects_duplicate_protocol_during_construction() {
    let error = NetworkConfig::new(
        allocate_loopback_addr(),
        [8, 8],
        NonZeroUsize::new(32).unwrap(),
        "ardosia-network-test",
        CookieMode::Enabled,
    )
    .unwrap_err();

    assert_eq!(error, NetworkConfigError::DuplicateProtocol { protocol: 8 });
}

fn protocol8_client_config() -> RaknetClientConfig {
    RaknetClientConfig {
        protocol_version: 8,
        ..RaknetClientConfig::default()
    }
}

#[tokio::test]
async fn protocol8_roundtrips_reliable_ordered_payload() {
    let addr = allocate_loopback_addr();
    let mut server = NetworkServer::bind(network_config(addr)).await.unwrap();

    let mut client = RaknetClient::connect_with_config(addr, protocol8_client_config())
        .await
        .unwrap();
    let mut connection = timeout(Duration::from_secs(2), server.accept())
        .await
        .unwrap()
        .unwrap();

    client
        .send_with_options(
            Bytes::from_static(b"client-to-server"),
            ClientSendOptions {
                reliability: RaknetReliability::ReliableOrdered,
                ..ClientSendOptions::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(
        timeout(Duration::from_secs(2), connection.recv())
            .await
            .unwrap()
            .unwrap(),
        Bytes::from_static(b"client-to-server")
    );

    connection
        .send(
            Bytes::from_static(b"server-to-client"),
            Reliability::ReliableOrdered,
        )
        .await
        .unwrap();

    let payload = timeout(Duration::from_secs(2), async {
        loop {
            match client.next_event().await {
                Some(RaknetClientEvent::Packet { payload, .. }) => break payload,
                Some(RaknetClientEvent::Disconnected { reason }) => {
                    panic!("client disconnected before reply: {reason:?}")
                }
                Some(_) => {}
                None => panic!("client event stream closed before reply"),
            }
        }
    })
    .await
    .unwrap();

    assert_eq!(payload, Bytes::from_static(b"server-to-client"));

    client.disconnect(None).await.unwrap();
    server.shutdown().await.unwrap();
}
