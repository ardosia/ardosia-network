use std::net::SocketAddr;
use std::time::Duration;

use ardosia_network::{
    Connection, NetworkConfig, NetworkError, NetworkMetrics, NetworkServer, Reliability,
};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{Instant, sleep, sleep_until};

use crate::frame::{BenchmarkFrame, FrameKind};
use crate::runner::RunnerError;
use crate::scenario::{Scenario, TrafficKind};
use crate::send_policy::counts_as_benchmark_send_error;
use crate::workload::{
    WorkloadCounts, WorkloadSide, build_traffic_lanes, deterministic_payload, frame_kind,
    next_lane_deadline,
};

#[derive(Debug, Default)]
pub(crate) struct ServerTargetResult {
    pub(crate) metrics: NetworkMetrics,
    pub(crate) workload: WorkloadCounts,
    pub(crate) protocol_errors: usize,
    pub(crate) send_errors: usize,
}

pub(crate) struct ServerTargetHandle {
    measure_tx: watch::Sender<bool>,
    snapshot_tx: mpsc::Sender<oneshot::Sender<NetworkMetrics>>,
    stop_tx: watch::Sender<bool>,
    task: JoinHandle<Result<ServerTargetResult, RunnerError>>,
}

impl ServerTargetHandle {
    pub(crate) fn begin_measurement(&self) {
        let _ = self.measure_tx.send(true);
    }

    pub(crate) async fn snapshot(&self) -> Result<NetworkMetrics, RunnerError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.snapshot_tx
            .send(response_tx)
            .await
            .map_err(|_| RunnerError::Task("server snapshot channel closed".into()))?;
        response_rx
            .await
            .map_err(|_| RunnerError::Task("server snapshot response dropped".into()))
    }

    pub(crate) async fn stop(self) -> Result<ServerTargetResult, RunnerError> {
        let _ = self.stop_tx.send(true);
        self.task
            .await
            .map_err(|error| RunnerError::Task(error.to_string()))?
    }
}

pub(crate) async fn spawn_local_target(
    bind_addr: SocketAddr,
    scenario: Scenario,
) -> Result<ServerTargetHandle, RunnerError> {
    let server = NetworkServer::bind(NetworkConfig {
        bind_addr,
        raknet_protocols: vec![scenario.protocol_version],
        max_connections: scenario.clients.saturating_add(64).max(1),
    })
    .await?;

    let (measure_tx, measure_rx) = watch::channel(false);
    let (snapshot_tx, snapshot_rx) = mpsc::channel(8);
    let (stop_tx, stop_rx) = watch::channel(false);
    let task = tokio::spawn(run_benchmark_server_loop(
        server,
        scenario,
        measure_rx,
        snapshot_rx,
        stop_rx,
    ));
    Ok(ServerTargetHandle {
        measure_tx,
        snapshot_tx,
        stop_tx,
        task,
    })
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

    run_passive_server_loop(server, stop_rx).await
}

async fn run_benchmark_server_loop(
    mut server: NetworkServer,
    scenario: Scenario,
    measure_rx: watch::Receiver<bool>,
    mut snapshot_rx: mpsc::Receiver<oneshot::Sender<NetworkMetrics>>,
    mut stop_rx: watch::Receiver<bool>,
) -> Result<ServerTargetResult, RunnerError> {
    let mut tasks = JoinSet::new();
    let mut aggregate = ConnectionTaskResult::default();

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
            snapshot = snapshot_rx.recv() => {
                if let Some(response) = snapshot {
                    let _ = response.send(server.metrics());
                }
            }
            accepted = server.accept() => {
                let connection = accepted?;
                tasks.spawn(run_connection_task(
                    connection,
                    scenario.clone(),
                    measure_rx.clone(),
                    stop_rx.clone(),
                ));
            }
            joined = tasks.join_next(), if !tasks.is_empty() => {
                collect_connection_result(joined, &mut aggregate);
            }
        }
    }

    while let Some(joined) = tasks.join_next().await {
        collect_connection_result(Some(joined), &mut aggregate);
    }

    let drain_deadline = Instant::now() + Duration::from_secs(2);
    while server.metrics().connected_current != 0 && Instant::now() < drain_deadline {
        sleep(Duration::from_millis(10)).await;
    }

    let metrics = server.metrics();
    server.shutdown().await?;
    Ok(ServerTargetResult {
        metrics,
        workload: aggregate.workload,
        protocol_errors: aggregate.protocol_errors,
        send_errors: aggregate.send_errors,
    })
}

