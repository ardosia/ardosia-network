# Ardosia Network Scaling Benchmarks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend `ardosia-loadgen` from a connection-count harness into a repeatable mixed-traffic scaling benchmark that reports transport health, RTT, server/loadgen CPU and RSS, and whole-host CPU/memory while keeping RakNet protocol 8 and MCPE game protocol concerns separate.

**Architecture:** Keep `ardosia-network` as the stable transport facade and ingest vendored RakNet telemetry inside its backend without exposing vendor types. Refactor `ardosia-loadgen local` into a parent orchestrator that spawns a child benchmark-server process, drives synthetic protocol-8 clients in the parent, samples both PIDs plus the host, coordinates an explicit measured steady-state window, and merges child/client/resource data into one structured report.

**Tech Stack:** Rust 2024, Rust 1.88, Tokio, `bytes`, Serde/JSON/TOML, Clap, vendored `mcbe-rs/raknet-rust` 0.2.0 at `3edfb4170e6cb5aeed992b09b50176fb7e5b6079`, Linux `/proc` resource accounting.

**Spec:** `docs/superpowers/specs/2026-08-18-scaling-benchmarks-design.md`

## Global Constraints

- `ardosia-network` remains transport-only; no MCPE packet IDs, codecs, login flow, world logic, or game semantics enter this repository.
- RakNet protocol 8 remains runtime configuration; do not patch the vendored default protocol constant.
- Vendored RakNet public types remain private implementation details of `ardosia-network`.
- Do not patch vendored RakNet internals unless a failing compatibility test or benchmark/profile proves a concrete need.
- Heavy 300/500/1000-client workflows remain `workflow_dispatch`-only.
- CPU, RSS, RTT, throughput, ACK/NACK, retransmission, and related performance/resource values are record-only in this phase; do not invent hard thresholds.
- `steady-300` is a strict correctness/transport-health gate: 300/300 establish, zero unexpected disconnects, zero protocol/decode errors, zero Ardosia queue/backpressure drops, required workload classes actually move traffic, and clients disconnect cleanly.
- Linux is the first-class resource-sampling target. Unsupported platforms report resource fields as unavailable rather than fabricated zeroes.
- Official scaling/capacity runs use `cargo run --release`; debug runs are valid for development correctness only. The report records build profile.
- Server process, load-generator process, and whole-host resource values are always reported separately for local runs.
- Ramp/handshake resource samples and measured steady-window samples are separate summaries; the primary steady-state CPU/RSS averages must not be diluted by ramp samples.

---

## File Structure for Phase 2

Existing files are kept where their responsibilities still fit. New files are split by one clear job:

```text
crates/ardosia-network/src/
├── backend.rs          # ingest RakNet events, including telemetry snapshots
├── metrics.rs          # Ardosia-owned lifecycle + rich transport metrics
└── server.rs           # unchanged public `NetworkServer::metrics()` entry point

crates/ardosia-loadgen/src/
├── child_protocol.rs   # JSON-line parent/child control messages
├── client_task.rs      # one RakNet client, workload send/recv, RTT probe tracking
├── frame.rs            # benchmark-only synthetic payload codec
├── latency.rs          # bounded RTT histogram + percentile summary
├── lib.rs
├── main.rs             # CLI, hidden child-server mode, terminal/JSON output
├── report.rs           # final environment/scenario/results report schema and gates
├── resource/
│   ├── mod.rs          # sampler/summary interfaces and unavailable fallback
│   └── linux.rs        # `/proc` parsing and Linux sampler
├── runner.rs           # parent orchestration and external-target client runs
├── scenario.rs         # declarative workload + RTT config and validation
├── server_target.rs    # child benchmark server and per-connection workload loop
└── workload.rs         # deterministic per-client/per-lane schedules

crates/ardosia-loadgen/tests/
├── child_protocol.rs
├── frame.rs
├── latency.rs
├── report.rs
├── resource.rs
├── scenario.rs
└── workload.rs

scenarios/
├── connect-300.toml
├── steady-300.toml
├── steady-500.toml
└── ceiling-1000.toml
```

---

### Task 1: Surface vendored RakNet telemetry through Ardosia-owned metrics

**Files:**
- Modify: `crates/ardosia-network/src/metrics.rs`
- Modify: `crates/ardosia-network/src/backend.rs`
- Modify: `crates/ardosia-network/src/lib.rs`
- Test: `crates/ardosia-network/src/metrics.rs` unit-test module

