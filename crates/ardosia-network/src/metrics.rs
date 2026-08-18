use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NetworkMetrics {
    pub accepted_total: u64,
    pub connected_current: u64,
    pub disconnected_total: u64,
    pub protocol_errors_total: u64,
    pub backpressure_disconnects_total: u64,
}

#[derive(Default)]
pub(crate) struct MetricsState {
    accepted_total: AtomicU64,
    connected_current: AtomicU64,
    disconnected_total: AtomicU64,
    protocol_errors_total: AtomicU64,
    backpressure_disconnects_total: AtomicU64,
}

impl MetricsState {
    pub(crate) fn snapshot(&self) -> NetworkMetrics {
        NetworkMetrics {
            accepted_total: self.accepted_total.load(Ordering::Relaxed),
            connected_current: self.connected_current.load(Ordering::Relaxed),
            disconnected_total: self.disconnected_total.load(Ordering::Relaxed),
            protocol_errors_total: self.protocol_errors_total.load(Ordering::Relaxed),
            backpressure_disconnects_total: self
                .backpressure_disconnects_total
                .load(Ordering::Relaxed),
        }
    }

    pub(crate) fn connected(&self) {
        self.accepted_total.fetch_add(1, Ordering::Relaxed);
        self.connected_current.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn disconnected(&self) {
        self.disconnected_total.fetch_add(1, Ordering::Relaxed);
        let _ =
            self.connected_current
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    Some(value.saturating_sub(1))
                });
    }

    pub(crate) fn protocol_error(&self) {
        self.protocol_errors_total.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn backpressure_disconnect(&self) {
        self.backpressure_disconnects_total
            .fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use raknet_rust::low_level::transport::TransportMetricsSnapshot;

    use super::MetricsState;

    #[test]
    fn transport_snapshots_aggregate_counters_gauges_and_weighted_timing() {
        let metrics = MetricsState::default();

        metrics.ingest_transport_snapshot(
            0,
            TransportMetricsSnapshot {
                session_count: 1,
                sessions_started_total: 2,
                sessions_closed_total: 1,
                timed_out_sessions: 1,
                packets_forwarded_total: 10,
                bytes_forwarded_total: 100,
                ingress_datagrams: 12,
                ingress_frames: 14,
                reliable_sent_datagrams: 20,
                resent_datagrams: 2,
                ack_out_total: 30,
                nack_out_total: 3,
                acked_datagrams: 28,
                nacked_datagrams: 4,
                pending_outgoing_frames: 3,
                pending_outgoing_bytes: 300,
                outgoing_queue_drops: 1,
                outgoing_queue_defers: 2,
                outgoing_queue_disconnects: 3,
                backpressure_delays: 4,
                backpressure_drops: 5,
                backpressure_disconnects: 6,
                duplicate_reliable_drops: 7,
                ordered_stale_drops: 8,
                ordered_buffer_full_drops: 9,
                sequenced_stale_drops: 10,
                split_ttl_drops: 11,
                avg_srtt_ms: 10.0,
                avg_rttvar_ms: 2.0,
                avg_resend_rto_ms: 30.0,
                avg_congestion_window_packets: 5.0,
                ..TransportMetricsSnapshot::default()
            },
            2,
        );

        metrics.ingest_transport_snapshot(
            1,
            TransportMetricsSnapshot {
                session_count: 2,
                sessions_started_total: 4,
                sessions_closed_total: 2,
                timed_out_sessions: 0,
                packets_forwarded_total: 20,
                bytes_forwarded_total: 200,
                ingress_datagrams: 22,
                ingress_frames: 24,
                reliable_sent_datagrams: 40,
                resent_datagrams: 5,
                ack_out_total: 60,
                nack_out_total: 6,
                acked_datagrams: 58,
                nacked_datagrams: 7,
                pending_outgoing_frames: 6,
                pending_outgoing_bytes: 600,
                outgoing_queue_drops: 10,
                outgoing_queue_defers: 20,
                outgoing_queue_disconnects: 30,
                backpressure_delays: 40,
                backpressure_drops: 50,
                backpressure_disconnects: 60,
                duplicate_reliable_drops: 70,
                ordered_stale_drops: 80,
                ordered_buffer_full_drops: 90,
                sequenced_stale_drops: 100,
                split_ttl_drops: 110,
                avg_srtt_ms: 25.0,
                avg_rttvar_ms: 8.0,
                avg_resend_rto_ms: 45.0,
                avg_congestion_window_packets: 11.0,
                ..TransportMetricsSnapshot::default()
            },
            3,
        );

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.transport.sessions.current, 3);
        assert_eq!(snapshot.transport.sessions.started_total, 6);
        assert_eq!(snapshot.transport.sessions.closed_total, 3);
        assert_eq!(snapshot.transport.sessions.timed_out_total, 1);
        assert_eq!(snapshot.transport.traffic.ingress_datagrams, 34);
        assert_eq!(snapshot.transport.traffic.ingress_frames, 38);
        assert_eq!(snapshot.transport.traffic.forwarded_packets, 30);
        assert_eq!(snapshot.transport.traffic.forwarded_bytes, 300);
        assert_eq!(snapshot.transport.reliability.retransmitted_datagrams, 7);
        assert_eq!(snapshot.transport.queues.pending_outgoing_frames, 9);
        assert_eq!(snapshot.transport.queues.pending_outgoing_bytes, 900);
        assert_eq!(snapshot.transport.ordering.split_ttl_drops, 121);
        assert_eq!(snapshot.transport.dropped_non_critical_events, 5);
        assert_eq!(snapshot.transport.timing.srtt_ms, Some(20.0));
        assert_eq!(snapshot.transport.timing.rtt_variance_ms, Some(6.0));
        assert_eq!(snapshot.transport.timing.resend_rto_ms, Some(40.0));
        assert_eq!(snapshot.transport.timing.congestion_window, Some(9.0));
    }

    #[test]
    fn zero_session_transport_timing_is_unavailable() {
        let metrics = MetricsState::default();
        metrics.ingest_transport_snapshot(
            0,
            TransportMetricsSnapshot {
                session_count: 0,
                avg_srtt_ms: 99.0,
                avg_rttvar_ms: 99.0,
                avg_resend_rto_ms: 99.0,
                avg_congestion_window_packets: 99.0,
                ..TransportMetricsSnapshot::default()
            },
            0,
        );

        let timing = metrics.snapshot().transport.timing;
        assert_eq!(timing.srtt_ms, None);
        assert_eq!(timing.rtt_variance_ms, None);
        assert_eq!(timing.resend_rto_ms, None);
        assert_eq!(timing.congestion_window, None);
    }
}
