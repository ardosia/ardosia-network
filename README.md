# ardosia-network

Ardosia-facing networking facade over the standalone `ardosia-raknet` transport implementation.

`ardosia-network` is intentionally game-agnostic: it moves opaque connected payloads and exposes transport configuration/metrics without interpreting MCPE packets.

## Current target

The active Ardosia stack targets:

- MCPE game protocol: `84`
- RakNet protocol: `8`
- Rust: `1.98.0`
- RakNet package: `raknet-rust` `0.2.0`
- hardfork repository: `ardosia/ardosia-raknet`
- pinned hardfork revision: `f127fce27a206a51a1d39ffa7a9bbed98d10ea14`
- preserved upstream baseline: `3edfb4170e6cb5aeed992b09b50176fb7e5b6079`

RakNet protocol selection, unconnected-pong advertisement, and handshake-cookie behavior are application-configurable. MCPE-specific values are supplied by `ardosia-server`; they are not hard-coded into this facade.

## Architecture

```text
ardosia-server
   |-- ardosia-protocol
   `-- ardosia-network
          `-- ardosia-raknet
```

### `ardosia-network` owns

- `NetworkServer` and Ardosia-facing connection lifecycle;
- opaque connected-payload send/receive abstractions;
- generic RakNet compatibility configuration exposed to applications;
- runtime/shard configuration;
- translation between Ardosia configuration/metrics and the RakNet implementation;
- integration/regression tests across the public facade.

### `ardosia-network` does not own

- RakNet handshake/reliability/retransmission/congestion algorithms;
- MCPE packet definitions, codecs, or session state;
- gameplay/world/application state;
- a production load-generation harness.

Consumers above this layer should not need to import hardfork implementation types directly.

## Legacy MCPE compatibility surface

The facade supports the transport knobs needed by the MCPE 0.15.10 server profile while remaining generic:

```rust
NetworkConfig {
    raknet_protocols: vec![8],
    advertisement: "MCPE;Ardosia;84;0.15.10;0;20".into(),
    send_cookie: false,
    // ...
}
```

Regression coverage verifies that:

- protocol `8` is accepted when configured;
- unsupported protocol versions are rejected;
- a protocol-8 hardfork client can complete the RakNet handshake and reach the Ardosia accept boundary;
- the compatibility options reach the underlying transport config;
- reliable-ordered payloads round-trip through the public facade;
- fragmented reliable-ordered payloads reassemble correctly;
- the RakNet implementation does not leak through the crate-root public surface.

A real MCPE 0.15.10 client has reached the `ardosia-server` protocol/session layer over this transport profile.

## Example

```rust
let mut connection = server.accept().await?;
let payload = connection.recv().await?;

connection
    .send(payload, Reliability::ReliableOrdered)
    .await?;
```

The payload is intentionally opaque to this crate.

## Dependency reproducibility

The workspace pins `ardosia-raknet` by exact Git revision rather than a moving branch:

```text
f127fce27a206a51a1d39ffa7a9bbed98d10ea14
```

A development machine therefore needs Git credentials capable of reading the private hardfork. If Cargo's built-in Git transport cannot use the local credential setup, `CARGO_NET_GIT_FETCH_WITH_CLI=true` can delegate fetching to the system Git client.

## Verification

Run the local Rust `1.98.0` gate:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo doc --no-deps
git diff --check
```

Local verification is the current source of truth for active development. Hosted CI is not required for every development commit.

## Historical benchmark evidence

The former in-repository load generator/scenario/benchmark harness was intentionally removed after localhost scaling failures were traced to test-environment ceilings and shared-source-IP artifacts rather than a demonstrated transport-capacity wall.

The evidence remains preserved in:

- `docs/benchmark-history.md` — interpretation and conclusions;
- `docs/results/` — raw historical reports.

Important preserved findings include the load-generator `RLIMIT_NOFILE` wall, the shared per-IP connected processing-budget artifact at large localhost client counts, and the fair worker-scheduling correction. These findings must not be generalized into universal capacity claims or used to weaken production abuse-control defaults.

## RakNet boundary

`ardosia-raknet` remains a standalone generally usable RakNet hardfork. Algorithmic transport changes belong there and should be backed by correctness evidence, regression tests, profiling, or meaningful benchmark evidence.

`ardosia-network` should remain small and stable as the facade between that implementation and Ardosia application code.

Ardosia is an independent project and is not affiliated with Mojang Studios or Microsoft.
