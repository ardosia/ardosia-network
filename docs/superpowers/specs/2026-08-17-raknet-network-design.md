# Ardosia Network and RakNet Benchmark Design

Date: 2026-08-17
Status: Design approved in chat; written spec pending user review

## Context

`ardosia-network` owns Ardosia's transport layer. It is responsible for UDP and RakNet behavior only. Minecraft/MCPE game packet definitions, codecs, login flow, and version-specific game protocol behavior belong in the separate `ardosia-protocol` repository.

The immediate goal is to avoid implementing RakNet from scratch while establishing confidence that the chosen implementation can support at least 300 concurrent players under realistic transport pressure. The selected base is `mcbe-rs/raknet-rust`.

Upstream snapshot to vendor:

- Repository: `https://github.com/mcbe-rs/raknet-rust`
- Commit: `3edfb4170e6cb5aeed992b09b50176fb7e5b6079`
- Crate version: `0.2.0`
- License: Apache-2.0
- Minimum Rust version declared upstream: 1.85

The snapshot is intentionally pinned. Ardosia will not vendor an unpinned branch.

## Goals

1. Provide a small, stable Ardosia transport API over RakNet.
2. Keep MCPE game protocol concerns completely outside this repository.
3. Vendor `raknet-rust` at a known revision while retaining upstream licensing and provenance.
4. Run RakNet protocol version 8 without patching upstream solely for version negotiation.
5. Establish a repeatable load generator for 300, 500, and 1000 concurrent RakNet sessions.
6. Measure throughput, latency, reliability pressure, resource usage, and disconnect/error behavior.
7. Patch the vendored implementation only when compatibility tests or profiling demonstrate a need.

## Non-goals

The first implementation will not:

- implement MCPE packet codecs or game packet IDs;
- implement MCPE login, encryption, world, entity, inventory, or gameplay behavior;
- emulate a complete MCPE 0.15.10 player in the load generator;
- rewrite RakNet reliability, congestion, or scheduling code preemptively;
- promise production capacity from localhost-only results;
- expose `raknet-rust` public types as Ardosia's stable public API.

## Repository Boundary

The intended ownership model is:

```text
ardosia-network
    UDP sockets/runtime
    RakNet offline handshake
    RakNet sessions
    ACK/NACK
    reliability
    ordering/sequencing
    fragmentation/reassembly
    retransmission/congestion
    MTU handling
    connection lifecycle
    transport metrics

ardosia-protocol
    MCPE packet IDs
    MCPE packet structs
    encode/decode
    varints and game framing
    login/game protocol
    version-specific packet behavior
```

Neither repository should depend on the other at the library layer. A future server/integration layer will combine them.

## Workspace Layout

```text
ardosia-network/
├── Cargo.toml
├── README.md
├── crates/
│   ├── ardosia-network/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs
│   │       ├── connection.rs
│   │       ├── server.rs
│   │       ├── reliability.rs
│   │       └── metrics.rs
│   └── ardosia-loadgen/
│       └── src/
│           ├── main.rs
│           ├── client.rs
│           ├── scenario.rs
│           ├── workload.rs
│           ├── metrics.rs
│           └── report.rs
├── vendor/
│   └── raknet-rust/
│       ├── UPSTREAM.md
│       ├── LICENSE
│       └── ... upstream snapshot ...
├── scenarios/
│   ├── connect-300.toml
│   ├── steady-300.toml
│   ├── steady-500.toml
│   ├── churn-500.toml
│   └── ceiling-1000.toml
└── docs/
    └── superpowers/specs/
```

`vendor/raknet-rust/UPSTREAM.md` will record the source repository, exact commit, crate version, vendoring date, and a short list of Ardosia-local modifications once any exist.

This design does not choose a license for Ardosia's own repository. The vendored upstream Apache-2.0 license is retained regardless of any later project-level licensing decision.

## Dependency Direction

Production code talks to `ardosia-network`, not directly to `raknet-rust`.

```text
Ardosia server/application
          |
          v
   ardosia-network API
          |
          v
 vendor/raknet-rust
          |
          v
         UDP
```

