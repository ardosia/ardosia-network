# RakNet Protocol-8 Baseline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first usable `ardosia-network` transport layer around a pinned `raknet-rust` snapshot, prove RakNet protocol 8 compatibility, and pass a repeatable 300-client connection/hold benchmark.

**Architecture:** `ardosia-network` exposes opaque-byte `NetworkServer`/`Connection` APIs and hides all `raknet-rust` public types. A single background backend task owns upstream `RaknetServer`, routes peer events into bounded per-connection channels, and accepts send/disconnect commands from public connection handles. `ardosia-loadgen` uses upstream `RaknetClient` only as a repository-local benchmark client and can run either against an external benchmark server or an in-process local server.

**Tech Stack:** Rust 2024, Rust 1.85+, Tokio, `bytes`, `thiserror`, pinned `mcbe-rs/raknet-rust` 0.2.0, Serde/TOML/JSON, Clap.

**Spec:** `docs/superpowers/specs/2026-08-17-raknet-network-design.md`

## Global Constraints

- `ardosia-network` owns UDP and RakNet transport only; MCPE packet/game protocol logic stays out of this repository.
- The initial upstream snapshot is exactly `mcbe-rs/raknet-rust` commit `3edfb4170e6cb5aeed992b09b50176fb7e5b6079`, crate version `0.2.0`, Apache-2.0.
- Preserve the upstream Apache-2.0 license and record provenance in `vendor/raknet-rust/UPSTREAM.md`.
- Do not alter the vendored transport implementation during this plan unless an implementation-blocking correctness failure is demonstrated by a test.
- RakNet protocol 8 is runtime configuration: server `supported_protocols = vec![8]`; client `protocol_version = 8`. Do not change upstream `RAKNET_PROTOCOL_VERSION` from 11.
- Public Ardosia APIs must not expose `raknet-rust` types.
- Public payloads are `bytes::Bytes`; this plan introduces no MCPE packet types.
- All queues introduced by Ardosia must be bounded.
- The baseline gate is 300/300 protocol-8 sessions established and held for 60 seconds with zero unexpected disconnects and zero protocol/decode errors.
- Heavy mixed traffic, `steady-500`, churn, `ceiling-1000`, process CPU/RSS collection, RTT histograms, and network impairment belong to the follow-up scaling plan after this baseline is working.
- The design intentionally does not choose a project-level license for Ardosia itself.

---

## File Map

The baseline implementation creates these Ardosia-owned files:

```text
Cargo.toml                          workspace definition and shared dependencies
.gitignore                         Rust/build and generated-result ignores
README.md                          scope, architecture, commands, protocol-8 notes
.github/workflows/ci.yml           lightweight fmt/clippy/test CI only

crates/ardosia-network/
├── Cargo.toml
├── src/
│   ├── lib.rs                     public exports only
│   ├── config.rs                  NetworkConfig and vendor config conversion
│   ├── reliability.rs             Ardosia reliability enum and private mapping
│   ├── error.rs                   stable NetworkError surface
│   ├── metrics.rs                 cheap atomic counters + public snapshot
│   ├── backend.rs                 sole owner/event loop for RaknetServer
│   ├── server.rs                  NetworkServer accept/shutdown facade
│   └── connection.rs              public peer handle, recv/send/close
└── tests/
    ├── vendor_smoke.rs             pinned vendor API smoke test
    ├── protocol8.rs                raw and full protocol-8 handshake tests
    └── transport.rs                payload, fragmentation, disconnect lifecycle

crates/ardosia-loadgen/
├── Cargo.toml
├── src/
│   ├── main.rs                     CLI: local / serve / run
│   ├── scenario.rs                 TOML model + validation
│   ├── runner.rs                   client orchestration and hold barrier
│   └── report.rs                   JSON/terminal correctness report
└── tests/
    └── scenario.rs                 scenario parsing/validation tests

scenarios/connect-300.toml          checked-in baseline workload

vendor/raknet-rust/                 exact upstream archive
└── UPSTREAM.md                     Ardosia-added provenance record
```

The follow-up scaling plan will add workload, histogram, richer telemetry, and impairment modules without changing the transport/game-protocol boundary established here.

---

### Task 1: Bootstrap the workspace and vendor the exact upstream snapshot

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `crates/ardosia-network/Cargo.toml`
- Create: `crates/ardosia-network/src/lib.rs`
- Create: `crates/ardosia-network/tests/vendor_smoke.rs`
- Create: `vendor/raknet-rust/**` from upstream archive
- Create: `vendor/raknet-rust/UPSTREAM.md`

**Interfaces:**
- Consumes: upstream Git commit `3edfb4170e6cb5aeed992b09b50176fb7e5b6079`.
- Produces: workspace dependency named `raknet-rust` at `vendor/raknet-rust`; no Ardosia production API yet.

- [ ] **Step 1: Write the vendor smoke test before the vendor path exists**

Create `crates/ardosia-network/tests/vendor_smoke.rs`:

```rust
use raknet_rust::low_level::protocol::constants::RAKNET_PROTOCOL_VERSION;
use raknet_rust::low_level::transport::TransportConfig;

#[test]
fn pinned_vendor_exposes_expected_protocol_configuration_surface() {
    let config = TransportConfig::default();

    assert_eq!(RAKNET_PROTOCOL_VERSION, 11);
    assert_eq!(config.supported_protocols, vec![11]);
}
```

Create the minimal `crates/ardosia-network/src/lib.rs`:

```rust
//! Ardosia's transport-only networking layer.
```

Create `crates/ardosia-network/Cargo.toml` referencing the not-yet-present path:

```toml
[package]
name = "ardosia-network"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
publish = false

[dependencies]
bytes.workspace = true
thiserror.workspace = true
tokio.workspace = true
raknet-rust.workspace = true
```

- [ ] **Step 2: Run the smoke test and verify it fails because the vendor path/workspace is absent**

Run:

```bash
cargo test -p ardosia-network --test vendor_smoke
```

Expected: FAIL before compilation because the workspace/path dependency is not yet resolvable.

- [ ] **Step 3: Create the workspace manifest and ignore rules**

Create root `Cargo.toml`:

```toml
[workspace]
members = [
    "crates/ardosia-network",
    "crates/ardosia-loadgen",
]
exclude = ["vendor/raknet-rust"]
resolver = "3"

[workspace.package]
edition = "2024"
rust-version = "1.85"

[workspace.dependencies]
bytes = "1.8"
thiserror = "2.0"
tokio = { version = "1.48", features = ["macros", "rt-multi-thread", "net", "sync", "time", "signal"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
clap = { version = "4.5", features = ["derive"] }
raknet-rust = { path = "vendor/raknet-rust" }
```

Because the workspace lists `crates/ardosia-loadgen`, also create its initial manifest and placeholder main so Cargo can resolve the workspace:

`crates/ardosia-loadgen/Cargo.toml`:

```toml
[package]
name = "ardosia-loadgen"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
publish = false

[dependencies]
```

`crates/ardosia-loadgen/src/main.rs`:

```rust
fn main() {}
```

Create `.gitignore`:

```gitignore
/target/
/results/
*.profraw
```

- [ ] **Step 4: Export the pinned upstream commit without `.git` metadata**

Run from the repository root:

