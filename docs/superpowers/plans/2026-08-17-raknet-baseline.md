# RakNet Protocol-8 Baseline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first usable `ardosia-network` transport layer around a pinned `raknet-rust` snapshot, prove RakNet protocol 8 compatibility, and pass a repeatable 300-client connection/hold benchmark.

**Architecture:** `ardosia-network` exposes opaque-byte `NetworkServer`/`Connection` APIs and hides all `raknet-rust` public types. One background task owns upstream `RaknetServer`, routes peer events into bounded per-connection queues, and accepts bounded send/disconnect commands from public connection handles. `ardosia-loadgen` uses upstream `RaknetClient` only as a repository-local benchmark client and can run against either an external benchmark server or a local in-process target built exclusively on the public Ardosia API.

**Tech Stack:** Rust 2024, Rust 1.85+, Tokio, `bytes`, `thiserror`, pinned `mcbe-rs/raknet-rust` 0.2.0, Serde, TOML, JSON, Clap.

**Spec:** `docs/superpowers/specs/2026-08-17-raknet-network-design.md`

## Global Constraints

- `ardosia-network` owns UDP and RakNet transport only; MCPE packet/game protocol logic stays out of this repository.
- The upstream snapshot is exactly `mcbe-rs/raknet-rust` commit `3edfb4170e6cb5aeed992b09b50176fb7e5b6079`, crate version `0.2.0`, Apache-2.0.
- Preserve the upstream Apache-2.0 license and record provenance in `vendor/raknet-rust/UPSTREAM.md`.
- Do not alter vendored transport behavior during this baseline unless a test isolates an implementation-blocking vendor defect.
- RakNet protocol 8 is runtime configuration: server `supported_protocols = vec![8]`; client `protocol_version = 8`. Upstream `RAKNET_PROTOCOL_VERSION` remains 11.
- Public Ardosia APIs must not expose `raknet-rust` types.
- Public payloads are `bytes::Bytes`; this plan introduces no MCPE packet types.
- Every Ardosia-owned queue is bounded.
- A slow application consumer must not block the central RakNet event loop.
- The baseline gate is 300/300 protocol-8 sessions established and held for 60 seconds with zero failed handshakes, zero unexpected disconnects, and zero protocol/decode errors.
- Mixed traffic, richer transport telemetry, CPU/RSS, RTT histograms, `steady-500`, churn, `ceiling-1000`, and loss/jitter are outside this baseline plan and get their own scaling plan after this gate.
- The design does not choose a project-level Ardosia license.

---

## File Map

```text
Cargo.toml
.gitignore
README.md
.github/workflows/ci.yml

crates/ardosia-network/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── config.rs
│   ├── error.rs
│   ├── reliability.rs
│   ├── metrics.rs
│   ├── backend.rs
│   ├── server.rs
│   └── connection.rs
└── tests/
    ├── vendor_smoke.rs
    ├── protocol8.rs
    └── transport.rs

crates/ardosia-loadgen/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── main.rs
│   ├── scenario.rs
│   ├── runner.rs
│   └── report.rs
└── tests/
    └── scenario.rs

scenarios/connect-300.toml

vendor/raknet-rust/
└── UPSTREAM.md
```

Responsibilities are intentionally narrow: `backend.rs` is the only owner of upstream server state, `connection.rs` is a handle around channels, `scenario.rs` has no networking, and `report.rs` has no orchestration.

---

### Task 1: Bootstrap the workspace and vendor the exact upstream snapshot

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `crates/ardosia-network/Cargo.toml`
- Create: `crates/ardosia-network/src/lib.rs`
- Create: `crates/ardosia-network/tests/vendor_smoke.rs`
- Create: `crates/ardosia-loadgen/Cargo.toml`
- Create: `crates/ardosia-loadgen/src/main.rs`
- Create: `vendor/raknet-rust/**` from upstream archive
- Create: `vendor/raknet-rust/UPSTREAM.md`

**Interfaces:**
- Consumes: upstream commit `3edfb4170e6cb5aeed992b09b50176fb7e5b6079`.
- Produces: workspace dependency `raknet-rust = { path = "vendor/raknet-rust" }`.

- [ ] **Step 1: Write the vendor smoke test before the dependency can resolve**

`crates/ardosia-network/tests/vendor_smoke.rs`:

```rust
use raknet_rust::low_level::protocol::constants::RAKNET_PROTOCOL_VERSION;
use raknet_rust::low_level::transport::TransportConfig;

#[test]
fn pinned_vendor_has_expected_protocol_configuration_surface() {
    let config = TransportConfig::default();
    assert_eq!(RAKNET_PROTOCOL_VERSION, 11);
    assert_eq!(config.supported_protocols, vec![11]);
}
```

`crates/ardosia-network/src/lib.rs`:

```rust
//! Ardosia transport-only networking.
```

`crates/ardosia-network/Cargo.toml`:

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

- [ ] **Step 2: Run the smoke test and verify the pre-vendor state fails**

Run:

```bash
cargo test -p ardosia-network --test vendor_smoke
```

Expected: FAIL because the workspace/path dependency is not yet present.

- [ ] **Step 3: Create workspace manifests and ignore rules**

Root `Cargo.toml`:

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

`.gitignore`:

```gitignore
/target/
/results/
*.profraw
```

- [ ] **Step 4: Export the pinned upstream revision without Git metadata**

Run from repository root:

```bash
rm -rf /tmp/ardosia-raknet-source /tmp/ardosia-raknet-export

git clone --filter=blob:none https://github.com/mcbe-rs/raknet-rust.git /tmp/ardosia-raknet-source
git -C /tmp/ardosia-raknet-source checkout 3edfb4170e6cb5aeed992b09b50176fb7e5b6079

mkdir -p /tmp/ardosia-raknet-export vendor/raknet-rust
git -C /tmp/ardosia-raknet-source archive 3edfb4170e6cb5aeed992b09b50176fb7e5b6079 \
  | tar -x -C /tmp/ardosia-raknet-export
cp -a /tmp/ardosia-raknet-export/. vendor/raknet-rust/
```

