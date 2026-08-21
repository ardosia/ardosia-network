use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use bytes::Bytes;
use raknet_rust::client::RaknetClient;
use raknet_rust::low_level::transport::{
    TransportConfig, TransportEvent, TransportRateLimitConfig, TransportServer,
};
use tokio::time::timeout;

fn allocate_loopback_bind_addr() -> SocketAddr {
    let socket = std::net::UdpSocket::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .expect("ephemeral loopback bind must succeed");
    socket
        .local_addr()
        .expect("ephemeral local addr must be available")
}

async fn connect_client(
    server: &mut TransportServer,
    server_addr: SocketAddr,
) -> io::Result<(RaknetClient, SocketAddr)> {
    let server_handshake = async {
        loop {
            if let TransportEvent::ConnectedFrames {
                addr,
                client_guid: Some(_),
                ..
            } = server.recv_and_process().await?
            {
                return Ok::<SocketAddr, io::Error>(addr);
            }
        }
    };

    let (client, peer_addr) = timeout(Duration::from_secs(3), async {
        tokio::join!(RaknetClient::connect(server_addr), server_handshake)
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "RakNet handshake timed out"))?;

    Ok((client?, peer_addr?))
}

async fn recv_application_payload(
    server: &mut TransportServer,
    peer_addr: SocketAddr,
) -> io::Result<Bytes> {
    timeout(Duration::from_secs(2), async {
        loop {
            match server.recv_and_process().await? {
                TransportEvent::ConnectedFrames { addr, frames, .. } if addr == peer_addr => {
                    if let Some(frame) = frames.into_iter().next() {
                        return Ok::<Bytes, io::Error>(frame.payload);
                    }
                }
                TransportEvent::RateLimited { addr } if addr == peer_addr => {
                    return Err(io::Error::other(
                        "established session was rejected by coarse IP rate limiter",
                    ));
                }
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "timed out waiting for payload"))?
}

async fn recv_peer_event(
    server: &mut TransportServer,
    peer_addr: SocketAddr,
) -> io::Result<TransportEvent> {
    timeout(Duration::from_secs(2), async {
        loop {
            let event = server.recv_and_process().await?;
            let matches_peer = match &event {
                TransportEvent::RateLimited { addr }
                | TransportEvent::ProxyDropped { addr }
                | TransportEvent::SessionLimitReached { addr }
                | TransportEvent::OfflinePacket { addr, .. }
                | TransportEvent::ConnectedFrames { addr, .. }
                | TransportEvent::ConnectedDatagramDroppedNoSession { addr }
                | TransportEvent::PeerDisconnected { addr, .. }
                | TransportEvent::DecodeError { addr, .. } => *addr == peer_addr,
            };
            if matches_peer {
                return Ok::<TransportEvent, io::Error>(event);
            }
        }
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "timed out waiting for peer event"))?
}

async fn connected_pair() -> io::Result<(TransportServer, RaknetClient, SocketAddr)> {
    let bind_addr = allocate_loopback_bind_addr();
    let config = TransportConfig {
        bind_addr,
        per_ip_packet_limit: 100_000,
        global_packet_limit: 1_000_000,
        ..TransportConfig::default()
    };
    let mut server = TransportServer::bind(config).await?;
    let server_addr = server.local_addr()?;
    let (client, peer_addr) = connect_client(&mut server, server_addr).await?;
    Ok((server, client, peer_addr))
}

#[tokio::test(flavor = "current_thread")]
async fn established_session_does_not_feed_coarse_ip_packet_limit() -> io::Result<()> {
    let (mut server, mut client, peer_addr) = connected_pair().await?;

    server.set_rate_limit_config(TransportRateLimitConfig {
        per_ip_packet_limit: 1,
        global_packet_limit: 1_000_000,
        rate_window: Duration::from_secs(60),
        block_duration: Duration::from_secs(10),
    });

    for payload in [
        Bytes::from_static(b"\xFEone"),
        Bytes::from_static(b"\xFEtwo"),
        Bytes::from_static(b"\xFEthree"),
    ] {
        client.send(payload.clone()).await?;
        assert_eq!(
            recv_application_payload(&mut server, peer_addr).await?,
            payload
        );
    }

    let metrics = server.metrics_snapshot();
    assert_eq!(metrics.rate_addresses_blocked_rate_exceeded, 0);
    assert_eq!(metrics.rate_ip_block_hits_rate_exceeded, 0);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn explicit_ip_block_still_rejects_established_session() -> io::Result<()> {
    let (mut server, mut client, peer_addr) = connected_pair().await?;

    assert!(server.block_address(peer_addr.ip()));
    client.send(Bytes::from_static(b"\xFEblocked")).await?;

    let event = recv_peer_event(&mut server, peer_addr).await?;
    assert!(
        matches!(event, TransportEvent::RateLimited { addr } if addr == peer_addr),
        "explicit IP block must continue to apply to an established session"
    );

    let metrics = server.metrics_snapshot();
    assert_eq!(metrics.rate_ip_block_hits_manual, 1);
    Ok(())
}
