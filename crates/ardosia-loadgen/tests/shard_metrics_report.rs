use std::str::FromStr;

use ardosia_loadgen::child_protocol::ChildEvent;
use ardosia_loadgen::report::{
    EnvironmentReport, ResourceWindowsReport, RunCounts, RunReport, RunReportInput,
    TransportMetricsReport, TransportShardMetricsReport, TransportShardWindowReport,
    TransportWindowReport,
};
use ardosia_loadgen::scenario::Scenario;
use ardosia_loadgen::workload::WorkloadCounts;

#[test]
fn child_snapshot_roundtrips_per_shard_metrics() {
    let event = ChildEvent::Snapshot {
        metrics: TransportMetricsReport {
            sessions_current: 5,
            ..TransportMetricsReport::default()
        },
        shard_metrics: vec![
            TransportShardMetricsReport {
                shard_id: 0,
                metrics: TransportMetricsReport {
                    sessions_current: 2,
                    ..TransportMetricsReport::default()
                },
            },
            TransportShardMetricsReport {
                shard_id: 1,
                metrics: TransportMetricsReport {
                    sessions_current: 3,
                    ..TransportMetricsReport::default()
                },
            },
        ],
    };

    let json = serde_json::to_string(&event).unwrap();
    let decoded: ChildEvent = serde_json::from_str(&json).unwrap();

    match decoded {
        ChildEvent::Snapshot {
            metrics,
            shard_metrics,
        } => {
            assert_eq!(metrics.sessions_current, 5);
            assert_eq!(shard_metrics.len(), 2);
            assert_eq!(shard_metrics[0].shard_id, 0);
            assert_eq!(shard_metrics[0].metrics.sessions_current, 2);
            assert_eq!(shard_metrics[1].shard_id, 1);
            assert_eq!(shard_metrics[1].metrics.sessions_current, 3);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn shard_window_tracks_delta_and_peaks_for_one_shard() {
    let start = TransportMetricsReport {
        sessions_current: 80,
        retransmitted_datagrams: 10,
        pending_outgoing_bytes: 100,
        ..TransportMetricsReport::default()
    };
    let middle = TransportMetricsReport {
        sessions_current: 82,
        retransmitted_datagrams: 25,
        pending_outgoing_bytes: 4_000,
        ..TransportMetricsReport::default()
    };
    let end = TransportMetricsReport {
        sessions_current: 79,
        retransmitted_datagrams: 40,
        pending_outgoing_bytes: 500,
        ..TransportMetricsReport::default()
    };

    let window = TransportShardWindowReport::from_snapshots(3, start, end, [middle]);

    assert_eq!(window.shard_id, 3);
    assert_eq!(window.delta.retransmitted_datagrams, 30);
    assert_eq!(window.peaks.sessions_current_peak, 82);
    assert_eq!(window.peaks.pending_outgoing_bytes_peak, 4_000);
}

#[test]
fn run_report_retains_per_shard_transport_windows() {
    let scenario = Scenario::from_str(
        r#"
name = "shard-report"
clients = 1
protocol_version = 8
ramp_up_seconds = 0
hold_seconds = 1
connect_timeout_seconds = 2
seed = 7
"#,
    )
    .unwrap();
    let shard_window = TransportShardWindowReport::from_snapshots(
        0,
        TransportMetricsReport {
            sessions_current: 1,
            ..TransportMetricsReport::default()
        },
        TransportMetricsReport {
            sessions_current: 1,
            retransmitted_datagrams: 2,
            ..TransportMetricsReport::default()
        },
        [],
    );

    let report = RunReport::assemble(
        EnvironmentReport::default(),
        scenario,
        RunReportInput {
            correctness: RunCounts {
                successful_handshakes: 1,
                clean_disconnects: 1,
                ..RunCounts::default()
            },
            workload: WorkloadCounts::default(),
            latency: Default::default(),
            transport: TransportWindowReport::default(),
            churn: None,
            resources: ResourceWindowsReport::default(),
            total_duration_ms: 1_000,
            measured_duration_ms: 1_000,
        },
    )
    .with_transport_shards(vec![shard_window]);

    assert_eq!(report.results.transport_shards.len(), 1);
    assert_eq!(report.results.transport_shards[0].shard_id, 0);
    assert_eq!(
        report.results.transport_shards[0]
            .delta
            .retransmitted_datagrams,
        2
    );

    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["results"]["transport_shards"].as_array().unwrap().len(), 1);
}
