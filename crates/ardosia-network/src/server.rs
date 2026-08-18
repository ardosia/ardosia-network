use std::sync::Arc;

use raknet_rust::server::RaknetServer;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::backend::{run_backend, BackendCommand, COMMAND_QUEUE_CAPACITY};
use crate::connection::Connection;
use crate::{MetricsState, NetworkConfig, NetworkError, NetworkMetrics};

pub struct NetworkServer {
    accept_rx: mpsc::Receiver<Result<Connection, NetworkError>>,
    commands: mpsc::Sender<BackendCommand>,
    metrics: Arc<MetricsState>,
    backend: JoinHandle<()>,
}

impl NetworkServer {
    pub async fn bind(config: NetworkConfig) -> Result<Self, NetworkError> {
        let transport = config.to_vendor_transport_config()?;
        let vendor = RaknetServer::builder()
            .transport_config(transport)
            .start()
            .await?;

        let (accept_tx, accept_rx) = mpsc::channel(config.max_connections);
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_CAPACITY);
        let metrics = Arc::new(MetricsState::default());
        let backend = tokio::spawn(run_backend(
            vendor,
            command_rx,
            accept_tx,
            command_tx.clone(),
            metrics.clone(),
        ));

        Ok(Self {
            accept_rx,
            commands: command_tx,
            metrics,
            backend,
        })
    }

    pub async fn accept(&mut self) -> Result<Connection, NetworkError> {
        self.accept_rx
            .recv()
            .await
            .ok_or(NetworkError::BackendStopped)?
    }

    pub fn metrics(&self) -> NetworkMetrics {
        self.metrics.snapshot()
    }

    pub async fn shutdown(self) -> Result<(), NetworkError> {
        let Self {
            accept_rx: _,
            commands,
            metrics: _,
            backend,
        } = self;

        let (response_tx, response_rx) = oneshot::channel();
        commands
            .send(BackendCommand::Shutdown {
                response: response_tx,
            })
            .await
            .map_err(|_| NetworkError::BackendStopped)?;

        let shutdown_result = response_rx
            .await
            .map_err(|_| NetworkError::BackendStopped)?;

        backend.await.map_err(|error| NetworkError::BackendFailure {
            message: format!("backend task join failed: {error}"),
        })?;

        shutdown_result
    }
}
