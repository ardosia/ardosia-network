use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
pub mod linux;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct ResourcePoint {
    pub process_cpu_pct: Option<f64>,
    pub process_rss_bytes: Option<u64>,
    pub host_cpu_pct: Option<f64>,
    pub host_memory_used_bytes: Option<u64>,
    pub host_memory_available_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ResourceSummary {
    pub sample_count: u64,
    pub process_cpu_avg_pct: Option<f64>,
    pub process_cpu_peak_pct: Option<f64>,
    pub process_rss_avg_bytes: Option<u64>,
    pub process_rss_peak_bytes: Option<u64>,
    pub host_cpu_avg_pct: Option<f64>,
    pub host_cpu_peak_pct: Option<f64>,
    pub host_memory_used_avg_bytes: Option<u64>,
    pub host_memory_used_peak_bytes: Option<u64>,
    pub host_memory_available_min_bytes: Option<u64>,
}

pub struct ResourceSampler {
    pid: u32,
    logical_cpus: u64,
    #[cfg(target_os = "linux")]
    previous_host: Option<linux::CpuTicks>,
    #[cfg(target_os = "linux")]
    previous_process_ticks: Option<u64>,
}

impl ResourceSampler {
    pub fn for_pid(pid: u32) -> Self {
        Self {
            pid,
            logical_cpus: std::thread::available_parallelism()
                .map(|value| u64::try_from(value.get()).unwrap_or(u64::MAX))
                .unwrap_or(1),
            #[cfg(target_os = "linux")]
            previous_host: None,
            #[cfg(target_os = "linux")]
            previous_process_ticks: None,
        }
    }

    pub fn sample(&mut self) -> ResourcePoint {
        #[cfg(target_os = "linux")]
        {
            self.sample_linux()
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = self.pid;
            let _ = self.logical_cpus;
            ResourcePoint::default()
        }
    }

    #[cfg(target_os = "linux")]
    fn sample_linux(&mut self) -> ResourcePoint {
        let host = linux::read_host_cpu_ticks();
        let process_ticks = linux::read_process_cpu_ticks(self.pid);
        let process_rss_bytes = linux::read_process_rss_bytes(self.pid);
        let memory = linux::read_meminfo();

        let host_cpu_pct = match (self.previous_host, host) {
            (Some(previous), Some(current)) => host_cpu_pct(previous, current),
            _ => None,
        };

        let process_cpu_pct = match (
            self.previous_host,
            host,
            self.previous_process_ticks,
            process_ticks,
        ) {
            (
                Some(previous_host),
                Some(current_host),
                Some(previous_process),
                Some(current_process),
            ) => process_cpu_pct(
                previous_host,
                current_host,
                previous_process,
                current_process,
                self.logical_cpus,
            ),
            _ => None,
        };

        self.previous_host = host;
        self.previous_process_ticks = process_ticks;

        ResourcePoint {
            process_cpu_pct,
            process_rss_bytes,
            host_cpu_pct,
            host_memory_used_bytes: memory.map(|value| value.used_bytes),
            host_memory_available_bytes: memory.map(|value| value.available_bytes),
        }
    }
}

#[derive(Debug, Default)]
pub struct ResourceAccumulator {
    sample_count: u64,
    process_cpu: FloatStats,
    process_rss: IntegerStats,
    host_cpu: FloatStats,
    host_memory_used: IntegerStats,
    host_memory_available: IntegerStats,
}

impl ResourceAccumulator {
    pub fn push(&mut self, point: ResourcePoint) {
        self.sample_count = self.sample_count.saturating_add(1);
        self.process_cpu.push(point.process_cpu_pct);
        self.process_rss.push(point.process_rss_bytes);
        self.host_cpu.push(point.host_cpu_pct);
        self.host_memory_used.push(point.host_memory_used_bytes);
        self.host_memory_available
            .push(point.host_memory_available_bytes);
    }

    pub fn finish(self) -> ResourceSummary {
        ResourceSummary {
            sample_count: self.sample_count,
            process_cpu_avg_pct: self.process_cpu.average(),
            process_cpu_peak_pct: self.process_cpu.max,
            process_rss_avg_bytes: self.process_rss.average(),
            process_rss_peak_bytes: self.process_rss.max,
            host_cpu_avg_pct: self.host_cpu.average(),
            host_cpu_peak_pct: self.host_cpu.max,
            host_memory_used_avg_bytes: self.host_memory_used.average(),
            host_memory_used_peak_bytes: self.host_memory_used.max,
            host_memory_available_min_bytes: self.host_memory_available.min,
        }
    }
}

#[derive(Debug, Default)]
struct FloatStats {
    sum: f64,
    count: u64,
    max: Option<f64>,
}

impl FloatStats {
    fn push(&mut self, value: Option<f64>) {
        let Some(value) = value.filter(|value| value.is_finite()) else {
            return;
        };
        self.sum += value;
        self.count = self.count.saturating_add(1);
        self.max = Some(self.max.map_or(value, |current| current.max(value)));
    }

    fn average(&self) -> Option<f64> {
        (self.count != 0).then(|| self.sum / self.count as f64)
    }
}

#[derive(Debug, Default)]
struct IntegerStats {
    sum: u128,
    count: u64,
    max: Option<u64>,
    min: Option<u64>,
}

impl IntegerStats {
    fn push(&mut self, value: Option<u64>) {
        let Some(value) = value else {
            return;
        };
        self.sum = self.sum.saturating_add(u128::from(value));
        self.count = self.count.saturating_add(1);
        self.max = Some(self.max.map_or(value, |current| current.max(value)));
        self.min = Some(self.min.map_or(value, |current| current.min(value)));
    }

    fn average(&self) -> Option<u64> {
        if self.count == 0 {
            return None;
        }
        let average = self.sum / u128::from(self.count);
        Some(u64::try_from(average).unwrap_or(u64::MAX))
    }
}

#[cfg(target_os = "linux")]
fn host_cpu_pct(previous: linux::CpuTicks, current: linux::CpuTicks) -> Option<f64> {
    let total_delta = current.total.checked_sub(previous.total)?;
    let idle_delta = current.idle.checked_sub(previous.idle)?;
    if total_delta == 0 || idle_delta > total_delta {
        return None;
    }
    Some((1.0 - idle_delta as f64 / total_delta as f64) * 100.0)
}

#[cfg(target_os = "linux")]
fn process_cpu_pct(
    previous_host: linux::CpuTicks,
    current_host: linux::CpuTicks,
    previous_process: u64,
    current_process: u64,
    logical_cpus: u64,
) -> Option<f64> {
    let host_delta = current_host.total.checked_sub(previous_host.total)?;
    let process_delta = current_process.checked_sub(previous_process)?;
    if host_delta == 0 {
        return None;
    }
    Some(process_delta as f64 / host_delta as f64 * logical_cpus as f64 * 100.0)
}
