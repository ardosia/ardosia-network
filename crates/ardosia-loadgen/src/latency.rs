use std::time::Duration;

const MAX_BUCKET_MS: usize = 10_000;
const OVERFLOW_BUCKET: usize = MAX_BUCKET_MS + 1;
const BUCKET_COUNT: usize = OVERFLOW_BUCKET + 1;

#[derive(Debug, Clone)]
pub struct LatencyHistogram {
    buckets: Vec<u64>,
    samples: u64,
    max: Option<Duration>,
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self {
            buckets: vec![0; BUCKET_COUNT],
            samples: 0,
            max: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LatencySummary {
    pub samples: u64,
    pub overflow_samples: u64,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub max_ms: Option<f64>,
}

impl LatencyHistogram {
    pub fn record(&mut self, value: Duration) {
        let millis = value.as_millis();
        let bucket = if millis > MAX_BUCKET_MS as u128 {
            OVERFLOW_BUCKET
        } else {
            millis as usize
        };
        self.buckets[bucket] = self.buckets[bucket].saturating_add(1);
        self.samples = self.samples.saturating_add(1);
        self.max = Some(self.max.map_or(value, |current| current.max(value)));
    }

    pub fn merge(&mut self, other: &Self) {
        for (left, right) in self.buckets.iter_mut().zip(&other.buckets) {
            *left = left.saturating_add(*right);
        }
        self.samples = self.samples.saturating_add(other.samples);
        self.max = match (self.max, other.max) {
            (Some(left), Some(right)) => Some(left.max(right)),
            (Some(value), None) | (None, Some(value)) => Some(value),
            (None, None) => None,
        };
    }

    pub fn summary(&self) -> LatencySummary {
        LatencySummary {
            samples: self.samples,
            overflow_samples: self.buckets[OVERFLOW_BUCKET],
            p50_ms: self.percentile(0.50),
            p95_ms: self.percentile(0.95),
            p99_ms: self.percentile(0.99),
            max_ms: self.max.map(duration_ms),
        }
    }

    fn percentile(&self, quantile: f64) -> Option<f64> {
        if self.samples == 0 {
            return None;
        }

        let rank = ((self.samples as f64) * quantile).ceil().max(1.0) as u64;
        let mut cumulative = 0u64;
        for (bucket, count) in self.buckets.iter().enumerate() {
            cumulative = cumulative.saturating_add(*count);
            if cumulative >= rank {
                return Some(bucket as f64);
            }
        }

        Some(OVERFLOW_BUCKET as f64)
    }
}

fn duration_ms(value: Duration) -> f64 {
    value.as_secs_f64() * 1_000.0
}
