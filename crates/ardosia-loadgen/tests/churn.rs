use std::time::Duration;

use ardosia_loadgen::churn::{
    ChurnRunMetrics, ChurnSchedule, ClientIdAllocator, SlotSelector,
    post_drain_transport_is_healthy,
};
use ardosia_loadgen::report::TransportMetricsReport;

#[test]
fn canonical_schedule_has_deadline_inclusive_1500_ticks() {
    let schedule = ChurnSchedule::new(25.0, Duration::from_secs(60)).unwrap();
    assert_eq!(schedule.planned_ticks(), 1500);
    assert_eq!(schedule.due_offset(0), Some(Duration::from_millis(40)));
    assert_eq!(schedule.due_offset(1499), Some(Duration::from_secs(60)));
    assert_eq!(schedule.due_offset(1500), None);
}

#[test]
fn client_ids_never_reuse() {
    let mut ids = ClientIdAllocator::after_initial_population(500).unwrap();
    assert_eq!(ids.next_id().unwrap(), 500);
    assert_eq!(ids.next_id().unwrap(), 501);
    assert_eq!(ids.next_id().unwrap(), 502);
}

#[test]
fn slot_selector_round_robins_only_eligible_slots() {
    let mut selector = SlotSelector::default();
    let eligible = [false, true, true];

    assert_eq!(selector.select(&eligible), Some(1));
    assert_eq!(selector.select(&eligible), Some(2));
    assert_eq!(selector.select(&eligible), Some(1));
    assert_eq!(selector.select(&[false, false, false]), None);
}

#[test]
fn invalid_schedule_rate_is_rejected() {
    assert!(ChurnSchedule::new(0.0, Duration::from_secs(60)).is_err());
    assert!(ChurnSchedule::new(-1.0, Duration::from_secs(60)).is_err());
    assert!(ChurnSchedule::new(f64::INFINITY, Duration::from_secs(60)).is_err());
    assert!(ChurnSchedule::new(f64::NAN, Duration::from_secs(60)).is_err());
}

#[test]
fn overlapping_replacements_reduce_population_then_recover() {
    let mut metrics = ChurnRunMetrics::for_target(500, 125, 625, 1500);
    metrics.observe_initial_population(500);

    metrics.planned_disconnect_started();
    metrics.planned_disconnect_started();
    assert_eq!(metrics.population_current(), 498);
    assert_eq!(metrics.population_min(), 498);

    metrics.completed_planned_disconnect();
    metrics.completed_planned_disconnect();
    metrics.replacement_attempt_started();
    metrics.replacement_attempt_started();
    assert_eq!(metrics.replacement_inflight_peak(), 2);

    metrics.replacement_connected(Duration::from_millis(8));
    metrics.replacement_connected(Duration::from_millis(11));
    assert_eq!(metrics.population_current(), 500);
    assert_eq!(metrics.population_end(), 500);
    assert_eq!(metrics.replacement_latency_summary().samples, 2);
}

#[test]
fn timed_out_replacement_records_failure_and_deficit() {
    let mut metrics = ChurnRunMetrics::for_target(500, 125, 625, 1);
    metrics.observe_initial_population(500);
    metrics.planned_disconnect_started();
    metrics.completed_planned_disconnect();
    metrics.replacement_attempt_started();
    metrics.replacement_failed(true);

    assert_eq!(metrics.population_current(), 499);
    assert_eq!(metrics.replacement_failures(), 1);
    assert_eq!(metrics.replacement_timeouts(), 1);
    assert_eq!(metrics.replacement_inflight(), 0);
}

#[test]
fn schedule_miss_is_accounted_without_changing_population() {
    let mut metrics = ChurnRunMetrics::for_target(500, 125, 625, 1);
    metrics.observe_initial_population(500);
    metrics.schedule_miss();

    assert_eq!(metrics.schedule_misses(), 1);
    assert_eq!(metrics.population_current(), 500);
}

#[test]
fn post_drain_population_accepts_lifecycle_totals_but_requires_target_current() {
    let healthy = TransportMetricsReport {
        sessions_current: 500,
        sessions_started_total: 2000,
        sessions_closed_total: 1500,
        timed_out_sessions: 0,
        ..TransportMetricsReport::default()
    };
    assert!(post_drain_transport_is_healthy(healthy.clone(), 500, 0));

    let deficit = TransportMetricsReport {
        sessions_current: 499,
        ..healthy.clone()
    };
    assert!(!post_drain_transport_is_healthy(deficit, 500, 0));

    let timeout_growth = TransportMetricsReport {
        timed_out_sessions: 1,
        ..healthy
    };
    assert!(!post_drain_transport_is_healthy(timeout_growth, 500, 0));
}
