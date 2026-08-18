use std::time::Duration;

use ardosia_loadgen::latency::LatencyHistogram;

#[test]
fn known_samples_produce_monotonic_percentiles_and_true_max() {
    let mut histogram = LatencyHistogram::default();
    for millis in [1, 2, 3, 4, 100] {
        histogram.record(Duration::from_millis(millis));
    }

    let summary = histogram.summary();
    assert_eq!(summary.samples, 5);
    assert_eq!(summary.overflow_samples, 0);
    assert_eq!(summary.max_ms, Some(100.0));
    assert!(summary.p50_ms.is_some());
    assert!(summary.p95_ms.is_some());
    assert!(summary.p99_ms.is_some());
    assert!(summary.p50_ms <= summary.p95_ms);
    assert!(summary.p95_ms <= summary.p99_ms);
}

#[test]
fn overflow_samples_are_counted_while_preserving_true_max() {
    let mut histogram = LatencyHistogram::default();
    histogram.record(Duration::from_millis(25));
    histogram.record(Duration::from_millis(12_345));

    let summary = histogram.summary();
    assert_eq!(summary.samples, 2);
    assert_eq!(summary.overflow_samples, 1);
    assert_eq!(summary.max_ms, Some(12_345.0));
}

#[test]
fn histograms_merge_without_retaining_raw_samples() {
    let mut left = LatencyHistogram::default();
    left.record(Duration::from_millis(10));
    left.record(Duration::from_millis(20));

    let mut right = LatencyHistogram::default();
    right.record(Duration::from_millis(30));
    right.record(Duration::from_millis(40));

    left.merge(&right);
    let summary = left.summary();
    assert_eq!(summary.samples, 4);
    assert_eq!(summary.max_ms, Some(40.0));
}

#[test]
fn empty_histogram_reports_unavailable_percentiles() {
    let summary = LatencyHistogram::default().summary();
    assert_eq!(summary.samples, 0);
    assert_eq!(summary.p50_ms, None);
    assert_eq!(summary.p95_ms, None);
    assert_eq!(summary.p99_ms, None);
    assert_eq!(summary.max_ms, None);
}
