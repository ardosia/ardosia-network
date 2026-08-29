# ardosia-network

Game-agnostic asynchronous payload transport for Ardosia over the standalone `ardosia-raknet` implementation.

`ardosia-network` owns listener and connection lifecycle, validated transport configuration, bounded payload delivery, backpressure handling, and graceful shutdown. It deliberately does not interpret Minecraft packets or own player, session, game, or world semantics.

## Current target

The active Ardosia stack targets:

- MCPE game protocol: `84`
- RakNet protocol: `8`
- Rust: `1.98.0`
- RakNet package: `raknet-rust` `0.2.0`
- hardfork repository: `ardosia/ardosia-raknet`
- pinned hardfork revision: `f127fce27a206a51a1d39ffa7a9bbed98d10ea14`
- preserved upstream baseline: `3edfb4170e6cb5aeed992b09b50176fb7e5b6079`

RakNet protocol selection, the opaque unconnected-pong advertisement, handshake-cookie mode, connection capacity, and optional worker sharding are supplied by the application rather than hard-coded into this crate.

## Architecture

```text
ardosia-server
   |-- ardosia-protocol
   `-- ardosia-network
          `-- ardosia-raknet
```

### `ardosia-network` owns

- `NetworkConfig` validation and translation to the pinned transport;
- `NetworkServer` listener lifecycle;
- accepted `Connection` lifecycle;
- opaque connected-payload send/receive operations;
- bounded queues and application-facing backpressure outcomes;
- integration/regression tests across the supported facade.

### `ardosia-network` does not own

- RakNet handshake, reliability, retransmission, congestion-control, or sharding algorithms;
- MCPE packet definitions, codecs, or session state;
- player, gameplay, world, or application policy;
- a production load-generation or observability subsystem.

Consumers above this layer should not need to import hardfork implementation types directly.

## Usage

```rust
use std::net::SocketAddr;
use std::num::NonZeroUsize;

use ardosia_network::{CookieMode, NetworkConfig, NetworkServer, Reliability};

let bind_addr: SocketAddr = "0.0.0.0:19132".parse()?;
let config = NetworkConfig::new(
    bind_addr,
    [8],
    NonZeroUsize::new(20).unwrap(),
    "ardosia-network",
    CookieMode::Disabled,
)?;

let mut server = NetworkServer::bind(config).await?;
let mut connection = server.accept().await?;
let payload = connection.recv().await?;
connection.send(payload, Reliability::ReliableOrdered).await?;
server.shutdown().await?;
```

The payload and advertisement are intentionally opaque to this crate. Call `.with_worker_shards(NonZeroUsize::new(n).unwrap())` only when the application needs to override the transport's default shard selection.

## Compatibility coverage

Regression coverage verifies that:

- protocol `8` is accepted when configured;
- unsupported RakNet protocol versions are rejected;
- a protocol-8 hardfork client can complete the RakNet handshake and reach the Ardosia accept boundary;
- cookie mode and the advertisement reach the underlying transport configuration;
- reliable-ordered payloads round-trip through the public facade;
- fragmented reliable-ordered payloads reassemble correctly;
- the RakNet implementation does not leak through the crate-root public surface.

A real MCPE 0.15.10 client has reached the `ardosia-server` protocol/session layer over this transport profile.

## Dependency reproducibility

The workspace pins `ardosia-raknet` by exact Git revision rather than a moving branch:

```text
f127fce27a206a51a1d39ffa7a9bbed98d10ea14
```

`ardosia-raknet` is the public reusable transport component of the stack, so fetching that Git dependency does not require credentials. The exact-SHA pin remains deliberate: this hardfork is pre-release and Ardosia records the transport revision it has actually verified instead of tracking a moving branch.

The surrounding `ardosia-network`, `ardosia-protocol`, and `ardosia-server` repositories remain private development components; their separate access and licensing decisions do not change the public availability of the RakNet hardfork.

## Verification

Run the local Rust `1.98.0` gate:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo doc --no-deps
RUSTDOCFLAGS="-D missing_docs" cargo doc --no-deps
git diff --check
```

Local verification is the current source of truth for active development. Hosted CI is not required for every development commit.

## Historical benchmark evidence

The former in-repository load generator/scenario/benchmark harness was intentionally removed after localhost scaling failures were traced to test-environment ceilings and shared-source-IP artifacts rather than a demonstrated transport-capacity wall.

The scoped evidence and provenance are preserved in:

- `docs/results/2026-08-18-connect-300.md`;
- `docs/results/2026-08-18-churn-500.md`;
- `docs/results/2026-08-18-steady-1000-profile.md`.

Important preserved findings include the load-generator `RLIMIT_NOFILE` wall, the shared per-IP connected processing-budget artifact at large localhost client counts, and the fair worker-scheduling correction. These findings must not be generalized into universal capacity claims or used to weaken production abuse-control defaults.

## RakNet boundary

`ardosia-raknet` remains a standalone generally usable RakNet hardfork. Algorithmic transport changes belong there and should be backed by correctness evidence, regression tests, profiling, or meaningful benchmark evidence.

`ardosia-network` should remain small and stable as the facade between that implementation and Ardosia application code.

Ardosia is an independent project and is not affiliated with Mojang Studios or Microsoft.