The load generator may use the vendored `RaknetClient` internally because it is a repository-local benchmark tool, not part of the stable application API. Compatibility tests must not rely exclusively on a same-library client/server round trip; at minimum, protocol-8 offline handshake fixtures will exercise the server with independently constructed bytes.

## Public API Shape

The public API should stay intentionally small. Exact Rust syntax may change during implementation, but the conceptual boundary is:

```rust
pub struct NetworkConfig {
    pub bind_addr: SocketAddr,
    pub raknet_protocols: Vec<u8>,
    pub max_connections: usize,
}

pub enum Reliability {
    Unreliable,
    UnreliableSequenced,
    Reliable,
    ReliableOrdered,
    ReliableSequenced,
}

pub struct NetworkServer { /* private backend */ }

impl NetworkServer {
    pub async fn bind(config: NetworkConfig) -> Result<Self, NetworkError>;
    pub async fn accept(&mut self) -> Result<Connection, NetworkError>;
    pub fn metrics(&self) -> NetworkMetrics;
}

pub struct Connection { /* private backend */ }

impl Connection {
    pub fn peer_addr(&self) -> SocketAddr;
    pub async fn recv(&mut self) -> Result<bytes::Bytes, NetworkError>;
    pub async fn send(
        &mut self,
        payload: bytes::Bytes,
        reliability: Reliability,
    ) -> Result<(), NetworkError>;
    pub async fn close(&mut self) -> Result<(), NetworkError>;
}
```

Ardosia defines its own `Reliability`, errors, configuration, connection handles, and metrics facade. Conversion into `raknet-rust` types stays private.

No MCPE packet type is accepted or returned by this API. Payloads are opaque bytes.

## Protocol Version 8

`raknet-rust` already supports configuring the RakNet version at both ends:

- Server: `TransportConfig::supported_protocols: Vec<u8>`
- Client: `RaknetClientConfig::protocol_version: u8`

The Ardosia server configuration for the initial target will therefore set:

```rust
supported_protocols = vec![8];
```

The load generator will set:

```rust
protocol_version = 8;
```

The upstream constant currently defaults to protocol 11, but Ardosia must not edit that constant merely to run protocol 8. Version 8 is an explicit runtime configuration.

If MCPE 0.15.10 later reveals behavior differences beyond the negotiated version byte, those changes will be isolated as compatibility patches with regression tests.

RakNet protocol version and MCPE game protocol version are separate concerns. `ardosia-network` owns the former only.

## Vendoring Policy

The initial vendor import should be mechanically identical to upstream commit `3edfb4170e6cb5aeed992b09b50176fb7e5b6079`, except for repository-placement changes that do not alter source behavior.

Rules:

1. Preserve the upstream Apache-2.0 license in the vendored directory.
2. Record upstream repository and commit in `UPSTREAM.md`.
3. Keep Ardosia-specific behavioral changes as focused commits and document them in `UPSTREAM.md`.
4. Do not modify vendored transport internals until a test, compatibility requirement, or profile justifies the change.
5. When updating upstream, compare against the pinned revision and re-run the complete compatibility and benchmark suite.

## Load Generator Architecture

`ardosia-loadgen` is a standalone binary in the workspace. Its first purpose is transport benchmarking, not game emulation.

A run consists of:

1. parse scenario configuration;
2. create the requested number of protocol-8 RakNet clients;
3. ramp clients according to scenario settings rather than always connecting all at once;
4. wait for the required number of established sessions;
5. execute one or more synthetic traffic workloads;
6. collect client, server, and process metrics;
7. disconnect cleanly where the scenario requires it;
8. print a human-readable summary and emit machine-readable JSON.

The load generator should support deterministic seeds for payload/workload generation so regressions can be reproduced.

## Scenario Model

Scenario files use TOML and should support at least:

```toml
name = "steady-300"
clients = 300
protocol_version = 8
ramp_up_seconds = 10
steady_seconds = 60

[traffic.unreliable]
packets_per_second_per_client = 20
payload_bytes = 64

[traffic.reliable_ordered]
packets_per_second_per_client = 5
payload_bytes = 256

[traffic.fragmented]
packets_per_second_per_client = 0.2
payload_bytes = 4096
```

Exact configuration field names may be refined during implementation, but scenarios must be declarative and checked into the repository.

### Required Initial Scenarios

#### `connect-300`

Purpose: handshake/session baseline.

- ramp from 0 to 300 clients;
- establish protocol-8 sessions;
- hold all sessions for 60 seconds;
- minimal application payload traffic.

Acceptance gate:

- 300/300 sessions established;
- all remain connected for 60 seconds;
- zero unexpected disconnects;
- zero protocol errors.

#### `steady-300`

Purpose: first realistic steady-state transport load.

- 300 established clients;
- high-frequency small unreliable traffic;
- lower-frequency reliable ordered traffic;
- occasional fragmented payloads;
- 60-second measured steady window after ramp-up.

#### `steady-500`

Purpose: determine headroom beyond the required 300-player target using the same workload shape as `steady-300`.

#### `churn-500`

Purpose: pressure handshake/session lifecycle paths.

- maintain a target population around 500;
- repeatedly disconnect and reconnect a configured percentage of clients;
- verify established sessions remain healthy while churn occurs.

#### `ceiling-1000`

Purpose: expose architectural cliffs, not define a production requirement.

- ramp toward 1000 concurrent sessions;
- use a moderate mixed workload;
- record failure point and degradation if full establishment is not achieved;
- this scenario is diagnostic and does not need to pass the same strict gate as `connect-300`.

## Network Impairment

Loss and jitter testing is required, but should not contaminate the first baseline implementation.

Phase 1 establishes repeatable localhost/LAN baselines without artificial impairment.

Phase 2 adds loss/jitter support. Prefer an explicit impairment layer or documented OS-level network emulation over adding random packet drops deep inside production RakNet code. The mechanism must be reproducible from scenario configuration or a checked-in benchmark script.

Target impairment profiles include 1%, 3%, and 5% packet loss plus configurable jitter.

## Metrics

The benchmark must make transport pressure observable rather than report only wall-clock duration.

### Session and correctness

- requested clients;
- successful handshakes;
- failed handshakes;
- currently established sessions;
- unexpected disconnects;
- protocol errors;
- clean disconnects.

### Traffic

- packets received/sent per second;
- bytes received/sent per second;
- useful payload bytes where measurable;
- fragmented messages/bytes where exposed.

### Reliability

Expose existing `raknet-rust` telemetry where available and extend the Ardosia facade only as needed:

- ACK count/rate;
- NACK count/rate;
- retransmissions;
- reliable-window occupancy/pressure;
- outgoing queue occupancy/bytes;
- dropped or rejected queued payloads;
- split/reassembly pressure.

### Latency

The synthetic workload will include an application-level echo payload with a monotonic sequence/timestamp so the benchmark can compute:

- p50 RTT;
- p95 RTT;
- p99 RTT;
- maximum observed RTT.

Percentiles must be based on bounded-memory histogram/summary data, not storing every sample indefinitely.

### Process/runtime

Where portable enough for the first implementation:

- process CPU utilization;
- resident memory (RSS);
- run duration.

If process metrics are not reliable on a supported platform, the benchmark must report them as unavailable rather than fabricate values. Transport/session metrics remain mandatory.

## Benchmark Server Behavior

The benchmark target server intentionally does almost no application work. It accepts opaque payloads and implements only the tiny synthetic behaviors required by the load test, such as echoing benchmark probe frames.

This prevents MCPE packet serialization, world simulation, or game logic from becoming the measured bottleneck.

The server used in benchmarks must exercise the same `ardosia-network` public server API intended for future application code. Benchmark-only shortcuts must remain outside the production library.

## Compatibility Tests

The first compatibility suite must include:

