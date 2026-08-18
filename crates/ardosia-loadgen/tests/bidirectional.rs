use std::net::SocketAddr;
use std::path::PathBuf;

use ardosia_loadgen::report::RunReport;
use ardosia_loadgen::workload::PendingProbes;
use tokio::process::Command;

fn allocate_loopback_addr() -> SocketAddr {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap()
}

fn scenario_path() -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("ardosia-bidirectional-{}.toml", std::process::id()));
    path
}

#[tokio::test]
async fn one_client_moves_every_workload_class_both_directions_and_measures_rtt() {
    let address = allocate_loopback_addr();
    let path = scenario_path();
    std::fs::write(
        &path,
        r#"
name = "bidirectional-smoke"
clients = 1
protocol_version = 8
ramp_up_seconds = 0
hold_seconds = 2
connect_timeout_seconds = 2
seed = 9

[[traffic]]
kind = "unreliable"
direction = "bidirectional"
packets_per_second_per_client = 8.0
payload_bytes = 64

[[traffic]]
kind = "reliable_ordered"
direction = "bidirectional"
packets_per_second_per_client = 4.0
payload_bytes = 256

[[traffic]]
kind = "fragmented_reliable_ordered"
direction = "bidirectional"
packets_per_second_per_client = 1.0
payload_bytes = 4096

[rtt]
probes_per_second_per_client = 4.0
payload_bytes = 32
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ardosia-loadgen"))
        .arg("local")
        .arg(&path)
        .arg("--bind")
        .arg(address.to_string())
        .output()
        .await
        .unwrap();
    let _ = std::fs::remove_file(&path);

    assert!(
        output.status.success(),
        "loadgen failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: RunReport = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(report.counts.successful_handshakes, 1);
    assert_eq!(report.counts.unexpected_disconnects, 0);
    assert_eq!(report.counts.protocol_errors, 0);
    assert_eq!(report.send_errors, 0);
    assert_eq!(report.counts.clean_disconnects, 1);

    assert!(report.workload.unreliable.tx_frames > 0);
    assert!(report.workload.unreliable.rx_frames > 0);
    assert!(report.workload.reliable_ordered.tx_frames > 0);
    assert!(report.workload.reliable_ordered.rx_frames > 0);
    assert!(report.workload.fragmented_reliable_ordered.tx_frames > 0);
    assert!(report.workload.fragmented_reliable_ordered.rx_frames > 0);
    assert_eq!(
        report
            .workload
            .fragmented_reliable_ordered
            .max_rx_payload_bytes,
        4096
    );
    assert!(report.workload.rtt_requests.tx_frames > 0);
    assert!(report.workload.rtt_responses.rx_frames > 0);
    assert!(report.latency.samples > 0);
}

#[test]
fn pending_rtt_probe_map_is_hard_bounded() {
    let mut probes = PendingProbes::with_capacity(2);
    let now = std::time::Instant::now();

    assert!(probes.insert(1, now).is_ok());
    assert!(probes.insert(2, now).is_ok());
    assert_eq!(probes.len(), 2);
    assert!(probes.insert(3, now).is_err());
    assert_eq!(probes.len(), 2);

    assert!(probes.complete(1).is_some());
    assert_eq!(probes.len(), 1);
}
