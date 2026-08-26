# Ardosia Network API Cleanup Design

Date: 2026-08-25
Status: approved for implementation planning

## Goal

Reduce `ardosia-network` to the smallest useful, documented, game-agnostic
transport facade needed by Ardosia. Remove unused observability machinery,
prevent invalid public configuration states where practical, and preserve all
connection behavior required by the MCPE 0.15.10 server.

## Scope

This cleanup may break the crate's pre-1.0 Rust API. All Ardosia consumers will
be updated in the same cleanup program.

The supported facade after cleanup is:

```text
NetworkConfig
NetworkConfigError
CookieMode
NetworkServer
Connection
Reliability
NetworkError
```

The crate remains a facade over `ardosia-raknet`. It does not own MCPE packet,
session, game, player, or world concepts.

## Configuration API

`NetworkConfig` owns private fields. Construction is explicit and validated:

```rust
pub fn new(
    bind_addr: SocketAddr,
    raknet_protocols: impl IntoIterator<Item = u8>,
    max_connections: NonZeroUsize,
    advertisement: impl Into<String>,
    cookie_mode: CookieMode,
) -> Result<Self, NetworkConfigError>;

pub fn with_worker_shards(self, worker_shards: NonZeroUsize) -> Self;
```

`CookieMode` has exactly two variants:

```rust
pub enum CookieMode {
    Enabled,
    Disabled,
}
```

The constructor rejects an empty protocol set and duplicate protocol values.
It does not interpret the opaque advertisement. `NonZeroUsize` makes zero
connections and zero worker shards unrepresentable. No public field access is
required by the current server consumer.

`NetworkRuntimeConfig` is removed. Worker-shard selection is the only runtime
override currently used, so a separate public configuration object is not
justified.

Internal conversion to the vendor `TransportConfig` remains fallible because
the transport owns additional invariants. Such failure is reported through a
typed configuration error carried by `NetworkError`; it is not represented as
an arbitrary public field/message pair.

## Metrics removal

Delete the facade metrics subsystem:

- `metrics.rs`;
- `MetricsState` and all atomic/snapshot aggregation;
- `NetworkServer::metrics`;
- `NetworkServer::shard_metrics`;
- all exported network and transport metrics structs;
- tests whose only purpose is to preserve that public surface.

No production consumer currently reads this data. Low-level RakNet telemetry
remains unchanged in `ardosia-raknet`.

The backend must continue polling `RaknetServer::next_event`. A
`RaknetServerEvent::Metrics { .. }` event is explicitly ignored after removal
of aggregation so metric delivery cannot obstruct other event processing.
Connection admission, packet forwarding, disconnect, backpressure, decode
error, worker failure, and shutdown behavior must not change.

## Source organization

The intended source tree is:

```text
crates/ardosia-network/src/
├── lib.rs
├── backend.rs
├── config.rs
├── connection.rs
├── error.rs
├── reliability.rs
└── server.rs
```

Each file keeps one responsibility. No additional abstraction is introduced
merely to reduce line count. Private helpers remain near their only caller
unless they form a coherent independently testable unit.

## Error handling and control flow

Configuration failures are typed and distinguish caller mistakes from vendor
transport rejection. Runtime transport failures continue to use
`NetworkError`.

Backend event handling uses `?`, `let ... else`, and early returns for missing
peers, stopped channels, and terminal worker conditions when this makes the
normal flow flatter. Ordinary event dispatch remains a direct `match`; the
cleanup does not replace clear branching with clever combinators.

Peer close races and backpressure disconnects remain defined outcomes rather
than panics. Queue and allocation bounds remain explicit.

## Documentation policy

The crate root adds:

```rust
#![forbid(unsafe_code)]
#![deny(missing_docs)]
```

Every supported public type, variant, field, constructor, and method documents
its responsibility, failure behavior, and important invariants. Crate-level
documentation includes one minimal bind/accept/send/shutdown example using the
new configuration API.

Comments inside private code explain non-obvious lifecycle or compatibility
constraints. They do not narrate syntax.

## Tests

Tests preserve:

- rejection of an empty protocol set;
- rejection of duplicate protocols;
- nonzero connection and worker-shard construction;
- cookie-mode forwarding;
- advertisement forwarding;
- protocol-8 acceptance and other-protocol rejection;
- connection acceptance;
- `ReliableOrdered` payload round trips;
- fragmentation/reassembly behavior visible through the facade;
- backpressure and close-state behavior;
- graceful shutdown;
- the intended public facade and absence of removed exports.

Metrics-only tests are deleted. Tests of private mapping and event behavior
stay beside their implementation; integration tests exercise only supported
consumer API.

## Verification

Rust 1.98.0 is required:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo doc --no-deps
RUSTDOCFLAGS="-D missing_docs" cargo doc --no-deps
git diff --check
```

The server consumer is updated only after this facade passes independently.

## Non-goals

This cleanup does not:

- modify `ardosia-raknet`;
- add MCPE awareness;
- add a new metrics backend;
- add tracing, OpenTelemetry, or an administration endpoint;
- create a new repository;
- weaken transport safety or abuse-control behavior;
- redesign RakNet scheduling, reliability, congestion, or sharding.