```bash
rm -rf /tmp/ardosia-raknet-source /tmp/ardosia-raknet-export

git clone --filter=blob:none https://github.com/mcbe-rs/raknet-rust.git /tmp/ardosia-raknet-source
git -C /tmp/ardosia-raknet-source checkout 3edfb4170e6cb5aeed992b09b50176fb7e5b6079

mkdir -p /tmp/ardosia-raknet-export vendor/raknet-rust
git -C /tmp/ardosia-raknet-source archive 3edfb4170e6cb5aeed992b09b50176fb7e5b6079 \
  | tar -x -C /tmp/ardosia-raknet-export
cp -a /tmp/ardosia-raknet-export/. vendor/raknet-rust/
```

- [ ] **Step 5: Add provenance without changing upstream source behavior**

Create `vendor/raknet-rust/UPSTREAM.md`:

```markdown
# Upstream provenance

Repository: https://github.com/mcbe-rs/raknet-rust
Commit: 3edfb4170e6cb5aeed992b09b50176fb7e5b6079
Crate version: 0.2.0
License: Apache-2.0
Vendored for: ardosia-network

## Ardosia-local behavioral changes

None. The initial snapshot is source-identical to the pinned upstream commit.
```

- [ ] **Step 6: Verify the vendored snapshot matches upstream except for `UPSTREAM.md`**

Run:

```bash
diff -ru --exclude=UPSTREAM.md /tmp/ardosia-raknet-export vendor/raknet-rust
```

Expected: no output and exit status 0.

Then verify the upstream license exists:

```bash
test -f vendor/raknet-rust/LICENSE
```

Expected: exit status 0.

- [ ] **Step 7: Run the vendor smoke test and upstream library tests**

Run:

```bash
cargo test -p ardosia-network --test vendor_smoke
cargo test --manifest-path vendor/raknet-rust/Cargo.toml --lib
```

Expected: both commands PASS.

- [ ] **Step 8: Commit the reproducible vendor baseline**

```bash
git add Cargo.toml .gitignore crates/ vendor/raknet-rust
git commit -m "build: vendor pinned raknet-rust baseline"
```

---

### Task 2: Define Ardosia configuration, errors, reliability, and metrics without leaking vendor types

**Files:**
- Modify: `crates/ardosia-network/src/lib.rs`
- Create: `crates/ardosia-network/src/config.rs`
- Create: `crates/ardosia-network/src/error.rs`
- Create: `crates/ardosia-network/src/reliability.rs`
- Create: `crates/ardosia-network/src/metrics.rs`

**Interfaces:**
- Consumes: `raknet_rust::low_level::transport::TransportConfig` and vendor `Reliability` privately.
- Produces:
  - `NetworkConfig { bind_addr, raknet_protocols, max_connections }`
  - `NetworkError`
  - `Reliability`
  - `NetworkMetrics`
  - crate-private `NetworkConfig::to_vendor_transport_config()`
  - crate-private `Reliability::into_vendor()`

- [ ] **Step 1: Write failing configuration and reliability tests**

Create `crates/ardosia-network/src/config.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol8_config_maps_to_vendor_supported_protocols() {
        let config = NetworkConfig {
            bind_addr: "127.0.0.1:19132".parse().unwrap(),
            raknet_protocols: vec![8],
            max_connections: 512,
        };

        let vendor = config.to_vendor_transport_config().unwrap();
        assert_eq!(vendor.supported_protocols, vec![8]);
        assert_eq!(vendor.max_sessions, 512);
        assert_eq!(vendor.bind_addr, config.bind_addr);
    }

    #[test]
    fn empty_protocol_list_is_rejected() {
        let config = NetworkConfig {
            bind_addr: "127.0.0.1:19132".parse().unwrap(),
            raknet_protocols: vec![],
            max_connections: 512,
        };

        assert!(matches!(
            config.validate(),
            Err(NetworkError::InvalidConfig { field: "raknet_protocols", .. })
        ));
    }

    #[test]
    fn zero_max_connections_is_rejected() {
        let config = NetworkConfig {
            bind_addr: "127.0.0.1:19132".parse().unwrap(),
            raknet_protocols: vec![8],
            max_connections: 0,
        };

        assert!(matches!(
            config.validate(),
            Err(NetworkError::InvalidConfig { field: "max_connections", .. })
        ));
    }
}
```

Create `crates/ardosia-network/src/reliability.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use raknet_rust::low_level::protocol::reliability::Reliability as VendorReliability;

    #[test]
    fn all_public_reliability_modes_map_exactly() {
        assert_eq!(Reliability::Unreliable.into_vendor(), VendorReliability::Unreliable);
        assert_eq!(
            Reliability::UnreliableSequenced.into_vendor(),
            VendorReliability::UnreliableSequenced
        );
        assert_eq!(Reliability::Reliable.into_vendor(), VendorReliability::Reliable);
        assert_eq!(
            Reliability::ReliableOrdered.into_vendor(),
            VendorReliability::ReliableOrdered
        );
        assert_eq!(
            Reliability::ReliableSequenced.into_vendor(),
            VendorReliability::ReliableSequenced
        );
    }
}
```

- [ ] **Step 2: Run the focused tests and verify they fail**

Run:

```bash
cargo test -p ardosia-network config::tests reliability::tests
```

Expected: FAIL because the production types/functions are not defined.

- [ ] **Step 3: Implement typed public errors**

Create `crates/ardosia-network/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("invalid network configuration field {field}: {message}")]
    InvalidConfig {
        field: &'static str,
        message: String,
    },

    #[error("network I/O failed: {0}")]
    Io(#[from] std::io::Error),

    #[error("connection is closed")]
    ConnectionClosed,

    #[error("connection was closed because its inbound queue exceeded the configured bound")]
    Backpressure,

    #[error("network backend stopped")]
    BackendStopped,
}
```

- [ ] **Step 4: Implement `NetworkConfig` and private vendor conversion**

Create `crates/ardosia-network/src/config.rs` above its test module:

```rust
use std::net::SocketAddr;

use raknet_rust::low_level::transport::TransportConfig;

use crate::NetworkError;

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub bind_addr: SocketAddr,
    pub raknet_protocols: Vec<u8>,
    pub max_connections: usize,
}

impl NetworkConfig {
    pub fn validate(&self) -> Result<(), NetworkError> {
        if self.raknet_protocols.is_empty() {
            return Err(NetworkError::InvalidConfig {
                field: "raknet_protocols",
                message: "must contain at least one RakNet protocol version".into(),
            });
        }
        if self.max_connections == 0 {
            return Err(NetworkError::InvalidConfig {
                field: "max_connections",
                message: "must be at least 1".into(),
            });
        }
        Ok(())
    }

    pub(crate) fn to_vendor_transport_config(&self) -> Result<TransportConfig, NetworkError> {
        self.validate()?;
        let mut config = TransportConfig::default();
        config.bind_addr = self.bind_addr;
        config.supported_protocols = self.raknet_protocols.clone();
        config.max_sessions = self.max_connections;
        config.validate().map_err(|error| NetworkError::InvalidConfig {
            field: "vendor_transport",
            message: error.to_string(),
        })?;
        Ok(config)
    }
}
```

Do not alter vendor defaults unrelated to Ardosia requirements in this task.

- [ ] **Step 5: Implement the public reliability enum and private mapping**

Create `crates/ardosia-network/src/reliability.rs` above its tests:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reliability {
    Unreliable,
    UnreliableSequenced,
    Reliable,
    ReliableOrdered,
    ReliableSequenced,
}