- [ ] **Step 5: Add provenance**

`vendor/raknet-rust/UPSTREAM.md`:

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

- [ ] **Step 6: Verify source identity and license retention**

Run:

```bash
diff -ru --exclude=UPSTREAM.md /tmp/ardosia-raknet-export vendor/raknet-rust
test -f vendor/raknet-rust/LICENSE
```

Expected: `diff` prints nothing; both commands exit 0.

- [ ] **Step 7: Run vendor smoke and upstream library tests**

```bash
cargo test -p ardosia-network --test vendor_smoke
cargo test --manifest-path vendor/raknet-rust/Cargo.toml --lib
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml .gitignore crates/ vendor/raknet-rust
git commit -m "build: vendor pinned raknet-rust baseline"
```

---

### Task 2: Define stable Ardosia config, errors, reliability, and metrics

**Files:**
- Modify: `crates/ardosia-network/src/lib.rs`
- Create: `crates/ardosia-network/src/config.rs`
- Create: `crates/ardosia-network/src/error.rs`
- Create: `crates/ardosia-network/src/reliability.rs`
- Create: `crates/ardosia-network/src/metrics.rs`

**Interfaces:**
- Produces:
  - `NetworkConfig { bind_addr, raknet_protocols, max_connections }`
  - `NetworkError`
  - `Reliability`
  - `NetworkMetrics`
  - crate-private config/reliability mapping into vendor types.

- [ ] **Step 1: Write failing config/reliability tests**

At the bottom of `config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol8_maps_to_vendor_supported_protocols() {
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
    fn empty_protocols_and_zero_capacity_are_rejected() {
        let mut config = NetworkConfig {
            bind_addr: "127.0.0.1:19132".parse().unwrap(),
            raknet_protocols: vec![],
            max_connections: 512,
        };
        assert!(matches!(config.validate(), Err(NetworkError::InvalidConfig { field: "raknet_protocols", .. })));

        config.raknet_protocols = vec![8];
        config.max_connections = 0;
        assert!(matches!(config.validate(), Err(NetworkError::InvalidConfig { field: "max_connections", .. })));
    }
}
```

At the bottom of `reliability.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use raknet_rust::low_level::protocol::reliability::Reliability as Vendor;

    #[test]
    fn every_public_reliability_mode_maps_exactly() {
        assert_eq!(Reliability::Unreliable.into_vendor(), Vendor::Unreliable);
        assert_eq!(Reliability::UnreliableSequenced.into_vendor(), Vendor::UnreliableSequenced);
        assert_eq!(Reliability::Reliable.into_vendor(), Vendor::Reliable);
        assert_eq!(Reliability::ReliableOrdered.into_vendor(), Vendor::ReliableOrdered);
        assert_eq!(Reliability::ReliableSequenced.into_vendor(), Vendor::ReliableSequenced);
    }
}
```

- [ ] **Step 2: Run and verify RED**

```bash
cargo test -p ardosia-network config::tests reliability::tests
```

Expected: FAIL because production types/functions do not exist.

- [ ] **Step 3: Implement `NetworkError`**

`error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("invalid network configuration field {field}: {message}")]
    InvalidConfig { field: &'static str, message: String },

    #[error("network I/O failed: {0}")]
    Io(#[from] std::io::Error),

    #[error("connection is closed")]
    ConnectionClosed,

    #[error("connection closed by Ardosia backpressure policy")]
    Backpressure,

    #[error("network backend stopped")]
    BackendStopped,

    #[error("network backend failed: {message}")]
    BackendFailure { message: String },
}
```

- [ ] **Step 4: Implement `NetworkConfig` and runtime protocol-8 mapping**

`config.rs` production portion:

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
        let mut vendor = TransportConfig::default();
        vendor.bind_addr = self.bind_addr;
        vendor.supported_protocols = self.raknet_protocols.clone();
        vendor.max_sessions = self.max_connections;
        vendor.validate().map_err(|error| NetworkError::InvalidConfig {
            field: "vendor_transport",
            message: error.to_string(),
        })?;
        Ok(vendor)
    }
}
```

- [ ] **Step 5: Implement Ardosia reliability without a public vendor trait impl**

`reliability.rs` production portion:

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
    pub(crate) fn into_vendor(self) -> raknet_rust::low_level::protocol::reliability::Reliability {
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

- [ ] **Step 6: Implement baseline atomic metrics**

`metrics.rs`:

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
            backpressure_disconnects_total: self.backpressure_disconnects_total.load(Ordering::Relaxed),
        }
    }

    pub(crate) fn connected(&self) {
        self.accepted_total.fetch_add(1, Ordering::Relaxed);
        self.connected_current.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn disconnected(&self) {
        self.disconnected_total.fetch_add(1, Ordering::Relaxed);
        let _ = self.connected_current.fetch_update(
            Ordering::Relaxed,
            Ordering::Relaxed,
            |value| Some(value.saturating_sub(1)),
        );
    }

    pub(crate) fn protocol_error(&self) {
        self.protocol_errors_total.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn backpressure_disconnect(&self) {
        self.backpressure_disconnects_total.fetch_add(1, Ordering::Relaxed);
    }
}
```

- [ ] **Step 7: Export only Ardosia types**

`lib.rs`:

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

