use std::str::FromStr;

use ardosia_loadgen::child_protocol::ChildCommand;
use ardosia_loadgen::cli::{Cli, Command};
use ardosia_loadgen::report::{
    EnvironmentReport, ResourceWindowsReport, RunCounts, RunReport, RunReportInput,
    ServerRuntimeReport, TransportWindowReport,
};
use ardosia_loadgen::scenario::Scenario;
use ardosia_loadgen::workload::WorkloadCounts;
use clap::Parser;

fn scenario() -> Scenario {
    Scenario::from_str(
        r#"
name = "shard-scaling"
clients = 1
protocol_version = 8
ramp_up_seconds = 0
hold_seconds = 1
connect_timeout_seconds = 2
seed = 7
"#,
    )
    .unwrap()
}

#[test]
fn local_cli_parses_worker_shards_override() {
    let cli = Cli::try_parse_from([
        "ardosia-loadgen",
        "local",
        "scenarios/steady-500.toml",
        "--worker-shards",
        "4",
    ])
    .unwrap();

    match cli.command {
        Command::Local { worker_shards, .. } => assert_eq!(worker_shards, Some(4)),
        other => panic!("expected local command, got {other:?}"),
    }
}

#[test]
fn profile_cli_parses_worker_shards_override() {
    let cli = Cli::try_parse_from([
        "ardosia-loadgen",
        "profile",
        "scenarios/steady-1000.toml",
        "--worker-shards",
        "8",
    ])
    .unwrap();

    match cli.command {
        Command::Profile { worker_shards, .. } => assert_eq!(worker_shards, Some(8)),
        other => panic!("expected profile command, got {other:?}"),
    }
}

#[test]
fn child_start_roundtrips_worker_shards_override() {
    let command = ChildCommand::Start {
        bind_addr: "127.0.0.1:19132".into(),
        scenario: scenario(),
        worker_shards: Some(4),
    };

    let json = serde_json::to_string(&command).unwrap();
    match serde_json::from_str::<ChildCommand>(&json).unwrap() {
        ChildCommand::Start { worker_shards, .. } => assert_eq!(worker_shards, Some(4)),
        other => panic!("expected start command, got {other:?}"),
    }
}

#[test]
fn report_records_requested_and_effective_worker_shards() {
    let report = RunReport::assemble(
        EnvironmentReport::default(),
        scenario(),
        RunReportInput {
            correctness: RunCounts {
                successful_handshakes: 1,
                clean_disconnects: 1,
                ..RunCounts::default()
            },
            workload: WorkloadCounts::default(),
            latency: Default::default(),
            transport: TransportWindowReport::default(),
            churn: None,
            resources: ResourceWindowsReport::default(),
            total_duration_ms: 1_000,
            measured_duration_ms: 1_000,
        },
    )
    .with_server_runtime(ServerRuntimeReport {
        requested_worker_shards: Some(4),
        effective_worker_shards: 4,
    });

    let runtime = report.server_runtime.expect("local report runtime metadata");
    assert_eq!(runtime.requested_worker_shards, Some(4));
    assert_eq!(runtime.effective_worker_shards, 4);
}
