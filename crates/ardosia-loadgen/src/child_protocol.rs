use std::net::SocketAddr;

use ardosia_network::NetworkMetrics;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

use crate::report::TransportMetricsReport;
use crate::scenario::Scenario;
use crate::server_target;
use crate::workload::WorkloadCounts;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChildCommand {
    Start {
        bind_addr: String,
        scenario: Scenario,
    },
    BeginMeasurement,
    Snapshot,
    Stop,
}

// ChildEvent values are ephemeral serde IPC messages. Keeping the periodic snapshot inline avoids
// a heap allocation on every sampling tick; the enum is never retained in a large collection.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChildEvent {
    Ready { pid: u32 },
    MeasurementStarted,
    Snapshot { metrics: TransportMetricsReport },
    Stopped { report: Box<ServerRunReport> },
    Error { message: String },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ServerRunReport {
    pub metrics: ServerMetricsReport,
    pub transport: TransportMetricsReport,
    pub workload: WorkloadCounts,
    pub benchmark_protocol_errors: usize,
    pub send_errors: usize,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerMetricsReport {
    pub accepted_total: u64,
    pub connected_current: u64,
    pub disconnected_total: u64,
    pub protocol_errors_total: u64,
    pub backpressure_disconnects_total: u64,
}

impl From<NetworkMetrics> for ServerMetricsReport {
    fn from(metrics: NetworkMetrics) -> Self {
        Self {
            accepted_total: metrics.accepted_total,
            connected_current: metrics.connected_current,
            disconnected_total: metrics.disconnected_total,
            protocol_errors_total: metrics.protocol_errors_total,
            backpressure_disconnects_total: metrics.backpressure_disconnects_total,
        }
    }
}

#[derive(Debug, Error)]
pub enum ChildProtocolError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("invalid child bind address {0}")]
    InvalidBindAddress(String),

    #[error("benchmark child server failed: {0}")]
    Server(String),
}

pub async fn run_child_session<R, W>(reader: R, mut writer: W) -> Result<(), ChildProtocolError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    let mut server = None;

    while let Some(line) = lines.next_line().await? {
        let command: ChildCommand = serde_json::from_str(&line)?;

        match command {
            ChildCommand::Start {
                bind_addr,
                scenario,
            } => {
                if server.is_some() {
                    write_event(
                        &mut writer,
                        &ChildEvent::Error {
                            message: "server already started".into(),
                        },
                    )
                    .await?;
                    continue;
                }

                let address: SocketAddr = bind_addr
                    .parse()
                    .map_err(|_| ChildProtocolError::InvalidBindAddress(bind_addr.clone()))?;
                let handle = server_target::spawn_local_target(address, scenario)
                    .await
                    .map_err(|error| ChildProtocolError::Server(error.to_string()))?;
                server = Some(handle);
                write_event(
                    &mut writer,
                    &ChildEvent::Ready {
                        pid: std::process::id(),
                    },
                )
                .await?;
            }
            ChildCommand::BeginMeasurement => {
                let Some(handle) = server.as_ref() else {
                    write_event(
                        &mut writer,
                        &ChildEvent::Error {
                            message: "server has not started".into(),
                        },
                    )
                    .await?;
                    continue;
                };
                handle.begin_measurement();
                write_event(&mut writer, &ChildEvent::MeasurementStarted).await?;
            }
            ChildCommand::Snapshot => {
                let Some(handle) = server.as_ref() else {
                    write_event(
                        &mut writer,
                        &ChildEvent::Error {
                            message: "server has not started".into(),
                        },
                    )
                    .await?;
                    continue;
                };
                let metrics = handle
                    .snapshot()
                    .await
                    .map_err(|error| ChildProtocolError::Server(error.to_string()))?;
                write_event(
                    &mut writer,
                    &ChildEvent::Snapshot {
                        metrics: metrics.transport.into(),
                    },
                )
                .await?;
            }
            ChildCommand::Stop => {
                let Some(handle) = server.take() else {
                    write_event(
                        &mut writer,
                        &ChildEvent::Error {
                            message: "server has not started".into(),
                        },
                    )
                    .await?;
                    continue;
                };

                let result = handle
                    .stop()
                    .await
                    .map_err(|error| ChildProtocolError::Server(error.to_string()))?;
                let transport = result.metrics.transport.into();
                write_event(
                    &mut writer,
                    &ChildEvent::Stopped {
                        report: Box::new(ServerRunReport {
                            metrics: result.metrics.into(),
                            transport,
                            workload: result.workload,
                            benchmark_protocol_errors: result.protocol_errors,
                            send_errors: result.send_errors,
                        }),
                    },
                )
                .await?;
                return Ok(());
            }
        }
    }

    if let Some(handle) = server {
        let _ = handle.stop().await;
    }

    Ok(())
}

pub async fn run_stdio_child() -> Result<(), ChildProtocolError> {
    run_child_session(tokio::io::stdin(), tokio::io::stdout()).await
}

async fn write_event<W>(writer: &mut W, event: &ChildEvent) -> Result<(), ChildProtocolError>
where
    W: AsyncWrite + Unpin,
{
    let json = serde_json::to_vec(event)?;
    writer.write_all(&json).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}
