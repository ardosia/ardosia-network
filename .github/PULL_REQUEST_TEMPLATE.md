## Summary

Describe the facade-level problem and the smallest change that addresses it.

## Scope

- [ ] This change belongs in `ardosia-network` rather than `ardosia-raknet` or a higher protocol/application layer.
- [ ] I did not add Minecraft/game semantics to the network facade.
- [ ] I did not change the pinned `ardosia-raknet` revision unless that change is explicit and justified here.
- [ ] I did not weaken bounded queues, backpressure handling, lifecycle safety, or transport abuse-control behavior to satisfy a local benchmark.

## Public API

- [ ] No public API change.
- [ ] Public API changed; rustdoc and public-surface coverage were updated.

Describe any public API change and migration impact:

## Verification

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- [ ] `cargo test --workspace --all-targets --locked`
- [ ] `cargo test --workspace --doc --locked`
- [ ] `cargo doc --workspace --no-deps --locked`
- [ ] `RUSTDOCFLAGS="-D missing_docs" cargo doc --workspace --no-deps --locked`
- [ ] `git diff --check`

Additional evidence, reproducer, or benchmark context:

## Security / lifecycle review

Describe effects on connection admission, peer lifecycle, backpressure, shutdown, malformed input, resource bounds, or failure behavior. Write `none` when genuinely not applicable.

## Licensing note

This repository does not yet have a project-level open-source license or finalized external contribution terms. External code contributions must not be merged until those terms are deliberately established. Maintainer-authored changes may proceed under the repository's current private-development policy.
