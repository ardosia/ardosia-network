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
        let _ = self.connected_current.fetch_update(
            Ordering::Relaxed,
            Ordering::Relaxed,
            |value| Some(value.saturating_sub(1)),
        );
    }

    pub(crate) fn protocol_error(&self) {
        self.protocol_errors_total.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn backpressure_disconnect(&self) {
        self.backpressure_disconnects_total
            .fetch_add(1, Ordering::Relaxed);
    }
}