async fn run_connection_task(
    mut connection: Connection,
    scenario: Scenario,
    mut measure_rx: watch::Receiver<bool>,
    mut stop_rx: watch::Receiver<bool>,
) -> ConnectionTaskResult {
    let mut result = ConnectionTaskResult::default();
    let mut client_id = None;
    let mut lanes = Vec::new();

    loop {
        if *stop_rx.borrow() {
            return result;
        }

        if *measure_rx.borrow() && client_id.is_some() && lanes.is_empty() {
            lanes = build_traffic_lanes(
                &scenario,
                client_id.unwrap_or_default(),
                WorkloadSide::Server,
                Instant::now(),
            );
        }

        let next_due = next_lane_deadline(&lanes)
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(86_400));

        tokio::select! {
            changed = stop_rx.changed() => {
                if changed.is_err() || *stop_rx.borrow() {
                    return result;
                }
            }
            changed = measure_rx.changed() => {
                if changed.is_err() {
                    result.protocol_errors += 1;
                    return result;
                }
            }
            received = connection.recv() => {
                match received {
                    Ok(payload) => {
                        handle_server_frame(
                            &connection,
                            &scenario,
                            &mut client_id,
                            &mut lanes,
                            payload,
                            &mut result,
                        ).await;
                    }
                    Err(NetworkError::ConnectionClosed) => return result,
                    Err(NetworkError::Backpressure) => {
                        result.protocol_errors += 1;
                        return result;
                    }
                    Err(_) => {
                        result.protocol_errors += 1;
                        return result;
                    }
                }
            }
            _ = sleep_until(next_due), if !lanes.is_empty() => {
                send_due_server_traffic(
                    &connection,
                    client_id.unwrap_or_default(),
                    &mut lanes,
                    Instant::now(),
                    &mut result,
                ).await;
            }
        }
    }
}

async fn handle_server_frame(
    connection: &Connection,
    scenario: &Scenario,
    client_id: &mut Option<u64>,
    lanes: &mut Vec<crate::workload::ScheduledTrafficLane>,
    payload: bytes::Bytes,
    result: &mut ConnectionTaskResult,
) {
    let frame = match BenchmarkFrame::decode(&payload) {
        Ok(frame) => frame,
        Err(_) => {
            result.protocol_errors += 1;
            return;
        }
    };

    if let Some(expected) = *client_id
        && frame.client_id != expected
    {
        result.protocol_errors += 1;
        return;
    }

    match frame.kind {
        FrameKind::ClientHello => {
            if client_id
                .replace(frame.client_id)
                .is_some_and(|old| old != frame.client_id)
            {
                result.protocol_errors += 1;
                return;
            }
            if lanes.is_empty() {
                *lanes = build_traffic_lanes(
                    scenario,
                    frame.client_id,
                    WorkloadSide::Server,
                    Instant::now(),
                );
            }
        }
        FrameKind::UnreliableData
        | FrameKind::ReliableOrderedData
        | FrameKind::FragmentedReliableOrderedData => {
            if client_id.is_none() {
                result.protocol_errors += 1;
                return;
            }
            result.workload.record_rx(frame.kind, frame.payload.len());
        }
        FrameKind::EchoRequest => {
            if client_id.is_none() {
                result.protocol_errors += 1;
                return;
            }
            result
                .workload
                .record_rx(FrameKind::EchoRequest, frame.payload.len());
            let response = BenchmarkFrame {
                kind: FrameKind::EchoResponse,
                client_id: frame.client_id,
                sequence: frame.sequence,
                probe_id: frame.probe_id,
                payload: frame.payload,
            };
            match connection
                .send(response.encode(), Reliability::ReliableOrdered)
                .await
            {
                Ok(()) => result
                    .workload
                    .record_tx(FrameKind::EchoResponse, response.payload.len()),
                Err(error) => {
                    if counts_as_benchmark_send_error(&error) {
                        result.send_errors += 1;
                    }
                }
            }
        }
        FrameKind::EchoResponse => {
            result.protocol_errors += 1;
        }
    }
}

async fn send_due_server_traffic(
    connection: &Connection,
    client_id: u64,
    lanes: &mut [crate::workload::ScheduledTrafficLane],
    now: Instant,
    result: &mut ConnectionTaskResult,
) {
    for lane in lanes.iter_mut().filter(|lane| lane.next_due <= now) {
        let kind = frame_kind(lane.kind);
        let frame = BenchmarkFrame {
            kind,
            client_id,
            sequence: lane.sequence,
            probe_id: 0,
            payload: deterministic_payload(client_id, lane.sequence, lane.payload_bytes),
        };
        let reliability = match lane.kind {
            TrafficKind::Unreliable => Reliability::Unreliable,
            TrafficKind::ReliableOrdered | TrafficKind::FragmentedReliableOrdered => {
                Reliability::ReliableOrdered
            }
        };

        match connection.send(frame.encode(), reliability).await {
            Ok(()) => result.workload.record_tx(kind, lane.payload_bytes),
            Err(error) => {
                if counts_as_benchmark_send_error(&error) {
                    result.send_errors += 1;
                } else {
                    return;
                }
            }
        }
        lane.sequence = lane.sequence.wrapping_add(1);
        lane.advance(now);
    }
}

fn collect_connection_result(
    joined: Option<Result<ConnectionTaskResult, tokio::task::JoinError>>,
    aggregate: &mut ConnectionTaskResult,
) {
    match joined {
        Some(Ok(result)) => {
            aggregate.workload.merge(result.workload);
            aggregate.protocol_errors = aggregate
                .protocol_errors
                .saturating_add(result.protocol_errors);
            aggregate.send_errors = aggregate.send_errors.saturating_add(result.send_errors);
        }
        Some(Err(_)) => aggregate.protocol_errors = aggregate.protocol_errors.saturating_add(1),
        None => {}
    }
}

#[derive(Debug, Default)]
struct ConnectionTaskResult {
    workload: WorkloadCounts,
    protocol_errors: usize,
    send_errors: usize,
}

async fn run_passive_server_loop(
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
