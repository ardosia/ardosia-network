use ardosia_network::{NetworkRuntimeConfig, TransportMetrics};
use serde::{Deserialize, Serialize};

use crate::latency::LatencySummary;
use crate::resource::ResourceSummary;
use crate::scenario::{Scenario, TrafficKind};
use crate::workload::{TrafficCounts, WorkloadCounts};

pub const VENDOR_REVISION: &str = "3edfb4170e6cb5aeed992b09b50176fb7e5b6079";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunReport {
    pub environment: EnvironmentReport,
    pub scenario: Scenario,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_runtime: Option<ServerRuntimeReport>,
    pub results: ResultsReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnvironmentReport {
    pub git_commit: Option<String>,
    pub vendor_revision: String,
    pub rust_version: Option<String>,
    pub os: String,
    pub kernel: Option<String>,
    pub architecture: String,
    pub logical_cpus: Option<usize>,
    pub total_memory_bytes: Option<u64>,
    pub build_profile: String,
}

impl Default for EnvironmentReport {
    fn default() -> Self {
        Self {
            git_commit: None,
            vendor_revision: VENDOR_REVISION.into(),
            rust_version: None,
            os: std::env::consts::OS.into(),
            kernel: None,
            architecture: std::env::consts::ARCH.into(),
            logical_cpus: std::thread::available_parallelism()
                .ok()
                .map(std::num::NonZeroUsize::get),
            total_memory_bytes: None,
            build_profile: if cfg!(debug_assertions) {
                "debug".into()
            } else {
                "release".into()
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerRuntimeReport {
    pub requested_worker_shards: Option<usize>,
    pub effective_worker_shards: usize,
}

impl ServerRuntimeReport {
    pub fn from_worker_shards(requested_worker_shards: Option<usize>) -> Self {
        let runtime = NetworkRuntimeConfig {
            worker_shards: requested_worker_shards,
        };
        Self {
            requested_worker_shards,
            effective_worker_shards: runtime.effective_worker_shards(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunCounts {
    pub successful_handshakes: usize,
    pub failed_handshakes: usize,
    pub unexpected_disconnects: usize,
    pub protocol_errors: usize,
    pub send_errors: usize,
    pub clean_disconnects: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChurnReport {
    pub admission_headroom: usize,
    pub server_max_connections: usize,
    pub planned_disconnects: u64,
    pub completed_planned_disconnects: u64,
    pub replacement_attempts: u64,
    pub replacement_handshakes: u64,
    pub replacement_failures: u64,
    pub replacement_timeouts: u64,
    pub schedule_misses: u64,
    pub population_min: usize,
    pub population_max: usize,
    pub population_end: usize,
    pub replacement_inflight_peak: usize,
    pub replacement_latency: LatencySummary,
    pub post_drain_transport: TransportMetricsReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResultsReport {
    pub correctness: RunCounts,
    pub workload: WorkloadReport,
    pub latency: LatencySummary,
    pub transport: TransportWindowReport,
    pub churn: Option<ChurnReport>,
    pub resources: ResourceWindowsReport,
    pub total_duration_ms: u64,
    pub measured_duration_ms: u64,
    pub passed: bool,
    pub failure_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunReportInput {
    pub correctness: RunCounts,
    pub workload: WorkloadCounts,
    pub latency: LatencySummary,
    pub transport: TransportWindowReport,
    pub churn: Option<ChurnReport>,
    pub resources: ResourceWindowsReport,
    pub total_duration_ms: u64,
    pub measured_duration_ms: u64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct WorkloadReport {
    pub counts: WorkloadCounts,
    pub tx_frames_per_second: f64,
    pub rx_frames_per_second: f64,
    pub tx_payload_bytes_per_second: f64,
    pub rx_payload_bytes_per_second: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ResourceWindowsReport {
    pub ramp_server: Option<ResourceSummary>,
    pub ramp_loadgen: Option<ResourceSummary>,
    pub ramp_host: Option<ResourceSummary>,
    pub steady_server: Option<ResourceSummary>,
    pub steady_loadgen: Option<ResourceSummary>,
    pub steady_host: Option<ResourceSummary>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct TransportWindowReport {
    pub start: TransportMetricsReport,
    pub end: TransportMetricsReport,
    pub delta: TransportCounterReport,
    pub peaks: TransportGaugeReport,
}

impl TransportWindowReport {
    pub fn from_snapshots(
        start: TransportMetricsReport,
        end: TransportMetricsReport,
        samples: impl IntoIterator<Item = TransportMetricsReport>,
    ) -> Self {
        let mut peaks = TransportGaugeReport::default();
        peaks.observe(start);
        for sample in samples {
            peaks.observe(sample);
        }
        peaks.observe(end);
        Self {
            start,
            end,
            delta: TransportCounterReport::between(start, end),
            peaks,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct TransportMetricsReport {
    pub sessions_current: u64,
    pub sessions_started_total: u64,
    pub sessions_closed_total: u64,
    pub timed_out_sessions: u64,
    pub ingress_datagrams: u64,
    pub ingress_frames: u64,
    pub forwarded_packets: u64,
    pub forwarded_bytes: u64,
    pub reliable_sent_datagrams: u64,
    pub retransmitted_datagrams: u64,
    pub acks_out: u64,
    pub nacks_out: u64,
    pub acked_datagrams: u64,
    pub nacked_datagrams: u64,
    pub pending_outgoing_frames: u64,
    pub pending_outgoing_bytes: u64,
    pub outgoing_queue_drops: u64,
    pub outgoing_queue_defers: u64,
    pub outgoing_queue_disconnects: u64,
    pub backpressure_delays: u64,
    pub backpressure_drops: u64,
    pub backpressure_disconnects: u64,
    pub duplicate_reliable_drops: u64,
    pub ordered_stale_drops: u64,
    pub ordered_buffer_full_drops: u64,
    pub sequenced_stale_drops: u64,
    pub split_ttl_drops: u64,
    pub dropped_non_critical_events: u64,
    pub srtt_ms: Option<f64>,
    pub rtt_variance_ms: Option<f64>,
    pub resend_rto_ms: Option<f64>,
    pub congestion_window: Option<f64>,
}

impl From<TransportMetrics> for TransportMetricsReport {
    fn from(metrics: TransportMetrics) -> Self {
        Self {
            sessions_current: metrics.sessions.current,
            sessions_started_total: metrics.sessions.started_total,
            sessions_closed_total: metrics.sessions.closed_total,
            timed_out_sessions: metrics.sessions.timed_out_total,
            ingress_datagrams: metrics.traffic.ingress_datagrams,
            ingress_frames: metrics.traffic.ingress_frames,
            forwarded_packets: metrics.traffic.forwarded_packets,
            forwarded_bytes: metrics.traffic.forwarded_bytes,
            reliable_sent_datagrams: metrics.reliability.reliable_sent_datagrams,
            retransmitted_datagrams: metrics.reliability.retransmitted_datagrams,
            acks_out: metrics.reliability.acks_out,
            nacks_out: metrics.reliability.nacks_out,
            acked_datagrams: metrics.reliability.acked_datagrams,
            nacked_datagrams: metrics.reliability.nacked_datagrams,
            pending_outgoing_frames: metrics.queues.pending_outgoing_frames,
            pending_outgoing_bytes: metrics.queues.pending_outgoing_bytes,
            outgoing_queue_drops: metrics.queues.outgoing_queue_drops,
            outgoing_queue_defers: metrics.queues.outgoing_queue_defers,
            outgoing_queue_disconnects: metrics.queues.outgoing_queue_disconnects,
            backpressure_delays: metrics.queues.backpressure_delays,
            backpressure_drops: metrics.queues.backpressure_drops,
            backpressure_disconnects: metrics.queues.backpressure_disconnects,
            duplicate_reliable_drops: metrics.ordering.duplicate_reliable_drops,
            ordered_stale_drops: metrics.ordering.ordered_stale_drops,
            ordered_buffer_full_drops: metrics.ordering.ordered_buffer_full_drops,
            sequenced_stale_drops: metrics.ordering.sequenced_stale_drops,
            split_ttl_drops: metrics.ordering.split_ttl_drops,
            dropped_non_critical_events: metrics.dropped_non_critical_events,
            srtt_ms: metrics.timing.srtt_ms,
            rtt_variance_ms: metrics.timing.rtt_variance_ms,
            resend_rto_ms: metrics.timing.resend_rto_ms,
            congestion_window: metrics.timing.congestion_window,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransportCounterReport {
    pub sessions_started: u64,
    pub sessions_closed: u64,
    pub timed_out_sessions: u64,
    pub ingress_datagrams: u64,
    pub ingress_frames: u64,
    pub forwarded_packets: u64,
    pub forwarded_bytes: u64,
    pub reliable_sent_datagrams: u64,
    pub retransmitted_datagrams: u64,
    pub acks_out: u64,
    pub nacks_out: u64,
    pub acked_datagrams: u64,
    pub nacked_datagrams: u64,
    pub outgoing_queue_drops: u64,
    pub outgoing_queue_defers: u64,
    pub outgoing_queue_disconnects: u64,
    pub backpressure_delays: u64,
    pub backpressure_drops: u64,
    pub backpressure_disconnects: u64,
    pub duplicate_reliable_drops: u64,
    pub ordered_stale_drops: u64,
    pub ordered_buffer_full_drops: u64,
    pub sequenced_stale_drops: u64,
    pub split_ttl_drops: u64,
    pub dropped_non_critical_events: u64,
}

impl TransportCounterReport {
    fn between(start: TransportMetricsReport, end: TransportMetricsReport) -> Self {
        Self {
            sessions_started: delta(start.sessions_started_total, end.sessions_started_total),
            sessions_closed: delta(start.sessions_closed_total, end.sessions_closed_total),
            timed_out_sessions: delta(start.timed_out_sessions, end.timed_out_sessions),
            ingress_datagrams: delta(start.ingress_datagrams, end.ingress_datagrams),
            ingress_frames: delta(start.ingress_frames, end.ingress_frames),
            forwarded_packets: delta(start.forwarded_packets, end.forwarded_packets),
            forwarded_bytes: delta(start.forwarded_bytes, end.forwarded_bytes),
            reliable_sent_datagrams: delta(
                start.reliable_sent_datagrams,
                end.reliable_sent_datagrams,
            ),
            retransmitted_datagrams: delta(
                start.retransmitted_datagrams,
                end.retransmitted_datagrams,
            ),
            acks_out: delta(start.acks_out, end.acks_out),
            nacks_out: delta(start.nacks_out, end.nacks_out),
            acked_datagrams: delta(start.acked_datagrams, end.acked_datagrams),
            nacked_datagrams: delta(start.nacked_datagrams, end.nacked_datagrams),
            outgoing_queue_drops: delta(start.outgoing_queue_drops, end.outgoing_queue_drops),
            outgoing_queue_defers: delta(start.outgoing_queue_defers, end.outgoing_queue_defers),
            outgoing_queue_disconnects: delta(
                start.outgoing_queue_disconnects,
                end.outgoing_queue_disconnects,
            ),
            backpressure_delays: delta(start.backpressure_delays, end.backpressure_delays),
            backpressure_drops: delta(start.backpressure_drops, end.backpressure_drops),
            backpressure_disconnects: delta(
                start.backpressure_disconnects,
                end.backpressure_disconnects,
            ),
            duplicate_reliable_drops: delta(
                start.duplicate_reliable_drops,
                end.duplicate_reliable_drops,
            ),
            ordered_stale_drops: delta(start.ordered_stale_drops, end.ordered_stale_drops),
            ordered_buffer_full_drops: delta(
                start.ordered_buffer_full_drops,
                end.ordered_buffer_full_drops,
            ),
            sequenced_stale_drops: delta(start.sequenced_stale_drops, end.sequenced_stale_drops),
            split_ttl_drops: delta(start.split_ttl_drops, end.split_ttl_drops),
            dropped_non_critical_events: delta(
                start.dropped_non_critical_events,
                end.dropped_non_critical_events,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct TransportGaugeReport {
    pub sessions_current_peak: u64,
    pub pending_outgoing_frames_peak: u64,
    pub pending_outgoing_bytes_peak: u64,
    pub srtt_ms_peak: Option<f64>,
    pub rtt_variance_ms_peak: Option<f64>,
    pub resend_rto_ms_peak: Option<f64>,
    pub congestion_window_peak: Option<f64>,
}

impl TransportGaugeReport {
    fn observe(&mut self, sample: TransportMetricsReport) {
        self.sessions_current_peak = self.sessions_current_peak.max(sample.sessions_current);
        self.pending_outgoing_frames_peak = self
            .pending_outgoing_frames_peak
            .max(sample.pending_outgoing_frames);
        self.pending_outgoing_bytes_peak = self
            .pending_outgoing_bytes_peak
            .max(sample.pending_outgoing_bytes);
        observe_max(&mut self.srtt_ms_peak, sample.srtt_ms);
        observe_max(&mut self.rtt_variance_ms_peak, sample.rtt_variance_ms);
        observe_max(&mut self.resend_rto_ms_peak, sample.resend_rto_ms);
        observe_max(&mut self.congestion_window_peak, sample.congestion_window);
    }
}

impl RunReport {
    pub fn assemble(
        environment: EnvironmentReport,
        scenario: Scenario,
        input: RunReportInput,
    ) -> Self {
        let workload = WorkloadReport::from_counts(input.workload, input.measured_duration_ms);
        let failure_reasons = gate_failures(
            &scenario,
            input.correctness,
            input.workload,
            input.latency,
            input.transport,
            input.churn.as_ref(),
        );
        let passed = failure_reasons.is_empty();
        Self {
            environment,
            scenario,
            server_runtime: crate::runtime_override::worker_shards_setting()
                .map(ServerRuntimeReport::from_worker_shards),
            results: ResultsReport {
                correctness: input.correctness,
                workload,
                latency: input.latency,
                transport: input.transport,
                churn: input.churn,
                resources: input.resources,
                total_duration_ms: input.total_duration_ms,
                measured_duration_ms: input.measured_duration_ms,
                passed,
                failure_reasons,
            },
        }
    }

    pub fn with_server_runtime(mut self, server_runtime: ServerRuntimeReport) -> Self {
        self.server_runtime = Some(server_runtime);
        self
    }
}

impl WorkloadReport {
    fn from_counts(counts: WorkloadCounts, measured_duration_ms: u64) -> Self {
        let seconds = measured_duration_ms as f64 / 1_000.0;
        let (tx_frames, rx_frames, tx_bytes, rx_bytes) = workload_totals(counts);
        let rate = |value: u64| {
            if seconds > 0.0 {
                value as f64 / seconds
            } else {
                0.0
            }
        };
        Self {
            counts,
            tx_frames_per_second: rate(tx_frames),
            rx_frames_per_second: rate(rx_frames),
            tx_payload_bytes_per_second: rate(tx_bytes),
            rx_payload_bytes_per_second: rate(rx_bytes),
        }
    }
}

fn gate_failures(
    scenario: &Scenario,
    correctness: RunCounts,
    workload: WorkloadCounts,
    latency: LatencySummary,
    transport: TransportWindowReport,
    churn: Option<&ChurnReport>,
) -> Vec<String> {
    let mut failures = Vec::new();
    if correctness.successful_handshakes != scenario.clients {
        failures.push(format!(
            "established {}/{} requested clients",
            correctness.successful_handshakes, scenario.clients
        ));
    }
    if correctness.failed_handshakes != 0 {
        failures.push(format!(
            "{} handshake(s) failed",
            correctness.failed_handshakes
        ));
    }
    if correctness.unexpected_disconnects != 0 {
        failures.push(format!(
            "{} unexpected disconnect(s)",
            correctness.unexpected_disconnects
        ));
    }
    if correctness.protocol_errors != 0 {
        failures.push(format!(
            "{} protocol/decode error(s)",
            correctness.protocol_errors
        ));
    }
    if correctness.send_errors != 0 {
        failures.push(format!(
            "{} benchmark send error(s)",
            correctness.send_errors
        ));
    }
    if correctness.clean_disconnects != scenario.clients {
        failures.push(format!(
            "only {}/{} clients disconnected cleanly",
            correctness.clean_disconnects, scenario.clients
        ));
    }

    let queue_drops = transport
        .delta
        .outgoing_queue_drops
        .saturating_add(transport.delta.outgoing_queue_disconnects)
        .saturating_add(transport.delta.backpressure_drops)
        .saturating_add(transport.delta.backpressure_disconnects);
    if queue_drops != 0 {
        failures.push(format!(
            "{queue_drops} queue/backpressure drop or disconnect event(s)"
        ));
    }

    match (&scenario.churn, churn) {
        (None, None) => {
            if transport.delta.sessions_started != 0 || transport.delta.sessions_closed != 0 {
                failures.push(format!(
                    "session churn during steady window: started={} closed={}",
                    transport.delta.sessions_started, transport.delta.sessions_closed
                ));
            }
            if transport.delta.timed_out_sessions != 0 {
                failures.push(format!(
                    "transport timeout growth during steady window: {}",
                    transport.delta.timed_out_sessions
                ));
            }
        }
        (None, Some(_)) => failures.push("churn report present for non-churn scenario".into()),
        (Some(_), None) => failures.push("churn report missing for churn scenario".into()),
        (Some(config), Some(churn)) => {
            let expected_planned =
                (config.replacements_per_second * scenario.hold_seconds as f64).floor() as u64;

            if churn.planned_disconnects != expected_planned {
                failures.push(format!(
                    "churn schedule planned {} replacement(s), expected {expected_planned}",
                    churn.planned_disconnects
                ));
            }
            if churn.schedule_misses != 0 {
                failures.push(format!(
                    "churn schedule missed {} nominal replacement tick(s)",
                    churn.schedule_misses
                ));
            }
            if churn.completed_planned_disconnects != churn.planned_disconnects
                || churn.replacement_attempts != churn.planned_disconnects
                || churn.replacement_handshakes != churn.replacement_attempts
                || churn.replacement_failures != 0
                || churn.replacement_timeouts != 0
            {
                failures.push(format!(
                    "churn replacement mismatch: planned={} completed={} attempts={} handshakes={} failures={} timeouts={}",
                    churn.planned_disconnects,
                    churn.completed_planned_disconnects,
                    churn.replacement_attempts,
                    churn.replacement_handshakes,
                    churn.replacement_failures,
                    churn.replacement_timeouts,
                ));
            }

            let expected_current = u64::try_from(scenario.clients).unwrap_or(u64::MAX);
            if churn.population_end != scenario.clients
                || churn.post_drain_transport.sessions_current != expected_current
            {
                failures.push(format!(
                    "churn drain did not recover target population: logical={} transport={} target={}",
                    churn.population_end,
                    churn.post_drain_transport.sessions_current,
                    scenario.clients,
                ));
            }

            if transport.delta.timed_out_sessions != 0
                || churn.post_drain_transport.timed_out_sessions
                    != transport.start.timed_out_sessions
            {
                failures.push(format!(
                    "transport timeout growth during churn: measured_delta={} baseline={} post_drain={}",
                    transport.delta.timed_out_sessions,
                    transport.start.timed_out_sessions,
                    churn.post_drain_transport.timed_out_sessions,
                ));
            }
        }
    }

    for spec in &scenario.traffic {
        let (name, counts) = traffic_counts(workload, spec.kind);
        if counts.tx_frames == 0 || counts.rx_frames == 0 {
            failures.push(format!(
                "configured workload {name} moved no traffic in at least one direction"
            ));
        }
    }

    if scenario.rtt.is_some()
        && (workload.rtt_requests.tx_frames == 0
            || workload.rtt_requests.rx_frames == 0
            || workload.rtt_responses.tx_frames == 0
            || workload.rtt_responses.rx_frames == 0
            || latency.samples == 0)
    {
        failures.push("configured RTT probes did not complete".into());
    }

    failures
}

fn traffic_counts(workload: WorkloadCounts, kind: TrafficKind) -> (&'static str, TrafficCounts) {
    match kind {
        TrafficKind::Unreliable => ("unreliable", workload.unreliable),
        TrafficKind::ReliableOrdered => ("reliable_ordered", workload.reliable_ordered),
        TrafficKind::FragmentedReliableOrdered => (
            "fragmented_reliable_ordered",
            workload.fragmented_reliable_ordered,
        ),
    }
}

fn workload_totals(counts: WorkloadCounts) -> (u64, u64, u64, u64) {
    let all = [
        counts.unreliable,
        counts.reliable_ordered,
        counts.fragmented_reliable_ordered,
        counts.rtt_requests,
        counts.rtt_responses,
    ];
    all.into_iter().fold((0, 0, 0, 0), |acc, counts| {
        (
            acc.0.saturating_add(counts.tx_frames),
            acc.1.saturating_add(counts.rx_frames),
            acc.2.saturating_add(counts.tx_payload_bytes),
            acc.3.saturating_add(counts.rx_payload_bytes),
        )
    })
}

fn delta(start: u64, end: u64) -> u64 {
    end.saturating_sub(start)
}

fn observe_max(target: &mut Option<f64>, sample: Option<f64>) {
    let Some(sample) = sample.filter(|value| value.is_finite()) else {
        return;
    };
    *target = Some(target.map_or(sample, |current| current.max(sample)));
}
