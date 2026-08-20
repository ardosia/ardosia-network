use ardosia_loadgen::child_protocol::ChildEvent;
use ardosia_loadgen::report::{
    TransportMetricsReport, TransportShardMetricsReport, TransportShardWindowReport,
};

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
