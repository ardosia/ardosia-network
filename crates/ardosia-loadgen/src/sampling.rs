#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::Instant;

    #[tokio::test]
    async fn steady_interval_waits_one_full_period_before_first_tick() {
        let period = Duration::from_millis(50);
        let started = Instant::now();
        let mut ticker = super::steady_interval(period);

        ticker.tick().await;

        assert!(started.elapsed() >= period);
    }
}
