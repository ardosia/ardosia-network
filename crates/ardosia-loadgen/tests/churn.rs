use std::time::Duration;

use ardosia_loadgen::churn::{ChurnSchedule, ClientIdAllocator, SlotSelector};

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
