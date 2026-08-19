use std::path::PathBuf;
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
    assert!(scenario.churn.is_none());
}

#[test]
fn checked_in_scaling_scenarios_parse_and_match_expected_shapes() {
    let steady_300 = load_checked_in("steady-300.toml");
    assert_scaling_shape(&steady_300, 300, 10, 20.0, 2.0, 0.2, 2.0);

    let steady_500 = load_checked_in("steady-500.toml");
    assert_scaling_shape(&steady_500, 500, 10, 20.0, 2.0, 0.2, 2.0);

    let ceiling_1000 = load_checked_in("ceiling-1000.toml");
    assert_scaling_shape(&ceiling_1000, 1000, 20, 10.0, 1.0, 0.1, 1.0);

    let steady_1000 = load_checked_in("steady-1000.toml");
    assert_scaling_shape(&steady_1000, 1000, 20, 20.0, 2.0, 0.2, 2.0);
}

#[test]
fn checked_in_churn_scenario_derives_canonical_admission_headroom() {
    let scenario = load_checked_in("churn-500.toml");
    assert_scaling_shape(&scenario, 500, 10, 20.0, 2.0, 0.2, 2.0);
    assert_eq!(
        scenario.churn.as_ref().unwrap().replacements_per_second,
        25.0
    );
    assert_eq!(scenario.churn_admission_headroom(), 125);
    assert_eq!(scenario.benchmark_max_connections(), 625);
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

#[test]
fn rejects_invalid_churn_rates() {
    for rate in ["0.0", "-1.0", "inf", "nan"] {
        let input = format!("{BASE}\n[churn]\nreplacements_per_second = {rate}\n");
        let error = Scenario::from_str(&input).unwrap_err();
        assert!(error.to_string().contains("replacements_per_second"));
    }
}

fn load_checked_in(name: &str) -> Scenario {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("scenarios")
        .join(name);
    let input = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    Scenario::from_str(&input)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn assert_scaling_shape(
    scenario: &Scenario,
    clients: usize,
    ramp_up_seconds: u64,
    unreliable_rate: f64,
    reliable_rate: f64,
    fragmented_rate: f64,
    rtt_rate: f64,
) {
    assert_eq!(scenario.clients, clients);
    assert_eq!(scenario.protocol_version, 8);
    assert_eq!(scenario.ramp_up_seconds, ramp_up_seconds);
    assert_eq!(scenario.hold_seconds, 60);
    assert_eq!(scenario.connect_timeout_seconds, 5);
    assert_eq!(scenario.seed, 1);
    assert_eq!(scenario.traffic.len(), 3);

    assert_eq!(scenario.traffic[0].kind, TrafficKind::Unreliable);
    assert_eq!(scenario.traffic[0].direction, Direction::Bidirectional);
    assert_eq!(
        scenario.traffic[0].packets_per_second_per_client,
        unreliable_rate
    );
    assert_eq!(scenario.traffic[0].payload_bytes, 64);

    assert_eq!(scenario.traffic[1].kind, TrafficKind::ReliableOrdered);
    assert_eq!(scenario.traffic[1].direction, Direction::Bidirectional);
    assert_eq!(
        scenario.traffic[1].packets_per_second_per_client,
        reliable_rate
    );
    assert_eq!(scenario.traffic[1].payload_bytes, 256);

    assert_eq!(
        scenario.traffic[2].kind,
        TrafficKind::FragmentedReliableOrdered
    );
    assert_eq!(scenario.traffic[2].direction, Direction::Bidirectional);
    assert_eq!(
        scenario.traffic[2].packets_per_second_per_client,
        fragmented_rate
    );
    assert_eq!(scenario.traffic[2].payload_bytes, 4096);

    let rtt = scenario.rtt.as_ref().expect("RTT config is required");
    assert_eq!(rtt.probes_per_second_per_client, rtt_rate);
    assert_eq!(rtt.payload_bytes, 32);
}
