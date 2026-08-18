use ardosia_loadgen::scenario::Scenario;

#[test]
fn parses_connect_300_shape() {
    let scenario = Scenario::from_str(
        r#"
name = "connect-300"
clients = 300
protocol_version = 8
ramp_up_seconds = 10
hold_seconds = 60
connect_timeout_seconds = 5
"#,
    )
    .unwrap();

    assert_eq!(scenario.clients, 300);
    assert_eq!(scenario.protocol_version, 8);
    assert_eq!(scenario.hold_seconds, 60);
}

#[test]
fn rejects_zero_clients() {
    let error = Scenario::from_str(
        r#"
name = "bad"
clients = 0
protocol_version = 8
ramp_up_seconds = 0
hold_seconds = 60
connect_timeout_seconds = 5
"#,
    )
    .unwrap_err();

    assert!(error.to_string().contains("clients"));
}