impl Reliability {
    pub(crate) fn into_vendor(
        self,
    ) -> raknet_rust::low_level::protocol::reliability::Reliability {
        use raknet_rust::low_level::protocol::reliability::Reliability as Vendor;

        match self {
            Self::Unreliable => Vendor::Unreliable,
            Self::UnreliableSequenced => Vendor::UnreliableSequenced,
            Self::Reliable => Vendor::Reliable,
            Self::ReliableOrdered => Vendor::ReliableOrdered,
            Self::ReliableSequenced => Vendor::ReliableSequenced,
        }
    }
}
```

- [ ] **Step 6: Implement a cheap baseline metrics facade**

Create `crates/ardosia-network/src/metrics.rs`:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NetworkMetrics {
    pub accepted_total: u64,
    pub connected_current: u64,
    pub disconnected_total: u64,
    pub protocol_errors_total: u64,
    pub backpressure_disconnects_total: u64,
}

#[derive(Default)]
pub(crate) struct MetricsState {
    accepted_total: AtomicU64,
    connected_current: AtomicU64,
    disconnected_total: AtomicU64,
    protocol_errors_total: AtomicU64,
    backpressure_disconnects_total: AtomicU64,
}

impl MetricsState {
    pub(crate) fn snapshot(&self) -> NetworkMetrics {
        NetworkMetrics {
            accepted_total: self.accepted_total.load(Ordering::Relaxed),
            connected_current: self.connected_current.load(Ordering::Relaxed),
            disconnected_total: self.disconnected_total.load(Ordering::Relaxed),
            protocol_errors_total: self.protocol_errors_total.load(Ordering::Relaxed),
            backpressure_disconnects_total: self
                .backpressure_disconnects_total
                .load(Ordering::Relaxed),
        }
    }

    pub(crate) fn connected(&self) {
        self.accepted_total.fetch_add(1, Ordering::Relaxed);
        self.connected_current.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn disconnected(&self) {
        self.disconnected_total.fetch_add(1, Ordering::Relaxed);
        self.connected_current.fetch_sub(1, Ordering::Relaxed);
    }

    pub(crate) fn protocol_error(&self) {
        self.protocol_errors_total.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn backpressure_disconnect(&self) {
        self.backpressure_disconnects_total
            .fetch_add(1, Ordering::Relaxed);
    }
}
```

- [ ] **Step 7: Export only Ardosia-owned public types**

Replace `crates/ardosia-network/src/lib.rs` with:

```rust
//! Transport-only networking for Ardosia.

mod config;
mod error;
mod metrics;
mod reliability;

pub use config::NetworkConfig;
pub use error::NetworkError;
pub use metrics::NetworkMetrics;
pub use reliability::Reliability;
```

No `pub use raknet_rust::...` statement is allowed.

- [ ] **Step 8: Run tests and commit**

Run:

```bash
cargo test -p ardosia-network config::tests reliability::tests
cargo test -p ardosia-network --test vendor_smoke
```

Expected: PASS.

Commit:

```bash
git add crates/ardosia-network
git commit -m "feat: define stable Ardosia transport types"
```

---

### Task 3: Implement the bounded backend actor, `NetworkServer`, and independent `Connection` handles

**Files:**
- Modify: `crates/ardosia-network/src/lib.rs`
- Create: `crates/ardosia-network/src/backend.rs`
- Create: `crates/ardosia-network/src/server.rs`
- Create: `crates/ardosia-network/src/connection.rs`
- Create: `crates/ardosia-network/tests/transport.rs`

**Interfaces:**
- Consumes:
  - `NetworkConfig::to_vendor_transport_config()`
  - `Reliability::into_vendor()`
  - vendor `RaknetServerBuilder`, `RaknetServerEvent`, `PeerId`, `SendOptions` privately.
- Produces:
  - `NetworkServer::bind(config) -> Result<NetworkServer, NetworkError>`
  - `NetworkServer::accept(&mut self) -> Result<Connection, NetworkError>`
  - `NetworkServer::metrics() -> NetworkMetrics`
  - `NetworkServer::shutdown(self) -> Result<(), NetworkError>`
  - `Connection::peer_addr() -> SocketAddr`
  - `Connection::recv(&mut self) -> Result<Bytes, NetworkError>`
  - `Connection::send(&self, Bytes, Reliability) -> Result<(), NetworkError>`
  - `Connection::close(&self) -> Result<(), NetworkError>`

- [ ] **Step 1: Write a failing real-UDP acceptance/payload integration test**

Create `crates/ardosia-network/tests/transport.rs`:

```rust
use std::net::SocketAddr;
use std::time::Duration;

use ardosia_network::{NetworkConfig, NetworkServer, Reliability};
use bytes::Bytes;
use raknet_rust::client::{ClientSendOptions, RaknetClient, RaknetClientConfig};
use raknet_rust::low_level::protocol::reliability::Reliability as VendorReliability;
use tokio::time::timeout;

fn allocate_loopback_addr() -> SocketAddr {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap()
}

fn protocol8_client_config() -> RaknetClientConfig {
    let mut config = RaknetClientConfig::default();
    config.protocol_version = 8;
    config
}

#[tokio::test]
async fn accepted_connection_routes_reliable_ordered_payload_both_directions() {
    let addr = allocate_loopback_addr();
    let mut server = NetworkServer::bind(NetworkConfig {
        bind_addr: addr,
        raknet_protocols: vec![8],
        max_connections: 32,
    })
    .await
    .unwrap();

    let mut client = RaknetClient::connect_with_config(addr, protocol8_client_config())
        .await
        .unwrap();
    let mut connection = timeout(Duration::from_secs(2), server.accept())
        .await
        .unwrap()
        .unwrap();

    client
        .send_with_options(
            Bytes::from_static(b"client-to-server"),
            ClientSendOptions {
                reliability: VendorReliability::ReliableOrdered,
                ..ClientSendOptions::default()
            },
        )
        .await
        .unwrap();

    let received = timeout(Duration::from_secs(2), connection.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received, Bytes::from_static(b"client-to-server"));

    connection
        .send(
            Bytes::from_static(b"server-to-client"),
            Reliability::ReliableOrdered,
        )
        .await
        .unwrap();

    let packet = timeout(Duration::from_secs(2), async {
        loop {
            if let Some(raknet_rust::client::RaknetClientEvent::Packet { payload, .. }) =
                client.next_event().await
            {
                break payload;
            }
        }
    })
    .await
    .unwrap();

    assert_eq!(packet, Bytes::from_static(b"server-to-client"));

    client.disconnect(None).await.unwrap();
    server.shutdown().await.unwrap();
}
```

- [ ] **Step 2: Run the integration test and verify it fails because server/connection APIs do not exist**

Run:

```bash
cargo test -p ardosia-network --test transport accepted_connection_routes_reliable_ordered_payload_both_directions -- --nocapture
```

Expected: FAIL at compile time for missing `NetworkServer`/`Connection` APIs.

- [ ] **Step 3: Define private backend command/event types**

Create `crates/ardosia-network/src/backend.rs` with these core internal types:

```rust
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use raknet_rust::server::{PeerId, RaknetServer, RaknetServerEvent, SendOptions};
use tokio::sync::{mpsc, oneshot};

use crate::{MetricsState, NetworkError, Reliability};
use crate::connection::Connection;

pub(crate) const COMMAND_QUEUE_CAPACITY: usize = 4096;
pub(crate) const PER_CONNECTION_INBOUND_CAPACITY: usize = 1024;

pub(crate) enum BackendCommand {
    Send {
        peer_id: PeerId,
        payload: Bytes,
        reliability: Reliability,
        response: oneshot::Sender<Result<(), NetworkError>>,
    },
    Disconnect {
        peer_id: PeerId,
        response: oneshot::Sender<Result<(), NetworkError>>,
    },
    Shutdown {
        response: oneshot::Sender<Result<(), NetworkError>>,
    },
}

pub(crate) struct PeerState {
    pub inbound: mpsc::Sender<Bytes>,
}
```

