use std::net::SocketAddr;
use std::str::FromStr;

use ardosia_loadgen::child_protocol::{
    ChildCommand, ChildEvent, ServerRunReport, run_child_session,
};
use ardosia_loadgen::scenario::Scenario;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn scenario() -> Scenario {
    Scenario::from_str(
        r#"
name = "child-smoke"
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

fn allocate_loopback_addr() -> SocketAddr {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap()
}

#[test]
fn child_commands_and_events_roundtrip_json() {
    let start = ChildCommand::Start {
        bind_addr: "127.0.0.1:19132".into(),
        scenario: scenario(),
        worker_shards: None,
    };
    let start_json = serde_json::to_string(&start).unwrap();
    match serde_json::from_str::<ChildCommand>(&start_json).unwrap() {
        ChildCommand::Start {
            bind_addr,
            scenario,
            worker_shards,
        } => {
            assert_eq!(bind_addr, "127.0.0.1:19132");
            assert_eq!(scenario.name, "child-smoke");
            assert_eq!(scenario.seed, 7);
            assert_eq!(worker_shards, None);
        }
        other => panic!("unexpected command: {other:?}"),
    }

    let begin = ChildCommand::BeginMeasurement;
    assert!(matches!(
        serde_json::from_str::<ChildCommand>(&serde_json::to_string(&begin).unwrap()).unwrap(),
        ChildCommand::BeginMeasurement
    ));

    let end = ChildCommand::EndMeasurement;
    assert!(matches!(
        serde_json::from_str::<ChildCommand>(&serde_json::to_string(&end).unwrap()).unwrap(),
        ChildCommand::EndMeasurement
    ));

    let ready = ChildEvent::Ready { pid: 1234 };
    assert!(matches!(
        serde_json::from_str::<ChildEvent>(&serde_json::to_string(&ready).unwrap()).unwrap(),
        ChildEvent::Ready { pid: 1234 }
    ));

    let ended = ChildEvent::MeasurementEnded;
    assert!(matches!(
        serde_json::from_str::<ChildEvent>(&serde_json::to_string(&ended).unwrap()).unwrap(),
        ChildEvent::MeasurementEnded
    ));

    let stopped = ChildEvent::Stopped {
        report: Box::new(ServerRunReport::default()),
    };
    assert!(matches!(
        serde_json::from_str::<ChildEvent>(&serde_json::to_string(&stopped).unwrap()).unwrap(),
        ChildEvent::Stopped { .. }
    ));
}

#[tokio::test]
async fn child_session_obeys_ready_measure_end_stop_order_and_reaps_server_task() {
    let bind_addr = allocate_loopback_addr();
    let (parent_io, child_io) = tokio::io::duplex(16 * 1024);
    let (parent_read, mut parent_write) = tokio::io::split(parent_io);
    let (child_read, child_write) = tokio::io::split(child_io);
    let mut parent_lines = BufReader::new(parent_read).lines();

    let child = tokio::spawn(run_child_session(child_read, child_write));

    send_command(
        &mut parent_write,
        &ChildCommand::Start {
            bind_addr: bind_addr.to_string(),
            scenario: scenario(),
            worker_shards: None,
        },
    )
    .await;
    assert!(matches!(
        read_event(&mut parent_lines).await,
        ChildEvent::Ready { .. }
    ));

    send_command(&mut parent_write, &ChildCommand::BeginMeasurement).await;
    assert!(matches!(
        read_event(&mut parent_lines).await,
        ChildEvent::MeasurementStarted
    ));

    send_command(&mut parent_write, &ChildCommand::EndMeasurement).await;
    assert!(matches!(
        read_event(&mut parent_lines).await,
        ChildEvent::MeasurementEnded
    ));

    send_command(&mut parent_write, &ChildCommand::Stop).await;
    let stopped = read_event(&mut parent_lines).await;
    match stopped {
        ChildEvent::Stopped { report } => {
            assert_eq!(report.metrics.connected_current, 0);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    child.await.unwrap().unwrap();
}

async fn send_command<W>(writer: &mut W, command: &ChildCommand)
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let line = serde_json::to_string(command).unwrap();
    writer.write_all(line.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();
}

async fn read_event<R>(lines: &mut tokio::io::Lines<BufReader<R>>) -> ChildEvent
where
    R: tokio::io::AsyncRead + Unpin,
{
    let line = lines.next_line().await.unwrap().unwrap();
    serde_json::from_str(&line).unwrap()
}
