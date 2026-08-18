use std::time::Duration;

pub fn initial_phase_offset(
    seed: u64,
    client_id: u64,
    lane_index: usize,
    period: Duration,
) -> Duration {
    let period_nanos = period.as_nanos();
    if period_nanos == 0 {
        return Duration::ZERO;
    }

    let lane = u64::try_from(lane_index).unwrap_or(u64::MAX);
    let input = seed
        ^ client_id.wrapping_mul(0xD6E8_FEB8_6659_FD93)
        ^ lane.wrapping_mul(0xA5A3_564E_27F8_864D);
    let offset_nanos = u128::from(mix64(input)) % period_nanos;
    let seconds = u64::try_from(offset_nanos / 1_000_000_000).unwrap_or(u64::MAX);
    let nanos = u32::try_from(offset_nanos % 1_000_000_000).unwrap_or(999_999_999);
    Duration::new(seconds, nanos)
}

fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}