Use a bounded `mpsc` command channel and bounded per-peer inbound channels. Do not use `unbounded_channel`.

- [ ] **Step 4: Implement the backend event loop with one owner of `RaknetServer`**

Implement:

```rust
pub(crate) async fn run_backend(
    mut server: RaknetServer,
    mut commands: mpsc::Receiver<BackendCommand>,
    accept_tx: mpsc::Sender<Result<Connection, NetworkError>>,
    command_tx: mpsc::Sender<BackendCommand>,
    metrics: Arc<MetricsState>,
) {
    let mut peers: HashMap<PeerId, PeerState> = HashMap::new();

    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else {
                    let _ = server.shutdown().await;
                    break;
                };
                if handle_command(&mut server, command).await {
                    break;
                }
            }
            event = server.next_event() => {
                let Some(event) = event else {
                    break;
                };
                handle_event(
                    &mut server,
                    event,
                    &mut peers,
                    &accept_tx,
                    &command_tx,
                    &metrics,
                ).await;
            }
        }
    }
}
```

`handle_command` must:

- map Ardosia reliability privately into vendor `SendOptions { reliability, ..Default::default() }`;
- send the actual upstream result through the command's oneshot;
- call `RaknetServer::disconnect` for explicit close;
- on shutdown, call `RaknetServer::shutdown`, answer the oneshot, and return `true` to stop the loop.

`handle_event` must:

- on `PeerConnected`, create a bounded per-peer inbound queue, insert it into `peers`, increment metrics, create a `Connection`, and send it through the bounded accept queue;
- on `Packet`, route `payload` to the matching peer using `try_send`;
- on a full peer queue, increment `backpressure_disconnects_total`, remove that peer sender, and request `server.disconnect(peer_id)` so one slow application consumer cannot stall the event loop;
- on `PeerDisconnected`, remove the peer and decrement `connected_current` exactly once;
- on `DecodeError`, increment `protocol_errors_total`;
- ignore non-fatal offline/receipt/metrics events in this baseline without exposing them publicly;
- if the accept queue is closed, request server shutdown rather than accumulating accepted connections nobody can consume.

Do not await on a per-peer inbound queue from inside the central event loop.

- [ ] **Step 5: Implement `Connection` as a vendor-hidden handle**

Create `crates/ardosia-network/src/connection.rs`:

```rust
use std::net::SocketAddr;

use bytes::Bytes;
use raknet_rust::server::PeerId;
use tokio::sync::{mpsc, oneshot};

use crate::backend::BackendCommand;
use crate::{NetworkError, Reliability};

pub struct Connection {
    peer_id: PeerId,
    peer_addr: SocketAddr,
    inbound: mpsc::Receiver<Bytes>,
    commands: mpsc::Sender<BackendCommand>,
}

impl Connection {
    pub(crate) fn new(
        peer_id: PeerId,
        peer_addr: SocketAddr,
        inbound: mpsc::Receiver<Bytes>,
        commands: mpsc::Sender<BackendCommand>,
    ) -> Self {
        Self { peer_id, peer_addr, inbound, commands }
    }

    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    pub async fn recv(&mut self) -> Result<Bytes, NetworkError> {
        self.inbound.recv().await.ok_or(NetworkError::ConnectionClosed)
    }

    pub async fn send(
        &self,
        payload: Bytes,
        reliability: Reliability,
    ) -> Result<(), NetworkError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.commands
            .send(BackendCommand::Send {
                peer_id: self.peer_id,
                payload,
                reliability,
                response: response_tx,
            })
            .await
            .map_err(|_| NetworkError::BackendStopped)?;
        response_rx.await.map_err(|_| NetworkError::BackendStopped)?
    }

    pub async fn close(&self) -> Result<(), NetworkError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.commands
            .send(BackendCommand::Disconnect {
                peer_id: self.peer_id,
                response: response_tx,
            })
            .await
            .map_err(|_| NetworkError::BackendStopped)?;
        response_rx.await.map_err(|_| NetworkError::BackendStopped)?
    }
}
```

The private `PeerId` field is an implementation detail; no public signature may mention it.

- [ ] **Step 6: Implement `NetworkServer` and start the backend task**

Create `crates/ardosia-network/src/server.rs`:

```rust
use std::sync::Arc;

use raknet_rust::server::RaknetServer;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::backend::{run_backend, BackendCommand, COMMAND_QUEUE_CAPACITY};
use crate::connection::Connection;
use crate::{MetricsState, NetworkConfig, NetworkError, NetworkMetrics};

pub struct NetworkServer {
    accept_rx: mpsc::Receiver<Result<Connection, NetworkError>>,
    commands: mpsc::Sender<BackendCommand>,
    metrics: Arc<MetricsState>,
    backend: JoinHandle<()>,
}

impl NetworkServer {
    pub async fn bind(config: NetworkConfig) -> Result<Self, NetworkError> {
        let transport = config.to_vendor_transport_config()?;
        let vendor = RaknetServer::builder()
            .transport_config(transport)
            .start()
            .await?;

        let (accept_tx, accept_rx) = mpsc::channel(config.max_connections);
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_CAPACITY);
        let metrics = Arc::new(MetricsState::default());

        let backend = tokio::spawn(run_backend(
            vendor,
            command_rx,
            accept_tx,
            command_tx.clone(),
            metrics.clone(),
        ));

        Ok(Self {
            accept_rx,
            commands: command_tx,
            metrics,
            backend,
        })
    }

    pub async fn accept(&mut self) -> Result<Connection, NetworkError> {
        self.accept_rx.recv().await.ok_or(NetworkError::BackendStopped)?
    }

    pub fn metrics(&self) -> NetworkMetrics {
        self.metrics.snapshot()
    }

    pub async fn shutdown(self) -> Result<(), NetworkError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.commands
            .send(BackendCommand::Shutdown { response: response_tx })
            .await
            .map_err(|_| NetworkError::BackendStopped)?;
        let result = response_rx.await.map_err(|_| NetworkError::BackendStopped)?;
        self.backend.await.map_err(|_| NetworkError::BackendStopped)?;
        result
    }
}
```

If exact ownership required by `RaknetServer::shutdown(self)` forces a small internal restructuring of `handle_command`, preserve the public signatures above and keep `RaknetServer` owned by exactly one task.

- [ ] **Step 7: Export the server and connection APIs**

Update `crates/ardosia-network/src/lib.rs`:

```rust
//! Transport-only networking for Ardosia.

mod backend;
mod config;
mod connection;
mod error;
mod metrics;
mod reliability;
mod server;

pub use config::NetworkConfig;
pub use connection::Connection;
pub use error::NetworkError;
pub use metrics::NetworkMetrics;
pub use reliability::Reliability;
pub use server::NetworkServer;

pub(crate) use metrics::MetricsState;
```

- [ ] **Step 8: Run the real-UDP test and fix only implementation defects in Ardosia-owned code**

Run:

```bash
cargo test -p ardosia-network --test transport accepted_connection_routes_reliable_ordered_payload_both_directions -- --nocapture
```

Expected: PASS.

If it fails because an upstream API signature differs from the plan snippet, adapt the private wrapper to the pinned upstream API. Do not change the public Ardosia boundary and do not patch vendor source merely to fit the wrapper.

- [ ] **Step 9: Run all Ardosia network tests and commit**

Run:

```bash
cargo test -p ardosia-network
```

Expected: PASS.

Commit:

```bash
git add crates/ardosia-network
git commit -m "feat: add bounded RakNet server and connection facade"
```

