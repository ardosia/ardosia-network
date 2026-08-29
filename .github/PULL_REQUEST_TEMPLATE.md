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

## Licensing

- [ ] I have the right to submit this contribution under Apache-2.0.
- [ ] I did not add third-party code, assets, dumps, or generated/decompiled material without documenting compatible provenance and licensing.

Unless explicitly stated otherwise, contributions intentionally submitted for inclusion are accepted under Apache-2.0, consistent with section 5 of the project license.
