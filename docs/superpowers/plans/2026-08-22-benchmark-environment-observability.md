# Benchmark Environment Observability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make benchmark and profiling reports record the active RakNet hardfork revision and the load-generator open-file limits that can create artificial client-admission ceilings.

**Architecture:** Keep environment metadata in `EnvironmentReport`, keep profiling metadata aligned in `ProfileMetadata`, preserve compatibility when deserializing historical artifacts that used `vendor_revision`, and collect Linux process limits through `/proc/self/limits` without introducing a new dependency. Do not change runtime limits, RakNet policies, pass/fail gates, or transport behavior.

**Tech Stack:** Rust 1.88.0, serde/serde_json, Linux procfs, existing Ardosia loadgen report/environment/profiling modules.

**Spec:** `docs/benchmarks.md`

## Global Constraints

- Rust toolchain remains exactly `1.88.0` for the quality gate.
- Do not add third-party dependencies.
- Do not mutate file-descriptor limits or transport abuse-control defaults.
- Historical run/profile artifacts containing `vendor_revision` must remain deserializable.
- New run/profile artifacts must distinguish the preserved upstream RakNet baseline from the active hardfork revision.
- Non-Linux platforms must continue to build; process-limit metadata may be unavailable there.

---

### Task 1: Make RakNet revision metadata explicit

**Files:**
- Modify: `crates/ardosia-loadgen/src/report.rs`
- Modify: `crates/ardosia-loadgen/src/profiling.rs`
- Modify: `crates/ardosia-loadgen/src/runner.rs`
- Test: `crates/ardosia-loadgen/tests/environment.rs`
- Test: `crates/ardosia-loadgen/tests/profiling.rs`

**Interfaces:**
- Produces: `EnvironmentReport::raknet_upstream_revision: String`
- Produces: `EnvironmentReport::raknet_hardfork_revision: String`
- Produces: `ProfileMetadata::raknet_upstream_revision: String`
- Produces: `ProfileMetadata::raknet_hardfork_revision: String`
- Produces: constants for upstream baseline `3edfb4170e6cb5aeed992b09b50176fb7e5b6079` and hardfork integration revision `f127fce27a206a51a1d39ffa7a9bbed98d10ea14`.

- [ ] **Step 1: Write the failing run-report serialization compatibility test**

Verify that a default environment report serializes `raknet_upstream_revision` and `raknet_hardfork_revision`, omits `vendor_revision`, and that JSON containing the historical `vendor_revision` key can be deserialized then re-serialized under the new upstream key.

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cargo +1.88.0 test -p ardosia-loadgen --locked --test environment environment_revision_metadata_is_explicit_and_legacy_compatible -- --exact
```

Expected: FAIL because the new serialized keys are absent.

- [ ] **Step 3: Implement the minimal serde-compatible environment schema change**

Rename the Rust field to `raknet_upstream_revision`, add `#[serde(alias = "vendor_revision")]`, add the hardfork revision field with a default for newly generated reports, and update `EnvironmentReport::default()`.

- [ ] **Step 4: Guard the hardfork constant against Cargo pin drift**

Add a test that reads the workspace `Cargo.toml` with `include_str!` and asserts that the active hardfork revision constant appears in the exact `raknet-rust` dependency pin.

- [ ] **Step 5: Align profiling metadata with the run-report schema**

Add the same explicit upstream/hardfork fields to `ProfileMetadata`, accept historical `vendor_revision` on deserialization, and keep an absent historical hardfork revision empty rather than inventing modern provenance.

- [ ] **Step 6: Propagate both revisions through profiling paths**

Update normal capture, post-processing failure, missing-capture failure, and early profiling failure paths in `runner.rs` to forward `raknet_upstream_revision` and `raknet_hardfork_revision` from the collected environment.

- [ ] **Step 7: Verify profiling compatibility**

```bash
cargo +1.88.0 test -p ardosia-loadgen --locked --test profiling profile_revision_metadata_is_explicit_and_legacy_compatible -- --exact
```

Require PASS and require serialized profile metadata to omit `vendor_revision`.

---

### Task 2: Record Linux open-file limits

**Files:**
- Modify: `crates/ardosia-loadgen/src/report.rs`
- Modify: `crates/ardosia-loadgen/src/environment.rs`
- Modify: `crates/ardosia-loadgen/src/resource/linux.rs`
- Test: `crates/ardosia-loadgen/tests/environment.rs`
- Test: `crates/ardosia-loadgen/tests/resource.rs`

**Interfaces:**
- Produces: `ResourceLimitReport { soft: Option<u64>, hard: Option<u64> }`, where `None` means an unlimited bound.
- Produces: `ProcessLimitsReport { open_files: Option<ResourceLimitReport> }`.
- Produces: `resource::linux::parse_open_file_limits(&str) -> Option<ResourceLimitReport>`.
- Produces: `resource::linux::read_open_file_limits() -> Option<ResourceLimitReport>`.

- [ ] **Step 1: Write the failing environment JSON test**

On Linux, serialize `collect_environment()` and require `process_limits.open_files` to be present with both `soft` and `hard` members.

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cargo +1.88.0 test -p ardosia-loadgen --locked --test environment linux_environment_records_open_file_limits -- --exact
```

Expected: FAIL because `process_limits` is absent.

- [ ] **Step 3: Implement procfs parsing**

Parse the `Max open files` row from `/proc/self/limits`. Parse decimal values as `Some(value)` and the literal `unlimited` as `None`. Return `None` for malformed or missing rows.

- [ ] **Step 4: Add deterministic parser fixture coverage**

Extend `tests/resource.rs` with finite-limit and unlimited-limit fixtures so procfs column parsing is verified independently of the host environment.

- [ ] **Step 5: Wire collection into `collect_environment()`**

Populate `process_limits` only when the Linux read succeeds. On non-Linux platforms leave it `None`.

- [ ] **Step 6: Run environment/resource tests and verify GREEN**

```bash
cargo +1.88.0 test -p ardosia-loadgen --locked --test environment --test resource
```

---

### Task 3: Document and gate the report change

**Files:**
- Modify: `docs/benchmarks.md`

**Interfaces:**
- Consumes the new run/profile metadata and process-limit fields from Tasks 1-2.

- [ ] **Step 1: Document interpretation**

State that `raknet_upstream_revision` is provenance, `raknet_hardfork_revision` is the active pinned implementation, and `process_limits.open_files.soft` must be checked before interpreting an admission plateau as transport capacity. Note that historical artifacts may omit the hardfork field because no trustworthy value can be inferred retroactively.

- [ ] **Step 2: Run the complete workspace gate**

```bash
cargo +1.88.0 fmt --all -- --check
cargo +1.88.0 clippy --workspace --all-targets --locked -- -D warnings
cargo +1.88.0 test --workspace --locked
git diff --check
```

- [ ] **Step 3: Review scope**

Require the final diff to contain only report/environment/profiling metadata, profiling propagation, resource parsing, tests, and docs changes. No transport algorithm, runtime policy, or benchmark pass/fail threshold changes are allowed.