---

### Task 4: Prove RakNet protocol 8 with independent raw offline handshake bytes

**Files:**
- Create: `crates/ardosia-network/tests/protocol8.rs`

**Interfaces:**
- Consumes: public `NetworkServer`/`NetworkConfig` and raw UDP only for offline request fixtures.
- Produces: regression coverage that protocol 8 is accepted and unsupported version 11 is rejected without relying on vendor client encode/decode for the raw fixture.

- [ ] **Step 1: Write raw fixture helpers with literal RakNet IDs/magic**

Create `crates/ardosia-network/tests/protocol8.rs`:

```rust
use std::net::SocketAddr;
use std::time::Duration;

use ardosia_network::{NetworkConfig, NetworkServer};
use tokio::net::UdpSocket;
use tokio::time::timeout;

const ID_OPEN_CONNECTION_REQUEST_1: u8 = 0x05;
const ID_OPEN_CONNECTION_REPLY_1: u8 = 0x06;
const ID_INCOMPATIBLE_PROTOCOL_VERSION: u8 = 0x19;
const MAGIC: [u8; 16] = [
    0x00, 0xff, 0xff, 0x00, 0xfe, 0xfe, 0xfe, 0xfe,
    0xfd, 0xfd, 0xfd, 0xfd, 0x12, 0x34, 0x56, 0x78,
];

fn allocate_loopback_addr() -> SocketAddr {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap()
}

fn raw_request1(protocol_version: u8, mtu: usize) -> Vec<u8> {
    let mut packet = Vec::with_capacity(mtu);
    packet.push(ID_OPEN_CONNECTION_REQUEST_1);
    packet.extend_from_slice(&MAGIC);
    packet.push(protocol_version);
    packet.resize(mtu, 0);
    packet
}

async fn first_reply_id(addr: SocketAddr, protocol_version: u8) -> u8 {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket.send_to(&raw_request1(protocol_version, 1200), addr).await.unwrap();

    let mut buffer = [0u8; 2048];
    let (len, _) = timeout(Duration::from_secs(2), socket.recv_from(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    assert!(len > 0);
    buffer[0]
}
```

- [ ] **Step 2: Add the protocol-8 acceptance and version-11 rejection tests**

Append:

```rust
#[tokio::test]
async fn raw_request1_protocol8_is_accepted() {
    let addr = allocate_loopback_addr();
    let server = NetworkServer::bind(NetworkConfig {
        bind_addr: addr,
        raknet_protocols: vec![8],
        max_connections: 32,
    })
    .await
    .unwrap();

    assert_eq!(first_reply_id(addr, 8).await, ID_OPEN_CONNECTION_REPLY_1);
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn raw_request1_protocol11_is_rejected_when_only_8_is_supported() {
    let addr = allocate_loopback_addr();
    let server = NetworkServer::bind(NetworkConfig {
        bind_addr: addr,
        raknet_protocols: vec![8],
        max_connections: 32,
    })
    .await
    .unwrap();

    assert_eq!(
        first_reply_id(addr, 11).await,
        ID_INCOMPATIBLE_PROTOCOL_VERSION
    );
    server.shutdown().await.unwrap();
}
```

- [ ] **Step 3: Run the tests and verify protocol behavior**

Run:

```bash
cargo test -p ardosia-network --test protocol8 -- --nocapture
```

Expected: both tests PASS with vendor source unchanged.

- [ ] **Step 4: Add a full connected protocol-8 client regression test**

Append imports:

```rust
use raknet_rust::client::{RaknetClient, RaknetClientConfig};
```

Append test:

```rust
#[tokio::test]
async fn vendor_client_configured_for_protocol8_reaches_ardosia_accept() {
    let addr = allocate_loopback_addr();
    let mut server = NetworkServer::bind(NetworkConfig {
        bind_addr: addr,
        raknet_protocols: vec![8],
        max_connections: 32,
    })
    .await
    .unwrap();

    let mut client_config = RaknetClientConfig::default();
    client_config.protocol_version = 8;
    let mut client = RaknetClient::connect_with_config(addr, client_config)
        .await
        .unwrap();

    let connection = timeout(Duration::from_secs(2), server.accept())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(connection.peer_addr().ip(), client.local_addr().unwrap().ip());

    client.disconnect(None).await.unwrap();
    server.shutdown().await.unwrap();
}
```

- [ ] **Step 5: Run protocol tests and commit**

Run:

```bash
cargo test -p ardosia-network --test protocol8 -- --nocapture
```

Expected: PASS.

Commit:

```bash
git add crates/ardosia-network/tests/protocol8.rs
git commit -m "test: lock RakNet protocol 8 compatibility"
```

---

### Task 5: Lock fragmentation and disconnect lifecycle behavior

**Files:**
- Modify: `crates/ardosia-network/tests/transport.rs`
- Modify only if tests expose wrapper defects: `crates/ardosia-network/src/backend.rs`, `connection.rs`, `server.rs`, `metrics.rs`

**Interfaces:**
- Consumes: Task 3 public transport APIs.
- Produces: regression coverage for >MTU payload reassembly and session cleanup.

- [ ] **Step 1: Add a failing 4096-byte fragmented payload test**

Append to `tests/transport.rs`:

```rust
#[tokio::test]
async fn fragmented_reliable_ordered_payload_reassembles_before_delivery() {
    let addr = allocate_loopback_addr();
    let mut server = NetworkServer::bind(NetworkConfig {
        bind_addr: addr,
        raknet_protocols: vec![8],
        max_connections: 32,
    })
    .await
    .unwrap();

    let mut client = RaknetClient::connect_with_config(addr, protocol8_client_config())
        .await
        .unwrap();
    let mut connection = timeout(Duration::from_secs(2), server.accept())
        .await
        .unwrap()
        .unwrap();

    let payload = Bytes::from(vec![0x5a; 4096]);
    client
        .send_with_options(
            payload.clone(),
            ClientSendOptions {
                reliability: VendorReliability::ReliableOrdered,
                ..ClientSendOptions::default()
            },
        )
        .await
        .unwrap();

    let received = timeout(Duration::from_secs(3), connection.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received, payload);

    client.disconnect(None).await.unwrap();
    server.shutdown().await.unwrap();
}
```

- [ ] **Step 2: Run the fragmentation test**

Run:

```bash
cargo test -p ardosia-network --test transport fragmented_reliable_ordered_payload_reassembles_before_delivery -- --nocapture
```

Expected: PASS without vendor changes. If it fails, first confirm the Ardosia wrapper is not truncating/altering `Bytes`; only classify it as an upstream compatibility problem after isolating the failure with a direct pinned-vendor reproduction.

- [ ] **Step 3: Add a lifecycle metrics test**

Append:

```rust
#[tokio::test]
async fn clean_client_disconnect_releases_connected_session_metric() {
    let addr = allocate_loopback_addr();
    let mut server = NetworkServer::bind(NetworkConfig {
        bind_addr: addr,
        raknet_protocols: vec![8],
        max_connections: 32,
    })
    .await
    .unwrap();

    let mut client = RaknetClient::connect_with_config(addr, protocol8_client_config())
        .await
        .unwrap();
    let _connection = timeout(Duration::from_secs(2), server.accept())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(server.metrics().connected_current, 1);
    client.disconnect(None).await.unwrap();

    timeout(Duration::from_secs(2), async {
        loop {
            if server.metrics().connected_current == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    assert_eq!(server.metrics().disconnected_total, 1);
    server.shutdown().await.unwrap();
}
```

