use std::net::SocketAddr;

use bytes::Bytes;
use raknet_rust::server::PeerId;
use tokio::sync::{mpsc, oneshot, watch};

use crate::backend::{BackendCommand, CloseState};
use crate::{NetworkError, Reliability};

pub struct Connection {
    peer_id: PeerId,
    peer_addr: SocketAddr,
    inbound: mpsc::Receiver<Bytes>,
    close: watch::Receiver<CloseState>,
    commands: mpsc::Sender<BackendCommand>,
}

impl Connection {
    pub(crate) fn new(
        peer_id: PeerId,
        peer_addr: SocketAddr,
        inbound: mpsc::Receiver<Bytes>,
        close: watch::Receiver<CloseState>,
        commands: mpsc::Sender<BackendCommand>,
    ) -> Self {
        Self {
            peer_id,
            peer_addr,
            inbound,
            close,
            commands,
        }
    }

    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    pub async fn recv(&mut self) -> Result<Bytes, NetworkError> {
        loop {
            if let Some(error) = close_state_error(*self.close.borrow()) {
                return Err(error);
            }

            tokio::select! {
                changed = self.close.changed() => {
                    if changed.is_err() {
                        return Err(NetworkError::BackendStopped);
                    }
                    if let Some(error) = close_state_error(*self.close.borrow()) {
                        return Err(error);
                    }
                }
                payload = self.inbound.recv() => {
                    if let Some(payload) = payload {
                        return Ok(payload);
                    }
                    return match *self.close.borrow() {
                        CloseState::Backpressure => Err(NetworkError::Backpressure),
                        CloseState::Closed => Err(NetworkError::ConnectionClosed),
                        CloseState::Open => Err(NetworkError::BackendStopped),
                    };
                }
            }
        }
    }

    pub async fn send(&self, payload: Bytes, reliability: Reliability) -> Result<(), NetworkError> {
        if let Some(error) = close_state_error(*self.close.borrow()) {
            return Err(error);
        }

        let (response_tx, response_rx) = oneshot::channel();
        self.commands
            .send(BackendCommand::Send {
                peer_id: self.peer_id,
                payload,
                reliability,
                response: response_tx,
            })
            .await
            .map_err(|_| NetworkError::BackendStopped)?;

        response_rx
            .await
            .map_err(|_| NetworkError::BackendStopped)?
    }

    pub async fn close(&self) -> Result<(), NetworkError> {
        if let Some(error) = close_state_error(*self.close.borrow()) {
            return Err(error);
        }

        let (response_tx, response_rx) = oneshot::channel();
        self.commands
            .send(BackendCommand::Disconnect {
                peer_id: self.peer_id,
                response: response_tx,
            })
            .await
            .map_err(|_| NetworkError::BackendStopped)?;

        response_rx
            .await
            .map_err(|_| NetworkError::BackendStopped)?
    }
}

fn close_state_error(state: CloseState) -> Option<NetworkError> {
    match state {
        CloseState::Open => None,
        CloseState::Closed => Some(NetworkError::ConnectionClosed),
        CloseState::Backpressure => Some(NetworkError::Backpressure),
    }
}
