# Network Policy Observability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make local benchmark reports record the effective Ardosia/RakNet abuse-control ceilings that can masquerade as transport capacity, without exposing hardfork types or adding policy-bypass knobs.

**Architecture:** `NetworkServer` will capture a read-only Ardosia-owned snapshot from the exact `TransportConfig` passed to the hardfork during bind. The local benchmark child will carry that snapshot through its existing stop report, and `RunReport` will serialize it as optional historical-compatible metadata. This is observability only: no rate-limit, processing-budget, transport, or benchmark correctness behavior changes.

**Tech Stack:** Rust 1.88.0, existing `ardosia-network` facade, serde/serde_json in `ardosia-loadgen`, existing child JSON protocol.

**Spec:** `docs/benchmarks.md`

## Global Constraints

- Rust quality gate remains `1.88.0` with `--locked`.
- Do not add third-party dependencies.
- Do not expose `raknet_rust` types in the public Ardosia API.
- Do not add setters or CLI flags for packet windows or processing budgets in this work.
- Do not change production hardfork defaults, localhost exceptions, benchmark pass/fail gates, or transport behavior.
- Reports from remote/client-only runs may omit server-policy metadata.
- Historical run/child JSON must remain deserializable through serde defaults.

---

### Task 1: Capture the effective policy at the network facade boundary

**Files:**
- Modify: `crates/ardosia-network/src/config.rs`
- Modify: `crates/ardosia-network/src/server.rs`
- Modify: `crates/ardosia-network/src/lib.rs`
- Create: `crates/ardosia-network/tests/policy_snapshot.rs`

**Interfaces:**
- Produces: `NetworkPolicySnapshot`
- Produces: `PacketWindowSnapshot`
- Produces: `ProcessingBudgetSnapshot`
- Produces: `NetworkServer::policy_snapshot(&self) -> NetworkPolicySnapshot`

`PacketWindowSnapshot` fields:
- `per_ip_packet_limit: usize`
- `global_packet_limit: usize`
- `window: Duration`
- `block_duration: Duration`

`ProcessingBudgetSnapshot` fields:
- `enabled: bool`
- `per_ip_refill_units_per_sec: u32`
- `per_ip_burst_units: u32`
- `global_refill_units_per_sec: u32`
- `global_burst_units: u32`
- `bucket_idle_ttl: Duration`

- [ ] **Step 1: Write the failing facade test**

Bind a normal local `NetworkServer`, call `policy_snapshot()`, and assert the effective hardfork defaults currently used by the facade: packet window `120` per IP / `100_000` global / `10ms` window / `10s` block, and processing budget enabled with per-IP refill `3_000_000`, per-IP burst `1_500_000`, global refill `128_000_000`, global burst `32_000_000`, idle TTL `30s`.

- [ ] **Step 2: Verify RED**

```bash
cargo +1.88.0 test -p ardosia-network --locked --test policy_snapshot
```

Expected: compile failure because the Ardosia-owned snapshot API does not exist yet.

- [ ] **Step 3: Add minimal Ardosia-owned snapshot types**

Build the snapshot from the validated `TransportConfig` inside the network crate. Keep conversion from the hardfork type crate-private. Do not make `TransportConfig` or `ProcessingBudgetConfig` part of any public signature.

- [ ] **Step 4: Store the exact bound snapshot in `NetworkServer`**

Capture the snapshot immediately before the `TransportConfig` is moved into `RaknetServer::builder()`. Return a copy from `policy_snapshot()`.

- [ ] **Step 5: Verify GREEN**

Run the focused test and the existing public-surface test. Require both to pass.

---

### Task 2: Carry policy metadata through the local benchmark child

**Files:**
- Modify: `crates/ardosia-loadgen/src/server_target.rs`
- Modify: `crates/ardosia-loadgen/src/child_protocol.rs`
- Modify: `crates/ardosia-loadgen/tests/child_protocol.rs`

**Interfaces:**
- `ServerTargetResult` gains the captured network policy snapshot.
- `ServerRunReport` gains `#[serde(default, skip_serializing_if = "Option::is_none")] server_policy: Option<ServerPolicyReport>`.

- [ ] **Step 1: Write a child-protocol regression test**

Require a local child stop report to include non-empty policy metadata while historical JSON without that field remains deserializable.

- [ ] **Step 2: Verify RED**

Run `cargo +1.88.0 test -p ardosia-loadgen --locked --test child_protocol` and require the new assertion to fail before implementation.

- [ ] **Step 3: Capture and map the snapshot**

Read `NetworkServer::policy_snapshot()` from the actual bound server and carry it in `ServerTargetResult`. Convert to loadgen-owned serde report types only at the loadgen boundary.

- [ ] **Step 4: Verify GREEN**

Require the child protocol tests to pass with no transport behavior change.

---

### Task 3: Record the policy in `run.json`

**Files:**
- Modify: `crates/ardosia-loadgen/src/report.rs`
- Modify: `crates/ardosia-loadgen/src/runner.rs`
- Modify: `crates/ardosia-loadgen/tests/report.rs`

**Interfaces:**
- Produces: `ServerPolicyReport`
- Produces: `PacketWindowReport`
- Produces: `ProcessingBudgetReport`
- `RunReport` gains optional `server_policy` with serde default/skip-none behavior.

Serialize durations as integer milliseconds so artifacts are human-readable and stable:
- `window_ms`
- `block_duration_ms`
- `bucket_idle_ttl_ms`

- [ ] **Step 1: Add report-shape tests**

Verify policy values survive assembly/serialization and historical JSON without `server_policy` remains readable.

- [ ] **Step 2: Attach the child-reported policy to every locally assembled report**

Use the `ServerRunReport` returned by the benchmark child. Do not synthesize defaults in the parent process.

- [ ] **Step 3: Keep remote/client-only runs optional**

`run_clients()` has no owned server and must leave `server_policy` absent rather than guessing.

- [ ] **Step 4: Verify report and local child tests**

Run focused loadgen tests with `--locked` and require zero failures.

---

### Task 4: Document interpretation and run the full gate

**Files:**
- Modify: `docs/benchmarks.md`

- [ ] **Step 1: Document the two policy families**

Explain that `packet_window` is the coarse packet-rate window and `processing_budget` is the CPU/work token budget. In the current hardfork, established connected traffic is not charged to the coarse packet window; the processing budget is the policy relevant to the previous shared-localhost-IP 3,000-client artifact.

- [ ] **Step 2: State the capacity rule**

Before describing a localhost degradation as shard/server capacity, inspect both `environment.process_limits.open_files` and `server_policy.processing_budget`, together with source-IP concentration. Do not infer production-safe tuning from localhost artifacts.

- [ ] **Step 3: Run the complete workspace gate**

```bash
cargo +1.88.0 fmt --all -- --check
cargo +1.88.0 clippy --workspace --all-targets --locked -- -D warnings
cargo +1.88.0 test --workspace --locked
git diff --check
```

- [ ] **Step 4: Review scope**

Final diff may contain network snapshot types/accessor, loadgen report/child plumbing, tests, and docs only. Reject any policy mutation API, hardfork algorithm/default change, or benchmark correctness-threshold change.