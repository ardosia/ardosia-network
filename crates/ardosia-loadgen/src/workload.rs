use std::collections::BTreeMap;
use std::time::{Duration, Instant as StdInstant};

use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use crate::frame::FrameKind;
use crate::scenario::{Direction, Scenario, TrafficKind};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrafficCounts {
    pub tx_frames: u64,
    pub tx_payload_bytes: u64,
    pub rx_frames: u64,
    pub rx_payload_bytes: u64,
    pub max_tx_payload_bytes: u64,
    pub max_rx_payload_bytes: u64,
}

impl TrafficCounts {
    fn record_tx(&mut self, payload_bytes: usize) {
        let payload_bytes = usize_to_u64(payload_bytes);
        self.tx_frames = self.tx_frames.saturating_add(1);
        self.tx_payload_bytes = self.tx_payload_bytes.saturating_add(payload_bytes);
        self.max_tx_payload_bytes = self.max_tx_payload_bytes.max(payload_bytes);
    }

    fn record_rx(&mut self, payload_bytes: usize) {
        let payload_bytes = usize_to_u64(payload_bytes);
        self.rx_frames = self.rx_frames.saturating_add(1);
        self.rx_payload_bytes = self.rx_payload_bytes.saturating_add(payload_bytes);
        self.max_rx_payload_bytes = self.max_rx_payload_bytes.max(payload_bytes);
    }

    fn merge(&mut self, other: Self) {
        self.tx_frames = self.tx_frames.saturating_add(other.tx_frames);
        self.tx_payload_bytes = self.tx_payload_bytes.saturating_add(other.tx_payload_bytes);
        self.rx_frames = self.rx_frames.saturating_add(other.rx_frames);
        self.rx_payload_bytes = self.rx_payload_bytes.saturating_add(other.rx_payload_bytes);
        self.max_tx_payload_bytes = self.max_tx_payload_bytes.max(other.max_tx_payload_bytes);
        self.max_rx_payload_bytes = self.max_rx_payload_bytes.max(other.max_rx_payload_bytes);
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkloadCounts {
    pub unreliable: TrafficCounts,
    pub reliable_ordered: TrafficCounts,
    pub fragmented_reliable_ordered: TrafficCounts,
    pub rtt_requests: TrafficCounts,
    pub rtt_responses: TrafficCounts,
    pub pending_probe_overflows: u64,
}

impl WorkloadCounts {
    pub fn record_tx(&mut self, kind: FrameKind, payload_bytes: usize) {
        if let Some(counts) = self.counts_mut(kind) {
            counts.record_tx(payload_bytes);
        }
    }

    pub fn record_rx(&mut self, kind: FrameKind, payload_bytes: usize) {
        if let Some(counts) = self.counts_mut(kind) {
            counts.record_rx(payload_bytes);
        }
    }

    pub fn merge(&mut self, other: Self) {
        self.unreliable.merge(other.unreliable);
        self.reliable_ordered.merge(other.reliable_ordered);
        self.fragmented_reliable_ordered
            .merge(other.fragmented_reliable_ordered);
        self.rtt_requests.merge(other.rtt_requests);
        self.rtt_responses.merge(other.rtt_responses);
        self.pending_probe_overflows = self
            .pending_probe_overflows
            .saturating_add(other.pending_probe_overflows);
    }

    fn counts_mut(&mut self, kind: FrameKind) -> Option<&mut TrafficCounts> {
        match kind {
            FrameKind::UnreliableData => Some(&mut self.unreliable),
            FrameKind::ReliableOrderedData => Some(&mut self.reliable_ordered),
            FrameKind::FragmentedReliableOrderedData => {
                Some(&mut self.fragmented_reliable_ordered)
            }
            FrameKind::EchoRequest => Some(&mut self.rtt_requests),
            FrameKind::EchoResponse => Some(&mut self.rtt_responses),
            FrameKind::ClientHello => None,
        }
    }
}

#[derive(Debug)]
pub struct PendingProbes {
    capacity: usize,
    entries: BTreeMap<u64, StdInstant>,
}

impl PendingProbes {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            entries: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, probe_id: u64, started: StdInstant) -> Result<(), PendingProbeOverflow> {
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&probe_id) {
            return Err(PendingProbeOverflow);
        }
        self.entries.insert(probe_id, started);
        Ok(())
    }

