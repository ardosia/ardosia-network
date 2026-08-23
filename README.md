# ardosia-network

Ardosia-facing networking facade, transport integration, and reproducible load-testing infrastructure.

`ardosia-network` owns the stable networking surface used by Ardosia and the benchmark/load-generation harness around it. RakNet transport algorithms live in the standalone `ardosia-raknet` hardfork. Minecraft: Pocket Edition packet definitions, codecs, login flow, and version-specific game behavior belong in `ardosia-protocol`.

## Status

Ardosia currently targets the historical Minecraft: Pocket Edition 0.15.10 stack:

- MCPE game protocol: `84`
- RakNet protocol: `8`
- Rust: `1.88+`
- RakNet package: `raknet-rust` `0.2.0`
- hardfork repository: `ardosia/ardosia-raknet`
- exact hardfork revision: `f127fce27a206a51a1d39ffa7a9bbed98d10ea14`
- preserved upstream baseline: `3edfb4170e6cb5aeed992b09b50176fb7e5b6079`

RakNet protocol selection is runtime-configurable. Protocol `8` is configured by Ardosia rather than hard-coded into the transport implementation.

This repository is pre-release. Benchmark evidence is intended for correctness and performance characterization on recorded environments, not as a production capacity guarantee.

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
- integration tests across the public facade;
- load generation, scenarios, benchmark reporting and profiling.

### `ardosia-network` does not own

- RakNet handshake, reliability, retransmission, congestion-control or transport-session algorithms;
- MCPE game packet definitions or codecs;
- gameplay/application state.

Public Ardosia networking consumers should not need to import hardfork implementation types directly.

## Repository layout

```text
crates/ardosia-network/   Ardosia networking facade
crates/ardosia-loadgen/   load generator, correctness gates, and profiling harness
scenarios/                declarative benchmark scenarios
docs/benchmarks.md        benchmark and profiling workflow
docs/results/             selected checked-in benchmark evidence
```

The former `vendor/raknet-rust/` source tree has been removed. Cargo resolves the standalone hardfork at the exact revision recorded in both `Cargo.toml` and `Cargo.lock`.

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

The server maps the configured RakNet protocol list into the hardfork transport configuration. The load generator independently configures the RakNet client protocol version.

Regression coverage verifies that:

- protocol `8` is accepted when configured;
- unsupported RakNet protocol versions are rejected;
- a full protocol-8 client can complete the RakNet handshake and reach the Ardosia server facade;
- the pinned hardfork exposes the expected low-level protocol/configuration surface.

## Dependency reproducibility

The workspace pins `ardosia-raknet` by exact Git revision rather than by a moving branch:

```text
f127fce27a206a51a1d39ffa7a9bbed98d10ea14
```

That commit is preserved in the `ardosia-raknet` default-branch ancestry. The hardfork repository owns its own formatting, Clippy, unit, integration, and soak gates; this repository validates the Ardosia integration against the pinned revision.

Because the hardfork repository is private, a development machine must have GitHub credentials that allow Cargo/Git to fetch it. If Cargo's built-in Git transport cannot use the local credential setup, `CARGO_NET_GIT_FETCH_WITH_CLI=true` can be used to delegate fetching to the system Git client.

## Verification

Run the workspace quality gate on Rust 1.88.0:

```bash
cargo +1.88.0 fmt --all -- --check
cargo +1.88.0 clippy --workspace --all-targets --locked -- -D warnings
cargo +1.88.0 test --workspace --locked
git diff --check
```

GitHub Actions workflows remain manual-only so hosted runner time is not consumed on every development commit.

## Performance and benchmarking

The repository includes repeatable local scenarios for connection correctness, mixed steady-state traffic, constant-population churn, shard scaling, and server CPU profiling.

See [`docs/benchmarks.md`](docs/benchmarks.md) for the canonical commands and interpretation rules.

Checked-in results under [`docs/results/`](docs/results/) record specific benchmark runs and their environments. They are engineering measurements from specific machines and commits, not production SLAs or universal capacity claims.

Recent shard-scaling work also identified benchmark-environment ceilings that must not be mistaken for transport capacity:

- the roughly 1,012-client admission wall was caused by the load-generator process `RLIMIT_NOFILE=1024`;
- severe 3,000-client degradation at lower shard counts came from all localhost clients sharing one per-IP connected processing budget;
- increasing only that experimental localhost budget restored healthy 3,000-client runs at higher shard counts;
- Linux UDP receive-buffer exhaustion was falsified in the observed failing case (`RcvbufErrors=0`, `InErrors=0`).

Those observations are harness/telemetry evidence and are not justification for weakening production abuse-control defaults.

## RakNet hardfork

`ardosia-raknet` is intended to remain a standalone, generally usable RakNet hardfork rather than becoming an Ardosia-specific game-server subsystem.

The extraction was deliberately behavior-preserving:

1. preserve the upstream history and baseline;
2. carry over the proven Ardosia transport delta without redesigning it;
3. verify the hardfork independently;
4. pin this repository to the exact verified hardfork commit;
5. remove the in-repository vendor tree;
6. keep Ardosia-facing API cleanup separate from the extraction.

The carried transport delta includes fair shard-worker scheduling, separation of established connected traffic from the coarse offline/unknown per-IP limiter, connected processing-budget accounting, and regression coverage for established-session blocking/rate-limit semantics.

## Project direction

Near-term networking work is focused on:

- keeping the Ardosia facade small and stable;
- improving transport-health and benchmark telemetry;
- making file-descriptor and policy ceilings explicit in reports;
- improving localhost source-IP modeling without weakening production security defaults;
- resuming capacity characterization with artificial host/policy ceilings visible;
- evolving RakNet algorithms in `ardosia-raknet`, independently of MCPE game protocol work.

Ardosia is an independent project and is not affiliated with Mojang Studios or Microsoft.