pub(crate) use metrics::MetricsState;
```

- [ ] **Step 8: Run GREEN and commit**

```bash
cargo test -p ardosia-network config::tests reliability::tests
cargo test -p ardosia-network --test vendor_smoke
git add crates/ardosia-network
git commit -m "feat: define stable Ardosia transport types"
```

Expected: tests PASS before commit.

---

### Task 3: Implement the bounded backend actor, `NetworkServer`, and `Connection`

**Files:**
- Modify: `crates/ardosia-network/src/lib.rs`
- Create: `crates/ardosia-network/src/backend.rs`
- Create: `crates/ardosia-network/src/server.rs`
- Create: `crates/ardosia-network/src/connection.rs`
- Create: `crates/ardosia-network/tests/transport.rs`

**Interfaces:**
- Produces:
  - `NetworkServer::bind(NetworkConfig) -> Result<Self, NetworkError>`
  - `NetworkServer::accept(&mut self) -> Result<Connection, NetworkError>`
  - `NetworkServer::metrics() -> NetworkMetrics`
  - `NetworkServer::shutdown(self) -> Result<(), NetworkError>`
  - `Connection::peer_addr() -> SocketAddr`
  - `Connection::recv(&mut self) -> Result<Bytes, NetworkError>`
  - `Connection::send(&self, Bytes, Reliability) -> Result<(), NetworkError>`
  - `Connection::close(&self) -> Result<(), NetworkError>`

- [ ] **Step 1: Write a failing real-UDP bidirectional transport test**

`tests/transport.rs`:

```rust
use std::net::SocketAddr;
use std::time::Duration;
use ardosia_network::{NetworkConfig, NetworkServer, Reliability};
use bytes::Bytes;
use raknet_rust::client::{ClientSendOptions, RaknetClient, RaknetClientConfig, RaknetClientEvent};
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
async fn routes_reliable_ordered_payload_both_directions() {
    let addr = allocate_loopback_addr();
    let mut server = NetworkServer::bind(NetworkConfig {
        bind_addr: addr,
        raknet_protocols: vec![8],
        max_connections: 32,
    }).await.unwrap();

    let mut client = RaknetClient::connect_with_config(addr, protocol8_client_config()).await.unwrap();
    let mut connection = timeout(Duration::from_secs(2), server.accept()).await.unwrap().unwrap();

    client.send_with_options(
        Bytes::from_static(b"client-to-server"),
        ClientSendOptions {
            reliability: VendorReliability::ReliableOrdered,
            ..ClientSendOptions::default()
        },
    ).await.unwrap();

    assert_eq!(
        timeout(Duration::from_secs(2), connection.recv()).await.unwrap().unwrap(),
        Bytes::from_static(b"client-to-server")
    );

    connection.send(
        Bytes::from_static(b"server-to-client"),
        Reliability::ReliableOrdered,
    ).await.unwrap();

    let payload = timeout(Duration::from_secs(2), async {
        loop {
            if let Some(RaknetClientEvent::Packet { payload, .. }) = client.next_event().await {
                break payload;
            }
        }
    }).await.unwrap();

    assert_eq!(payload, Bytes::from_static(b"server-to-client"));
    client.disconnect(None).await.unwrap();
    server.shutdown().await.unwrap();
}
```

- [ ] **Step 2: Run and verify RED**

```bash
cargo test -p ardosia-network --test transport routes_reliable_ordered_payload_both_directions -- --nocapture
```

Expected: FAIL because `NetworkServer`/`Connection` do not exist.

- [ ] **Step 3: Define internal connection close state and bounded command types**

Core `backend.rs` types:

```rust
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use bytes::Bytes;
use raknet_rust::server::{PeerId, RaknetServer, RaknetServerEvent, SendOptions};
use tokio::sync::{mpsc, oneshot, watch};
use crate::{MetricsState, NetworkError, Reliability};
use crate::connection::Connection;

pub(crate) const COMMAND_QUEUE_CAPACITY: usize = 4096;
pub(crate) const PER_CONNECTION_INBOUND_CAPACITY: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloseState {
    Open,
    Closed,
    Backpressure,
}

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

struct PeerState {
    inbound: mpsc::Sender<Bytes>,
    close: watch::Sender<CloseState>,
}
```

No `unbounded_channel` is permitted.

- [ ] **Step 4: Implement the backend loop with explicit shutdown ownership**

Use this ownership shape in `run_backend`; do not call `RaknetServer::shutdown(self)` through `&mut RaknetServer`:

```rust
pub(crate) async fn run_backend(
    mut server: RaknetServer,
    mut commands: mpsc::Receiver<BackendCommand>,
    accept_tx: mpsc::Sender<Result<Connection, NetworkError>>,
    command_tx: mpsc::Sender<BackendCommand>,
    metrics: Arc<MetricsState>,
) {
    let mut peers: HashMap<PeerId, PeerState> = HashMap::new();
    let mut shutdown_response = None;

    'run: loop {
        tokio::select! {
            command = commands.recv() => {
                match command {
                    Some(BackendCommand::Send { peer_id, payload, reliability, response }) => {
                        let result = server.send_with_options(
                            peer_id,
                            payload,
                            SendOptions {
                                reliability: reliability.into_vendor(),
                                ..SendOptions::default()
                            },
                        ).await.map_err(NetworkError::from);
                        let _ = response.send(result);
                    }
                    Some(BackendCommand::Disconnect { peer_id, response }) => {
                        let result = server.disconnect(peer_id).await.map_err(NetworkError::from);
                        let _ = response.send(result);
                    }
                    Some(BackendCommand::Shutdown { response }) => {
                        shutdown_response = Some(response);
                        break 'run;
                    }
                    None => break 'run,
                }
            }
            event = server.next_event() => {
                let Some(event) = event else { break 'run; };
                if handle_server_event(
                    &mut server,
                    event,
                    &mut peers,
                    &accept_tx,
                    &command_tx,
                    &metrics,
                ).await {
                    break 'run;
                }
            }
        }
    }

    for peer in peers.into_values() {
        let _ = peer.close.send(CloseState::Closed);
    }

    let shutdown_result = server.shutdown().await.map_err(NetworkError::from);
    if let Some(response) = shutdown_response {
        let _ = response.send(shutdown_result);
    }
}
```

`handle_server_event(...) -> bool` returns `true` only when the backend should terminate.

Its event policy is exact:

- `PeerConnected`: create bounded inbound + close-watch channels; insert peer; increment metrics; build `Connection`; deliver it with `accept_tx.try_send`.
- Accept queue `Full`: remove the just-added peer, send `CloseState::Backpressure`, increment `backpressure_disconnects_total` and `disconnected_total`, call `server.disconnect(peer_id)`, continue.
- Accept queue `Closed`: remove peer, send `CloseState::Closed`, call `server.disconnect(peer_id)`, return `true`.
- `Packet`: use `peer.inbound.try_send(payload)`; never await the per-peer queue.
- Per-peer queue `Full`: remove peer, send `CloseState::Backpressure`, increment backpressure + disconnected metrics, call `server.disconnect(peer_id)`. The later vendor-generated disconnect event sees no peer entry and therefore does not decrement a second time.
- Per-peer queue `Closed`: remove peer, send `CloseState::Closed`, increment disconnected metrics, call `server.disconnect(peer_id)`.
- `PeerDisconnected`: remove only if present; send `CloseState::Closed`; increment disconnected metrics once.
- `DecodeError`: increment `protocol_errors_total`.
- `WorkerError`: attempt `accept_tx.try_send(Err(NetworkError::BackendFailure { message }))`, then return `true`.
- Other offline, receipt, rate-limit, metrics, and non-fatal events are ignored by the public baseline facade.

- [ ] **Step 5: Implement `Connection` with observable close/backpressure state**

`connection.rs`:

```rust
use std::net::SocketAddr;
use bytes::Bytes;
use raknet_rust::server::PeerId;
use tokio::sync::{mpsc, oneshot, watch};
use crate::backend::{BackendCommand, CloseState};
use crate::{NetworkError, Reliability};

