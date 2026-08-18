use std::net::SocketAddr;
use std::process::Stdio;
use std::time::Duration;

use ardosia_network::{NetworkError, NetworkMetrics};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use crate::child_protocol::{ChildCommand, ChildEvent, ServerRunReport};
use crate::client_task::{ConnectOutcome, Phase, run_client_task};
use crate::report::{RunCounts, RunReport};
use crate::scenario::Scenario;
use crate::server_target;

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error(transparent)]
    Network(#[from] NetworkError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("benchmark task failed: {0}")]
    Task(String),

    #[error("child benchmark protocol failed: {0}")]
    ChildProtocol(String),

    #[error("child benchmark process exited unexpectedly: {0}")]
    ChildExited(String),
}

pub async fn run_clients(target: SocketAddr, scenario: &Scenario) -> RunReport {
    let started = Instant::now();
    let (phase_tx, phase_rx) = watch::channel(Phase::Ramp);
    let (outcome_tx, mut outcome_rx) = mpsc::channel(scenario.clients.max(1));
    let mut tasks = Vec::with_capacity(scenario.clients);

    for index in 0..scenario.clients {
        tasks.push(tokio::spawn(run_client_task(
            target,
            scenario.protocol_version,
            Duration::from_secs(scenario.connect_timeout_seconds),
            stagger_delay(index, scenario.clients, scenario.ramp_up_seconds),
            phase_rx.clone(),
            outcome_tx.clone(),
        )));
    }
    drop(outcome_tx);

    let mut successful_handshakes = 0usize;
    let mut failed_handshakes = 0usize;
    for _ in 0..scenario.clients {
        match outcome_rx.recv().await {
            Some(ConnectOutcome::Ready) => successful_handshakes += 1,
            Some(ConnectOutcome::Failed) => failed_handshakes += 1,
            None => break,
        }
    }

    let reported = successful_handshakes.saturating_add(failed_handshakes);
    failed_handshakes += scenario.clients.saturating_sub(reported);

    if successful_handshakes == scenario.clients && failed_handshakes == 0 {
        let deadline = Instant::now() + Duration::from_secs(scenario.hold_seconds);
        let _ = phase_tx.send(Phase::Hold { deadline });
    } else {
        let _ = phase_tx.send(Phase::Abort);
    }

    let mut counts = RunCounts {
        successful_handshakes,
        failed_handshakes,
        ..RunCounts::default()
    };

    for task in tasks {
        match task.await {
            Ok(result) => {
                counts.unexpected_disconnects += result.unexpected_disconnects;
                counts.protocol_errors += result.protocol_errors;
                counts.clean_disconnects += result.clean_disconnects;
            }
            Err(_) => counts.unexpected_disconnects += 1,
        }
    }

    let duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    RunReport::from_counts(scenario.name.clone(), scenario.clients, counts, duration_ms)
}

pub async fn run_local(
    bind_addr: SocketAddr,
    scenario: &Scenario,
) -> Result<RunReport, RunnerError> {
    let mut child = ChildTarget::spawn(bind_addr, scenario).await?;

    if let Err(error) = child.begin_measurement().await {
        child.abort().await;
        return Err(error);
    }

    let mut report = run_clients(bind_addr, scenario).await;
    let server_report = match child.stop().await {
        Ok(report) => report,
        Err(error) => {
            child.abort().await;
            return Err(error);
        }
    };

    if let Err(error) = child.reap().await {
        child.abort().await;
        return Err(error);
    }

    report.add_protocol_errors(server_report.metrics.protocol_errors_total as usize);
    Ok(report)
}

pub async fn serve_until(
    bind_addr: SocketAddr,
    protocol_version: u8,
    max_connections: usize,
    stop_rx: watch::Receiver<bool>,
) -> Result<NetworkMetrics, RunnerError> {
    server_target::serve_until(bind_addr, protocol_version, max_connections, stop_rx).await
}

struct ChildTarget {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    pid: u32,
}

impl ChildTarget {
    async fn spawn(bind_addr: SocketAddr, scenario: &Scenario) -> Result<Self, RunnerError> {
        let executable = std::env::current_exe()?;
        let mut child = Command::new(executable)
            .arg("serve-child")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()?;

        let pid = child
            .id()
            .ok_or_else(|| RunnerError::ChildExited("process has no PID".into()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| RunnerError::ChildProtocol("child stdin was not piped".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RunnerError::ChildProtocol("child stdout was not piped".into()))?;
        let mut target = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout).lines(),
            pid,
        };

        if let Err(error) = target
            .send(&ChildCommand::Start {
                bind_addr: bind_addr.to_string(),
                scenario: scenario.clone(),
            })
            .await
        {
            target.abort().await;
            return Err(error);
        }

        match target.recv().await {
            Ok(ChildEvent::Ready { pid }) if pid == target.pid => Ok(target),
            Ok(ChildEvent::Ready { pid }) => {
                target.abort().await;
                Err(RunnerError::ChildProtocol(format!(
                    "ready PID {pid} did not match spawned PID {}",
                    target.pid
                )))
            }
            Ok(ChildEvent::Error { message }) => {
                target.abort().await;
                Err(RunnerError::ChildProtocol(message))
            }
            Ok(other) => {
                target.abort().await;
                Err(RunnerError::ChildProtocol(format!(
                    "expected ready event, got {other:?}"
                )))
            }
            Err(error) => {
                target.abort().await;
                Err(error)
            }
        }
    }

    async fn begin_measurement(&mut self) -> Result<(), RunnerError> {
        self.send(&ChildCommand::BeginMeasurement).await?;
        match self.recv().await? {
            ChildEvent::MeasurementStarted => Ok(()),
            ChildEvent::Error { message } => Err(RunnerError::ChildProtocol(message)),
            other => Err(RunnerError::ChildProtocol(format!(
                "expected measurement_started event, got {other:?}"
            ))),
        }
    }

    async fn stop(&mut self) -> Result<ServerRunReport, RunnerError> {
        self.send(&ChildCommand::Stop).await?;
        match self.recv().await? {
            ChildEvent::Stopped { report } => Ok(report),
            ChildEvent::Error { message } => Err(RunnerError::ChildProtocol(message)),
            other => Err(RunnerError::ChildProtocol(format!(
                "expected stopped event, got {other:?}"
            ))),
        }
    }

    async fn send(&mut self, command: &ChildCommand) -> Result<(), RunnerError> {
        let json = serde_json::to_vec(command)
            .map_err(|error| RunnerError::ChildProtocol(error.to_string()))?;
        self.stdin.write_all(&json).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<ChildEvent, RunnerError> {
        let Some(line) = self.stdout.next_line().await? else {
            let status = self.child.wait().await?;
            return Err(RunnerError::ChildExited(status.to_string()));
        };
        serde_json::from_str(&line).map_err(|error| RunnerError::ChildProtocol(error.to_string()))
    }

    async fn reap(&mut self) -> Result<(), RunnerError> {
        let status = self.child.wait().await?;
        if status.success() {
            Ok(())
        } else {
            Err(RunnerError::ChildExited(status.to_string()))
        }
    }

    async fn abort(&mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

fn stagger_delay(index: usize, clients: usize, ramp_up_seconds: u64) -> Duration {
    if clients <= 1 || ramp_up_seconds == 0 {
        return Duration::ZERO;
    }

    let total_ms = u128::from(ramp_up_seconds).saturating_mul(1_000);
    let offset_ms = total_ms
        .saturating_mul(index as u128)
        .checked_div(clients as u128)
        .unwrap_or(0)
        .min(u64::MAX as u128) as u64;
    Duration::from_millis(offset_ms)
}
