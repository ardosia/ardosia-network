# ardosia-network

Ardosia-facing networking facade and RakNet transport integration.

`ardosia-network` owns the stable transport surface used by Ardosia. RakNet transport algorithms live in the standalone `ardosia-raknet` hardfork. Minecraft: Pocket Edition packet definitions, codecs, login flow, and version-specific game behavior belong in `ardosia-protocol`.

The former in-repository load generator, scenarios, benchmark runner, and profiling harness were intentionally removed in August 2026 after the major localhost scaling failures were traced to benchmark-environment ceilings rather than a demonstrated transport-capacity wall. Historical measurements and the diagnosis are preserved in [`docs/benchmark-history.md`](docs/benchmark-history.md).

## Status

Ardosia currently targets the historical Minecraft: Pocket Edition 0.15.10 stack:

- MCPE game protocol: `84`
- RakNet protocol: `8`
- Rust: `1.98+`
- RakNet package: `raknet-rust` `0.2.0`
- hardfork repository: `ardosia/ardosia-raknet`
- exact pinned hardfork revision: `f127fce27a206a51a1d39ffa7a9bbed98d10ea14`
- preserved upstream baseline: `3edfb4170e6cb5aeed992b09b50176fb7e5b6079`

RakNet protocol selection is runtime-configurable. Protocol `8` is configured by Ardosia rather than hard-coded into the transport implementation.

## Architectural boundary

```text
server / game
    |
    v
ardosia-protocol
MCPE protocol 84
    |
    v
ardosia-network
Ardosia facade + integration
    |
    v
ardosia-raknet
UDP + RakNet algorithms
```

### `ardosia-network` owns

- `NetworkServer` and the Ardosia-facing connection lifecycle;
- opaque payload send/receive abstractions;
- Ardosia runtime and shard configuration;
- translation between Ardosia configuration/metrics and the RakNet implementation;
- integration and regression tests across the public facade.

### `ardosia-network` does not own

- RakNet handshake, reliability, retransmission, congestion-control or transport-session algorithms;
- MCPE game packet definitions or codecs;
- gameplay/application state;
- a load-generation or benchmark harness.

Public Ardosia networking consumers should not need to import hardfork implementation types directly.

## Repository layout

```text
crates/ardosia-network/     Ardosia networking facade
docs/benchmark-history.md  Historical benchmark findings and interpretation
docs/results/              Preserved raw historical benchmark reports
```

The former `vendor/raknet-rust/` source tree is also gone. Cargo resolves the standalone hardfork at the exact revision recorded in `Cargo.toml` and `Cargo.lock`.

## Example

```rust
let mut connection = server.accept().await?;

let payload = connection.recv().await?;

connection
    .send(payload, Reliability::ReliableOrdered)
    .await?;
```

The transport does not interpret the payload as an MCPE game packet.

## Protocol 8

The server maps the configured RakNet protocol list into the hardfork transport configuration.

Regression coverage verifies that:

- protocol `8` is accepted when configured;
- unsupported RakNet protocol versions are rejected;
- a full protocol-8 client can complete the RakNet handshake and reach the Ardosia server facade;
- the pinned hardfork exposes the expected low-level protocol/configuration surface without leaking those implementation types through the Ardosia public API.

## Dependency reproducibility

The workspace pins `ardosia-raknet` by exact Git revision rather than by a moving branch:

```text
f127fce27a206a51a1d39ffa7a9bbed98d10ea14
```

That commit is preserved in the `ardosia-raknet` default-branch ancestry. The hardfork repository owns its own formatting, Clippy, unit, integration, and soak gates; this repository validates the Ardosia integration against the pinned revision.

Because the hardfork repository is private, a development machine must have GitHub credentials that allow Cargo/Git to fetch it. If Cargo's built-in Git transport cannot use the local credential setup, `CARGO_NET_GIT_FETCH_WITH_CLI=true` can delegate fetching to the system Git client.

## Verification

Run the workspace quality gate on Rust 1.98.0:

```bash
cargo +1.98.0 fmt --all -- --check
CARGO_NET_GIT_FETCH_WITH_CLI=true cargo +1.98.0 clippy --workspace --all-targets --locked -- -D warnings
CARGO_NET_GIT_FETCH_WITH_CLI=true cargo +1.98.0 test --workspace --locked
git diff --check
```

`rust-toolchain.toml` and the manual GitHub Actions workflow are pinned to Rust `1.98.0`, so local and CI verification use the same compiler, rustfmt, and Clippy baseline. GitHub Actions remain manual-only so hosted runner time is not consumed on every development commit.

## Historical benchmark evidence

The removed harness produced useful evidence, but it also demonstrated why a localhost benchmark driver must not be confused with server capacity. The preserved record documents the trusted 300/500/1000-session runs, the CPU profile, churn characterization, the ~1,012 file-descriptor wall, and the 3,000-client shared-source-IP processing-budget artifact.

See [`docs/benchmark-history.md`](docs/benchmark-history.md). The raw historical reports under [`docs/results/`](docs/results/) remain preserved as evidence, not as an active benchmark workflow.

## RakNet hardfork

`ardosia-raknet` is intended to remain a standalone, generally usable RakNet hardfork rather than becoming an Ardosia-specific game-server subsystem.

The carried Ardosia transport delta includes fair shard-worker scheduling, separation of established connected traffic from the coarse offline/unknown per-IP limiter, connected processing-budget accounting, and regression coverage for established-session blocking/rate-limit semantics.

Production abuse-control defaults must not be weakened to accommodate localhost benchmarking artifacts.

## Project direction

Near-term networking work is focused on:

- keeping the Ardosia facade small and stable;
- preserving transport correctness and useful health metrics;
- evolving RakNet algorithms only from correctness/profiling evidence in `ardosia-raknet`;
- building the MCPE protocol-84 layer in `ardosia-protocol` without coupling it to RakNet internals.

If large-scale capacity characterization is resumed later, it should use a purpose-built harness that models independent source IPs or distributed generators and records host/process ceilings from the start, rather than resurrecting the removed localhost harness unchanged.

Ardosia is an independent project and is not affiliated with Mojang Studios or Microsoft.
