use std::time::Duration;

use ardosia_loadgen::workload::initial_phase_offset;

#[test]
fn phase_offsets_are_deterministic_and_bounded() {
    let period = Duration::from_millis(500);
    let first = initial_phase_offset(42, 7, 0, period);
    let second = initial_phase_offset(42, 7, 0, period);

    assert_eq!(first, second);
    assert!(first < period);
}

#[test]
fn phase_offsets_spread_clients_and_lanes() {
    let period = Duration::from_secs(1);
    let base = initial_phase_offset(42, 7, 0, period);
    let other_client = initial_phase_offset(42, 8, 0, period);
    let other_lane = initial_phase_offset(42, 7, 1, period);

    assert_ne!(base, other_client);
    assert_ne!(base, other_lane);
    assert!(other_client < period);
    assert!(other_lane < period);
}

#[test]
fn zero_period_has_zero_offset() {
    assert_eq!(
        initial_phase_offset(42, 7, 0, Duration::ZERO),
        Duration::ZERO
    );
}
