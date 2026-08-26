# Network API Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the broad mutable network facade with a validated documented configuration API and remove the unused metrics subsystem.

**Architecture:** Keep the existing connection/backend/server boundary. Make invalid configuration difficult to represent, translate it privately to RakNet, ignore metrics events without aggregating them, and expose only the transport behavior used by Ardosia.

**Tech Stack:** Rust 1.98.0, Tokio, thiserror, ardosia-raknet

**Spec:** `docs/superpowers/specs/2026-08-25-network-api-cleanup-design.md`

## Global Constraints

- Do not modify the `ardosia-raknet` revision or source.
- Preserve protocol selection, cookies, advertisement, sharding, backpressure, connection lifecycle, and shutdown behavior.
- Add no observability replacement.
- Public documentation is a compile-time gate.
- Use TDD and commit after each independently reviewable task.

---

### Task 1: Establish cleanup branch and generated-file policy

**Files:**
- Create: `.gitignore` if absent, otherwise modify it
- Modify: `crates/ardosia-network/src/lib.rs`

**Interfaces:**
- Consumes: verified `main` from the baseline plan
- Produces: local hygiene and crate-level safety/documentation policy

- [ ] **Step 1: Create the cleanup branch**

```bash
git switch main
git pull --ff-only origin main
git switch -c cleanup/network-api
```

- [ ] **Step 2: Add generated paths to `.gitignore`**

```gitignore
/target/
/.codegraph/
```

- [ ] **Step 3: Add crate policy and crate-level documentation**

Place this before module declarations in `crates/ardosia-network/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

//! Game-agnostic asynchronous payload transport for Ardosia.
//!
//! The crate owns listener and connection lifecycle over the pinned RakNet
//! transport. It deliberately has no MCPE packet, player, or world knowledge.
```

- [ ] **Step 4: Run rustdoc to record the expected RED state**

```bash
RUSTDOCFLAGS="-D missing_docs" cargo doc --no-deps
```

Expected: FAIL on currently undocumented public API. Do not enable the crate-level
`missing_docs` deny lint until Task 4 has documented the complete facade, so
intermediate TDD checkpoints continue to compile.

- [ ] **Step 5: Commit the policy checkpoint**

```bash
git add .gitignore crates/ardosia-network/src/lib.rs
git commit -m "chore(network): enforce facade documentation"
```

### Task 2: Introduce validated network configuration

**Files:**
- Modify: `crates/ardosia-network/src/config.rs`
- Modify: `crates/ardosia-network/src/error.rs`
- Modify: `crates/ardosia-network/src/lib.rs`
- Modify: `crates/ardosia-network/src/server.rs`
- Modify: `crates/ardosia-network/tests/public_api.rs`
- Modify: `crates/ardosia-network/tests/protocol8.rs`
- Modify: `crates/ardosia-network/tests/transport.rs`

**Interfaces:**
- Produces: `CookieMode`, `NetworkConfig::new`, `NetworkConfig::with_worker_shards`, `NetworkConfigError`
- Consumed by: network integration tests and the later server cleanup

- [ ] **Step 1: Replace construction tests with the new contract**

Add these imports and tests to `tests/public_api.rs` before migrating the round-trip fixture:

```rust
use std::num::NonZeroUsize;

use ardosia_network::{CookieMode, NetworkConfig, NetworkConfigError};

#[test]
fn rejects_empty_protocol_set_during_construction() {
    let error = NetworkConfig::new(
        allocate_loopback_addr(),
        [],
        NonZeroUsize::new(32).unwrap(),
        "ardosia-network-test",
        CookieMode::Enabled,
    )
    .unwrap_err();

    assert_eq!(error, NetworkConfigError::NoProtocols);
}

#[test]
fn rejects_duplicate_protocol_during_construction() {
    let error = NetworkConfig::new(
        allocate_loopback_addr(),
        [8, 8],
        NonZeroUsize::new(32).unwrap(),
        "ardosia-network-test",
        CookieMode::Enabled,
    )
    .unwrap_err();

    assert_eq!(error, NetworkConfigError::DuplicateProtocol { protocol: 8 });
}
```

