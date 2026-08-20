use std::str::FromStr;

use ardosia_loadgen::latency::LatencySummary;
use ardosia_loadgen::report::{
    ChurnReport, EnvironmentReport, ResourceWindowsReport, RunCounts, RunReport, RunReportInput,
    TransportCounterReport, TransportMetricsReport, TransportWindowReport,
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

fn churn_scenario() -> Scenario {
    Scenario::from_str(include_str!("../../../scenarios/churn-500.toml")).unwrap()
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

fn clean_churn_counts() -> RunCounts {
    RunCounts {
        successful_handshakes: 500,
        failed_handshakes: 0,
        unexpected_disconnects: 0,
        protocol_errors: 0,
        send_errors: 0,
        clean_disconnects: 500,
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

fn latency() -> LatencySummary {
    LatencySummary {
        samples: 1,
        p50_ms: Some(1.0),
        p95_ms: Some(1.0),
        p99_ms: Some(1.0),
        max_ms: Some(1.0),
        ..LatencySummary::default()
    }
}

fn report(scenario: Scenario, counts: RunCounts, workload: WorkloadCounts) -> RunReport {
    RunReport::assemble(
        EnvironmentReport::default(),
        scenario,
        RunReportInput {
            correctness: counts,
            workload,
            latency: latency(),
            transport: TransportWindowReport::default(),
            churn: None,
            resources: ResourceWindowsReport::default(),
            total_duration_ms: 70_000,
            measured_duration_ms: 60_000,
        },
    )
}

fn passing_churn_input() -> RunReportInput {
    let mut transport = TransportWindowReport::default();
    transport.start = TransportMetricsReport {
        sessions_current: 500,
        timed_out_sessions: 0,
        ..TransportMetricsReport::default()
    };
    transport.end = TransportMetricsReport {
        sessions_current: 500,
        sessions_started_total: 1_999,
        sessions_closed_total: 1_499,
        timed_out_sessions: 0,
        ..TransportMetricsReport::default()
    };
    transport.delta = TransportCounterReport {
        sessions_started: 1_499,
        sessions_closed: 1_499,
        timed_out_sessions: 0,
        ..TransportCounterReport::default()
    };

    RunReportInput {
        correctness: clean_churn_counts(),
        workload: complete_workload(),
        latency: latency(),
        transport,
        churn: Some(ChurnReport {
            admission_headroom: 125,
            server_max_connections: 625,
            planned_disconnects: 1_500,
            completed_planned_disconnects: 1_500,
            replacement_attempts: 1_500,
            replacement_handshakes: 1_500,
            replacement_failures: 0,
            replacement_timeouts: 0,
            schedule_misses: 0,
            population_min: 499,
            population_max: 500,
            population_end: 500,
            replacement_inflight_peak: 1,
            replacement_latency: latency(),
            post_drain_transport: TransportMetricsReport {
                sessions_current: 500,
                sessions_started_total: 2_000,
                sessions_closed_total: 1_500,
                timed_out_sessions: 0,
                ..TransportMetricsReport::default()
            },
        }),
        resources: ResourceWindowsReport::default(),
        total_duration_ms: 70_000,
        measured_duration_ms: 60_000,
    }
}

#[test]
fn clean_connect_style_report_still_passes_without_workload() {
    let report = report(
        connect_scenario(),
        clean_counts(),
        WorkloadCounts::default(),
    );
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
        latency: latency(),
        transport: TransportWindowReport::default(),
        churn: None,
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
fn steady_report_fails_on_session_churn() {
    let mut input = RunReportInput {
        correctness: clean_counts(),
        workload: complete_workload(),
        latency: latency(),
        transport: TransportWindowReport::default(),
        churn: None,
        resources: ResourceWindowsReport::default(),
        total_duration_ms: 70_000,
        measured_duration_ms: 60_000,
    };
    input.transport.delta.sessions_started = 1;

    let report = RunReport::assemble(EnvironmentReport::default(), steady_scenario(), input);
    assert!(!report.results.passed);
    assert!(
        report
            .results
            .failure_reasons
            .iter()
            .any(|reason| reason.contains("session churn"))
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
        churn: None,
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
    assert!(
        report.results.passed,
        "{:?}",
        report.results.failure_reasons
    );
}

#[test]
fn clean_churn_report_passes_with_lifecycle_totals() {
    let report = RunReport::assemble(
        EnvironmentReport::default(),
        churn_scenario(),
        passing_churn_input(),
    );
    assert!(report.results.passed, "{:?}", report.results.failure_reasons);
    assert!(report.results.churn.is_some());
}

#[test]
fn churn_report_rejects_replacement_failure() {
    let mut input = passing_churn_input();
    input.churn.as_mut().unwrap().replacement_failures = 1;
    input.churn.as_mut().unwrap().replacement_handshakes = 1_499;

    let report = RunReport::assemble(EnvironmentReport::default(), churn_scenario(), input);
    assert!(!report.results.passed);
    assert!(report.results.failure_reasons.iter().any(|reason| reason.contains("churn replacement")));
}

#[test]
fn churn_report_rejects_schedule_miss() {
    let mut input = passing_churn_input();
    input.churn.as_mut().unwrap().schedule_misses = 1;

    let report = RunReport::assemble(EnvironmentReport::default(), churn_scenario(), input);
    assert!(!report.results.passed);
    assert!(report.results.failure_reasons.iter().any(|reason| reason.contains("churn schedule")));
}

#[test]
fn churn_report_rejects_bad_post_drain_population_or_timeout_growth() {
    let mut population = passing_churn_input();
    population.churn.as_mut().unwrap().population_end = 499;
    population.churn.as_mut().unwrap().post_drain_transport.sessions_current = 499;
    let report = RunReport::assemble(EnvironmentReport::default(), churn_scenario(), population);
    assert!(!report.results.passed);
    assert!(report.results.failure_reasons.iter().any(|reason| reason.contains("churn drain")));

    let mut timeout = passing_churn_input();
    timeout.churn.as_mut().unwrap().post_drain_transport.timed_out_sessions = 1;
    let report = RunReport::assemble(EnvironmentReport::default(), churn_scenario(), timeout);
    assert!(!report.results.passed);
    assert!(report.results.failure_reasons.iter().any(|reason| reason.contains("transport timeout")));
}
