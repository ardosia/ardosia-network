use std::time::Duration;

use ardosia_loadgen::churn::{
    ChurnRunMetrics, ChurnSchedule, ClientIdAllocator, SlotSelector,
};

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