1. server accepts `OpenConnectionRequest1` advertising RakNet protocol 8;
2. unsupported protocol versions are rejected correctly;
3. a protocol-8 client can complete the offline and connected handshake;
4. reliable ordered payload delivery works across a protocol-8 session;
5. fragmented payloads reassemble correctly;
6. disconnect/timeout behavior releases session state;
7. independently constructed raw protocol-8 offline handshake bytes are accepted, avoiding a test suite that proves only that the same library can talk to itself.

A later integration test with an actual MCPE 0.15.10 client or packet capture should verify historical-client behavior. Full MCPE login is not part of this repository.

## Error Handling and Backpressure

`ardosia-network` must distinguish normal transport lifecycle from internal failure. Public errors should be typed rather than string-only.

Expected categories include:

- bind/socket failure;
- configuration error;
- handshake rejection;
- connection closed/timed out;
- send backpressure/queue rejection;
- malformed transport packet;
- internal backend error.

The wrapper must not silently turn bounded-queue rejection into success. If the vendored implementation rejects payload because of transport limits, application code and benchmarks need to observe that event.

## Testing Strategy

### Unit tests

Cover Ardosia-owned mapping and configuration logic:

- public reliability enum to vendor reliability mapping;
- protocol list/config propagation;
- error conversion;
- scenario parsing/validation;
- metrics aggregation.

### Integration tests

Run real UDP sockets on loopback:

- protocol-8 handshake;
- unsupported version rejection;
- send/receive reliability modes used by Ardosia;
- fragmentation/reassembly;
- session timeout/close behavior;
- raw handshake fixture interoperability.

### Load tests

Load scenarios are not ordinary `cargo test` tests. They are explicit binaries/commands with structured output. The 300-client baseline can later be used as a CI/nightly smoke benchmark, but the first implementation should not make normal unit-test execution depend on a heavy load run.

## Results and Reproducibility

Each load run prints a concise terminal report and can write JSON containing:

- scenario name and full resolved configuration;
- git commit of `ardosia-network` when available;
- pinned vendor revision;
- OS/architecture;
- logical CPU count;
- Rust version;
- start time and duration;
- all collected metrics and percentile summaries;
- pass/fail reason for scenarios with an acceptance gate.

Generated benchmark output is not committed by default. Checked-in scenario definitions and result schema are committed.

## Initial Decision Gates

Implementation proceeds in this order:

1. Vendor the exact `raknet-rust` snapshot with provenance/license intact.
2. Build the `ardosia-network` wrapper without modifying vendor behavior.
3. Prove protocol-8 handshake and transport compatibility through tests.
4. Build `connect-300` and pass its 300/300, 60-second acceptance gate.
5. Add mixed workloads and pass `steady-300` without correctness failures.
6. Run `steady-500`, `churn-500`, and `ceiling-1000` to locate headroom and cliffs.
7. Profile observed bottlenecks.
8. Only then propose vendor patches, each backed by a failing test or benchmark/profile evidence.

## Success Criteria

The first implementation phase is successful when:

- `ardosia-network` exposes a small transport-only API with no MCPE game protocol dependency;
- `raknet-rust` is pinned and auditable;
- RakNet protocol 8 is configured without changing the upstream default constant;
- compatibility tests cover handshake, reliability, fragmentation, and lifecycle;
- the load generator reproduces checked-in scenarios;
- `connect-300` establishes and holds 300/300 protocol-8 sessions for 60 seconds with zero unexpected disconnects and zero protocol errors;
- `steady-300` produces usable throughput, latency, reliability, and resource metrics;
- higher-load scenarios provide evidence for whether vendor optimization is actually necessary.

## Deferred Decisions

The following are deliberately deferred until evidence exists:

- whether `raknet-rust` needs scheduler/sharding changes for Ardosia;
- whether Ardosia needs a separate maintained fork rather than an in-tree vendor snapshot;
- exact production socket buffer sizes and shard counts;
- the real-player traffic distribution used after `ardosia-protocol` exists;
- CI frequency and hardware for heavyweight load scenarios;
- multi-host load generation for loads that exceed one machine's useful benchmark range.
