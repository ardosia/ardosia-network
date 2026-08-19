use std::net::SocketAddr;
use std::time::{Duration, Instant as StdInstant};

use bytes::Bytes;
use raknet_rust::client::{ClientSendOptions, RaknetClient, RaknetClientConfig, RaknetClientEvent};
use raknet_rust::low_level::protocol::Reliability as VendorReliability;
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, sleep, sleep_until, timeout};

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

#[derive(Debug, Default)]
pub(crate) struct ClientTaskResult {
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
    mut phase_rx: watch::Receiver<Phase>,
    outcome_tx: mpsc::Sender<ConnectOutcome>,
) -> ClientTaskResult {
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
        Ok(Err(_)) | Err(_) => {
            let _ = outcome_tx.send(ConnectOutcome::Failed).await;
            return ClientTaskResult::default();
        }
    };

    let _ = outcome_tx.send(ConnectOutcome::Ready).await;
    let mut result = ClientTaskResult::default();

    loop {
        let phase = *phase_rx.borrow();
        match phase {
            Phase::Abort | Phase::Shutdown => {
                finish_client(&mut client, &mut result).await;
                return result;
            }
            Phase::Drain => {
                wait_post_measurement(&mut client, &mut phase_rx, &mut result).await;
                return result;
            }
            Phase::Ramp => {
                tokio::select! {
                    changed = phase_rx.changed() => {
                        if changed.is_err() {
                            result.unexpected_disconnects += 1;
                            return result;
                        }
                    }
                    event = client.next_event() => {
                        if !handle_client_event(
                            event,
                            client_id,
                            &mut result,
                            None,
                        ) {
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
            result.send_errors += 1;
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
        if Instant::now() >= deadline {
            wait_post_measurement(client, phase_rx, result).await;
            return;
        }

        let next_traffic = next_lane_deadline(&lanes);
        let next_rtt = rtt_lane.as_ref().map(|lane| lane.next_due);
        let next_due = min_deadline(next_traffic, next_rtt)
            .unwrap_or(deadline)
            .min(deadline);

        tokio::select! {
            _ = sleep_until(deadline) => {
                wait_post_measurement(client, phase_rx, result).await;
                return;
            }
            changed = phase_rx.changed() => {
                if changed.is_err() {
                    result.unexpected_disconnects += 1;
                    return;
                }
                match *phase_rx.borrow() {
                    Phase::Abort | Phase::Shutdown => {
                        finish_client(client, result).await;
                        return;
                    }
                    Phase::Drain => {
                        wait_post_measurement(client, phase_rx, result).await;
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
    result: &mut ClientTaskResult,
) {
    loop {
        match *phase_rx.borrow() {
            Phase::Abort | Phase::Shutdown => {
                finish_client(client, result).await;
                return;
            }
            Phase::Ramp | Phase::Measure { .. } | Phase::Drain => {}
        }

        tokio::select! {
            changed = phase_rx.changed() => {
                if changed.is_err() {
                    result.unexpected_disconnects += 1;
                    return;
                }
            }
            event = client.next_event() => {
                match event {
                    Some(RaknetClientEvent::Packet { .. }) => {}
                    Some(RaknetClientEvent::DecodeError { .. }) => {
                        result.protocol_errors += 1;
                    }
                    Some(RaknetClientEvent::Disconnected { .. }) | None => {
                        result.unexpected_disconnects += 1;
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
            Err(()) => result.send_errors += 1,
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
        result.protocol_errors += 1;
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
            result.send_errors += 1;
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
                    result.protocol_errors += 1;
                    return true;
                }
            };
            if frame.client_id != client_id {
                result.protocol_errors += 1;
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
                        None => result.protocol_errors += 1,
                    }
                }
                FrameKind::EchoRequest | FrameKind::ClientHello => {
                    result.protocol_errors += 1;
                }
            }
            true
        }
        Some(RaknetClientEvent::DecodeError { .. }) => {
            result.protocol_errors += 1;
            true
        }
        Some(RaknetClientEvent::Disconnected { .. }) | None => {
            result.unexpected_disconnects += 1;
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

async fn finish_client(client: &mut RaknetClient, result: &mut ClientTaskResult) {
    if client.disconnect(None).await.is_ok() {
        result.clean_disconnects += 1;
    } else {
        result.unexpected_disconnects += 1;
    }
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
