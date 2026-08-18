# Upstream provenance

Repository: https://github.com/mcbe-rs/raknet-rust
Commit: 3edfb4170e6cb5aeed992b09b50176fb7e5b6079
Crate version: 0.2.0
License: Apache-2.0
Vendored for: ardosia-network

## Ardosia-local behavioral changes

None. The initial snapshot is source-identical to the pinned upstream commit.

## Toolchain compatibility note

The upstream `Cargo.toml` declares `rust-version = "1.85"`, but this pinned source uses Rust 2024 let-chains, which stabilized in Rust 1.88. Ardosia therefore builds this snapshot with Rust 1.88 or newer without modifying upstream source behavior.
