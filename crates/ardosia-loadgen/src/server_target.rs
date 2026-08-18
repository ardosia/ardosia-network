use std::net::SocketAddr;
use std::time::Duration;

use ardosia_network::{Connection, NetworkConfig, NetworkMetrics, NetworkServer};
use tokio::sync::watch;
use tokio::time::{Instant, sleep};

use crate::runner::RunnerError;

pub(crate) async fn spawn_local_target(
    bind_addr: SocketAddr,
    protocol_version: u8,
    max_connections: usize,
) -> Result<
    (
        watch::Sender<bool>,
        tokio::task::JoinHandle<Result<NetworkMetrics, RunnerError>>,
    ),
    RunnerError,
> {
    let server = NetworkServer::bind(NetworkConfig {
        bind_addr,
        raknet_protocols: vec![protocol_version],
        max_connections,
    })
    .await?;

    let (stop_tx, stop_rx) = watch::channel(false);
    let task = tokio::spawn(run_server_loop(server, stop_rx));
    Ok((stop_tx, task))
}

pub(crate) async fn serve_until(
    bind_addr: SocketAddr,
    protocol_version: u8,
    max_connections: usize,
    stop_rx: watch::Receiver<bool>,
) -> Result<NetworkMetrics, RunnerError> {
    let server = NetworkServer::bind(NetworkConfig {
        bind_addr,
        raknet_protocols: vec![protocol_version],
        max_connections,
    })
    .await?;

    run_server_loop(server, stop_rx).await
}

async fn run_server_loop(
    mut server: NetworkServer,
    mut stop_rx: watch::Receiver<bool>,
) -> Result<NetworkMetrics, RunnerError> {
    let mut connections: Vec<Connection> = Vec::new();

    loop {
        if *stop_rx.borrow() {
            break;
        }

        tokio::select! {
            changed = stop_rx.changed() => {
                if changed.is_err() || *stop_rx.borrow() {
                    break;
                }
            }
            accepted = server.accept() => {
                connections.push(accepted?);
            }
        }
    }

    let drain_deadline = Instant::now() + Duration::from_secs(2);
    while server.metrics().connected_current != 0 && Instant::now() < drain_deadline {
        sleep(Duration::from_millis(10)).await;
    }

    let metrics = server.metrics();
    server.shutdown().await?;
    drop(connections);
    Ok(metrics)
}