- [ ] **Step 2: Run the focused test to verify RED**

```bash
cargo test --test public_api rejects_empty_protocol_set_during_construction
```

Expected: FAIL because `CookieMode`, `NetworkConfigError`, and the constructor do not exist.

- [ ] **Step 3: Implement the typed configuration errors**

In `config.rs`, define:

```rust
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NetworkConfigError {
    #[error("at least one RakNet protocol must be configured")]
    NoProtocols,
    #[error("RakNet protocol {protocol} is configured more than once")]
    DuplicateProtocol { protocol: u8 },
    #[error("RakNet rejected the transport configuration: {message}")]
    TransportRejected { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieMode {
    Enabled,
    Disabled,
}
```

Make `NetworkConfig` fields private and implement the spec signatures with `NonZeroUsize`. Reject duplicates with a local `HashSet<u8>`. Store worker shards as `Option<NonZeroUsize>` and translate `CookieMode` to the vendor boolean only in `to_vendor_transport_config`.

- [ ] **Step 4: Integrate configuration errors with `NetworkError`**

Replace the stringly invalid-config variant with:

```rust
#[error(transparent)]
Configuration(#[from] NetworkConfigError),
```

Map vendor validation to `NetworkConfigError::TransportRejected`.

- [ ] **Step 5: Update `NetworkServer::bind` to use private crate accessors**

Use crate-private methods for `max_connections()` and `worker_shards()`; do not add public getters. Convert nonzero values with `.get()` at the channel/vendor boundary.

- [ ] **Step 6: Migrate every test fixture to one helper**

Use this construction shape in integration tests:

```rust
fn network_config(addr: SocketAddr) -> NetworkConfig {
    NetworkConfig::new(
        addr,
        [8],
        NonZeroUsize::new(32).unwrap(),
        "ardosia-network-test",
        CookieMode::Enabled,
    )
    .unwrap()
}
```

Use `.with_worker_shards(NonZeroUsize::new(4).unwrap())` in tests that require explicit sharding.

- [ ] **Step 7: Run configuration and transport tests**

```bash
cargo test --test public_api
cargo test --test protocol8
cargo test --test transport
```

Expected: PASS.

- [ ] **Step 8: Commit the validated facade**

```bash
git add crates/ardosia-network/src/config.rs crates/ardosia-network/src/error.rs crates/ardosia-network/src/lib.rs crates/ardosia-network/src/server.rs crates/ardosia-network/tests
git commit -m "refactor(network): validate transport configuration"
```

### Task 3: Remove unused metrics aggregation

**Files:**
- Delete: `crates/ardosia-network/src/metrics.rs`
- Delete: `crates/ardosia-network/tests/shard_metrics.rs`
- Modify: `crates/ardosia-network/src/lib.rs`
- Modify: `crates/ardosia-network/src/server.rs`
- Modify: `crates/ardosia-network/src/backend.rs`
- Modify: `crates/ardosia-network/tests/public_surface.rs`

**Interfaces:**
- Removes: `NetworkServer::metrics`, `NetworkServer::shard_metrics`, all metrics exports
- Preserves: backend polling and all connection behavior

- [ ] **Step 1: Add source-surface assertions for removed metrics API**

Extend `tests/public_surface.rs`:

```rust
#[test]
fn crate_root_exposes_no_metrics_facade() {
    let crate_root = include_str!("../src/lib.rs");
    assert!(!crate_root.contains("NetworkMetrics"));
    assert!(!crate_root.contains("NetworkShardMetrics"));
    assert!(!crate_root.contains("TransportMetrics"));
}
```

- [ ] **Step 2: Run the assertion to verify RED**

```bash
cargo test --test public_surface crate_root_exposes_no_metrics_facade
```

Expected: FAIL because metrics types are still re-exported.

- [ ] **Step 3: Remove metrics ownership from `NetworkServer`**

Delete the `metrics` field, its allocation, its backend argument, both public snapshot methods, and its destructuring entry during shutdown.

- [ ] **Step 4: Remove metric mutation from the backend**

Remove the `MetricsState` argument from `run_backend`, `handle_server_event`, and `close_peer_for_backpressure`. Preserve event draining with:

