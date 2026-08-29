# Contributing to ardosia-network

`ardosia-network` is a small, game-agnostic asynchronous payload-transport facade over the pinned `ardosia-raknet` hardfork.

## License and contribution terms

This repository is licensed under Apache-2.0. Unless you explicitly state otherwise, contributions intentionally submitted for inclusion in this repository are provided under the same Apache-2.0 terms, consistent with section 5 of the license.

Do not submit code, assets, protocol dumps, decompiled material, credentials, or other content that you do not have the right to contribute.

## Scope

Changes belong in this repository when they affect the stable facade between an application and RakNet transport, including:

- validated transport configuration;
- listener and connection lifecycle;
- opaque connected-payload send/receive behavior;
- bounded queues and application-facing backpressure outcomes;
- graceful shutdown;
- facade-level integration and regression coverage.

Changes do **not** belong here when they primarily concern:

- RakNet handshake, reliability, retransmission, fragmentation, congestion, sharding, pacing, or transport abuse-control algorithms — those belong in `ardosia-raknet`;
- Minecraft packet definitions, protocol sequencing, login semantics, or protocol-84 codecs — those belong in the protocol layer;
- player, gameplay, world, persistence, or application policy — those belong in the application layer;
- a new benchmark/load-generation or observability platform without a separately approved design.

Keep the facade small. Do not expose `raknet-rust` implementation types merely for convenience.

## Compatibility and dependency policy

The active Ardosia integration currently uses RakNet protocol `8` and pins `ardosia-raknet` by exact Git revision. Do not move that revision, change handshake policy, or broaden compatibility claims incidentally in an unrelated change.

Historical MCPE 0.15.10 / protocol-84 context may explain why a transport option exists, but this crate must remain game-agnostic.

## Development environment

Use Rust `1.98.0`.

Run the complete gate before proposing a change:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo doc --workspace --no-deps --locked
RUSTDOCFLAGS="-D missing_docs" cargo doc --workspace --no-deps --locked
git diff --check
```

Public API additions must have rustdoc and focused tests. The crate uses `#![forbid(unsafe_code)]` and `#![deny(missing_docs)]`.

## Evidence expectations

A change should include evidence proportional to its risk:

- lifecycle/backpressure behavior: targeted regression or integration tests;
- public API changes: public-surface and rustdoc coverage;
- transport-facing changes: a clear explanation of why the change belongs in the facade rather than `ardosia-raknet`;
- performance claims: exact commit, machine, workload, toolchain, and measurement method.

Do not generalize the historical localhost benchmark reports into universal player-capacity claims. The old load generator and benchmark workflow were removed and are not current development infrastructure.

## Pull requests

Keep pull requests narrowly scoped and explain:

1. what facade behavior or documentation changes;
2. why the change belongs in `ardosia-network`;
3. whether the public API changes;
4. whether the pinned RakNet revision changes;
5. what verification was run;
6. any security, backpressure, lifecycle, compatibility, or licensing implications.

Do not weaken checks, abuse-control behavior, or bounded-queue behavior to make a local benchmark pass.

By opening a pull request, you represent that you have the right to submit the contribution under Apache-2.0.

## Security reports

Do not disclose suspected vulnerabilities in a public issue. Follow `SECURITY.md` instead.