**Interfaces:**
- Consumes: `raknet_rust::server::RaknetServerEvent::Metrics { shard_id, snapshot, dropped_non_critical_events }` and `TransportMetricsSnapshot` internally only.
- Produces: `NetworkMetrics { ... existing lifecycle fields ..., transport: TransportMetrics }` where all public nested structs are Ardosia-owned.

Public shape to implement:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NetworkMetrics {
    pub accepted_total: u64,
    pub connected_current: u64,
    pub disconnected_total: u64,
    pub protocol_errors_total: u64,
    pub backpressure_disconnects_total: u64,
    pub transport: TransportMetrics,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TransportMetrics {
    pub sessions: TransportSessionMetrics,
    pub traffic: TransportTrafficMetrics,
    pub reliability: TransportReliabilityMetrics,
    pub queues: TransportQueueMetrics,
    pub ordering: TransportOrderingMetrics,
    pub timing: TransportTimingMetrics,
    pub dropped_non_critical_events: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransportSessionMetrics {
    pub current: u64,
    pub started_total: u64,
    pub closed_total: u64,
    pub timed_out_total: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransportTrafficMetrics {
    pub ingress_datagrams: u64,
    pub ingress_frames: u64,
    pub forwarded_packets: u64,
    pub forwarded_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransportReliabilityMetrics {
    pub reliable_sent_datagrams: u64,
    pub retransmitted_datagrams: u64,
    pub acks_out: u64,
    pub nacks_out: u64,
    pub acked_datagrams: u64,
    pub nacked_datagrams: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransportQueueMetrics {
    pub pending_outgoing_frames: u64,
    pub pending_outgoing_bytes: u64,
    pub outgoing_queue_drops: u64,
    pub outgoing_queue_defers: u64,
    pub outgoing_queue_disconnects: u64,
    pub backpressure_delays: u64,
    pub backpressure_drops: u64,
    pub backpressure_disconnects: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransportOrderingMetrics {
    pub duplicate_reliable_drops: u64,
    pub ordered_stale_drops: u64,
    pub ordered_buffer_full_drops: u64,
    pub sequenced_stale_drops: u64,
    pub split_ttl_drops: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TransportTimingMetrics {
    pub srtt_ms: Option<f64>,
    pub rtt_variance_ms: Option<f64>,
    pub resend_rto_ms: Option<f64>,
    pub congestion_window: Option<f64>,
}
```

`MetricsState` keeps current vendor telemetry per shard behind a short-held `std::sync::Mutex<BTreeMap<usize, ShardSnapshot>>`. Lifecycle atomics remain unchanged. Aggregate counter fields by sum, queue/session gauges by sum, and timing/congestion values by session-count weighted average. If aggregate session weight is zero, timing values are `None`.

- [ ] **Step 1: Write failing aggregation tests**

Add unit tests in `metrics.rs` that construct two private shard snapshots with known values, ingest them, and assert summed counters/gauges plus session-weighted timing. Example assertions:

```rust
assert_eq!(snapshot.transport.reliability.retransmitted_datagrams, 7);
assert_eq!(snapshot.transport.queues.pending_outgoing_bytes, 900);
assert_eq!(snapshot.transport.sessions.current, 3);
assert_eq!(snapshot.transport.timing.srtt_ms, Some(20.0));
```

Also test zero-session timing returns `None`.

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
cargo test -p ardosia-network metrics::tests -- --nocapture
```

Expected: compile failure because rich transport metric structs/ingestion methods do not exist.

- [ ] **Step 3: Implement Ardosia metric structs and private shard aggregation**

Keep all conversion from `TransportMetricsSnapshot` inside `metrics.rs`. Do not `pub use` vendor telemetry types.

- [ ] **Step 4: Handle `RaknetServerEvent::Metrics` in `backend.rs`**

Add a match arm before the fallback arm:

```rust
RaknetServerEvent::Metrics {
    shard_id,
    snapshot,
    dropped_non_critical_events,
} => {
    metrics.ingest_transport_snapshot(
        shard_id,
        *snapshot,
        dropped_non_critical_events,
    );
    false
}
```

- [ ] **Step 5: Export only Ardosia-owned metric types from `lib.rs`**

- [ ] **Step 6: Run network tests**

```bash
cargo test -p ardosia-network
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/ardosia-network
git commit -m "feat: expose transport telemetry metrics"
```

---

### Task 2: Extend scenario configuration and deterministic workload scheduling

**Files:**
- Modify: `crates/ardosia-loadgen/src/scenario.rs`
- Create: `crates/ardosia-loadgen/src/workload.rs`
- Modify: `crates/ardosia-loadgen/src/lib.rs`
- Modify: `crates/ardosia-loadgen/tests/scenario.rs`
- Create: `crates/ardosia-loadgen/tests/workload.rs`

**Interfaces:**
- Produces scenario types used by client/server tasks:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Scenario {
    pub name: String,
    pub clients: usize,
    pub protocol_version: u8,
    pub ramp_up_seconds: u64,
    pub hold_seconds: u64,
    pub connect_timeout_seconds: u64,
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default)]
    pub traffic: Vec<TrafficSpec>,
    #[serde(default)]
    pub rtt: Option<RttConfig>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrafficKind {
    Unreliable,
    ReliableOrdered,
    FragmentedReliableOrdered,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    ClientToServer,
    ServerToClient,
    Bidirectional,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrafficSpec {
    pub kind: TrafficKind,
    pub direction: Direction,
    pub packets_per_second_per_client: f64,
    pub payload_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RttConfig {
    pub probes_per_second_per_client: f64,
    pub payload_bytes: usize,
}
```

`workload.rs` produces deterministic lane timing without an RNG dependency:

```rust
pub(crate) struct WorkloadLane {
    pub kind: TrafficKind,
    pub direction: Direction,
    pub payload_bytes: usize,
    period: Duration,
    next: Instant,
}

pub(crate) fn initial_phase_offset(
    seed: u64,
    client_id: u64,
    lane_index: usize,
    period: Duration,
) -> Duration;
```

Use a tiny deterministic integer mixer (SplitMix64-style arithmetic) to map `(seed, client_id, lane_index)` into `[0, period)`. No synchronized all-client burst at `t=0`.

- [ ] **Step 1: Add scenario parsing tests for `steady-300` shape and Phase-1 compatibility**

Tests must prove the old `connect-300` TOML still parses with `traffic.is_empty()`, `rtt.is_none()`, and default seed `1`, while mixed TOML parses all three traffic kinds plus RTT.

- [ ] **Step 2: Add validation tests**

Reject:
- zero clients;
- non-finite, zero, or negative packet/probe rates;
- zero payload sizes;
- zero hold/connect timeout values.

- [ ] **Step 3: Run scenario tests and verify RED**

```bash
cargo test -p ardosia-loadgen --test scenario
```

- [ ] **Step 4: Implement scenario types/validation**

- [ ] **Step 5: Add deterministic schedule tests**

Given the same `(seed, client_id, lane_index, period)`, offsets must be identical across calls; changing client ID or lane should normally change the offset; every offset must be `< period`.

- [ ] **Step 6: Implement `workload.rs` mixer/scheduler helpers**

- [ ] **Step 7: Run focused tests**

```bash
cargo test -p ardosia-loadgen --test scenario --test workload
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/ardosia-loadgen/src/scenario.rs crates/ardosia-loadgen/src/workload.rs crates/ardosia-loadgen/src/lib.rs crates/ardosia-loadgen/tests/scenario.rs crates/ardosia-loadgen/tests/workload.rs
git commit -m "feat: add deterministic mixed workload scenarios"
```

---

### Task 3: Add benchmark frame codec and bounded RTT aggregation

**Files:**
- Create: `crates/ardosia-loadgen/src/frame.rs`
- Create: `crates/ardosia-loadgen/src/latency.rs`
- Modify: `crates/ardosia-loadgen/src/lib.rs`
- Create: `crates/ardosia-loadgen/tests/frame.rs`
- Create: `crates/ardosia-loadgen/tests/latency.rs`

**Interfaces:**

Benchmark framing stays entirely inside `ardosia-loadgen`:

```rust
pub(crate) const FRAME_MAGIC: [u8; 4] = *b"ARD2";
pub(crate) const FRAME_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameKind {
    UnreliableData = 1,
    ReliableOrderedData = 2,
    FragmentedReliableOrderedData = 3,
    EchoRequest = 4,
    EchoResponse = 5,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BenchmarkFrame {
    pub kind: FrameKind,
    pub client_id: u64,
    pub sequence: u64,
    pub probe_id: u64,
    pub payload: Bytes,
}

impl BenchmarkFrame {
    pub(crate) fn encode(&self) -> Bytes;
    pub(crate) fn decode(input: &[u8]) -> Result<Self, FrameError>;
}
```

Use a fixed header with magic/version/kind/client ID/sequence/probe ID followed by payload. Decode checks minimum length, magic, version, and known frame kind. Arbitrary input must return `FrameError`, never panic.

RTT aggregation uses one bounded histogram per client aggregate that can later be merged. Use 1 ms buckets from 0 through 10,000 ms plus an overflow bucket; store counts as `u32` or `u64`. This is fixed memory and gives millisecond-level p50/p95/p99 resolution without a new dependency.

```rust
#[derive(Debug, Clone, Default)]
pub(crate) struct LatencyHistogram { /* fixed buckets */ }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct LatencySummary {
    pub samples: u64,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub max_ms: Option<f64>,
    pub overflow_samples: u64,
}
```

- [ ] **Step 1: Write frame round-trip and malformed-input tests**

Cover every `FrameKind`, empty payload, 4 KiB payload, wrong magic, unsupported version, unknown kind, and truncated header.

- [ ] **Step 2: Run frame tests and verify RED**

```bash
cargo test -p ardosia-loadgen --test frame
```

- [ ] **Step 3: Implement the frame codec**

- [ ] **Step 4: Write latency tests**

Record known samples `[1, 2, 3, 4, 100] ms` and assert sample count, monotonic percentile ordering, max `100`, and non-empty percentile values. Record one sample above 10 seconds and assert `overflow_samples == 1` while max still records the true duration.

- [ ] **Step 5: Implement histogram record/merge/summary**

Provide:

```rust
pub(crate) fn record(&mut self, value: Duration);
pub(crate) fn merge(&mut self, other: &Self);
pub(crate) fn summary(&self) -> LatencySummary;
```

- [ ] **Step 6: Run focused tests**

```bash
cargo test -p ardosia-loadgen --test frame --test latency
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/ardosia-loadgen/src/frame.rs crates/ardosia-loadgen/src/latency.rs crates/ardosia-loadgen/src/lib.rs crates/ardosia-loadgen/tests/frame.rs crates/ardosia-loadgen/tests/latency.rs
git commit -m "feat: add benchmark framing and RTT histogram"
```

---

### Task 4: Implement Linux process and host resource sampling

**Files:**
- Create: `crates/ardosia-loadgen/src/resource/mod.rs`
- Create: `crates/ardosia-loadgen/src/resource/linux.rs`
- Modify: `crates/ardosia-loadgen/src/lib.rs`
- Create: `crates/ardosia-loadgen/tests/resource.rs`

**Interfaces:**

```rust
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ResourcePoint {
    pub process_cpu_pct: Option<f64>,
    pub process_rss_bytes: Option<u64>,
    pub host_cpu_pct: Option<f64>,
    pub host_memory_used_bytes: Option<u64>,
    pub host_memory_available_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceSummary {
    pub sample_count: u64,
    pub process_cpu_avg_pct: Option<f64>,
    pub process_cpu_peak_pct: Option<f64>,
    pub process_rss_avg_bytes: Option<u64>,
    pub process_rss_peak_bytes: Option<u64>,
    pub host_cpu_avg_pct: Option<f64>,
    pub host_cpu_peak_pct: Option<f64>,
    pub host_memory_used_avg_bytes: Option<u64>,
    pub host_memory_used_peak_bytes: Option<u64>,
    pub host_memory_available_min_bytes: Option<u64>,
}

pub(crate) struct ResourceSampler { /* prior counters + target pid */ }

impl ResourceSampler {
    pub(crate) fn for_pid(pid: u32) -> Self;
    pub(crate) fn sample(&mut self) -> ResourcePoint;
}

#[derive(Debug, Default)]
pub(crate) struct ResourceAccumulator;

impl ResourceAccumulator {
    pub(crate) fn push(&mut self, point: ResourcePoint);
    pub(crate) fn finish(self) -> ResourceSummary;
}
```

Linux implementation reads:
- `/proc/stat` for host total/idle ticks;
- `/proc/<pid>/stat` for process user+system ticks;
- `/proc/<pid>/status` `VmRSS:` for RSS;
- `/proc/meminfo` for `MemTotal`, `MemAvailable`; `used = total - available`.

Process CPU must allow values above 100%. Calculate it from process-tick delta relative to host-total-tick delta and multiply by logical CPU count:

```text
process_cpu_pct = process_delta / host_total_delta * logical_cpus * 100
```

This avoids requiring libc/sysconf clock-tick conversion. First sample has CPU `None` because there is no delta baseline yet.

- [ ] **Step 1: Add parser fixture tests**

Use literal fixture strings for `/proc/stat`, `/proc/<pid>/stat`, `/proc/<pid>/status`, and `/proc/meminfo`; do not make parser correctness depend only on the live CI host.

- [ ] **Step 2: Add accumulator tests**

Given three resource points, assert average/peak/min calculations and that missing optional values are ignored rather than treated as zero.

- [ ] **Step 3: Run tests and verify RED**

```bash
cargo test -p ardosia-loadgen --test resource
```

- [ ] **Step 4: Implement platform-neutral interfaces and Linux parser/sampler**

On non-Linux targets, `sample()` returns all optional fields unavailable. Keep `cfg(target_os = "linux")` localized to the resource module.

- [ ] **Step 5: Run resource tests**

```bash
cargo test -p ardosia-loadgen --test resource
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/ardosia-loadgen/src/resource crates/ardosia-loadgen/src/lib.rs crates/ardosia-loadgen/tests/resource.rs
git commit -m "feat: sample process and host resources"
```

---

### Task 5: Split local benchmarking into parent and child processes

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/ardosia-loadgen/src/child_protocol.rs`
- Modify: `crates/ardosia-loadgen/src/lib.rs`
- Modify: `crates/ardosia-loadgen/src/main.rs`
- Modify: `crates/ardosia-loadgen/src/runner.rs`
- Modify: `crates/ardosia-loadgen/src/server_target.rs`
- Create: `crates/ardosia-loadgen/tests/child_protocol.rs`

**Interfaces:**

Add Tokio features `process` and `io-util` to the existing workspace Tokio dependency. Do not add another process-control crate.

Parent/child communication is newline-delimited JSON on stdin/stdout; human diagnostics go to stderr.

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ChildCommand {
    Start {
        bind_addr: String,
        scenario: Scenario,
    },
    BeginMeasurement,
    Stop,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ChildEvent {
    Ready { pid: u32 },
    MeasurementStarted,
    Stopped { report: ServerRunReport },
    Error { message: String },
}
```

Add a hidden Clap subcommand:

```rust
#[command(hide = true)]
ServeChild
```

`run_local` uses `std::env::current_exe()` and `tokio::process::Command` with piped stdin/stdout and inherited stderr. It sends `Start`, waits for `Ready`, and guarantees child cleanup/reaping on success or error. Unexpected child exit becomes `RunnerError::ChildExited`.

For this task, child `ServerRunReport` may contain only final `NetworkMetrics` and empty/default workload counters; traffic behavior is Task 6.

- [ ] **Step 1: Add JSON protocol round-trip tests**

Serialize and deserialize `Start`, `BeginMeasurement`, `Ready`, and `Stopped`; assert enum variants and scenario values survive intact.

- [ ] **Step 2: Run protocol tests and verify RED**

```bash
cargo test -p ardosia-loadgen --test child_protocol
```

- [ ] **Step 3: Implement `child_protocol.rs` and export internally**

- [ ] **Step 4: Add Tokio process/io features and hidden child command**

- [ ] **Step 5: Implement child read-loop and parent spawn/read/write helpers**

Use `tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader}`. Every command/event is one compact JSON line plus `\n`.

- [ ] **Step 6: Refactor `run_local` to use the child server instead of `spawn_local_target`**

At the end of this step, the existing `connect-300` behavior must still work, but server and clients are different PIDs.

- [ ] **Step 7: Add an integration smoke test for child ready/stop**

Create one small in-process test scenario with 1 client and 1-second hold. Spawn the compiled current test binary only if practical; otherwise test the child loop by connecting it to duplex async IO. The test must prove `Start -> Ready -> BeginMeasurement -> MeasurementStarted -> Stop -> Stopped` ordering and no orphan task.

- [ ] **Step 8: Run workspace tests**

```bash
cargo test --workspace
```

Expected: PASS, including existing protocol-8 transport tests.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml crates/ardosia-loadgen
git commit -m "feat: isolate local benchmark server process"
```

---

### Task 6: Implement bidirectional synthetic workload and RTT probes

**Files:**
- Modify: `crates/ardosia-loadgen/src/client_task.rs`
- Modify: `crates/ardosia-loadgen/src/server_target.rs`
- Modify: `crates/ardosia-loadgen/src/workload.rs`
- Modify: `crates/ardosia-loadgen/src/frame.rs`
- Modify: `crates/ardosia-loadgen/src/runner.rs`
- Modify: `crates/ardosia-loadgen/src/child_protocol.rs`
- Create or modify focused tests under `crates/ardosia-loadgen/tests/`

**Interfaces:**

Refactor client phase to explicit measurement semantics:

```rust
pub(crate) enum Phase {
    Ramp,
    Measure { deadline: Instant },
    Abort,
}
```

Client result becomes:

```rust
#[derive(Debug, Default)]
pub(crate) struct ClientTaskResult {
    pub(crate) unexpected_disconnects: usize,
    pub(crate) protocol_errors: usize,
    pub(crate) clean_disconnects: usize,
    pub(crate) send_errors: usize,
    pub(crate) workload: WorkloadCounts,
    pub(crate) latency: LatencyHistogram,
}
```

`WorkloadCounts` tracks tx/rx frames and payload bytes separately for unreliable, reliable-ordered, fragmented-reliable-ordered, and RTT probes/responses.

Each client owns one RakNet client and one event loop. Do not spawn concurrent tasks that need mutable access to the same `RaknetClient`. Instead select between `client.next_event()`, phase changes, measurement deadline, and the next scheduled outgoing lane.

Map traffic kinds to vendor client reliability only inside loadgen:
- `Unreliable` -> vendor `Unreliable`;
- `ReliableOrdered` -> vendor `ReliableOrdered`;
- `FragmentedReliableOrdered` -> vendor `ReliableOrdered` with payload size large enough for RakNet fragmentation.

RTT logic:
- assign monotonically increasing `probe_id` per client;
- store `probe_id -> Instant` in a bounded pending map;
- send `EchoRequest` at scenario rate;
- server returns `EchoResponse` carrying the same client ID/sequence/probe ID/payload;
- on response, remove the pending probe and record `Instant::elapsed()` in the client histogram;
- bound pending probe entries to a small constant such as 256; expiry/overflow increments protocol/workload error counters rather than growing memory indefinitely.

Server behavior:
- every accepted `Connection` is owned by one server connection task;
- during measurement it receives/decodes benchmark frames and independently schedules configured server-to-client traffic for that connection;
- `EchoRequest` is echoed immediately as `EchoResponse` using `Reliability::ReliableOrdered`;
- server-generated traffic uses the scenario's configured reliability and deterministic per-connection schedule;
- no global broadcast/fanout loop exists in this task.

- [ ] **Step 1: Add a 1-client bidirectional workload integration test**

Use a short scenario with all traffic kinds and RTT. Assert non-zero tx/rx counts in both directions, at least one RTT sample, zero protocol errors, and clean disconnect.

- [ ] **Step 2: Run the integration test and verify RED**

```bash
cargo test -p ardosia-loadgen bidirectional -- --nocapture
```

- [ ] **Step 3: Implement client-side workload scheduling and incoming frame counting**

- [ ] **Step 4: Implement server per-connection workload loop and RTT echo**

- [ ] **Step 5: Add fragmented 4 KiB traffic assertion**

Assert the received benchmark frame payload length matches the configured 4096-byte payload and no decode error occurs.

- [ ] **Step 6: Add pending-RTT bound test**

Drive more than the configured pending-probe capacity without responses and assert the structure remains bounded and reports the overflow condition.

- [ ] **Step 7: Run loadgen tests**

```bash
cargo test -p ardosia-loadgen
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/ardosia-loadgen
git commit -m "feat: generate mixed RakNet workloads"
```

---

### Task 7: Coordinate measurement windows, resources, transport deltas, and final report schema

**Files:**
- Modify: `crates/ardosia-loadgen/src/runner.rs`
- Modify: `crates/ardosia-loadgen/src/report.rs`
- Modify: `crates/ardosia-loadgen/src/main.rs`
- Modify: `crates/ardosia-loadgen/src/resource/mod.rs`
- Modify: `crates/ardosia-loadgen/src/child_protocol.rs`
- Modify: `crates/ardosia-loadgen/tests/report.rs`
- Add focused aggregation tests as needed

**Interfaces:**

Replace the Phase-1 flat report with the approved three-domain shape while retaining explicit correctness fields:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub environment: EnvironmentReport,
    pub scenario: Scenario,
    pub results: ResultsReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentReport {
    pub ardosia_git_commit: Option<String>,
    pub vendor_revision: String,
    pub rust_version: Option<String>,
    pub os: String,
    pub kernel: Option<String>,
    pub architecture: String,
    pub logical_cpus: Option<usize>,
    pub total_memory_bytes: Option<u64>,
    pub build_profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultsReport {
    pub correctness: RunCounts,
    pub workload: WorkloadReport,
    pub latency: LatencySummary,
    pub transport: TransportWindowReport,
    pub resources: ResourceWindowsReport,
    pub total_duration_ms: u64,
    pub measured_duration_ms: u64,
    pub passed: bool,
    pub failure_reasons: Vec<String>,
}
```

Resource windows:

```rust
pub struct ResourceWindowsReport {
    pub ramp_server: Option<ResourceSummary>,
    pub ramp_loadgen: ResourceSummary,
    pub ramp_host: ResourceSummary,
    pub steady_server: Option<ResourceSummary>,
    pub steady_loadgen: ResourceSummary,
    pub steady_host: ResourceSummary,
}
```

If sampler implementation naturally combines process+host in one point, the report may normalize into separate server/loadgen/host structs; the user-visible requirement is separate values, not this exact storage internals.

Transport window reporting captures:
- start `NetworkMetrics` from the child/server near measurement start;
- periodic snapshots during the steady window;
- end snapshot;
- counter deltas (`end - start`, saturating);
- peak queue gauges across periodic samples.

Because telemetry lives in the child process, extend the child protocol with a periodic `Metrics` event or a parent `Snapshot` command. Prefer a parent `Snapshot` command once per resource-sampling tick to avoid unsolicited stdout traffic:

```rust
ChildCommand::Snapshot
ChildEvent::Snapshot { metrics: NetworkMetrics, workload: WorkloadCounts }
```

To serialize Ardosia metrics across the child control protocol, add Serde derives to Ardosia-owned metric structs only if necessary; never derive/serialize vendor types.

Measurement sequence for `local`:

1. spawn child and receive `Ready`;
2. start ramp resource accumulators and connect all clients;
3. if establishment fails, send `Stop`, collect final report, fail without starting a steady window;
4. send `BeginMeasurement`; wait for `MeasurementStarted`;
5. switch resource accumulators to steady window and send `Phase::Measure` to clients;
6. every ~1 second sample server PID, loadgen PID, host, and request child metrics snapshot;
7. after the hold window, await client completion;
8. send `Stop`, collect final server report, reap child;
9. compute deltas/peaks/report gate.

Environment collection is best-effort:
- git SHA: `git rev-parse HEAD` if available;
- Rust version: `rustc --version` if available;
- kernel: `/proc/sys/kernel/osrelease` on Linux;
- architecture: `std::env::consts::ARCH`;
- build profile: `"debug"` when `cfg!(debug_assertions)`, otherwise `"release"`.

- [ ] **Step 1: Rewrite report tests first**

Tests must prove:
- a clean `connect-300`-style report still passes;
- `steady-300` fails on one unexpected disconnect;
- `steady-300` fails on non-zero queue/backpressure drops;
- `steady-300` fails if a required configured workload class has zero sent/received frames;
- high CPU/RSS/RTT/retransmit values alone do not fail the run.

- [ ] **Step 2: Run report tests and verify RED**

```bash
cargo test -p ardosia-loadgen --test report
```

- [ ] **Step 3: Implement child snapshot request/event and transport window aggregation**

- [ ] **Step 4: Refactor `run_local` into ramp + steady measurement phases with two PID samplers**

Create one sampler for child PID and one for `std::process::id()`. Host samples may be taken once per tick and fed to host accumulators, rather than reading `/proc/stat` twice.

- [ ] **Step 5: Implement environment report collection and new `RunReport` assembly**

- [ ] **Step 6: Update terminal summary**

Print to stderr a concise summary containing at least:

```text
sessions: 300/300
errors: disconnect=0 protocol=0 backpressure_drop=0
traffic: tx=... pkt/s rx=... pkt/s
rtt: p50=... p95=... p99=... max=...
raknet: retransmits=... ack=... nack=... queue_peak=... bytes
server: cpu_avg=... cpu_peak=... rss_peak=...
loadgen: cpu_avg=... cpu_peak=... rss_peak=...
host: cpu_avg=... memory_peak=...
result: PASS|FAIL
```

Keep stdout as the JSON report so scripts can redirect it cleanly.

- [ ] **Step 7: Run loadgen tests**

```bash
cargo test -p ardosia-loadgen
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/ardosia-loadgen
git commit -m "feat: report scaling telemetry and resources"
```

---

### Task 8: Add scaling scenarios, manual release workflow, and verify `steady-300`

**Files:**
- Create: `scenarios/steady-300.toml`
- Create: `scenarios/steady-500.toml`
- Create: `scenarios/ceiling-1000.toml`
- Modify: `.github/workflows/baseline.yml`
- Create: `docs/results/2026-08-18-steady-300.md` only after an actual successful run; if the run fails, document the observed failure instead of claiming success.

**Scenario content:**

`steady-300.toml`:

```toml
name = "steady-300"
clients = 300
protocol_version = 8
ramp_up_seconds = 10
hold_seconds = 60
connect_timeout_seconds = 5
seed = 1

[[traffic]]
kind = "unreliable"
direction = "bidirectional"
packets_per_second_per_client = 20.0
payload_bytes = 64

[[traffic]]
kind = "reliable_ordered"
direction = "bidirectional"
packets_per_second_per_client = 2.0
payload_bytes = 256

[[traffic]]
kind = "fragmented_reliable_ordered"
direction = "bidirectional"
packets_per_second_per_client = 0.2
payload_bytes = 4096

[rtt]
probes_per_second_per_client = 2.0
payload_bytes = 32
```

`steady-500.toml` uses the same workload and timing with `clients = 500`.

`ceiling-1000.toml` uses `clients = 1000`, a 20-second ramp, 60-second hold, and a deliberately moderate workload:

```toml
[[traffic]]
kind = "unreliable"
direction = "bidirectional"
packets_per_second_per_client = 10.0
payload_bytes = 64

[[traffic]]
kind = "reliable_ordered"
direction = "bidirectional"
packets_per_second_per_client = 1.0
payload_bytes = 256

[[traffic]]
kind = "fragmented_reliable_ordered"
direction = "bidirectional"
packets_per_second_per_client = 0.1
payload_bytes = 4096

[rtt]
probes_per_second_per_client = 1.0
payload_bytes = 32
```

Update the manual workflow to keep `workflow_dispatch` as the only trigger and add a scenario choice. Official heavy runs use release mode:

```yaml
on:
  workflow_dispatch:
    inputs:
      scenario:
        description: Benchmark scenario
        required: true
        type: choice
        default: steady-300
        options:
          - connect-300
          - steady-300
          - steady-500
          - ceiling-1000

# ...
- name: Run selected RakNet benchmark
  run: cargo run --release -p ardosia-loadgen -- local scenarios/${{ inputs.scenario }}.toml
```

- [ ] **Step 1: Add scenario parser tests using all checked-in scaling files**

Read each scenario file and assert validation succeeds plus expected client counts/workload entries.

- [ ] **Step 2: Run all tests locally or in a single explicit verification run**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --manifest-path vendor/raknet-rust/Cargo.toml --lib
```

All must pass before running the heavy benchmark.

- [ ] **Step 3: Confirm workflow trigger is still manual-only**

Inspect `.github/workflows/baseline.yml` and `.github/workflows/ci.yml`; neither heavy benchmark nor ordinary CI should gain `push`/`pull_request` triggers unless the user explicitly changes the budget policy.

- [ ] **Step 4: Run `steady-300` in release mode**

```bash
cargo run --release -p ardosia-loadgen -- local scenarios/steady-300.toml > steady-300.json
```

Expected correctness gate:
- successful handshakes: 300/300;
- unexpected disconnects: 0;
- protocol/decode errors: 0;
- Ardosia queue/backpressure drops: 0;
- all configured workload classes have non-zero tx/rx counts in required directions;
- RTT sample count > 0;
- 300 clean disconnects;
- resource fields are present on Linux but do not affect pass/fail.

- [ ] **Step 5: Inspect benchmark evidence before any optimization**

Record from the JSON:
- server/loadgen/host steady CPU summaries;
- server/loadgen RSS peak/average;
- host used/available memory;
- workload packet/byte rates;
- p50/p95/p99/max RTT;
- ACK/NACK counts;
- retransmissions;
- queue peaks and drops;
- split/order pressure;
- actual measured duration and build profile.

Do not change vendor code merely because a number is non-zero. A patch proposal requires a correctness failure or profile-backed bottleneck.

- [ ] **Step 6: Document the actual `steady-300` result**

Create `docs/results/2026-08-18-steady-300.md` with hardware/environment, git SHA, vendor SHA, scenario, pass/fail, and the key observed metrics. If the run fails, document the failure and stop before `steady-500`.

- [ ] **Step 7: If and only if `steady-300` passes, run characterization scenarios intentionally**

Run `steady-500` next. Run `ceiling-1000` only after the 500-session report is inspected. These are characterization runs; high resource/latency values are observations, but correctness failures must be reported precisely.

- [ ] **Step 8: Final commit**

```bash
git add scenarios .github/workflows/baseline.yml docs/results
git commit -m "bench: add mixed traffic scaling scenarios"
```

---

## Final Verification Gate

Before declaring Phase 2 implementation complete, run fresh commands against the final feature-branch source tree:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --manifest-path vendor/raknet-rust/Cargo.toml --lib
```

Then verify the latest release-mode `steady-300` report actually corresponds to the final source SHA. If Rust source changes after that heavy run, rerun the correctness suite and decide whether the change can affect benchmark behavior; rerun `steady-300` whenever it can.

Phase 2 is complete only when the implementation produces a truthful report with separate server/loadgen/host resource metrics and the `steady-300` strict correctness gate passes, or when a concrete failure is documented and the work stops at that evidence for diagnosis.
