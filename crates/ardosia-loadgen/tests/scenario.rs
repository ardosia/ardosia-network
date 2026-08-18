use std::str::FromStr;

use ardosia_loadgen::scenario::{Direction, Scenario, TrafficKind};

const BASE: &str = r#"
name = "connect-300"
clients = 300
protocol_version = 8
ramp_up_seconds = 10
hold_seconds = 60
connect_timeout_seconds = 5
"#;

#[test]
fn phase1_connect_scenario_keeps_backward_compatible_defaults() {
    let scenario = Scenario::from_str(BASE).unwrap();

    assert_eq!(scenario.clients, 300);
    assert_eq!(scenario.protocol_version, 8);
    assert_eq!(scenario.hold_seconds, 60);
    assert_eq!(scenario.seed, 1);
    assert!(scenario.traffic.is_empty());
    assert!(scenario.rtt.is_none());
}

#[test]
fn parses_steady_mixed_traffic_shape() {
    let scenario = Scenario::from_str(
        r#"
name = "steady-300"
clients = 300
protocol_version = 8
ramp_up_seconds = 10
hold_seconds = 60
connect_timeout_seconds = 5
seed = 42

[[traffic]]
kind = "unreliable"
direction = "bidirectional"
packets_per_second_per_client = 20.0
payload_bytes = 64

[[traffic]]
kind = "reliable_ordered"
direction = "client_to_server"
packets_per_second_per_client = 2.0
payload_bytes = 256

[[traffic]]
kind = "fragmented_reliable_ordered"
direction = "server_to_client"
packets_per_second_per_client = 0.2
payload_bytes = 4096

[rtt]
probes_per_second_per_client = 2.0
payload_bytes = 32
"#,
    )
    .unwrap();

    assert_eq!(scenario.seed, 42);
    assert_eq!(scenario.traffic.len(), 3);
    assert_eq!(scenario.traffic[0].kind, TrafficKind::Unreliable);
    assert_eq!(scenario.traffic[0].direction, Direction::Bidirectional);
    assert_eq!(scenario.traffic[1].kind, TrafficKind::ReliableOrdered);
    assert_eq!(scenario.traffic[1].direction, Direction::ClientToServer);
    assert_eq!(
        scenario.traffic[2].kind,
        TrafficKind::FragmentedReliableOrdered
    );
    assert_eq!(scenario.traffic[2].direction, Direction::ServerToClient);
    assert_eq!(
        scenario.rtt.as_ref().unwrap().probes_per_second_per_client,
        2.0
    );
}

#[test]
fn rejects_zero_clients() {
    let error = Scenario::from_str(&BASE.replace("clients = 300", "clients = 0")).unwrap_err();
    assert!(error.to_string().contains("clients"));
}

#[test]
fn rejects_zero_hold_seconds() {
    let error =
        Scenario::from_str(&BASE.replace("hold_seconds = 60", "hold_seconds = 0")).unwrap_err();
    assert!(error.to_string().contains("hold_seconds"));
}

#[test]
fn rejects_zero_connect_timeout() {
    let error = Scenario::from_str(
        &BASE.replace("connect_timeout_seconds = 5", "connect_timeout_seconds = 0"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("connect_timeout_seconds"));
}

#[test]
fn rejects_invalid_traffic_rates_and_payloads() {
    for rate in ["0.0", "-1.0", "inf", "nan"] {
        let input = format!(
            "{BASE}\n[[traffic]]\nkind = \"unreliable\"\ndirection = \"bidirectional\"\npackets_per_second_per_client = {rate}\npayload_bytes = 64\n"
        );
        let error = Scenario::from_str(&input).unwrap_err();
        assert!(error.to_string().contains("packets_per_second_per_client"));
    }

    let input = format!(
        "{BASE}\n[[traffic]]\nkind = \"unreliable\"\ndirection = \"bidirectional\"\npackets_per_second_per_client = 1.0\npayload_bytes = 0\n"
    );
    let error = Scenario::from_str(&input).unwrap_err();
    assert!(error.to_string().contains("payload_bytes"));
}

#[test]
fn rejects_invalid_rtt_rates_and_payloads() {
    for rate in ["0.0", "-1.0", "inf", "nan"] {
        let input =
            format!("{BASE}\n[rtt]\nprobes_per_second_per_client = {rate}\npayload_bytes = 32\n");
        let error = Scenario::from_str(&input).unwrap_err();
        assert!(error.to_string().contains("probes_per_second_per_client"));
    }

    let input = format!("{BASE}\n[rtt]\nprobes_per_second_per_client = 1.0\npayload_bytes = 0\n");
    let error = Scenario::from_str(&input).unwrap_err();
    assert!(error.to_string().contains("payload_bytes"));
}