```rust
RaknetServerEvent::Metrics { .. } => false,
```

Keep every existing disconnect and close notification even where its adjacent counter call disappears.

- [ ] **Step 5: Delete the metrics module and tests**

Delete `src/metrics.rs`, delete `tests/shard_metrics.rs`, remove `mod metrics`, remove metrics re-exports, and remove `MetricsState` from the crate root.

- [ ] **Step 6: Run all behavioral network tests**

```bash
cargo test --all-targets
```

Expected: PASS with no metrics test target.

- [ ] **Step 7: Commit metrics removal**

```bash
git add -A crates/ardosia-network/src crates/ardosia-network/tests
git commit -m "refactor(network): remove unused metrics facade"
```

### Task 4: Complete public documentation and facade contract

**Files:**
- Modify: every public item under `crates/ardosia-network/src/`
- Modify: `crates/ardosia-network/tests/public_surface.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: final supported public types from Tasks 2-3
- Produces: documented compile-clean public facade

- [ ] **Step 1: Document all public types and behavior**

Add rustdoc for `NetworkConfig`, `NetworkConfigError`, `CookieMode`, `NetworkServer`, `Connection`, `Reliability`, `NetworkError`, their variants, and methods. Include failure behavior for bind, accept, receive, send, close, and shutdown.

After all public items are documented, add this crate-level gate below
`#![forbid(unsafe_code)]`:

```rust
#![deny(missing_docs)]
```

- [ ] **Step 2: Add the intended export ledger**

Extend `tests/public_surface.rs` with exact crate-root names:

```rust
#[test]
fn crate_root_contains_only_the_intended_module_exports() {
    let crate_root = include_str!("../src/lib.rs");
    for expected in [
        "CookieMode",
        "NetworkConfig",
        "NetworkConfigError",
        "Connection",
        "NetworkError",
        "Reliability",
        "NetworkServer",
    ] {
        assert!(crate_root.contains(expected), "missing {expected}");
    }
    assert!(!crate_root.contains("NetworkRuntimeConfig"));
    assert!(!crate_root.contains("Metrics"));
}
```

- [ ] **Step 3: Update README usage to the validated constructor**

The example must use `NonZeroUsize`, `CookieMode`, `NetworkConfig::new`, `NetworkServer::bind`, `accept`, and `shutdown`; it must contain no MCPE packet semantics.

- [ ] **Step 4: Run the documentation and surface gates**

```bash
cargo test --test public_surface
RUSTDOCFLAGS="-D missing_docs" cargo doc --no-deps
```

Expected: PASS.

- [ ] **Step 5: Commit documentation**

```bash
git add README.md crates/ardosia-network/src crates/ardosia-network/tests/public_surface.rs
git commit -m "docs(network): document minimal facade"
```

### Task 5: Verify and publish the network cleanup

**Files:**
- Verify: entire repository

**Interfaces:**
- Produces: reviewed cleanup branch ready for protocol/server consumers

- [ ] **Step 1: Synchronize CodeGraph and inspect affected tests**

```bash
codegraph sync .
codegraph affected crates/ardosia-network/src/config.rs crates/ardosia-network/src/backend.rs crates/ardosia-network/src/server.rs
```

- [ ] **Step 2: Run the complete gate**

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo doc --no-deps
RUSTDOCFLAGS="-D missing_docs" cargo doc --no-deps
git diff --check
```

- [ ] **Step 3: Confirm removed symbols are absent**

```bash
rg -n 'NetworkRuntimeConfig|MetricsState|NetworkMetrics|NetworkShardMetrics|TransportMetrics|fn metrics\(|fn shard_metrics\(' crates/ardosia-network/src crates/ardosia-network/tests
```

Expected: no matches.

- [ ] **Step 4: Push and open the cleanup PR**

```bash
git push -u origin cleanup/network-api
gh pr create -R ardosia/ardosia-network --base main --head cleanup/network-api --title "Clean and document the network facade" --body "Removes the unused metrics facade, validates configuration construction, minimizes exports, and enforces complete public rustdoc without changing RakNet behavior."
```

Expected: PR remains open for review until the execution workflow explicitly reaches its merge gate.
