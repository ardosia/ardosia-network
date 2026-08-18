use std::fs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTicks {
    pub total: u64,
    pub idle: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryInfo {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
}

pub fn parse_host_cpu_ticks(input: &str) -> Option<CpuTicks> {
    let line = input.lines().find(|line| line.starts_with("cpu "))?;
    let mut fields = line.split_whitespace();
    if fields.next()? != "cpu" {
        return None;
    }

    let mut values = [0u64; 8];
    for value in &mut values {
        *value = fields.next()?.parse().ok()?;
    }

    let total = values
        .iter()
        .copied()
        .fold(0u64, u64::saturating_add);
    let idle = values[3].saturating_add(values[4]);
    Some(CpuTicks { total, idle })
}

pub fn parse_process_cpu_ticks(input: &str) -> Option<u64> {
    let close_paren = input.rfind(')')?;
    let remainder = input.get(close_paren + 1..)?.trim();
    let fields: Vec<&str> = remainder.split_whitespace().collect();
    let user_ticks: u64 = fields.get(11)?.parse().ok()?;
    let system_ticks: u64 = fields.get(12)?.parse().ok()?;
    Some(user_ticks.saturating_add(system_ticks))
}

pub fn parse_process_rss_bytes(input: &str) -> Option<u64> {
    let line = input.lines().find(|line| line.starts_with("VmRSS:"))?;
    parse_kib_line(line)
}

pub fn parse_meminfo(input: &str) -> Option<MemoryInfo> {
    let total_bytes = input
        .lines()
        .find(|line| line.starts_with("MemTotal:"))
        .and_then(parse_kib_line)?;
    let available_bytes = input
        .lines()
        .find(|line| line.starts_with("MemAvailable:"))
        .and_then(parse_kib_line)?;

    Some(MemoryInfo {
        total_bytes,
        available_bytes,
        used_bytes: total_bytes.saturating_sub(available_bytes),
    })
}

pub(crate) fn read_host_cpu_ticks() -> Option<CpuTicks> {
    parse_host_cpu_ticks(&fs::read_to_string("/proc/stat").ok()?)
}

pub(crate) fn read_process_cpu_ticks(pid: u32) -> Option<u64> {
    parse_process_cpu_ticks(&fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

pub(crate) fn read_process_rss_bytes(pid: u32) -> Option<u64> {
    parse_process_rss_bytes(&fs::read_to_string(format!("/proc/{pid}/status")).ok()?)
}

pub(crate) fn read_meminfo() -> Option<MemoryInfo> {
    parse_meminfo(&fs::read_to_string("/proc/meminfo").ok()?)
}

fn parse_kib_line(line: &str) -> Option<u64> {
    let mut fields = line.split_whitespace();
    let _name = fields.next()?;
    let kib: u64 = fields.next()?.parse().ok()?;
    let unit = fields.next()?;
    if unit != "kB" {
        return None;
    }
    Some(kib.saturating_mul(1024))
}
