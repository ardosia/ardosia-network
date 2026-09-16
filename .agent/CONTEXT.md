# Repository Context

## Purpose
`ardosia-network` is Ardosia's game-agnostic asynchronous payload transport facade over the standalone `ardosia-raknet` hardfork.

## Ownership boundary
This crate owns:
- validated `NetworkConfig` translation;
- listener/server lifecycle;
- accepted `Connection` lifecycle;
- opaque connected-payload send/receive;
- bounded queues/backpressure outcomes;
- graceful shutdown;
- facade-level integration/regression coverage.

It does not own Minecraft packet semantics, player/game/world state, or RakNet algorithms.

## Public API target
The intended crate-root surface is deliberately small: `CookieMode`, `NetworkConfig`, `NetworkConfigError`, `NetworkServer`, `Connection`, `Reliability`, `NetworkError`.

## Toolchain and validation
Rust 1.98.0.

Primary gate:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo doc --workspace --no-deps --locked
RUSTDOCFLAGS="-D missing_docs" cargo doc --workspace --no-deps --locked
git diff --check
```

## Dependency relationship
This repository pins `ardosia-raknet` by exact Git revision. The currently documented transport pin is `55b57787b6715ef2a931631ef4b690e3df0651e5`.

`ardosia-server` currently pins network revision `f71da57dab3c28e6ccbcee5cbdc3376aac9f7fdd` from `cleanup/runtime-hardening-clean` rather than `main`.

## Documentation location
Durable architecture/results documentation is being centralized in `ardosia/ardosia-docs`. Until migration is persisted and verified, existing repo-local docs must not be deleted. `.agent/` is the local operational harness defined by `PROJECT_WORKFLOW.md`.
