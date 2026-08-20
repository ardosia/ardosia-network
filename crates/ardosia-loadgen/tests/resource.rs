use ardosia_loadgen::resource::{ResourceAccumulator, ResourcePoint};

#[cfg(target_os = "linux")]
use ardosia_loadgen::resource::linux::{
    parse_host_cpu_ticks, parse_meminfo, parse_process_cpu_ticks, parse_process_rss_bytes,
};

#[cfg(target_os = "linux")]
#[test]
fn parses_proc_fixture_values() {
    let cpu =
        parse_host_cpu_ticks("cpu  100 20 30 400 50 6 7 8 9 10\ncpu0 1 2 3 4 5 6 7 8\n").unwrap();
    assert_eq!(cpu.total, 621);
    assert_eq!(cpu.idle, 450);

    let process = parse_process_cpu_ticks(
        "123 (worker name) S 1 1 1 0 -1 0 0 0 0 0 30 20 0 0 0 0 0 0 0 0 0 0 0 0 0 0",
    )
    .unwrap();
    assert_eq!(process, 50);

    assert_eq!(
        parse_process_rss_bytes("Name:\tworker\nVmSize:\t9999 kB\nVmRSS:\t1234 kB\n"),
        Some(1_263_616)
    );

    let memory = parse_meminfo(
        "MemTotal:        8000 kB\nMemFree:         1000 kB\nMemAvailable:    3000 kB\n",
    )
    .unwrap();
    assert_eq!(memory.total_bytes, 8_192_000);
    assert_eq!(memory.available_bytes, 3_072_000);
    assert_eq!(memory.used_bytes, 5_120_000);
}

#[test]
fn accumulator_ignores_missing_values_instead_of_treating_them_as_zero() {
    let mut accumulator = ResourceAccumulator::default();
    accumulator.push(ResourcePoint {
        process_cpu_pct: Some(10.0),
        process_rss_bytes: Some(100),
        host_cpu_pct: Some(50.0),
        host_memory_used_bytes: Some(1_000),
        host_memory_available_bytes: Some(9_000),
    });
    accumulator.push(ResourcePoint {
        process_cpu_pct: None,
        process_rss_bytes: Some(200),
        host_cpu_pct: Some(70.0),
        host_memory_used_bytes: Some(2_000),
        host_memory_available_bytes: Some(8_000),
    });
    accumulator.push(ResourcePoint {
        process_cpu_pct: Some(30.0),
        process_rss_bytes: None,
        host_cpu_pct: None,
        host_memory_used_bytes: None,
        host_memory_available_bytes: Some(7_000),
    });

    let summary = accumulator.finish();
    assert_eq!(summary.sample_count, 3);
    assert_eq!(summary.process_cpu_avg_pct, Some(20.0));
    assert_eq!(summary.process_cpu_peak_pct, Some(30.0));
    assert_eq!(summary.process_rss_avg_bytes, Some(150));
    assert_eq!(summary.process_rss_peak_bytes, Some(200));
    assert_eq!(summary.host_cpu_avg_pct, Some(60.0));
    assert_eq!(summary.host_cpu_peak_pct, Some(70.0));
    assert_eq!(summary.host_memory_used_avg_bytes, Some(1_500));
    assert_eq!(summary.host_memory_used_peak_bytes, Some(2_000));
    assert_eq!(summary.host_memory_available_min_bytes, Some(7_000));
}