pub struct Connection {
    peer_id: PeerId,
    peer_addr: SocketAddr,
    inbound: mpsc::Receiver<Bytes>,
    close: watch::Receiver<CloseState>,
    commands: mpsc::Sender<BackendCommand>,
}

impl Connection {
    pub(crate) fn new(
        peer_id: PeerId,
        peer_addr: SocketAddr,
        inbound: mpsc::Receiver<Bytes>,
        close: watch::Receiver<CloseState>,
        commands: mpsc::Sender<BackendCommand>,
    ) -> Self {
        Self { peer_id, peer_addr, inbound, close, commands }
    }

    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    pub async fn recv(&mut self) -> Result<Bytes, NetworkError> {
        loop {
            tokio::select! {
                changed = self.close.changed() => {
                    if changed.is_err() {
                        return Err(NetworkError::BackendStopped);
                    }
                    match *self.close.borrow() {
                        CloseState::Open => continue,
                        CloseState::Closed => return Err(NetworkError::ConnectionClosed),
                        CloseState::Backpressure => return Err(NetworkError::Backpressure),
                    }
                }
                payload = self.inbound.recv() => {
                    if let Some(payload) = payload {
                        return Ok(payload);
                    }
                    return match *self.close.borrow() {
                        CloseState::Backpressure => Err(NetworkError::Backpressure),
                        CloseState::Open => Err(NetworkError::BackendStopped),
                        CloseState::Closed => Err(NetworkError::ConnectionClosed),
                    };
                }
            }
        }
    }

    pub async fn send(&self, payload: Bytes, reliability: Reliability) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.commands.send(BackendCommand::Send {
            peer_id: self.peer_id,
            payload,
            reliability,
            response: tx,
        }).await.map_err(|_| NetworkError::BackendStopped)?;
        rx.await.map_err(|_| NetworkError::BackendStopped)?
    }

    pub async fn close(&self) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.commands.send(BackendCommand::Disconnect {
            peer_id: self.peer_id,
            response: tx,
        }).await.map_err(|_| NetworkError::BackendStopped)?;
        rx.await.map_err(|_| NetworkError::BackendStopped)?
    }
}
```

`PeerId` remains a private field; it never appears in a public signature.

- [ ] **Step 6: Implement `NetworkServer`**

`server.rs`:

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
        let vendor = RaknetServer::builder().transport_config(transport).start().await?;
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
        Ok(Self { accept_rx, commands: command_tx, metrics, backend })
    }

    pub async fn accept(&mut self) -> Result<Connection, NetworkError> {
        self.accept_rx.recv().await.ok_or(NetworkError::BackendStopped)?
    }

    pub fn metrics(&self) -> NetworkMetrics {
        self.metrics.snapshot()
    }

    pub async fn shutdown(self) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.commands.send(BackendCommand::Shutdown { response: tx })
            .await.map_err(|_| NetworkError::BackendStopped)?;
        let shutdown = rx.await.map_err(|_| NetworkError::BackendStopped)?;
        self.backend.await.map_err(|_| NetworkError::BackendStopped)?;
        shutdown
    }
}
```

- [ ] **Step 7: Export `NetworkServer` and `Connection`**

Update `lib.rs`:

```rust
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

- [ ] **Step 8: Run GREEN and commit**

```bash
cargo test -p ardosia-network --test transport routes_reliable_ordered_payload_both_directions -- --nocapture
cargo test -p ardosia-network
git add crates/ardosia-network
git commit -m "feat: add bounded RakNet server and connection facade"
```

Expected: tests PASS before commit.

---

### Task 4: Lock protocol-8 negotiation with raw bytes and a full client

**Files:**
- Create: `crates/ardosia-network/tests/protocol8.rs`

**Interfaces:**
- Raw fixture uses only UDP + literal RakNet IDs/magic.
- Full handshake uses `RaknetClientConfig::protocol_version = 8`.

- [ ] **Step 1: Add independent raw Request1 fixture and tests**

`protocol8.rs`:

```rust
use std::net::SocketAddr;
use std::time::Duration;
use ardosia_network::{NetworkConfig, NetworkServer};
use raknet_rust::client::{RaknetClient, RaknetClientConfig};
use tokio::net::UdpSocket;
use tokio::time::timeout;

