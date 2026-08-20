use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use ardosia_loadgen::child_protocol::{ChildCommand, ChildEvent, run_child_session};
use ardosia_loadgen::scenario::Scenario;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::{Instant, sleep};

fn scenario() -> Scenario {
    Scenario::from_str(
        r#"
name = "child-shard-snapshot"
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

#[tokio::test]
async fn child_snapshot_exposes_configured_transport_shards() {
    let bind_addr = allocate_loopback_addr();
    let (parent_io, child_io) = tokio::io::duplex(32 * 1024);
    let (parent_read, mut parent_write) = tokio::io::split(parent_io);
    let (child_read, child_write) = tokio::io::split(child_io);
    let mut parent_lines = BufReader::new(parent_read).lines();

    let child = tokio::spawn(run_child_session(child_read, child_write));

    send_command(
        &mut parent_write,
        &ChildCommand::Start {
            bind_addr: bind_addr.to_string(),
            scenario: scenario(),
            worker_shards: Some(2),
        },
    )
    .await;
    assert!(matches!(
        read_event(&mut parent_lines).await,
        ChildEvent::Ready { .. }
    ));

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut observed_ids = Vec::new();
    loop {
        send_command(&mut parent_write, &ChildCommand::Snapshot).await;
        match read_event(&mut parent_lines).await {
            ChildEvent::Snapshot { shard_metrics, .. } => {
                observed_ids = shard_metrics
                    .iter()
                    .map(|shard| shard.shard_id)
                    .collect::<Vec<_>>();
                if observed_ids == [0, 1] {
                    break;
                }
            }
            other => panic!("unexpected event while waiting for shard metrics: {other:?}"),
        }

        assert!(
            Instant::now() < deadline,
            "expected shard ids [0, 1], observed {observed_ids:?}"
        );
        sleep(Duration::from_millis(50)).await;
    }

    send_command(&mut parent_write, &ChildCommand::Stop).await;
    assert!(matches!(
        read_event(&mut parent_lines).await,
        ChildEvent::Stopped { .. }
    ));
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
