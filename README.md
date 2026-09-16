# ardosia-network

[![CI](https://github.com/ardosia/ardosia-network/actions/workflows/ci.yml/badge.svg)](https://github.com/ardosia/ardosia-network/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Game-agnostic asynchronous payload transport for Ardosia over the standalone [`ardosia-raknet`](https://github.com/ardosia/ardosia-raknet) hardfork.

`ardosia-network` owns validated transport configuration, listener/connection lifecycle, opaque connected-payload delivery, bounded queues/backpressure, and graceful shutdown. Minecraft packet semantics, players, gameplay, worlds, and application policy live above this crate.

## Status

- Rust baseline: `1.98.0`
- RakNet package: `raknet-rust` `0.2.0`
- exact hardfork pin: `55b57787b6715ef2a931631ef4b690e3df0651e5`
- crate publication: `publish = false`
- license: Apache-2.0

The wider Ardosia application currently uses RakNet protocol 8 for the fixed MCPE 0.15.10 / game-protocol-84 target. That game protocol is not part of this crate.

## Public API

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

The advertisement and connected payloads are intentionally opaque to this layer.

## Verification

Use Rust `1.98.0` and run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo doc --workspace --no-deps --locked
RUSTDOCFLAGS="-D missing_docs" cargo doc --workspace --no-deps --locked
git diff --check
```

Historical benchmark reports and broader architecture/status prose are maintained in the centralized Ardosia documentation repository rather than duplicated here. Their old repository-local copies are historical Git content, not current operational guidance or universal capacity claims.

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for contribution scope and [`SECURITY.md`](SECURITY.md) for vulnerability reporting.

## License

Licensed under the [Apache License 2.0](LICENSE). Contributions intentionally submitted for inclusion are accepted under the same license unless explicitly stated otherwise, consistent with Apache-2.0 section 5.

`ardosia-network` is part of the independent Ardosia project and is not affiliated with Mojang Studios or Microsoft.