const REQUEST1: u8 = 0x05;
const REPLY1: u8 = 0x06;
const INCOMPATIBLE: u8 = 0x19;
const MAGIC: [u8; 16] = [
    0x00, 0xff, 0xff, 0x00, 0xfe, 0xfe, 0xfe, 0xfe,
    0xfd, 0xfd, 0xfd, 0xfd, 0x12, 0x34, 0x56, 0x78,
];

fn allocate_loopback_addr() -> SocketAddr {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap()
}

fn raw_request1(protocol: u8, mtu: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(mtu);
    out.push(REQUEST1);
    out.extend_from_slice(&MAGIC);
    out.push(protocol);
    out.resize(mtu, 0);
    out
}

async fn reply_id(addr: SocketAddr, protocol: u8) -> u8 {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket.send_to(&raw_request1(protocol, 1200), addr).await.unwrap();
    let mut buffer = [0u8; 2048];
    let (len, _) = timeout(Duration::from_secs(2), socket.recv_from(&mut buffer)).await.unwrap().unwrap();
    assert!(len > 0);
    buffer[0]
}

#[tokio::test]
async fn raw_protocol8_request1_is_accepted() {
    let addr = allocate_loopback_addr();
    let server = NetworkServer::bind(NetworkConfig {
        bind_addr: addr,
        raknet_protocols: vec![8],
        max_connections: 32,
    }).await.unwrap();
    assert_eq!(reply_id(addr, 8).await, REPLY1);
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn raw_protocol11_request1_is_rejected_when_only_8_is_supported() {
    let addr = allocate_loopback_addr();
    let server = NetworkServer::bind(NetworkConfig {
        bind_addr: addr,
        raknet_protocols: vec![8],
        max_connections: 32,
    }).await.unwrap();
    assert_eq!(reply_id(addr, 11).await, INCOMPATIBLE);
    server.shutdown().await.unwrap();
}
```

- [ ] **Step 2: Run raw compatibility tests**

```bash
cargo test -p ardosia-network --test protocol8 -- --nocapture
```

Expected: both raw tests PASS with vendor source unchanged.

- [ ] **Step 3: Add full connected protocol-8 regression**

Append:

```rust
#[tokio::test]
async fn protocol8_vendor_client_reaches_ardosia_accept() {
    let addr = allocate_loopback_addr();
    let mut server = NetworkServer::bind(NetworkConfig {
        bind_addr: addr,
        raknet_protocols: vec![8],
        max_connections: 32,
    }).await.unwrap();

    let mut client_config = RaknetClientConfig::default();
    client_config.protocol_version = 8;
    let mut client = RaknetClient::connect_with_config(addr, client_config).await.unwrap();

    let connection = timeout(Duration::from_secs(2), server.accept()).await.unwrap().unwrap();
    assert_eq!(connection.peer_addr().ip(), client.local_addr().unwrap().ip());

    client.disconnect(None).await.unwrap();
    server.shutdown().await.unwrap();
}
```

- [ ] **Step 4: Run and commit**

```bash
cargo test -p ardosia-network --test protocol8 -- --nocapture
git add crates/ardosia-network/tests/protocol8.rs
git commit -m "test: lock RakNet protocol 8 compatibility"
```

Expected: PASS before commit.

---

### Task 5: Lock fragmentation and disconnect lifecycle

**Files:**
- Modify: `crates/ardosia-network/tests/transport.rs`
- Modify Ardosia wrapper files only if these tests expose an Ardosia-owned defect.

**Interfaces:**
- Produces regression coverage for >MTU reassembly and session cleanup.

- [ ] **Step 1: Add 4096-byte reliable-ordered reassembly test**

Append to `transport.rs`:

```rust
#[tokio::test]
async fn fragmented_reliable_ordered_payload_reassembles() {
    let addr = allocate_loopback_addr();
    let mut server = NetworkServer::bind(NetworkConfig {
        bind_addr: addr,
        raknet_protocols: vec![8],
        max_connections: 32,
    }).await.unwrap();
    let mut client = RaknetClient::connect_with_config(addr, protocol8_client_config()).await.unwrap();
    let mut connection = timeout(Duration::from_secs(2), server.accept()).await.unwrap().unwrap();

    let payload = Bytes::from(vec![0x5a; 4096]);
    client.send_with_options(
        payload.clone(),
        ClientSendOptions {
            reliability: VendorReliability::ReliableOrdered,
            ..ClientSendOptions::default()
        },
    ).await.unwrap();

    assert_eq!(
        timeout(Duration::from_secs(3), connection.recv()).await.unwrap().unwrap(),
        payload
    );
    client.disconnect(None).await.unwrap();
    server.shutdown().await.unwrap();
}
```

- [ ] **Step 2: Run fragmentation test**

```bash
cargo test -p ardosia-network --test transport fragmented_reliable_ordered_payload_reassembles -- --nocapture
```

Expected: PASS. If it fails, reproduce the same 4096-byte transfer directly with pinned vendor APIs before touching vendor source.

- [ ] **Step 3: Add clean-disconnect metric test**

Append:

```rust
#[tokio::test]
async fn clean_disconnect_releases_connected_session_metric() {
    let addr = allocate_loopback_addr();
    let mut server = NetworkServer::bind(NetworkConfig {
        bind_addr: addr,
        raknet_protocols: vec![8],
        max_connections: 32,
    }).await.unwrap();
    let mut client = RaknetClient::connect_with_config(addr, protocol8_client_config()).await.unwrap();
    let _connection = timeout(Duration::from_secs(2), server.accept()).await.unwrap().unwrap();

    assert_eq!(server.metrics().connected_current, 1);
    client.disconnect(None).await.unwrap();

    timeout(Duration::from_secs(2), async {
        loop {
            if server.metrics().connected_current == 0 { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();

    assert_eq!(server.metrics().disconnected_total, 1);
    server.shutdown().await.unwrap();
}
```

- [ ] **Step 4: Run suite and commit**

```bash
cargo test -p ardosia-network --test transport -- --nocapture
git add crates/ardosia-network
git commit -m "test: cover fragmentation and session lifecycle"
```

Expected: PASS before commit.

---

### Task 6: Define the declarative `connect-300` scenario

**Files:**
- Modify: `crates/ardosia-loadgen/Cargo.toml`
- Create: `crates/ardosia-loadgen/src/lib.rs`
- Create: `crates/ardosia-loadgen/src/scenario.rs`
- Create: `crates/ardosia-loadgen/src/runner.rs` as an empty module with documentation only
- Create: `crates/ardosia-loadgen/src/report.rs` as an empty module with documentation only
- Create: `crates/ardosia-loadgen/tests/scenario.rs`
- Create: `scenarios/connect-300.toml`

**Interfaces:**
- Produces `Scenario::from_str` and deterministic validation.

- [ ] **Step 1: Write failing scenario tests**

`tests/scenario.rs`:

```rust
use ardosia_loadgen::scenario::Scenario;

#[test]
fn parses_connect_300_shape() {
    let scenario = Scenario::from_str(r#"
name = "connect-300"
clients = 300
protocol_version = 8
ramp_up_seconds = 10
hold_seconds = 60
connect_timeout_seconds = 5
"#).unwrap();
    assert_eq!(scenario.clients, 300);
    assert_eq!(scenario.protocol_version, 8);
    assert_eq!(scenario.hold_seconds, 60);
}

#[test]
fn rejects_zero_clients() {
    let error = Scenario::from_str(r#"
name = "bad"
clients = 0
protocol_version = 8
ramp_up_seconds = 0
hold_seconds = 60
connect_timeout_seconds = 5
"#).unwrap_err();
    assert!(error.to_string().contains("clients"));
}
```

- [ ] **Step 2: Run and verify RED**

```bash
cargo test -p ardosia-loadgen --test scenario
```

Expected: FAIL because the loadgen library/module does not exist.

- [ ] **Step 3: Add loadgen dependencies and library exports**

`crates/ardosia-loadgen/Cargo.toml`:

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
thiserror.workspace = true
tokio.workspace = true
toml.workspace = true
```

`src/lib.rs`:

```rust
pub mod report;
pub mod runner;
pub mod scenario;
```

`runner.rs`:

```rust
//! Protocol-8 benchmark client orchestration.
```

`report.rs`:

```rust
//! Machine-readable baseline benchmark report.
```

- [ ] **Step 4: Implement scenario parsing and validation**

`scenario.rs`:

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
    Invalid { field: &'static str, message: &'static str },
}

impl Scenario {
    pub fn from_str(input: &str) -> Result<Self, ScenarioError> {
        let scenario: Self = toml::from_str(input)?;
        scenario.validate()?;
        Ok(scenario)
    }

    pub fn validate(&self) -> Result<(), ScenarioError> {
        if self.clients == 0 {
            return Err(ScenarioError::Invalid { field: "clients", message: "must be at least 1" });
        }
        if self.hold_seconds == 0 {
            return Err(ScenarioError::Invalid { field: "hold_seconds", message: "must be at least 1" });
        }
        if self.connect_timeout_seconds == 0 {
            return Err(ScenarioError::Invalid { field: "connect_timeout_seconds", message: "must be at least 1" });
        }
        Ok(())
    }
}
```

- [ ] **Step 5: Check in exact baseline scenario**

`scenarios/connect-300.toml`:

```toml
name = "connect-300"
clients = 300
protocol_version = 8
ramp_up_seconds = 10
hold_seconds = 60
connect_timeout_seconds = 5
```

- [ ] **Step 6: Run GREEN and commit**

```bash
cargo test -p ardosia-loadgen --test scenario
git add crates/ardosia-loadgen scenarios/connect-300.toml
git commit -m "feat: add declarative connect-300 scenario"
```

Expected: PASS before commit.

---

### Task 7: Implement load orchestration, report gate, and CLI

**Files:**
- Modify: `crates/ardosia-loadgen/src/main.rs`
- Replace: `crates/ardosia-loadgen/src/runner.rs`
- Replace: `crates/ardosia-loadgen/src/report.rs`

**Interfaces:**
- Produces:
  - `run_clients(target, &Scenario) -> RunReport`
  - `serve_until(bind, protocol, max_connections, stop_rx)` benchmark target using only `ardosia-network`
  - CLI `local`, `serve`, `run`.

- [ ] **Step 1: Write failing report gate tests**

At bottom of `report.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(report.failure_reason.unwrap().contains("unexpected disconnect"));
    }
}
```

- [ ] **Step 2: Run and verify RED**

```bash
cargo test -p ardosia-loadgen report::tests
```

Expected: FAIL because `RunReport`/`RunCounts` are absent.

- [ ] **Step 3: Implement report schema and exact gate**

`report.rs` production portion:

```rust
use serde::{Deserialize, Serialize};

pub const VENDOR_REVISION: &str = "3edfb4170e6cb5aeed992b09b50176fb7e5b6079";

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct RunCounts {
    pub successful_handshakes: usize,
    pub failed_handshakes: usize,
    pub unexpected_disconnects: usize,
    pub protocol_errors: usize,
    pub clean_disconnects: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub scenario: String,
    pub requested_clients: usize,
    pub counts: RunCounts,
    pub duration_ms: u64,
    pub vendor_revision: String,
    pub passed: bool,
    pub failure_reason: Option<String>,
}

impl RunReport {
    pub fn from_counts(
        scenario: String,
        requested_clients: usize,
        counts: RunCounts,
        duration_ms: u64,
    ) -> Self {
        let failure_reason = if counts.successful_handshakes != requested_clients {
            Some(format!("established {}/{} requested clients", counts.successful_handshakes, requested_clients))
        } else if counts.failed_handshakes != 0 {
            Some(format!("{} handshake(s) failed", counts.failed_handshakes))
        } else if counts.unexpected_disconnects != 0 {
            Some(format!("{} unexpected disconnect(s)", counts.unexpected_disconnects))
        } else if counts.protocol_errors != 0 {
            Some(format!("{} protocol/decode error(s)", counts.protocol_errors))
        } else if counts.clean_disconnects != requested_clients {
            Some(format!("only {}/{} clients completed the hold window cleanly", counts.clean_disconnects, requested_clients))
        } else {
            None
        };

        Self {
            scenario,
            requested_clients,
            counts,
            duration_ms,
            vendor_revision: VENDOR_REVISION.into(),
            passed: failure_reason.is_none(),
            failure_reason,
        }
    }
}
```

- [ ] **Step 4: Run report tests and verify GREEN**

```bash
cargo test -p ardosia-loadgen report::tests
```

Expected: PASS.

- [ ] **Step 5: Implement client orchestration with active polling during ramp and hold**

`runner.rs` uses these exact internal states:

```rust
#[derive(Debug, Clone, Copy)]
enum Phase {
    Ramp,
    Hold { deadline: tokio::time::Instant },
    Abort,
}

enum ConnectOutcome {
    Ready,
    Failed,
}
```

Each of the `scenario.clients` tasks does this sequence:

```rust
sleep(stagger_delay).await;
let mut config = RaknetClientConfig::default();
config.protocol_version = scenario.protocol_version;

let connect = timeout(
    Duration::from_secs(scenario.connect_timeout_seconds),
    RaknetClient::connect_with_config(target, config),
).await;
```

On connect failure, increment `failed_handshakes`, send exactly one `ConnectOutcome::Failed` into a bounded status channel, and return.

On success, increment `successful_handshakes`, send exactly one `ConnectOutcome::Ready`, then continuously poll `client.next_event()` while waiting for the common hold phase. A disconnected/closed client during ramp increments `unexpected_disconnects` once and returns; a `DecodeError` increments `protocol_errors` and continues.

Use this helper so a closed event stream cannot spin:

```rust
fn classify_event(event: Option<RaknetClientEvent>, counts: &AtomicCounts) -> bool {
    match event {
        Some(RaknetClientEvent::DecodeError { .. }) => {
            counts.protocol_errors.fetch_add(1, Ordering::Relaxed);
            false
        }
        Some(RaknetClientEvent::Disconnected { .. }) | None => {
            counts.unexpected_disconnects.fetch_add(1, Ordering::Relaxed);
            true
        }
        _ => false,
    }
}
```

Ramp loop:

```rust
loop {
    match *phase_rx.borrow_and_update() {
        Phase::Ramp => {
            tokio::select! {
                changed = phase_rx.changed() => {
                    if changed.is_err() { return; }
                }
                event = client.next_event() => {
                    if classify_event(event, &counts) { return; }
                }
            }
        }
        Phase::Hold { deadline } => {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => {
                    if client.disconnect(None).await.is_ok() {
                        counts.clean_disconnects.fetch_add(1, Ordering::Relaxed);
                    }
                    return;
                }
                changed = phase_rx.changed() => {
                    if changed.is_err() { return; }
                }
                event = client.next_event() => {
                    if classify_event(event, &counts) { return; }
                }
            }
        }
        Phase::Abort => {
            let _ = client.disconnect(None).await;
            return;
        }
    }
}
```

Coordinator rules:

1. Create a bounded `mpsc::channel::<ConnectOutcome>(scenario.clients)` and a `watch::channel(Phase::Ramp)`.
2. Spawn all clients with stagger delay `ramp_up_seconds * index / max(clients - 1, 1)`.
3. Receive exactly `scenario.clients` connect outcomes, bounded by `ramp_up_seconds + connect_timeout_seconds + 2` seconds.
4. If and only if every outcome is `Ready`, broadcast `Phase::Hold { deadline: now + hold_seconds }`.
5. Otherwise broadcast `Phase::Abort`.
6. Await every client task.
7. Build `RunReport` from atomics and wall-clock duration.

This ensures early clients continue driving RakNet keepalive/retransmission ticks instead of sitting behind a barrier without polling.

- [ ] **Step 6: Implement benchmark target ownership and clean shutdown**

Add:

```rust
pub async fn serve_until(
    bind: SocketAddr,
    protocol: u8,
    max_connections: usize,
    mut stop: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut server = NetworkServer::bind(NetworkConfig {
        bind_addr: bind,
        raknet_protocols: vec![protocol],
        max_connections,
    }).await?;

    loop {
        tokio::select! {
            _ = &mut stop => break,
            accepted = server.accept() => {
                let mut connection = accepted?;
                tokio::spawn(async move {
                    while connection.recv().await.is_ok() {}
                });
            }
        }
    }

    server.shutdown().await?;
    Ok(())
}
```

No benchmark target code may import an MCPE packet type or call vendor server APIs directly.

- [ ] **Step 7: Implement `local`, `serve`, and `run` CLI modes**

`main.rs` defines:

```rust
#[derive(clap::Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    Local {
        #[arg(long)] scenario: std::path::PathBuf,
        #[arg(long)] json: Option<std::path::PathBuf>,
    },
    Serve {
        #[arg(long, default_value = "127.0.0.1:19132")] bind: std::net::SocketAddr,
        #[arg(long, default_value_t = 8)] protocol: u8,
        #[arg(long, default_value_t = 512)] max_connections: usize,
    },
    Run {
        #[arg(long)] target: std::net::SocketAddr,
        #[arg(long)] scenario: std::path::PathBuf,
        #[arg(long)] json: Option<std::path::PathBuf>,
    },
}
```

Exact behavior:

- `Local`: parse scenario; reserve a loopback port using `std::net::UdpSocket::bind("127.0.0.1:0")`, read its address, drop it, start `serve_until` with `max_connections = max(scenario.clients, 512)`, run clients, send the stop oneshot, await server task, emit report.
- `Serve`: create stop oneshot, spawn `serve_until`, await `tokio::signal::ctrl_c()`, send stop, await server task.
- `Run`: parse scenario, call `run_clients(target, &scenario)`, emit report.
- Report emission prints `serde_json::to_string_pretty(&report)` and writes the same bytes to `--json` when supplied.
- `Local` and `Run` exit non-zero when `report.passed == false`.

- [ ] **Step 8: Run a fast 10-client smoke**

Create `/tmp/connect-10.toml`:

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

Expected: PASS report with 10 successful handshakes, 10 clean disconnects, zero failed handshakes, zero unexpected disconnects, zero protocol errors.

- [ ] **Step 9: Commit**

```bash
git add crates/ardosia-loadgen
git commit -m "feat: add protocol-8 RakNet connection load generator"
```

---

### Task 8: Verify the 300-client gate, document usage, and add lightweight CI

**Files:**
- Create: `README.md`
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Produces reproducible baseline commands and verified acceptance evidence outside Git.

- [ ] **Step 1: Add lightweight CI**

`.github/workflows/ci.yml`:

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

The heavy 60-second load gate is not part of normal push/PR CI.

- [ ] **Step 2: Write README boundary and benchmark commands**

`README.md` states:

- this repo is UDP/RakNet transport only;
- `ardosia-protocol` owns MCPE packet/game protocol;
- pinned upstream repo + revision + Apache-2.0 vendor license;
- protocol 8 is runtime configuration and vendor default 11 is unchanged;
- public users depend on Ardosia types, not `raknet-rust` types;
- unit/integration command:

```bash
cargo test --workspace
```

- local gate:

```bash
cargo run --release -p ardosia-loadgen -- \
  local --scenario scenarios/connect-300.toml \
  --json /tmp/ardosia-connect-300.json
```

- external target:

```bash
cargo run --release -p ardosia-loadgen -- \
  serve --bind 127.0.0.1:19132 --protocol 8 --max-connections 512
```

and in another process:

```bash
cargo run --release -p ardosia-loadgen -- \
  run --target 127.0.0.1:19132 \
  --scenario scenarios/connect-300.toml \
  --json /tmp/ardosia-connect-300.json
```

- localhost success proves this transport baseline on the measured host, not production Internet capacity.

- [ ] **Step 3: Run full static/test verification**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --manifest-path vendor/raknet-rust/Cargo.toml --lib
```

Expected: all four commands exit 0.

- [ ] **Step 4: Run the release `connect-300` gate**

```bash
cargo run --release -p ardosia-loadgen -- \
  local --scenario scenarios/connect-300.toml \
  --json /tmp/ardosia-connect-300.json
cat /tmp/ardosia-connect-300.json
```

Required evidence:

```text
requested_clients                300
counts.successful_handshakes     300
counts.failed_handshakes         0
counts.unexpected_disconnects    0
counts.protocol_errors           0
counts.clean_disconnects         300
passed                           true
```

If the gate fails, classify the first reproducible cause as one of: Ardosia wrapper, load harness, host/socket limits, or pinned vendor. Wrapper/harness fixes require a smaller failing regression test first. A vendor change requires a direct pinned-vendor reproduction first.

- [ ] **Step 5: Re-run all verification after any gate-driven source change**

If Step 4 causes a code change, repeat Step 3 and Step 4 from the updated tree. Do not claim the gate passes based on the earlier run.

- [ ] **Step 6: Commit README/CI after verification**

```bash
git add README.md .github/workflows/ci.yml
git commit -m "docs: document RakNet baseline benchmark"
```

Generated result JSON remains outside Git.

---

## Baseline Completion Checklist

- [ ] Vendor tree matches upstream commit `3edfb4170e6cb5aeed992b09b50176fb7e5b6079` except `UPSTREAM.md`.
- [ ] Vendor Apache-2.0 license is retained.
- [ ] Vendor `RAKNET_PROTOCOL_VERSION` remains 11.
- [ ] Ardosia server maps runtime configuration to `supported_protocols = vec![8]`.
- [ ] Load clients set `RaknetClientConfig::protocol_version = 8`.
- [ ] No public Ardosia signature mentions a `raknet_rust` type.
- [ ] No MCPE game protocol type exists in `ardosia-network`.
- [ ] All Ardosia-owned queues are bounded.
- [ ] Slow-consumer backpressure is observable as `NetworkError::Backpressure` and a metric.
- [ ] Raw protocol-8 Request1 is accepted.
- [ ] Raw unsupported protocol-11 Request1 is rejected.
- [ ] Full protocol-8 connected handshake reaches `NetworkServer::accept`.
- [ ] Reliable-ordered payload works both directions.
- [ ] 4096-byte payload reassembles correctly.
- [ ] Clean disconnect releases connected-session metrics.
- [ ] `cargo fmt --all --check` exits 0.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
- [ ] `cargo test --workspace` exits 0.
- [ ] Pinned upstream library tests exit 0.
- [ ] Release `connect-300` reports 300/300 successful handshakes, 300 clean hold completions, zero failures/disconnects/protocol errors, and `passed = true`.

After this checklist is satisfied, create a separate scaling plan for mixed `steady-300` traffic, RTT histograms, vendor telemetry facade, CPU/RSS, `steady-500`, churn, `ceiling-1000`, and loss/jitter. Vendor optimization is allowed only when those measurements identify a concrete bottleneck.
