use ardosia_loadgen::report::{RunCounts, RunReport};

#[test]
fn perfect_result_passes() {
    let report = RunReport::from_counts(
        "connect-300".into(),
        300,
        RunCounts {
            successful_handshakes: 300,
            failed_handshakes: 0,
            unexpected_disconnects: 0,
            protocol_errors: 0,
            clean_disconnects: 300,
        },
        70_000,
    );

    assert!(report.passed);
    assert!(report.failure_reason.is_none());
}

#[test]
fn unexpected_disconnect_fails() {
    let report = RunReport::from_counts(
        "connect-300".into(),
        300,
        RunCounts {
            successful_handshakes: 300,
            failed_handshakes: 0,
            unexpected_disconnects: 1,
            protocol_errors: 0,
            clean_disconnects: 299,
        },
        70_000,
    );

    assert!(!report.passed);
    assert!(
        report
            .failure_reason
            .as_deref()
            .unwrap()
            .contains("unexpected disconnect")
    );
}

#[test]
fn server_protocol_errors_can_fail_an_otherwise_clean_run() {
    let mut report = RunReport::from_counts(
        "connect-300".into(),
        300,
        RunCounts {
            successful_handshakes: 300,
            failed_handshakes: 0,
            unexpected_disconnects: 0,
            protocol_errors: 0,
            clean_disconnects: 300,
        },
        70_000,
    );

    report.add_protocol_errors(1);

    assert!(!report.passed);
    assert_eq!(report.counts.protocol_errors, 1);
}