    pub fn complete(&mut self, probe_id: u64) -> Option<Duration> {
        self.entries.remove(&probe_id).map(|started| started.elapsed())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingProbeOverflow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkloadSide {
    Client,
    Server,
}

#[derive(Debug, Clone)]
pub(crate) struct ScheduledTrafficLane {
    pub(crate) kind: TrafficKind,
    pub(crate) payload_bytes: usize,
    pub(crate) period: Duration,
    pub(crate) next_due: Instant,
    pub(crate) sequence: u64,
}

impl ScheduledTrafficLane {
    pub(crate) fn advance(&mut self, now: Instant) {
        loop {
            self.next_due += self.period;
            if self.next_due > now {
                break;
            }
        }
    }
}

pub(crate) fn build_traffic_lanes(
    scenario: &Scenario,
    client_id: u64,
    side: WorkloadSide,
    start: Instant,
) -> Vec<ScheduledTrafficLane> {
    scenario
        .traffic
        .iter()
        .enumerate()
        .filter(|(_, spec)| direction_includes(spec.direction, side))
        .map(|(index, spec)| {
            let period = period_from_rate(spec.packets_per_second_per_client);
            let lane_index = index.saturating_mul(2).saturating_add(match side {
                WorkloadSide::Client => 0,
                WorkloadSide::Server => 1,
            });
            ScheduledTrafficLane {
                kind: spec.kind,
                payload_bytes: spec.payload_bytes,
                period,
                next_due: start
                    + initial_phase_offset(scenario.seed, client_id, lane_index, period),
                sequence: 0,
            }
        })
        .collect()
}

pub(crate) fn next_lane_deadline(lanes: &[ScheduledTrafficLane]) -> Option<Instant> {
    lanes.iter().map(|lane| lane.next_due).min()
}

pub(crate) fn frame_kind(kind: TrafficKind) -> FrameKind {
    match kind {
        TrafficKind::Unreliable => FrameKind::UnreliableData,
        TrafficKind::ReliableOrdered => FrameKind::ReliableOrderedData,
        TrafficKind::FragmentedReliableOrdered => FrameKind::FragmentedReliableOrderedData,
    }
}

pub(crate) fn period_from_rate(rate: f64) -> Duration {
    Duration::from_secs_f64(1.0 / rate)
}

pub fn initial_phase_offset(
    seed: u64,
    client_id: u64,
    lane_index: usize,
    period: Duration,
) -> Duration {
    let period_nanos = period.as_nanos();
    if period_nanos == 0 {
        return Duration::ZERO;
    }

    let lane = u64::try_from(lane_index).unwrap_or(u64::MAX);
    let input = seed
        ^ client_id.wrapping_mul(0xD6E8_FEB8_6659_FD93)
        ^ lane.wrapping_mul(0xA5A3_564E_27F8_864D);
    let offset_nanos = u128::from(mix64(input)) % period_nanos;
    let seconds = u64::try_from(offset_nanos / 1_000_000_000).unwrap_or(u64::MAX);
    let nanos = u32::try_from(offset_nanos % 1_000_000_000).unwrap_or(999_999_999);
    Duration::new(seconds, nanos)
}

fn direction_includes(direction: Direction, side: WorkloadSide) -> bool {
    matches!(direction, Direction::Bidirectional)
        || matches!(
            (direction, side),
            (Direction::ClientToServer, WorkloadSide::Client)
                | (Direction::ServerToClient, WorkloadSide::Server)
        )
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}
