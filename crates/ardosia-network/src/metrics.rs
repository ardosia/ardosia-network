use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use raknet_rust::low_level::transport::TransportMetricsSnapshot;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NetworkMetrics {
    pub accepted_total: u64,
    pub connected_current: u64,
    pub disconnected_total: u64,
    pub protocol_errors_total: u64,
    pub backpressure_disconnects_total: u64,
    pub transport: TransportMetrics,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NetworkShardMetrics {
    pub shard_id: usize,
    pub transport: TransportMetrics,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TransportMetrics {
    pub sessions: TransportSessionMetrics,
    pub traffic: TransportTrafficMetrics,
    pub reliability: TransportReliabilityMetrics,
    pub queues: TransportQueueMetrics,
    pub ordering: TransportOrderingMetrics,
    pub timing: TransportTimingMetrics,
    pub dropped_non_critical_events: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransportSessionMetrics {
    pub current: u64,
    pub started_total: u64,
    pub closed_total: u64,
    pub timed_out_total: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransportTrafficMetrics {
    pub ingress_datagrams: u64,
    pub ingress_frames: u64,
    pub forwarded_packets: u64,
    pub forwarded_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransportReliabilityMetrics {
    pub reliable_sent_datagrams: u64,
    pub retransmitted_datagrams: u64,
    pub acks_out: u64,
    pub nacks_out: u64,
    pub acked_datagrams: u64,
    pub nacked_datagrams: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransportQueueMetrics {
    pub pending_outgoing_frames: u64,
    pub pending_outgoing_bytes: u64,
    pub outgoing_queue_drops: u64,
    pub outgoing_queue_defers: u64,
    pub outgoing_queue_disconnects: u64,
    pub backpressure_delays: u64,
    pub backpressure_drops: u64,
    pub backpressure_disconnects: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransportOrderingMetrics {
    pub duplicate_reliable_drops: u64,
    pub ordered_stale_drops: u64,
    pub ordered_buffer_full_drops: u64,
    pub sequenced_stale_drops: u64,
    pub split_ttl_drops: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TransportTimingMetrics {
    pub srtt_ms: Option<f64>,
    pub rtt_variance_ms: Option<f64>,
    pub resend_rto_ms: Option<f64>,
    pub congestion_window: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
struct ShardSnapshot {
    snapshot: TransportMetricsSnapshot,
    dropped_non_critical_events: u64,
}

#[derive(Default)]
pub(crate) struct MetricsState {
    accepted_total: AtomicU64,
    connected_current: AtomicU64,
    disconnected_total: AtomicU64,
    protocol_errors_total: AtomicU64,
    backpressure_disconnects_total: AtomicU64,
    transport_shards: Mutex<BTreeMap<usize, ShardSnapshot>>,
}

impl MetricsState {
    pub(crate) fn snapshot(&self) -> NetworkMetrics {
        let transport = {
            let shards = self
                .transport_shards
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            aggregate_transport(&shards)
        };

        NetworkMetrics {
            accepted_total: self.accepted_total.load(Ordering::Relaxed),
            connected_current: self.connected_current.load(Ordering::Relaxed),
            disconnected_total: self.disconnected_total.load(Ordering::Relaxed),
            protocol_errors_total: self.protocol_errors_total.load(Ordering::Relaxed),
            backpressure_disconnects_total: self
                .backpressure_disconnects_total
                .load(Ordering::Relaxed),
            transport,
        }
    }

    pub(crate) fn shard_metrics(&self) -> Vec<NetworkShardMetrics> {
        let shards = self
            .transport_shards
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        shards
            .iter()
            .map(|(&shard_id, shard)| NetworkShardMetrics {
                shard_id,
                transport: transport_from_shard(*shard),
            })
            .collect()
    }

    pub(crate) fn ingest_transport_snapshot(
        &self,
        shard_id: usize,
        snapshot: TransportMetricsSnapshot,
        dropped_non_critical_events: u64,
    ) {
        let mut shards = self
            .transport_shards
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        shards.insert(
            shard_id,
            ShardSnapshot {
                snapshot,
                dropped_non_critical_events,
            },
        );
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

fn transport_from_shard(shard: ShardSnapshot) -> TransportMetrics {
    let snapshot = shard.snapshot;
    let timing = if snapshot.session_count == 0 {
        TransportTimingMetrics::default()
    } else {
        TransportTimingMetrics {
            srtt_ms: Some(snapshot.avg_srtt_ms),
            rtt_variance_ms: Some(snapshot.avg_rttvar_ms),
            resend_rto_ms: Some(snapshot.avg_resend_rto_ms),
            congestion_window: Some(snapshot.avg_congestion_window_packets),
        }
    };

    TransportMetrics {
        sessions: TransportSessionMetrics {
            current: usize_to_u64(snapshot.session_count),
            started_total: snapshot.sessions_started_total,
            closed_total: snapshot.sessions_closed_total,
            timed_out_total: snapshot.timed_out_sessions,
        },
        traffic: TransportTrafficMetrics {
            ingress_datagrams: snapshot.ingress_datagrams,
            ingress_frames: snapshot.ingress_frames,
            forwarded_packets: snapshot.packets_forwarded_total,
            forwarded_bytes: snapshot.bytes_forwarded_total,
        },
        reliability: TransportReliabilityMetrics {
            reliable_sent_datagrams: snapshot.reliable_sent_datagrams,
            retransmitted_datagrams: snapshot.resent_datagrams,
            acks_out: snapshot.ack_out_total,
            nacks_out: snapshot.nack_out_total,
            acked_datagrams: snapshot.acked_datagrams,
            nacked_datagrams: snapshot.nacked_datagrams,
        },
        queues: TransportQueueMetrics {
            pending_outgoing_frames: usize_to_u64(snapshot.pending_outgoing_frames),
            pending_outgoing_bytes: usize_to_u64(snapshot.pending_outgoing_bytes),
            outgoing_queue_drops: snapshot.outgoing_queue_drops,
            outgoing_queue_defers: snapshot.outgoing_queue_defers,
            outgoing_queue_disconnects: snapshot.outgoing_queue_disconnects,
            backpressure_delays: snapshot.backpressure_delays,
            backpressure_drops: snapshot.backpressure_drops,
            backpressure_disconnects: snapshot.backpressure_disconnects,
        },
        ordering: TransportOrderingMetrics {
            duplicate_reliable_drops: snapshot.duplicate_reliable_drops,
            ordered_stale_drops: snapshot.ordered_stale_drops,
            ordered_buffer_full_drops: snapshot.ordered_buffer_full_drops,
            sequenced_stale_drops: snapshot.sequenced_stale_drops,
            split_ttl_drops: snapshot.split_ttl_drops,
        },
        timing,
        dropped_non_critical_events: shard.dropped_non_critical_events,
    }
}

fn aggregate_transport(shards: &BTreeMap<usize, ShardSnapshot>) -> TransportMetrics {
    let mut aggregate = TransportMetrics::default();
    let mut timing_weight = 0.0;
    let mut srtt_weighted_sum = 0.0;
    let mut rtt_variance_weighted_sum = 0.0;
    let mut resend_rto_weighted_sum = 0.0;
    let mut congestion_window_weighted_sum = 0.0;

    for shard in shards.values() {
        let snapshot = shard.snapshot;
        let session_count = usize_to_u64(snapshot.session_count);

        aggregate.sessions.current = aggregate.sessions.current.saturating_add(session_count);
        aggregate.sessions.started_total = aggregate
            .sessions
            .started_total
            .saturating_add(snapshot.sessions_started_total);
        aggregate.sessions.closed_total = aggregate
            .sessions
            .closed_total
            .saturating_add(snapshot.sessions_closed_total);
        aggregate.sessions.timed_out_total = aggregate
            .sessions
            .timed_out_total
            .saturating_add(snapshot.timed_out_sessions);

        aggregate.traffic.ingress_datagrams = aggregate
            .traffic
            .ingress_datagrams
            .saturating_add(snapshot.ingress_datagrams);
        aggregate.traffic.ingress_frames = aggregate
            .traffic
            .ingress_frames
            .saturating_add(snapshot.ingress_frames);
        aggregate.traffic.forwarded_packets = aggregate
            .traffic
            .forwarded_packets
            .saturating_add(snapshot.packets_forwarded_total);
        aggregate.traffic.forwarded_bytes = aggregate
            .traffic
            .forwarded_bytes
            .saturating_add(snapshot.bytes_forwarded_total);

        aggregate.reliability.reliable_sent_datagrams = aggregate
            .reliability
            .reliable_sent_datagrams
            .saturating_add(snapshot.reliable_sent_datagrams);
        aggregate.reliability.retransmitted_datagrams = aggregate
            .reliability
            .retransmitted_datagrams
            .saturating_add(snapshot.resent_datagrams);
        aggregate.reliability.acks_out = aggregate
            .reliability
            .acks_out
            .saturating_add(snapshot.ack_out_total);
        aggregate.reliability.nacks_out = aggregate
            .reliability
            .nacks_out
            .saturating_add(snapshot.nack_out_total);
        aggregate.reliability.acked_datagrams = aggregate
            .reliability
            .acked_datagrams
            .saturating_add(snapshot.acked_datagrams);
        aggregate.reliability.nacked_datagrams = aggregate
            .reliability
            .nacked_datagrams
            .saturating_add(snapshot.nacked_datagrams);

        aggregate.queues.pending_outgoing_frames = aggregate
            .queues
            .pending_outgoing_frames
            .saturating_add(usize_to_u64(snapshot.pending_outgoing_frames));
        aggregate.queues.pending_outgoing_bytes = aggregate
            .queues
            .pending_outgoing_bytes
            .saturating_add(usize_to_u64(snapshot.pending_outgoing_bytes));
        aggregate.queues.outgoing_queue_drops = aggregate
            .queues
            .outgoing_queue_drops
            .saturating_add(snapshot.outgoing_queue_drops);
        aggregate.queues.outgoing_queue_defers = aggregate
            .queues
            .outgoing_queue_defers
            .saturating_add(snapshot.outgoing_queue_defers);
        aggregate.queues.outgoing_queue_disconnects = aggregate
            .queues
            .outgoing_queue_disconnects
            .saturating_add(snapshot.outgoing_queue_disconnects);
        aggregate.queues.backpressure_delays = aggregate
            .queues
            .backpressure_delays
            .saturating_add(snapshot.backpressure_delays);
        aggregate.queues.backpressure_drops = aggregate
            .queues
            .backpressure_drops
            .saturating_add(snapshot.backpressure_drops);
        aggregate.queues.backpressure_disconnects = aggregate
            .queues
            .backpressure_disconnects
            .saturating_add(snapshot.backpressure_disconnects);

        aggregate.ordering.duplicate_reliable_drops = aggregate
            .ordering
            .duplicate_reliable_drops
            .saturating_add(snapshot.duplicate_reliable_drops);
        aggregate.ordering.ordered_stale_drops = aggregate
            .ordering
            .ordered_stale_drops
            .saturating_add(snapshot.ordered_stale_drops);
        aggregate.ordering.ordered_buffer_full_drops = aggregate
            .ordering
            .ordered_buffer_full_drops
            .saturating_add(snapshot.ordered_buffer_full_drops);
        aggregate.ordering.sequenced_stale_drops = aggregate
            .ordering
            .sequenced_stale_drops
            .saturating_add(snapshot.sequenced_stale_drops);
        aggregate.ordering.split_ttl_drops = aggregate
            .ordering
            .split_ttl_drops
            .saturating_add(snapshot.split_ttl_drops);

        aggregate.dropped_non_critical_events = aggregate
            .dropped_non_critical_events
            .saturating_add(shard.dropped_non_critical_events);

        if snapshot.session_count != 0 {
            let weight = snapshot.session_count as f64;
            timing_weight += weight;
            srtt_weighted_sum += snapshot.avg_srtt_ms * weight;
            rtt_variance_weighted_sum += snapshot.avg_rttvar_ms * weight;
            resend_rto_weighted_sum += snapshot.avg_resend_rto_ms * weight;
            congestion_window_weighted_sum += snapshot.avg_congestion_window_packets * weight;
        }
    }

    if timing_weight > 0.0 {
        aggregate.timing = TransportTimingMetrics {
            srtt_ms: Some(srtt_weighted_sum / timing_weight),
            rtt_variance_ms: Some(rtt_variance_weighted_sum / timing_weight),
            resend_rto_ms: Some(resend_rto_weighted_sum / timing_weight),
            congestion_window: Some(congestion_window_weighted_sum / timing_weight),
        };
    }

    aggregate
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
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
