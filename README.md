# ardosia-network

Transport-only networking for Ardosia.

`ardosia-network` owns UDP and RakNet transport. Minecraft: Pocket Edition packet definitions, codecs, login flow, and version-specific game protocol behavior belong in `ardosia-protocol`, not here.

## Status

Ardosia currently targets the historical Minecraft: Pocket Edition 0.15.10 stack:

- MCPE game protocol: `84`
- RakNet protocol: `8`
- Rust: `1.88+`
- vendored RakNet implementation: `mcbe-rs/raknet-rust` `0.2.0`
- pinned upstream revision: `3edfb4170e6cb5aeed992b09b50176fb7e5b6079`

RakNet protocol selection is runtime-configurable. Protocol `8` is configured by Ardosia rather than patched into the vendored source.

This repository is pre-release. Current benchmark evidence is intended for correctness and performance characterization, not as a production capacity guarantee.

## Architectural boundary

The public transport surface deals in connections and opaque bytes:

```text
game/server
    |
    v
ardosia-protocol
MCPE protocol 84
    |
    v
ardosia-network
UDP + RakNet
```

`ardosia-network` must not own game packets such as login, movement, chunks, inventory, entities, or world protocol codecs.

Likewise, `ardosia-protocol` must not own UDP sockets, RakNet handshakes, retransmission, congestion control, or transport session mechanics.

Public Ardosia networking APIs do not expose vendored `raknet-rust` types.

## Repository layout

```text
crates/ardosia-network/   public Ardosia transport facade
crates/ardosia-loadgen/   load generator, correctness gates, and profiling harness
scenarios/                declarative benchmark scenarios
docs/benchmarks.md        benchmark and profiling workflow
docs/results/             checked-in benchmark evidence
vendor/raknet-rust/       pinned upstream RakNet snapshot
```

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

The server maps the configured RakNet protocol list into the vendor transport configuration. The load generator independently configures the RakNet client protocol version.

Regression coverage verifies that:

- protocol `8` is accepted when configured;
- unsupported RakNet protocol versions are rejected;
- a full protocol-8 client can complete the RakNet handshake and reach the Ardosia server facade.

## Verification

Run the workspace quality gate with:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --manifest-path vendor/raknet-rust/Cargo.toml --lib
```

GitHub Actions workflows are manual-only so hosted runner time is not consumed on every development commit.

## Performance and benchmarking

The repository includes repeatable local scenarios for connection correctness, mixed steady-state traffic, 500-session churn, 1000-session characterization, and server CPU profiling.

See [`docs/benchmarks.md`](docs/benchmarks.md) for the canonical commands and interpretation rules.

Checked-in results under [`docs/results/`](docs/results/) record specific benchmark runs and their environments. Current evidence includes:

- connection correctness at 300 sessions;
- steady-state scaling characterization at 500 and 1000 sessions;
- a 1000-session CPU profile;
- constant-population churn at 500 sessions with 1500 successful planned replacements.

These are engineering measurements from specific machines and commits. They are not production SLAs or universal capacity claims.

## Vendored RakNet

Ardosia currently vendors a pinned `mcbe-rs/raknet-rust` snapshot so transport behavior is reproducible and inspectable while the networking layer is being developed.

Vendor changes are evidence-driven. A benchmark, compatibility test, or profile should identify a concrete need before transport algorithms are patched.

See `vendor/raknet-rust/UPSTREAM.md` for provenance and compatibility notes.

## Project direction

Near-term networking work is focused on:

- scaling and multicore characterization;
- reducing avoidable per-session/runtime overhead;
- preserving strict correctness under steady load and churn;
- keeping the game protocol boundary separate from transport;
- avoiding speculative changes to reliability or congestion behavior.

Ardosia is not affiliated with Mojang Studios or Microsoft.