- [ ] **Step 4: Run the full transport suite**

Run:

```bash
cargo test -p ardosia-network --test transport -- --nocapture
```

Expected: all transport tests PASS.

- [ ] **Step 5: Commit lifecycle coverage**

```bash
git add crates/ardosia-network
git commit -m "test: cover fragmentation and session lifecycle"
```

---

### Task 6: Define and validate the declarative `connect-300` scenario

**Files:**
- Modify: `crates/ardosia-loadgen/Cargo.toml`
- Create: `crates/ardosia-loadgen/src/scenario.rs`
- Create: `crates/ardosia-loadgen/tests/scenario.rs`
- Create: `scenarios/connect-300.toml`

**Interfaces:**
- Consumes: TOML text/file.
- Produces:
  - `Scenario::from_str(&str) -> Result<Scenario, ScenarioError>`
  - fields `name`, `clients`, `protocol_version`, `ramp_up_seconds`, `hold_seconds`, `connect_timeout_seconds`
  - validation guaranteeing clients > 0, hold > 0, connect timeout > 0.

- [ ] **Step 1: Write failing scenario parsing tests**

Create `crates/ardosia-loadgen/tests/scenario.rs`:

```rust
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

    assert_eq!(scenario.name, "connect-300");
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
```

- [ ] **Step 2: Run tests and verify they fail because the loadgen library module does not exist**

Run:

```bash
cargo test -p ardosia-loadgen --test scenario
```

Expected: FAIL at compile time.

- [ ] **Step 3: Convert `ardosia-loadgen` into a library + binary crate with parsing dependencies**

Replace `crates/ardosia-loadgen/Cargo.toml`:

```toml
[package]
name = "ardosia-loadgen"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
publish = false

[dependencies]
ardosia-network = { path = "../ardosia-network" }
clap.workspace = true
raknet-rust.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
toml.workspace = true
thiserror.workspace = true
```

Create `crates/ardosia-loadgen/src/lib.rs`:

```rust
pub mod report;
pub mod runner;
pub mod scenario;
```

The `report` and `runner` modules may initially contain only module-level documentation comments so this task compiles; their behavior is implemented in Task 7.

- [ ] **Step 4: Implement exact scenario parsing and validation**

Create `crates/ardosia-loadgen/src/scenario.rs`:

```rust
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Scenario {
    pub name: String,
    pub clients: usize,
    pub protocol_version: u8,
    pub ramp_up_seconds: u64,
    pub hold_seconds: u64,
    pub connect_timeout_seconds: u64,
}

#[derive(Debug, Error)]
pub enum ScenarioError {
    #[error("invalid TOML scenario: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid scenario field {field}: {message}")]
    Invalid {
        field: &'static str,
        message: &'static str,
    },
}

impl Scenario {
    pub fn from_str(input: &str) -> Result<Self, ScenarioError> {
        let scenario: Self = toml::from_str(input)?;
        scenario.validate()?;
        Ok(scenario)
    }

    pub fn validate(&self) -> Result<(), ScenarioError> {
        if self.clients == 0 {
            return Err(ScenarioError::Invalid {
                field: "clients",
                message: "must be at least 1",
            });
        }
        if self.hold_seconds == 0 {
            return Err(ScenarioError::Invalid {
                field: "hold_seconds",
                message: "must be at least 1",
            });
        }
        if self.connect_timeout_seconds == 0 {
            return Err(ScenarioError::Invalid {
                field: "connect_timeout_seconds",
                message: "must be at least 1",
            });
        }
        Ok(())
    }
}
```

- [ ] **Step 5: Check in the exact baseline scenario**

Create `scenarios/connect-300.toml`:

```toml
name = "connect-300"
clients = 300
protocol_version = 8
ramp_up_seconds = 10
hold_seconds = 60
connect_timeout_seconds = 5
```

- [ ] **Step 6: Run scenario tests and commit**

Run:

```bash
cargo test -p ardosia-loadgen --test scenario
```

Expected: PASS.

Commit:

```bash
git add crates/ardosia-loadgen scenarios/connect-300.toml
git commit -m "feat: add declarative connect-300 scenario"
```

---

### Task 7: Implement the protocol-8 load runner, acceptance report, and local/external CLI

**Files:**
- Modify: `crates/ardosia-loadgen/src/main.rs`
- Create/replace: `crates/ardosia-loadgen/src/runner.rs`
- Create/replace: `crates/ardosia-loadgen/src/report.rs`
- Add unit tests in: `crates/ardosia-loadgen/src/report.rs`

**Interfaces:**
- Consumes:
  - `Scenario`
  - target `SocketAddr`
  - vendor `RaknetClientConfig { protocol_version }`
  - optional in-process `NetworkServer`.
- Produces:
  - `run_clients(target, scenario) -> RunReport`
  - JSON-serializable `RunReport`
  - CLI subcommands:
    - `ardosia-loadgen local --scenario <path> [--json <path>]`
    - `ardosia-loadgen serve --bind <addr> --protocol 8 --max-connections <n>`
    - `ardosia-loadgen run --target <addr> --scenario <path> [--json <path>]`

- [ ] **Step 1: Write failing acceptance-report tests**

Create `crates/ardosia-loadgen/src/report.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_connect_300_result_passes() {
        let report = RunReport::new(
            "connect-300".into(),
            300,
            300,
            0,
            0,
            0,
            300,
            70_000,
        );
        assert!(report.passed);
        assert!(report.failure_reason.is_none());
    }

    #[test]
    fn unexpected_disconnect_fails_gate() {
        let report = RunReport::new(
            "connect-300".into(),
            300,
            300,
            0,
            1,
            0,
            299,
            70_000,
        );
        assert!(!report.passed);
        assert!(report.failure_reason.unwrap().contains("unexpected disconnect"));
    }
}
```

- [ ] **Step 2: Run the focused tests and verify they fail**

Run:

```bash
cargo test -p ardosia-loadgen report::tests
```

Expected: FAIL because `RunReport` is not defined.

- [ ] **Step 3: Implement the baseline report schema and exact pass/fail gate**

Implement above the tests:

```rust
use serde::{Deserialize, Serialize};

pub const VENDOR_REVISION: &str = "3edfb4170e6cb5aeed992b09b50176fb7e5b6079";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub scenario: String,
    pub requested_clients: usize,
    pub successful_handshakes: usize,
    pub failed_handshakes: usize,
    pub unexpected_disconnects: usize,
    pub protocol_errors: usize,
    pub clean_disconnects: usize,
    pub duration_ms: u128,
    pub vendor_revision: String,
    pub passed: bool,
    pub failure_reason: Option<String>,
}

impl RunReport {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        scenario: String,
        requested_clients: usize,
        successful_handshakes: usize,
        failed_handshakes: usize,
        unexpected_disconnects: usize,
        protocol_errors: usize,
        clean_disconnects: usize,
        duration_ms: u128,
    ) -> Self {
        let failure_reason = if successful_handshakes != requested_clients {
            Some(format!(
                "established {successful_handshakes}/{requested_clients} requested clients"
            ))
        } else if failed_handshakes != 0 {
            Some(format!("{failed_handshakes} handshake(s) failed"))
        } else if unexpected_disconnects != 0 {
            Some(format!("{unexpected_disconnects} unexpected disconnect(s)"))
        } else if protocol_errors != 0 {
            Some(format!("{protocol_errors} protocol/decode error(s)"))
        } else if clean_disconnects != requested_clients {
            Some(format!(
                "only {clean_disconnects}/{requested_clients} clients completed the hold window cleanly"
            ))
        } else {
            None
        };

        Self {
            scenario,
            requested_clients,
            successful_handshakes,
            failed_handshakes,
            unexpected_disconnects,
            protocol_errors,
            clean_disconnects,
            duration_ms,
            vendor_revision: VENDOR_REVISION.into(),
            passed: failure_reason.is_none(),
            failure_reason,
        }
    }
}
```

