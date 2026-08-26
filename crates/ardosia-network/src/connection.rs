use std::net::SocketAddr;

use bytes::Bytes;
use raknet_rust::server::PeerId;
use tokio::sync::{mpsc, oneshot, watch};

use crate::backend::{BackendCommand, CloseState};
use crate::{NetworkError, Reliability};

/// One accepted transport connection carrying opaque connected payloads.
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

    /// Returns the remote socket address associated with this connection.
    #[must_use]
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    /// Receives the next opaque connected payload from the peer.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError::ConnectionClosed`] after a normal close,
    /// [`NetworkError::Backpressure`] after a policy close caused by a full
    /// inbound queue, or [`NetworkError::BackendStopped`] if the backend ends
    /// before a terminal peer state is delivered.
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

    /// Sends one opaque connected payload with the requested delivery semantics.
    ///
    /// # Errors
    ///
    /// Returns the connection's terminal close error when already closed,
    /// [`NetworkError::BackendStopped`] when the backend command path is gone,
    /// or the transport error produced while sending the payload.
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

    /// Requests a transport-level disconnect for this peer.
    ///
    /// # Errors
    ///
    /// Returns the connection's terminal close error when already closed,
    /// [`NetworkError::BackendStopped`] when the backend command path is gone,
    /// or the transport error produced while disconnecting the peer.
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
