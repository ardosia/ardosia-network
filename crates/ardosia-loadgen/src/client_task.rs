use std::net::SocketAddr;
use std::time::Duration;

use raknet_rust::client::{RaknetClient, RaknetClientConfig, RaknetClientEvent};
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, sleep, sleep_until, timeout};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Phase {
    Ramp,
    Hold { deadline: Instant },
    Abort,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ConnectOutcome {
    Ready,
    Failed,
}

#[derive(Debug, Default)]
pub(crate) struct ClientTaskResult {
    pub(crate) unexpected_disconnects: usize,
    pub(crate) protocol_errors: usize,
    pub(crate) clean_disconnects: usize,
}

pub(crate) async fn run_client_task(
    target: SocketAddr,
    protocol_version: u8,
    connect_timeout: Duration,
    stagger: Duration,
    mut phase_rx: watch::Receiver<Phase>,
    outcome_tx: mpsc::Sender<ConnectOutcome>,
) -> ClientTaskResult {
    if !stagger.is_zero() {
        sleep(stagger).await;
    }

    let mut config = RaknetClientConfig::default();
    config.protocol_version = protocol_version;

    let connect = timeout(
        connect_timeout,
        RaknetClient::connect_with_config(target, config),
    )
    .await;

    let mut client = match connect {
        Ok(Ok(client)) => client,
        Ok(Err(_)) | Err(_) => {
            let _ = outcome_tx.send(ConnectOutcome::Failed).await;
            return ClientTaskResult::default();
        }
    };

    let _ = outcome_tx.send(ConnectOutcome::Ready).await;
    let mut result = ClientTaskResult::default();

    loop {
        let phase = *phase_rx.borrow();
        match phase {
            Phase::Abort => {
                let _ = client.disconnect(None).await;
                return result;
            }
            Phase::Ramp => {
                tokio::select! {
                    changed = phase_rx.changed() => {
                        if changed.is_err() {
                            result.unexpected_disconnects += 1;
                            return result;
                        }
                    }
                    event = client.next_event() => {
                        if !handle_client_event(event, &mut result) {
                            return result;
                        }
                    }
                }
            }
            Phase::Hold { deadline } => {
                if Instant::now() >= deadline {
                    finish_client(&mut client, &mut result).await;
                    return result;
                }

                tokio::select! {
                    _ = sleep_until(deadline) => {
                        finish_client(&mut client, &mut result).await;
                        return result;
                    }
                    changed = phase_rx.changed() => {
                        if changed.is_err() {
                            result.unexpected_disconnects += 1;
                            return result;
                        }
                    }
                    event = client.next_event() => {
                        if !handle_client_event(event, &mut result) {
                            return result;
                        }
                    }
                }
            }
        }
    }
}

fn handle_client_event(event: Option<RaknetClientEvent>, result: &mut ClientTaskResult) -> bool {
    match event {
        Some(RaknetClientEvent::DecodeError { .. }) => {
            result.protocol_errors += 1;
            true
        }
        Some(RaknetClientEvent::Disconnected { .. }) | None => {
            result.unexpected_disconnects += 1;
            false
        }
        Some(_) => true,
    }
}

async fn finish_client(client: &mut RaknetClient, result: &mut ClientTaskResult) {
    if client.disconnect(None).await.is_ok() {
        result.clean_disconnects += 1;
    } else {
        result.unexpected_disconnects += 1;
    }
}