- [ ] **Step 4: Run report tests and verify they pass**

Run:

```bash
cargo test -p ardosia-loadgen report::tests
```

Expected: PASS.

- [ ] **Step 5: Implement client orchestration that keeps each session actively polled during ramp and hold**

Create `crates/ardosia-loadgen/src/runner.rs` around this state model:

```rust
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use raknet_rust::client::{RaknetClient, RaknetClientConfig, RaknetClientEvent};
use tokio::sync::watch;
use tokio::time::{sleep, timeout};

use crate::report::RunReport;
use crate::scenario::Scenario;

#[derive(Debug, Clone, Copy)]
enum Phase {
    Ramp,
    Hold { deadline: tokio::time::Instant },
    Stop,
}

#[derive(Default)]
struct Counters {
    successful_handshakes: AtomicUsize,
    failed_handshakes: AtomicUsize,
    unexpected_disconnects: AtomicUsize,
    protocol_errors: AtomicUsize,
    clean_disconnects: AtomicUsize,
}

pub async fn run_clients(target: SocketAddr, scenario: &Scenario) -> RunReport {
    let started = Instant::now();
    let counters = Arc::new(Counters::default());
    let (phase_tx, phase_rx) = watch::channel(Phase::Ramp);
    let (ready_tx, mut ready_rx) = tokio::sync::mpsc::channel::<()>(scenario.clients);
    let mut tasks = Vec::with_capacity(scenario.clients);

    for index in 0..scenario.clients {
        let delay = if scenario.clients <= 1 {
            Duration::ZERO
        } else {
            Duration::from_secs_f64(
                scenario.ramp_up_seconds as f64 * index as f64 / (scenario.clients - 1) as f64,
            )
        };
        let scenario = scenario.clone();
        let counters = counters.clone();
        let ready_tx = ready_tx.clone();
        let mut phase_rx = phase_rx.clone();

        tasks.push(tokio::spawn(async move {
            sleep(delay).await;

            let mut config = RaknetClientConfig::default();
            config.protocol_version = scenario.protocol_version;

            let connect = timeout(
                Duration::from_secs(scenario.connect_timeout_seconds),
                RaknetClient::connect_with_config(target, config),
            )
            .await;

            let mut client = match connect {
                Ok(Ok(client)) => client,
                _ => {
                    counters.failed_handshakes.fetch_add(1, Ordering::Relaxed);
                    return;
                }
            };

            counters.successful_handshakes.fetch_add(1, Ordering::Relaxed);
            let _ = ready_tx.send(()).await;

            loop {
                match *phase_rx.borrow_and_update() {
                    Phase::Ramp => {
                        tokio::select! {
                            changed = phase_rx.changed() => {
                                if changed.is_err() { return; }
                            }
                            event = client.next_event() => {
                                classify_pre_hold_event(event, &counters);
                            }
                        }
                    }
                    Phase::Hold { deadline } => {
                        tokio::select! {
                            _ = tokio::time::sleep_until(deadline) => {
                                if client.disconnect(None).await.is_ok() {
                                    counters.clean_disconnects.fetch_add(1, Ordering::Relaxed);
                                }
                                return;
                            }
                            event = client.next_event() => {
                                if classify_hold_event(event, &counters) {
                                    return;
                                }
                            }
                            changed = phase_rx.changed() => {
                                if changed.is_err() { return; }
                            }
                        }
                    }
                    Phase::Stop => return,
                }
            }
        }));
    }

    drop(ready_tx);

    let ramp_deadline = tokio::time::Instant::now()
        + Duration::from_secs(scenario.ramp_up_seconds + scenario.connect_timeout_seconds + 2);
    let mut ready = 0usize;
    while ready < scenario.clients {
        match timeout(ramp_deadline.saturating_duration_since(tokio::time::Instant::now()), ready_rx.recv()).await {
            Ok(Some(())) => ready += 1,
            _ => break,
        }
    }

    if ready == scenario.clients {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(scenario.hold_seconds);
        let _ = phase_tx.send(Phase::Hold { deadline });
        tokio::time::sleep_until(deadline).await;
    } else {
        let _ = phase_tx.send(Phase::Stop);
    }

    for task in tasks {
        let _ = task.await;
    }

    RunReport::new(
        scenario.name.clone(),
        scenario.clients,
        counters.successful_handshakes.load(Ordering::Relaxed),
        counters.failed_handshakes.load(Ordering::Relaxed),
        counters.unexpected_disconnects.load(Ordering::Relaxed),
        counters.protocol_errors.load(Ordering::Relaxed),
        counters.clean_disconnects.load(Ordering::Relaxed),
        started.elapsed().as_millis(),
    )
}
```

Implement the two classifiers explicitly:

```rust
fn classify_pre_hold_event(event: Option<RaknetClientEvent>, counters: &Counters) {
    match event {
        Some(RaknetClientEvent::DecodeError { .. }) => {
            counters.protocol_errors.fetch_add(1, Ordering::Relaxed);
        }
        Some(RaknetClientEvent::Disconnected { .. }) | None => {
            counters.unexpected_disconnects.fetch_add(1, Ordering::Relaxed);
        }
        _ => {}
    }
}

fn classify_hold_event(event: Option<RaknetClientEvent>, counters: &Counters) -> bool {
    match event {
        Some(RaknetClientEvent::DecodeError { .. }) => {
            counters.protocol_errors.fetch_add(1, Ordering::Relaxed);
            false
        }
        Some(RaknetClientEvent::Disconnected { .. }) | None => {
            counters.unexpected_disconnects.fetch_add(1, Ordering::Relaxed);
            true
        }
        _ => false,
    }
}
```

Important: clients must call `next_event()` while waiting for the common hold start so RakNet keepalive/retransmission ticks continue. A barrier that stops polling clients during the ramp is not acceptable.

- [ ] **Step 6: Implement the benchmark server loop using only the public Ardosia API**

In `runner.rs`, add:

```rust
use ardosia_network::{NetworkConfig, NetworkServer};

pub async fn serve(bind: SocketAddr, protocol: u8, max_connections: usize) -> Result<(), Box<dyn std::error::Error>> {
    let mut server = NetworkServer::bind(NetworkConfig {
        bind_addr: bind,
        raknet_protocols: vec![protocol],
        max_connections,
    })
    .await?;

    loop {
        let mut connection = server.accept().await?;
        tokio::spawn(async move {
            while connection.recv().await.is_ok() {}
        });
    }
}
```

For the baseline there is no synthetic application payload, so the benchmark server only drains any received payload. The later scaling plan will replace this tiny task body with explicit benchmark frame/echo behavior.

- [ ] **Step 7: Implement the CLI with `local`, `serve`, and `run` modes**

Replace `crates/ardosia-loadgen/src/main.rs` with a Clap CLI containing:

```rust
#[derive(clap::Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    Local {
        #[arg(long)]
        scenario: std::path::PathBuf,
        #[arg(long)]
        json: Option<std::path::PathBuf>,
    },
    Serve {
        #[arg(long, default_value = "127.0.0.1:19132")]
        bind: std::net::SocketAddr,
        #[arg(long, default_value_t = 8)]
        protocol: u8,
        #[arg(long, default_value_t = 512)]
        max_connections: usize,
    },
    Run {
        #[arg(long)]
        target: std::net::SocketAddr,
        #[arg(long)]
        scenario: std::path::PathBuf,
        #[arg(long)]
        json: Option<std::path::PathBuf>,
    },
}
```

