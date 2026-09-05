# ardosia-network

[![CI](https://github.com/ardosia/ardosia-network/actions/workflows/ci.yml/badge.svg)](https://github.com/ardosia/ardosia-network/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Game-agnostic asynchronous payload transport for Ardosia over the standalone [`ardosia-raknet`](https://github.com/ardosia/ardosia-raknet) implementation.

`ardosia-network` owns listener and connection lifecycle, validated transport configuration, bounded payload delivery, backpressure handling, and graceful shutdown. It deliberately does not interpret Minecraft packets or own player, session, game, or world semantics.

> **Pre-release status:** the source is licensed under Apache-2.0, but the crate remains `publish = false` and is not published to crates.io. Git dependencies should be pinned to an exact verified revision.

## Current target

The active Ardosia stack currently uses:

- Rust: `1.98.0`
- RakNet protocol: `8`
- RakNet package: `raknet-rust` `0.2.0`
- hardfork repository: `ardosia/ardosia-raknet`
- pinned hardfork revision: `55b57787b6715ef2a931631ef4b690e3df0651e5`
- preserved upstream baseline: `3edfb4170e6cb5aeed992b09b50176fb7e5b6079`

Ardosia's application currently configures this facade for the historical MCPE 0.15.10 / game-protocol-84 target, but that game protocol is not part of this crate. RakNet protocol selection, the opaque unconnected-pong advertisement, handshake-cookie mode, connection capacity, and optional worker sharding are supplied by the application.

## Architecture and scope

```text
application / game layer
   |-- protocol semantics
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
- Minecraft packet definitions, codecs, or game-session state;
- player, gameplay, world, or application policy;
- a production load-generation or observability subsystem.

Algorithmic RakNet transport changes belong in `ardosia-raknet`. Game-protocol semantics and application behavior belong above this facade.

## Supported public API

The intended crate-root surface is deliberately small:

```text
CookieMode
NetworkConfig
NetworkConfigError
NetworkServer
Connection
Reliability
NetworkError
```

Consumers should not need to import `raknet-rust` implementation types directly.

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
- a protocol-8 hardfork client can complete the RakNet handshake and reach the facade's accept boundary;
- cookie mode and the advertisement reach the underlying transport configuration;
- reliable-ordered payloads round-trip through the public facade;
- fragmented reliable-ordered payloads reassemble correctly;
- the RakNet implementation does not leak through the crate-root public surface.

The larger Ardosia stack has also reached its historical MCPE 0.15.10 pre-chunk session boundary over this transport profile. That is integration context, not a claim that this crate implements Minecraft semantics.

## Dependency reproducibility

The workspace pins `ardosia-raknet` by exact Git revision rather than a moving branch:

```text
55b57787b6715ef2a931631ef4b690e3df0651e5
```

The RakNet hardfork is public, so fetching this dependency does not require private Git credentials. The exact-SHA pin remains deliberate: Ardosia records the transport revision it has actually verified instead of tracking a moving branch.

`ardosia-network` and `ardosia-raknet` are each licensed under Apache-2.0 independently. The Network license applies to Ardosia-owned code in this repository; it is not inherited implicitly from the dependency.

## Historical benchmark evidence

The old in-repository load generator, benchmark scenarios, profiling tooling, and benchmark workflow were intentionally removed after their diagnostic questions were answered. They are not part of the current workspace and should not be resurrected as current operational guidance.

Three dated reports remain as historical engineering evidence:

- [`docs/results/2026-08-18-connect-300.md`](docs/results/2026-08-18-connect-300.md)
- [`docs/results/2026-08-18-churn-500.md`](docs/results/2026-08-18-churn-500.md)
- [`docs/results/2026-08-18-steady-1000-profile.md`](docs/results/2026-08-18-steady-1000-profile.md)

Those reports describe the repository layout, toolchain, branch names, vendoring model, and benchmark harness **as they existed at measurement time**. They are not current setup instructions or production-capacity claims.

Important preserved findings include the load-generator `RLIMIT_NOFILE` wall, the shared-source-IP policy artifact at large localhost client counts, and a transport worker-scheduling correction. Results are machine/commit/workload-specific and must not be generalized into universal player-capacity claims or used to weaken production abuse-control defaults.

## Development and verification

Use Rust `1.98.0` and run the complete local gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo doc --workspace --no-deps --locked
RUSTDOCFLAGS="-D missing_docs" cargo doc --workspace --no-deps --locked
git diff --check
```

GitHub Actions runs the same Rust gate on pushes to `main`, pull requests, and manual dispatches. Local verification remains useful before pushing.

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for scope and contribution rules and [`SECURITY.md`](SECURITY.md) for vulnerability reporting.

## Publication status

The source repository is licensed under the [Apache License 2.0](LICENSE). The crate remains intentionally `publish = false`; making the source public does not imply a crates.io release.

For Git consumers, pin an exact commit rather than a moving branch while the API is pre-release.

## License

Licensed under the [Apache License 2.0](LICENSE).

Contributions intentionally submitted for inclusion are accepted under the same license unless explicitly stated otherwise, consistent with Apache-2.0 section 5.

## Project relationship

`ardosia-network` is part of the independent Ardosia project and is not affiliated with Mojang Studios or Microsoft. `ardosia-raknet` is maintained separately as a generally reusable RakNet hardfork; upstream `mcbe-rs/raknet-rust` does not endorse or support Ardosia.
