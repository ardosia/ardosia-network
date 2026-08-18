use std::str::FromStr;

use ardosia_loadgen::latency::LatencySummary;
use ardosia_loadgen::report::{
    EnvironmentReport, ResourceWindowsReport, RunCounts, RunReport, RunReportInput,
    TransportCounterReport, TransportWindowReport,
};
use ardosia_loadgen::resource::ResourceSummary;
use ardosia_loadgen::scenario::Scenario;
use ardosia_loadgen::workload::WorkloadCounts;

fn connect_scenario() -> Scenario {
    Scenario::from_str(
        r#"
name = "connect-300"
clients = 300
protocol_version = 8
ramp_up_seconds = 10
hold_seconds = 60
connect_timeout_seconds = 5
"#,
    )
    .unwrap()
}

fn steady_scenario() -> Scenario {
    Scenario::from_str(
        r#"
name = "steady-300"
clients = 300
protocol_version = 8
ramp_up_seconds = 10
hold_seconds = 60
connect_timeout_seconds = 5
seed = 1

[[traffic]]
kind = "unreliable"
direction = "bidirectional"
packets_per_second_per_client = 20.0
payload_bytes = 64

[[traffic]]
kind = "reliable_ordered"
direction = "bidirectional"
packets_per_second_per_client = 2.0
payload_bytes = 256

[[traffic]]
kind = "fragmented_reliable_ordered"
direction = "bidirectional"
packets_per_second_per_client = 0.2
payload_bytes = 4096

[rtt]
probes_per_second_per_client = 2.0
payload_bytes = 32
"#,
    )
    .unwrap()
}

fn clean_counts() -> RunCounts {
    RunCounts {
        successful_handshakes: 300,
        failed_handshakes: 0,
        unexpected_disconnects: 0,
        protocol_errors: 0,
        send_errors: 0,
        clean_disconnects: 300,
    }
}

fn complete_workload() -> WorkloadCounts {
    let mut workload = WorkloadCounts::default();
    workload.unreliable.tx_frames = 1;
    workload.unreliable.rx_frames = 1;
    workload.reliable_ordered.tx_frames = 1;
    workload.reliable_ordered.rx_frames = 1;
    workload.fragmented_reliable_ordered.tx_frames = 1;
    workload.fragmented_reliable_ordered.rx_frames = 1;
    workload.rtt_requests.tx_frames = 1;
    workload.rtt_requests.rx_frames = 1;
    workload.rtt_responses.tx_frames = 1;
    workload.rtt_responses.rx_frames = 1;
    workload
}

fn report(scenario: Scenario, counts: RunCounts, workload: WorkloadCounts) -> RunReport {
    RunReport::assemble(
        EnvironmentReport::default(),
        scenario,
        RunReportInput {
            correctness: counts,
            workload,
            latency: LatencySummary {
                samples: 1,
                p50_ms: Some(1.0),
                p95_ms: Some(1.0),
                p99_ms: Some(1.0),
                max_ms: Some(1.0),
                ..LatencySummary::default()
            },
            transport: TransportWindowReport::default(),
            resources: ResourceWindowsReport::default(),
            total_duration_ms: 70_000,
            measured_duration_ms: 60_000,
        },
    )
}

#[test]
fn clean_connect_style_report_still_passes_without_workload() {
    let report = report(connect_scenario(), clean_counts(), WorkloadCounts::default());
    assert!(report.results.passed);
    assert!(report.results.failure_reasons.is_empty());
}

#[test]
fn steady_report_fails_on_unexpected_disconnect() {
    let mut counts = clean_counts();
    counts.unexpected_disconnects = 1;
    counts.clean_disconnects = 299;

    let report = report(steady_scenario(), counts, complete_workload());
    assert!(!report.results.passed);
    assert!(
        report
            .results
            .failure_reasons
            .iter()
            .any(|reason| reason.contains("unexpected disconnect"))
    );
}

#[test]
fn steady_report_fails_on_queue_or_backpressure_drop() {
    let mut input = RunReportInput {
        correctness: clean_counts(),
        workload: complete_workload(),
        latency: LatencySummary {
            samples: 1,
            p50_ms: Some(1.0),
            p95_ms: Some(1.0),
            p99_ms: Some(1.0),
            max_ms: Some(1.0),
            ..LatencySummary::default()
        },
        transport: TransportWindowReport::default(),
        resources: ResourceWindowsReport::default(),
        total_duration_ms: 70_000,
        measured_duration_ms: 60_000,
    };
    input.transport.delta = TransportCounterReport {
        outgoing_queue_drops: 1,
        backpressure_drops: 2,
        ..TransportCounterReport::default()
    };

    let report = RunReport::assemble(EnvironmentReport::default(), steady_scenario(), input);
    assert!(!report.results.passed);
    assert!(
        report
            .results
            .failure_reasons
            .iter()
            .any(|reason| reason.contains("queue/backpressure"))
    );
}

#[test]
fn steady_report_fails_when_configured_workload_class_moves_no_traffic() {
    let mut workload = complete_workload();
    workload.fragmented_reliable_ordered.rx_frames = 0;

    let report = report(steady_scenario(), clean_counts(), workload);
    assert!(!report.results.passed);
    assert!(
        report
            .results
            .failure_reasons
            .iter()
            .any(|reason| reason.contains("fragmented_reliable_ordered"))
    );
}

#[test]
fn high_resource_latency_and_retransmit_values_are_record_only() {
    let mut input = RunReportInput {
        correctness: clean_counts(),
        workload: complete_workload(),
        latency: LatencySummary {
            samples: 1,
            p50_ms: Some(1_000.0),
            p95_ms: Some(5_000.0),
            p99_ms: Some(9_000.0),
            max_ms: Some(10_000.0),
            ..LatencySummary::default()
        },
        transport: TransportWindowReport::default(),
        resources: ResourceWindowsReport::default(),
        total_duration_ms: 70_000,
        measured_duration_ms: 60_000,
    };
    input.transport.delta.retransmitted_datagrams = 999_999;
    input.resources.steady_server = Some(ResourceSummary {
        sample_count: 60,
        process_cpu_avg_pct: Some(700.0),
        process_cpu_peak_pct: Some(800.0),
        process_rss_avg_bytes: Some(32 * 1024 * 1024 * 1024),
        process_rss_peak_bytes: Some(64 * 1024 * 1024 * 1024),
        ..ResourceSummary::default()
    });

    let report = RunReport::assemble(EnvironmentReport::default(), steady_scenario(), input);
    assert!(report.results.passed, "{:?}", report.results.failure_reasons);
}