Factor helpers so `Local`:

1. allocates an unused loopback address;
2. starts `NetworkServer` with `supported_protocols = vec![scenario.protocol_version]` and `max_connections >= scenario.clients`;
3. spawns a task that continuously accepts connections and drains them;
4. calls `run_clients(addr, &scenario)`;
5. shuts down the server;
6. prints the report and writes JSON if requested;
7. exits with status 1 when `report.passed == false`.

`Run` calls the same client runner against an external target.

`Serve` starts the public-API benchmark server and runs until Ctrl-C; structure `serve` so the server's explicit shutdown path is called after the signal rather than simply terminating the process.

Use:

```rust
let json = serde_json::to_string_pretty(&report)?;
println!("{json}");
```

for the machine-readable terminal output in this baseline. The later scaling plan may add a denser human summary in addition to JSON.

- [ ] **Step 8: Run unit tests and a small fast smoke scenario before the 300-client gate**

Create `/tmp/connect-10.toml` locally:

```toml
name = "connect-10-smoke"
clients = 10
protocol_version = 8
ramp_up_seconds = 1
hold_seconds = 2
connect_timeout_seconds = 3
```

Run:

```bash
cargo test -p ardosia-loadgen
cargo run -p ardosia-loadgen -- local --scenario /tmp/connect-10.toml
```

Expected: tests PASS and the smoke report has `passed: true`, 10 successful handshakes, 10 clean disconnects, and zero unexpected disconnects/protocol errors.

- [ ] **Step 9: Commit the load runner**

```bash
git add crates/ardosia-loadgen
git commit -m "feat: add protocol-8 RakNet connection load generator"
```

---

### Task 8: Run the 300-client acceptance gate, document usage, and add lightweight CI

**Files:**
- Create/modify: `README.md`
- Create: `.github/workflows/ci.yml`
- Modify Ardosia-owned implementation files only if the gate reveals a defect backed by a reproducible test.

**Interfaces:**
- Consumes: all baseline implementation tasks.
- Produces: documented reproducible command and verified 300-client baseline; CI covers lightweight correctness but does not run the 60-second load benchmark on every push.

- [ ] **Step 1: Add lightweight CI before claiming repository health**

Create `.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
  pull_request:

jobs:
  rust:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: 1.85.0
          components: rustfmt, clippy
      - name: Format
        run: cargo fmt --all --check
      - name: Clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
      - name: Tests
        run: cargo test --workspace
```

Do not put the 60-second 300-client load run in normal push/PR CI in this baseline.

- [ ] **Step 2: Document the transport boundary and exact baseline commands**

Create `README.md` containing these concrete points:

- `ardosia-network` is UDP/RakNet transport only;
- MCPE packet/game protocol belongs to `ardosia-protocol`;
- vendored upstream repository and exact revision;
- protocol 8 is configured at runtime and upstream default 11 is unchanged;
- public application code depends on Ardosia types, not `raknet-rust` types;
- run unit/integration tests:

```bash
cargo test --workspace
```

- run local 300-client gate:

```bash
cargo run --release -p ardosia-loadgen -- \
  local \
  --scenario scenarios/connect-300.toml \
  --json /tmp/ardosia-connect-300.json
```

- external process mode:

```bash
cargo run --release -p ardosia-loadgen -- \
  serve --bind 127.0.0.1:19132 --protocol 8 --max-connections 512
```

and separately:

```bash
cargo run --release -p ardosia-loadgen -- \
  run --target 127.0.0.1:19132 \
  --scenario scenarios/connect-300.toml \
  --json /tmp/ardosia-connect-300.json
```

- state that localhost results prove transport correctness/headroom on that machine, not production Internet capacity.

- [ ] **Step 3: Run full static and test verification**

Run exactly:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --manifest-path vendor/raknet-rust/Cargo.toml --lib
```

Expected: every command exits 0.

If Clippy exposes intentionally excessive argument count only on `RunReport::new`, keep the narrowly scoped `#[allow(clippy::too_many_arguments)]` already shown; do not globally disable warnings.

- [ ] **Step 4: Run the release-mode `connect-300` gate and preserve the JSON evidence outside Git**

Run:

```bash
cargo run --release -p ardosia-loadgen -- \
  local \
  --scenario scenarios/connect-300.toml \
  --json /tmp/ardosia-connect-300.json
```

Then inspect:

```bash
cat /tmp/ardosia-connect-300.json
```

The gate passes only if the JSON reports all of:

```text
requested_clients        = 300
successful_handshakes    = 300
failed_handshakes        = 0
unexpected_disconnects   = 0
protocol_errors          = 0
clean_disconnects        = 300
passed                   = true
```

A failing gate is not an optimization trigger by itself. First classify the failure as:

1. Ardosia wrapper bug;
2. benchmark harness bug;
3. host/socket-limit problem;
4. pinned vendor correctness/capacity problem.

For categories 1 or 2, add a smaller deterministic failing regression test before changing code. For category 4, create a direct pinned-vendor reproduction before modifying `vendor/raknet-rust`.

- [ ] **Step 5: Re-run verification after any gate-driven fix**

If Step 4 required any source change, run the complete commands from Steps 3 and 4 again from a clean working tree state before claiming the baseline passes.

- [ ] **Step 6: Commit documentation/CI and record the gate result in the commit message body**

```bash
git add README.md .github/workflows/ci.yml
git commit -m "docs: document RakNet baseline benchmark"
```

Do not commit `/tmp/ardosia-connect-300.json` or generated benchmark result files.

---

## Phase-1 Completion Checklist

Before starting the follow-up scaling plan, verify every item explicitly:

- [ ] `vendor/raknet-rust` matches upstream commit `3edfb4170e6cb5aeed992b09b50176fb7e5b6079` except `UPSTREAM.md`.
- [ ] Upstream Apache-2.0 license exists inside the vendor directory.
- [ ] `RAKNET_PROTOCOL_VERSION` remains 11 in vendored source.
- [ ] Ardosia server config explicitly uses `supported_protocols = vec![8]` through the wrapper.
- [ ] Load clients explicitly use `RaknetClientConfig::protocol_version = 8`.
- [ ] No public Ardosia signature contains a `raknet_rust` type.
- [ ] No MCPE game packet/codecs exist in `ardosia-network`.
- [ ] Raw protocol-8 Request1 test passes.
- [ ] Unsupported protocol-11 Request1 rejection test passes.
- [ ] Full protocol-8 connected handshake test passes.
- [ ] Reliable ordered bidirectional payload test passes.
- [ ] 4096-byte fragmented payload test passes.
- [ ] Clean disconnect releases session metrics.
- [ ] All Ardosia queues are bounded.
- [ ] `cargo fmt --all --check` exits 0.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
- [ ] `cargo test --workspace` exits 0.
- [ ] Pinned upstream library tests exit 0.
- [ ] Release-mode `connect-300` reports 300/300 successful handshakes, 300 clean hold-window completions, zero failed handshakes, zero unexpected disconnects, zero protocol errors, and `passed = true`.

Once this checklist is satisfied, write the next implementation plan for mixed traffic (`steady-300`), richer vendor telemetry exposure, process CPU/RSS, RTT histograms, then `steady-500`, churn, `ceiling-1000`, and impairment testing. Vendor optimization is permitted only after those measurements identify a concrete bottleneck.
