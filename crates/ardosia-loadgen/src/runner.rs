use std::net::SocketAddr;
use std::time::Duration;

use ardosia_network::{NetworkError, NetworkMetrics};
use thiserror::Error;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use crate::client_task::{ConnectOutcome, Phase, run_client_task};
use crate::report::{RunCounts, RunReport};
use crate::scenario::Scenario;
use crate::server_target;

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error(transparent)]
    Network(#[from] NetworkError),

    #[error("benchmark task failed: {0}")]
    Task(String),
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
    let (stop_tx, server_task) = server_target::spawn_local_target(
        bind_addr,
        scenario.protocol_version,
        scenario.clients.saturating_add(64).max(1),
    )
    .await?;

    let mut report = run_clients(bind_addr, scenario).await;
    let _ = stop_tx.send(true);

    let metrics = server_task
        .await
        .map_err(|error| RunnerError::Task(error.to_string()))??;
    report.add_protocol_errors(metrics.protocol_errors_total as usize);
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
