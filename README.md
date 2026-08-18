# ardosia-network

Transport-only networking for Ardosia.

This repository owns UDP and RakNet transport. Minecraft/MCPE game packet definitions, codecs, login flow, and version-specific game protocol behavior belong in `ardosia-protocol`, not here.

## Current baseline

- Rust 1.88+
- Vendored `mcbe-rs/raknet-rust` 0.2.0
- Upstream revision: `3edfb4170e6cb5aeed992b09b50176fb7e5b6079`
- RakNet protocol version is runtime-configurable
- MCPE 0.15.10 target uses RakNet protocol `8`
- Public transport payloads are opaque `bytes::Bytes`
- Public Ardosia APIs do not expose `raknet-rust` types
- Ardosia-owned queues are bounded

The upstream source itself remains behaviorally unchanged. See `vendor/raknet-rust/UPSTREAM.md` for provenance and the Rust toolchain compatibility note.

## Layout

```text
crates/ardosia-network/   public transport facade
crates/ardosia-loadgen/   protocol-8 load generator and correctness gate
scenarios/                 declarative load scenarios
vendor/raknet-rust/        pinned upstream RakNet snapshot
```

## Public transport boundary

`ardosia-network` deals in connections and raw bytes:

```rust
let mut connection = server.accept().await?;
let payload = connection.recv().await?;
connection
    .send(payload, Reliability::ReliableOrdered)
    .await?;
```

It must not contain MCPE packet types such as Login, StartGame, MovePlayer, LevelChunk, inventory packets, or world/game protocol codecs.

## Protocol 8

The RakNet version is configured rather than patched into the vendored source.

Server-side Ardosia configuration maps protocol `8` into the vendor's `TransportConfig::supported_protocols`. The load generator maps protocol `8` into `RaknetClientConfig::protocol_version`.

Regression tests independently verify that:

- a raw `OpenConnectionRequest1` advertising protocol 8 is accepted;
- protocol 11 is rejected when the server is configured for only protocol 8;
- a full protocol-8 RakNet client reaches `NetworkServer::accept()`.

## Tests

```bash
cargo test --workspace
cargo test --manifest-path vendor/raknet-rust/Cargo.toml --lib
```

GitHub Actions CI is deliberately `workflow_dispatch`-only so development commits do not consume hosted-runner minutes automatically.

## Load generator

Run the checked-in 300-session baseline locally:

```bash
cargo run -p ardosia-loadgen -- local scenarios/connect-300.toml
```

Run clients against an external target:

```bash
cargo run -p ardosia-loadgen -- run scenarios/connect-300.toml --target 127.0.0.1:19132
```

Run only the Ardosia benchmark server:

```bash
cargo run -p ardosia-loadgen -- serve --bind 0.0.0.0:19132 --protocol 8 --max-connections 1024
```

The load clients continue polling RakNet events throughout ramp-up and hold time; they do not simply connect and sleep.

## Verified connect-300 result

The first hosted baseline completed successfully on August 18, 2026:

```text
requested clients:       300
successful handshakes:   300
failed handshakes:       0
unexpected disconnects:  0
protocol/decode errors:  0
clean disconnects:       300
benchmark duration:      69,995 ms
RakNet protocol:         8
```

See `docs/results/2026-08-18-connect-300.md` for the exact result and scope.

## What this result does not prove

`connect-300` is a connection/hold correctness baseline, not the final capacity claim for Ardosia. It does not yet measure realistic movement/game traffic, fanout, packet loss, jitter, CPU/RSS, RTT percentiles, retransmission pressure, ACK/NACK rates, or 500-1000 client ceilings. Those belong to the next scaling phase.
