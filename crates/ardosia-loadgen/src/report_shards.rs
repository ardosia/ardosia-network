#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct TransportShardMetricsReport {
    pub shard_id: usize,
    pub metrics: TransportMetricsReport,
}

impl From<ardosia_network::NetworkShardMetrics> for TransportShardMetricsReport {
    fn from(metrics: ardosia_network::NetworkShardMetrics) -> Self {
        Self {
            shard_id: metrics.shard_id,
            metrics: metrics.transport.into(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TransportShardWindowReport {
    pub shard_id: usize,
    pub start: TransportMetricsReport,
    pub end: TransportMetricsReport,
    pub delta: TransportCounterReport,
    pub peaks: TransportGaugeReport,
}

impl TransportShardWindowReport {
    pub fn from_snapshots(
        shard_id: usize,
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
            shard_id,
            start,
            end,
            delta: TransportCounterReport::between(start, end),
            peaks,
        }
    }
}
