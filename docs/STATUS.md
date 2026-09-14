# ardosia-network status

This file records how the Network repository relates to the active Ardosia stack without turning application-specific behavior into Network responsibilities.

## Current consumed line

The active `ardosia-server:feat/gameplay-multiplayer-propagation` line pins Network revision:

```text
f71da57dab3c28e6ccbcee5cbdc3376aac9f7fdd
```

That revision belongs to `cleanup/runtime-hardening-clean`. Consumers should continue to pin an exact revision; repository branch HEAD and consumer revision are intentionally separate concepts.

## RakNet dependency

This Network line pins:

```text
ardosia/ardosia-raknet
55b57787b6715ef2a931631ef4b690e3df0651e5
```

Do not replace the exact pin with RakNet `main` merely because the hardfork repository contains newer commits. A pin move is a compatibility change and should be reviewed/verified as such.

## Owned behavior

Network owns the game-agnostic facade only:

- transport configuration validation;
- listener/backend lifecycle;
- accepted connection handles;
- opaque connected-payload send/receive;
- bounded application-facing queues/backpressure;
- graceful shutdown and backend joining.

Minecraft packet semantics, session state, inventory/gameplay/world behavior, and native compatibility evidence remain outside this repository.

## Documentation rule

The root README is the public project overview. Historical benchmark reports under `docs/results/` are evidence from their recorded commits/workloads, not current setup or capacity guidance.

When status text conflicts with manifests, `Cargo.toml`/`Cargo.lock` and the exact consumer pin are authoritative.

## Verification

Behavior changes require the full documented Rust 1.98 gate. Documentation-only or rustdoc-only maintenance performed without compilation must be labeled as uncompiled maintenance rather than described as CI-verified.
