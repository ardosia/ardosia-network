use std::collections::HashMap;

use bytes::Bytes;
use raknet_rust::server::{PeerId, RaknetServer, RaknetServerEvent, SendOptions};
use tokio::sync::{mpsc, oneshot, watch};

use crate::connection::Connection;
use crate::{NetworkError, Reliability};

pub(crate) const COMMAND_QUEUE_CAPACITY: usize = 4096;
pub(crate) const PER_CONNECTION_INBOUND_CAPACITY: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloseState {
    Open,
    Closed,
    Backpressure,
}

pub(crate) enum BackendCommand {
    Send {
        peer_id: PeerId,
        payload: Bytes,
        reliability: Reliability,
        response: oneshot::Sender<Result<(), NetworkError>>,
    },
    Disconnect {
        peer_id: PeerId,
        response: oneshot::Sender<Result<(), NetworkError>>,
    },
    Shutdown {
        response: oneshot::Sender<Result<(), NetworkError>>,
    },
}

struct PeerState {
    inbound: mpsc::Sender<Bytes>,
    close: watch::Sender<CloseState>,
}

pub(crate) async fn run_backend(
    mut server: RaknetServer,
    mut commands: mpsc::Receiver<BackendCommand>,
    accept_tx: mpsc::Sender<Result<Connection, NetworkError>>,
    command_tx: mpsc::Sender<BackendCommand>,
) {
    let mut peers: HashMap<PeerId, PeerState> = HashMap::new();
    let mut shutdown_response = None;

    'run: loop {
        tokio::select! {
            command = commands.recv() => {
                match command {
                    Some(BackendCommand::Send {
                        peer_id,
                        payload,
                        reliability,
                        response,
                    }) => {
                        let result = server
                            .send_with_options(
                                peer_id,
                                payload,
                                SendOptions {
                                    reliability: reliability.into_vendor(),
                                    ..SendOptions::default()
                                },
                            )
                            .await
                            .map_err(NetworkError::from);
                        let _ = response.send(result);
                    }
                    Some(BackendCommand::Disconnect { peer_id, response }) => {
                        let result = server.disconnect(peer_id).await.map_err(NetworkError::from);
                        let _ = response.send(result);
                    }
                    Some(BackendCommand::Shutdown { response }) => {
                        shutdown_response = Some(response);
                        break 'run;
                    }
                    None => break 'run,
                }
            }
            event = server.next_event() => {
                let Some(event) = event else {
                    break 'run;
                };

                if handle_server_event(
                    &mut server,
                    event,
                    &mut peers,
                    &accept_tx,
                    &command_tx,
                )
                .await
                {
                    break 'run;
                }
            }
        }
    }

    for peer in peers.into_values() {
        let _ = peer.close.send(CloseState::Closed);
    }

    let shutdown_result = server.shutdown().await.map_err(NetworkError::from);
    if let Some(response) = shutdown_response {
        let _ = response.send(shutdown_result);
    }
}

async fn handle_server_event(
    server: &mut RaknetServer,
    event: RaknetServerEvent,
    peers: &mut HashMap<PeerId, PeerState>,
    accept_tx: &mpsc::Sender<Result<Connection, NetworkError>>,
    command_tx: &mpsc::Sender<BackendCommand>,
) -> bool {
    match event {
        RaknetServerEvent::PeerConnected { peer_id, addr, .. } => {
            let (inbound_tx, inbound_rx) = mpsc::channel(PER_CONNECTION_INBOUND_CAPACITY);
            let (close_tx, close_rx) = watch::channel(CloseState::Open);

            peers.insert(
                peer_id,
                PeerState {
                    inbound: inbound_tx,
                    close: close_tx,
                },
            );

            let connection =
                Connection::new(peer_id, addr, inbound_rx, close_rx, command_tx.clone());

            match accept_tx.try_send(Ok(connection)) {
                Ok(()) => false,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    close_peer_for_backpressure(server, peers, peer_id).await;
                    false
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    if let Some(peer) = peers.remove(&peer_id) {
                        let _ = peer.close.send(CloseState::Closed);
                    }
                    let _ = server.disconnect(peer_id).await;
                    true
                }
            }
        }
        RaknetServerEvent::Packet {
            peer_id, payload, ..
        } => {
            let dispatch = match peers.get(&peer_id) {
                Some(peer) => peer.inbound.try_send(payload),
                None => return false,
            };

            match dispatch {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    close_peer_for_backpressure(server, peers, peer_id).await;
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    if let Some(peer) = peers.remove(&peer_id) {
                        let _ = peer.close.send(CloseState::Closed);
                    }
                    let _ = server.disconnect(peer_id).await;
                }
            }
            false
        }
        RaknetServerEvent::PeerDisconnected { peer_id, .. } => {
            if let Some(peer) = peers.remove(&peer_id) {
                let _ = peer.close.send(CloseState::Closed);
            }
            false
        }
        RaknetServerEvent::DecodeError { .. } => false,
        RaknetServerEvent::WorkerError { shard_id, message } => {
            let message = format!("RakNet worker {shard_id} failed: {message}");
            let _ = accept_tx.try_send(Err(NetworkError::BackendFailure { message }));
            true
        }
        RaknetServerEvent::WorkerStopped { shard_id } => {
            let message = format!("RakNet worker {shard_id} stopped unexpectedly");
            let _ = accept_tx.try_send(Err(NetworkError::BackendFailure { message }));
            true
        }
        RaknetServerEvent::Metrics { .. } => false,
        _ => false,
    }
}

async fn close_peer_for_backpressure(
    server: &mut RaknetServer,
    peers: &mut HashMap<PeerId, PeerState>,
    peer_id: PeerId,
) {
    if let Some(peer) = peers.remove(&peer_id) {
        let _ = peer.close.send(CloseState::Backpressure);
    }
    let _ = server.disconnect(peer_id).await;
}
