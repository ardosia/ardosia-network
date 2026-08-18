use serde::{Deserialize, Serialize};

use crate::latency::LatencySummary;
use crate::workload::WorkloadCounts;

pub const VENDOR_REVISION: &str = "3edfb4170e6cb5aeed992b09b50176fb7e5b6079";

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct RunCounts {
    pub successful_handshakes: usize,
    pub failed_handshakes: usize,
    pub unexpected_disconnects: usize,
    pub protocol_errors: usize,
    pub clean_disconnects: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub scenario: String,
    pub requested_clients: usize,
    pub counts: RunCounts,
    pub send_errors: usize,
    pub workload: WorkloadCounts,
    pub latency: LatencySummary,
    pub duration_ms: u64,
    pub vendor_revision: String,
    pub passed: bool,
    pub failure_reason: Option<String>,
}

impl RunReport {
    pub fn from_counts(
        scenario: String,
        requested_clients: usize,
        counts: RunCounts,
        duration_ms: u64,
    ) -> Self {
        let mut report = Self {
            scenario,
            requested_clients,
            counts,
            send_errors: 0,
            workload: WorkloadCounts::default(),
            latency: LatencySummary::default(),
            duration_ms,
            vendor_revision: VENDOR_REVISION.into(),
            passed: false,
            failure_reason: None,
        };
        report.recompute_gate();
        report
    }

    pub fn add_protocol_errors(&mut self, additional: usize) {
        self.counts.protocol_errors = self.counts.protocol_errors.saturating_add(additional);
        self.recompute_gate();
    }

    pub fn add_send_errors(&mut self, additional: usize) {
        self.send_errors = self.send_errors.saturating_add(additional);
        self.recompute_gate();
    }

    pub fn add_workload(&mut self, workload: WorkloadCounts) {
        self.workload.merge(workload);
    }

    pub fn set_latency(&mut self, latency: LatencySummary) {
        self.latency = latency;
    }

    fn recompute_gate(&mut self) {
        self.failure_reason = if self.counts.successful_handshakes != self.requested_clients {
            Some(format!(
                "established {}/{} requested clients",
                self.counts.successful_handshakes, self.requested_clients
            ))
        } else if self.counts.failed_handshakes != 0 {
            Some(format!(
                "{} handshake(s) failed",
                self.counts.failed_handshakes
            ))
        } else if self.counts.unexpected_disconnects != 0 {
            Some(format!(
                "{} unexpected disconnect(s)",
                self.counts.unexpected_disconnects
            ))
        } else if self.counts.protocol_errors != 0 {
            Some(format!(
                "{} protocol/decode error(s)",
                self.counts.protocol_errors
            ))
        } else if self.send_errors != 0 {
            Some(format!("{} benchmark send error(s)", self.send_errors))
        } else if self.counts.clean_disconnects != self.requested_clients {
            Some(format!(
                "only {}/{} clients completed the hold window cleanly",
                self.counts.clean_disconnects, self.requested_clients
            ))
        } else {
            None
        };

        self.passed = self.failure_reason.is_none();
    }
}
