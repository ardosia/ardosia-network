use std::net::SocketAddr;
use std::time::{Duration, Instant as StdInstant};

use bytes::Bytes;
use raknet_rust::client::{ClientSendOptions, RaknetClient, RaknetClientConfig, RaknetClientEvent};
use raknet_rust::low_level::protocol::Reliability as VendorReliability;
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, sleep, sleep_until, timeout};

use crate::churn::{
    DisconnectCounts, DisconnectIntent, DisconnectOutcome, classify_disconnect,
};
use crate::frame::{BenchmarkFrame, FrameKind};
use crate::latency::LatencyHistogram;
use crate::scenario::{Scenario, TrafficKind};
use crate::workload::{
    PendingProbes, WorkloadCounts, WorkloadSide, build_traffic_lanes, deterministic_payload,
    frame_kind, initial_phase_offset, next_lane_deadline, period_from_rate,
};

const PENDING_PROBE_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Phase {
    Ramp,
    Measure { deadline: Instant },
    Drain,
    Shutdown,
    Abort,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ConnectOutcome {
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GenerationDirective {
    Continue,
    PlannedDisconnect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GenerationConnectOutcome {
    Ready { client_id: u64 },
    Failed { client_id: u64, timed_out: bool },
}

#[derive(Debug, Default)]
pub(crate) struct ClientTaskResult {
    pub(crate) client_id: u64,
    pub(crate) completed_planned_disconnects: usize,
    pub(crate) unexpected_disconnects: usize,
    pub(crate) protocol_errors: usize,
    pub(crate) clean_disconnects: usize,
    pub(crate) send_errors: usize,
    pub(crate) workload: WorkloadCounts,
    pub(crate) latency: LatencyHistogram,
}

pub(crate) async fn run_client_task(
    target: SocketAddr,
    client_id: u64,
    scenario: Scenario,
    stagger: Duration,
    phase_rx: watch::Receiver<Phase>,
    outcome_tx: mpsc::Sender<ConnectOutcome>,
) -> ClientTaskResult {
    let (directive_tx, directive_rx) = watch::channel(GenerationDirective::Continue);
    let result = run_client_generation_inner(
        target,
        client_id,
        scenario,
        stagger,
        phase_rx,
        directive_rx,
        OutcomeSink::Steady(outcome_tx),
    )
    .await;
    drop(directive_tx);
    result
}

pub(crate) async fn run_client_generation(
    target: SocketAddr,
    client_id: u64,
    scenario: Scenario,
    stagger: Duration,
    phase_rx: watch::Receiver<Phase>,
    directive_rx: watch::Receiver<GenerationDirective>,
    outcome_tx: mpsc::Sender<GenerationConnectOutcome>,
) -> ClientTaskResult {
    run_client_generation_inner(
        target,
        client_id,
        scenario,
        stagger,
        phase_rx,
        directive_rx,
        OutcomeSink::Generation(outcome_tx),
    )
    .await
}

async fn run_client_generation_inner(
    target: SocketAddr,
    client_id: u64,
    scenario: Scenario,
    stagger: Duration,
    mut phase_rx: watch::Receiver<Phase>,
    mut directive_rx: watch::Receiver<GenerationDirective>,
    outcome_tx: OutcomeSink,
) -> ClientTaskResult {
    let mut result = ClientTaskResult {
        client_id,
        ..ClientTaskResult::default()
    };

    if !stagger.is_zero() {
        sleep(stagger).await;
    }

    let config = RaknetClientConfig {
        protocol_version: scenario.protocol_version,
        ..RaknetClientConfig::default()
    };

    let connect = timeout(
        Duration::from_secs(scenario.connect_timeout_seconds),
        RaknetClient::connect_with_config(target, config),
    )
    .await;

    let mut client = match connect {
        Ok(Ok(client)) => client,
        Ok(Err(_)) => {
            outcome_tx.failed(client_id, false).await;
            return result;
        }
        Err(_) => {
            outcome_tx.failed(client_id, true).await;
            return result;
        }
    };

    outcome_tx.ready(client_id).await;

    loop {
        if planned_disconnect_requested(&directive_rx) {
            finish_client(
                &mut client,
                &mut result,
                DisconnectIntent::PlannedChurn,
                scenario.connect_timeout_seconds,
            )
            .await;
            return result;
        }

        let phase = *phase_rx.borrow();
        match phase {
            Phase::Abort | Phase::Shutdown => {
                finish_client(
                    &mut client,
                    &mut result,
                    DisconnectIntent::FinalShutdown,
                    scenario.connect_timeout_seconds,
                )
                .await;
                return result;
            }
            Phase::Drain => {
                wait_post_measurement(
                    &mut client,
                    &mut phase_rx,
                    &mut directive_rx,
                    &mut result,
                    scenario.connect_timeout_seconds,
                )
                .await;
                return result;
            }
            Phase::Ramp => {
                tokio::select! {
                    biased;
                    changed = directive_rx.changed() => {
                        if changed.is_err() {
                            result.unexpected_disconnects =
                                result.unexpected_disconnects.saturating_add(1);
                            return result;
                        }
                        if planned_disconnect_requested(&directive_rx) {
                            finish_client(
                                &mut client,
                                &mut result,
                                DisconnectIntent::PlannedChurn,
                                scenario.connect_timeout_seconds,
                            )
                            .await;
                            return result;
                        }
                    }
                    changed = phase_rx.changed() => {
                        if changed.is_err() {
                            result.unexpected_disconnects =
                                result.unexpected_disconnects.saturating_add(1);
                            return result;
                        }
                    }
                    event = client.next_event() => {
                        if !handle_client_event(event, client_id, &mut result, None) {
                            return result;
                        }
                    }
                }
            }
            Phase::Measure { deadline } => {
                run_measurement(
                    &mut client,
                    client_id,
                    &scenario,
                    deadline,
                    &mut phase_rx,
                    &mut directive_rx,
                    &mut result,
                )
                .await;
                return result;
            }
        }
    }
}

async fn run_measurement(
    client: &mut RaknetClient,
    client_id: u64,
    scenario: &Scenario,
    deadline: Instant,
    phase_rx: &mut watch::Receiver<Phase>,
    directive_rx: &mut watch::Receiver<GenerationDirective>,
    result: &mut ClientTaskResult,
) {
    if !scenario.traffic.is_empty() || scenario.rtt.is_some() {
        let hello = BenchmarkFrame {
            kind: FrameKind::ClientHello,
            client_id,
            sequence: 0,
            probe_id: 0,
            payload: Bytes::new(),
        };
        if send_client_frame(client, &hello, VendorReliability::ReliableOrdered)
            .await
            .is_err()
        {
            result.send_errors = result.send_errors.saturating_add(1);
        }
    }

    let start = Instant::now();
    let mut lanes = build_traffic_lanes(scenario, client_id, WorkloadSide::Client, start);
    let mut rtt_lane = scenario.rtt.as_ref().map(|rtt| {
        let period = period_from_rate(rtt.probes_per_second_per_client);
        RttLane {
            payload_bytes: rtt.payload_bytes,
            period,
            next_due: start
                + initial_phase_offset(
                    scenario.seed,
                    client_id,
                    scenario.traffic.len().saturating_mul(2).saturating_add(2),
                    period,
                ),
            probe_id: 0,
            sequence: 0,
        }
    });
    let mut pending = PendingProbes::with_capacity(PENDING_PROBE_CAPACITY);

    loop {
        if planned_disconnect_requested(directive_rx) {
            finish_client(
                client,
                result,
                DisconnectIntent::PlannedChurn,
                scenario.connect_timeout_seconds,
            )
            .await;
            return;
        }

        if Instant::now() >= deadline {
            wait_post_measurement(
                client,
                phase_rx,
                directive_rx,
                result,
                scenario.connect_timeout_seconds,
            )
            .await;
            return;
        }

        let next_traffic = next_lane_deadline(&lanes);
        let next_rtt = rtt_lane.as_ref().map(|lane| lane.next_due);
        let next_due = min_deadline(next_traffic, next_rtt)
            .unwrap_or(deadline)
            .min(deadline);

        tokio::select! {
            biased;
            changed = directive_rx.changed() => {
                if changed.is_err() {
                    result.unexpected_disconnects =
                        result.unexpected_disconnects.saturating_add(1);
                    return;
                }
                if planned_disconnect_requested(directive_rx) {
                    finish_client(
                        client,
                        result,
                        DisconnectIntent::PlannedChurn,
                        scenario.connect_timeout_seconds,
                    )
                    .await;
                    return;
                }
            }
            _ = sleep_until(deadline) => {
                wait_post_measurement(
                    client,
                    phase_rx,
                    directive_rx,
                    result,
                    scenario.connect_timeout_seconds,
                )
                .await;
                return;
            }
            changed = phase_rx.changed() => {
                if changed.is_err() {
                    result.unexpected_disconnects =
                        result.unexpected_disconnects.saturating_add(1);
                    return;
                }
                let phase = *phase_rx.borrow();
                match phase {
                    Phase::Abort | Phase::Shutdown => {
                        finish_client(
                            client,
                            result,
                            DisconnectIntent::FinalShutdown,
                            scenario.connect_timeout_seconds,
                        )
                        .await;
                        return;
                    }
                    Phase::Drain => {
                        wait_post_measurement(
                            client,
                            phase_rx,
                            directive_rx,
                            result,
                            scenario.connect_timeout_seconds,
                        )
                        .await;
                        return;
                    }
                    Phase::Ramp | Phase::Measure { .. } => {}
                }
            }
            event = client.next_event() => {
                if !handle_client_event(event, client_id, result, Some(&mut pending)) {
                    return;
                }
            }
            _ = sleep_until(next_due) => {
                let now = Instant::now();
                send_due_traffic(client, client_id, &mut lanes, now, result).await;
                if let Some(lane) = rtt_lane.as_mut()
                    && lane.next_due <= now
                {
                    send_rtt_probe(client, client_id, lane, &mut pending, result).await;
                    lane.advance(now);
                }
            }
        }
    }
}

async fn wait_post_measurement(
    client: &mut RaknetClient,
    phase_rx: &mut watch::Receiver<Phase>,
    directive_rx: &mut watch::Receiver<GenerationDirective>,
    result: &mut ClientTaskResult,
    disconnect_timeout_seconds: u64,
) {
    loop {
        if planned_disconnect_requested(directive_rx) {
            finish_client(
                client,
                result,
                DisconnectIntent::PlannedChurn,
                disconnect_timeout_seconds,
            )
            .await;
            return;
        }

        let phase = *phase_rx.borrow();
        match phase {
            Phase::Abort | Phase::Shutdown => {
                finish_client(
                    client,
                    result,
                    DisconnectIntent::FinalShutdown,
                    disconnect_timeout_seconds,
                )
                .await;
                return;
            }
            Phase::Ramp | Phase::Measure { .. } | Phase::Drain => {}
        }

        tokio::select! {
            biased;
            changed = directive_rx.changed() => {
                if changed.is_err() {
                    result.unexpected_disconnects =
                        result.unexpected_disconnects.saturating_add(1);
                    return;
                }
                if planned_disconnect_requested(directive_rx) {
                    finish_client(
                        client,
                        result,
                        DisconnectIntent::PlannedChurn,
                        disconnect_timeout_seconds,
                    )
                    .await;
                    return;
                }
            }
            changed = phase_rx.changed() => {
                if changed.is_err() {
                    result.unexpected_disconnects =
                        result.unexpected_disconnects.saturating_add(1);
                    return;
                }
            }
            event = client.next_event() => {
                match event {
                    Some(RaknetClientEvent::Packet { .. }) => {}
                    Some(RaknetClientEvent::DecodeError { .. }) => {
                        result.protocol_errors = result.protocol_errors.saturating_add(1);
                    }
                    Some(RaknetClientEvent::Disconnected { .. }) | None => {
                        result.unexpected_disconnects =
                            result.unexpected_disconnects.saturating_add(1);
                        return;
                    }
                    Some(_) => {}
                }
            }
        }
    }
}

async fn send_due_traffic(
    client: &mut RaknetClient,
    client_id: u64,
    lanes: &mut [crate::workload::ScheduledTrafficLane],
    now: Instant,
    result: &mut ClientTaskResult,
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
            TrafficKind::Unreliable => VendorReliability::Unreliable,
            TrafficKind::ReliableOrdered | TrafficKind::FragmentedReliableOrdered => {
                VendorReliability::ReliableOrdered
            }
        };

        match send_client_frame(client, &frame, reliability).await {
            Ok(()) => result.workload.record_tx(kind, lane.payload_bytes),
            Err(()) => result.send_errors = result.send_errors.saturating_add(1),
        }
        lane.sequence = lane.sequence.wrapping_add(1);
        lane.advance(now);
    }
}

async fn send_rtt_probe(
    client: &mut RaknetClient,
    client_id: u64,
    lane: &mut RttLane,
    pending: &mut PendingProbes,
    result: &mut ClientTaskResult,
) {
    let probe_id = lane.probe_id;
    lane.probe_id = lane.probe_id.wrapping_add(1);
    if pending.insert(probe_id, StdInstant::now()).is_err() {
        result.workload.pending_probe_overflows =
            result.workload.pending_probe_overflows.saturating_add(1);
        result.protocol_errors = result.protocol_errors.saturating_add(1);
        return;
    }

    let frame = BenchmarkFrame {
        kind: FrameKind::EchoRequest,
        client_id,
        sequence: lane.sequence,
        probe_id,
        payload: deterministic_payload(client_id, lane.sequence, lane.payload_bytes),
    };
    lane.sequence = lane.sequence.wrapping_add(1);

    match send_client_frame(client, &frame, VendorReliability::ReliableOrdered).await {
        Ok(()) => result
            .workload
            .record_tx(FrameKind::EchoRequest, lane.payload_bytes),
        Err(()) => {
            let _ = pending.complete(probe_id);
            result.send_errors = result.send_errors.saturating_add(1);
        }
    }
}

fn handle_client_event(
    event: Option<RaknetClientEvent>,
    client_id: u64,
    result: &mut ClientTaskResult,
    mut pending: Option<&mut PendingProbes>,
) -> bool {
    match event {
        Some(RaknetClientEvent::Packet { payload, .. }) => {
            let frame = match BenchmarkFrame::decode(&payload) {
                Ok(frame) => frame,
                Err(_) => {
                    result.protocol_errors = result.protocol_errors.saturating_add(1);
                    return true;
                }
            };
            if frame.client_id != client_id {
                result.protocol_errors = result.protocol_errors.saturating_add(1);
                return true;
            }

            match frame.kind {
                FrameKind::UnreliableData
                | FrameKind::ReliableOrderedData
                | FrameKind::FragmentedReliableOrderedData => {
                    result.workload.record_rx(frame.kind, frame.payload.len());
                }
                FrameKind::EchoResponse => {
                    result
                        .workload
                        .record_rx(FrameKind::EchoResponse, frame.payload.len());
                    match pending
                        .as_mut()
                        .and_then(|pending| pending.complete(frame.probe_id))
                    {
                        Some(elapsed) => result.latency.record(elapsed),
                        None => {
                            result.protocol_errors = result.protocol_errors.saturating_add(1);
                        }
                    }
                }
                FrameKind::EchoRequest | FrameKind::ClientHello => {
                    result.protocol_errors = result.protocol_errors.saturating_add(1);
                }
            }
            true
        }
        Some(RaknetClientEvent::DecodeError { .. }) => {
            result.protocol_errors = result.protocol_errors.saturating_add(1);
            true
        }
        Some(RaknetClientEvent::Disconnected { .. }) | None => {
            result.unexpected_disconnects = result.unexpected_disconnects.saturating_add(1);
            false
        }
        Some(_) => true,
    }
}

async fn send_client_frame(
    client: &mut RaknetClient,
    frame: &BenchmarkFrame,
    reliability: VendorReliability,
) -> Result<(), ()> {
    client
        .send_with_options(
            frame.encode(),
            ClientSendOptions {
                reliability,
                ..ClientSendOptions::default()
            },
        )
        .await
        .map_err(|_| ())
}

async fn finish_client(
    client: &mut RaknetClient,
    result: &mut ClientTaskResult,
    intent: DisconnectIntent,
    disconnect_timeout_seconds: u64,
) {
    let outcome = match timeout(
        Duration::from_secs(disconnect_timeout_seconds),
        client.disconnect(None),
    )
    .await
    {
        Ok(Ok(())) => DisconnectOutcome::Clean,
        Ok(Err(_)) | Err(_) => DisconnectOutcome::Failed,
    };
    apply_disconnect_counts(result, classify_disconnect(intent, outcome));
}

fn apply_disconnect_counts(result: &mut ClientTaskResult, counts: DisconnectCounts) {
    result.completed_planned_disconnects = result
        .completed_planned_disconnects
        .saturating_add(counts.completed_planned_disconnects);
    result.clean_disconnects = result
        .clean_disconnects
        .saturating_add(counts.clean_disconnects);
    result.unexpected_disconnects = result
        .unexpected_disconnects
        .saturating_add(counts.unexpected_disconnects);
}

fn planned_disconnect_requested(
    directive_rx: &watch::Receiver<GenerationDirective>,
) -> bool {
    *directive_rx.borrow() == GenerationDirective::PlannedDisconnect
}

fn min_deadline(left: Option<Instant>, right: Option<Instant>) -> Option<Instant> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

struct RttLane {
    payload_bytes: usize,
    period: Duration,
    next_due: Instant,
    probe_id: u64,
    sequence: u64,
}

impl RttLane {
    fn advance(&mut self, now: Instant) {
        loop {
            self.next_due += self.period;
            if self.next_due > now {
                break;
            }
        }
    }
}

enum OutcomeSink {
    Steady(mpsc::Sender<ConnectOutcome>),
    Generation(mpsc::Sender<GenerationConnectOutcome>),
}

impl OutcomeSink {
    async fn ready(&self, client_id: u64) {
        match self {
            Self::Steady(tx) => {
                let _ = tx.send(ConnectOutcome::Ready).await;
            }
            Self::Generation(tx) => {
                let _ = tx
                    .send(GenerationConnectOutcome::Ready { client_id })
                    .await;
            }
        }
    }

    async fn failed(&self, client_id: u64, timed_out: bool) {
        match self {
            Self::Steady(tx) => {
                let _ = tx.send(ConnectOutcome::Failed).await;
            }
            Self::Generation(tx) => {
                let _ = tx
                    .send(GenerationConnectOutcome::Failed {
                        client_id,
                        timed_out,
                    })
                    .await;
            }
        }
    }
}
