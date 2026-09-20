# Current State

Last updated: 2026-09-17
Current milestone: repository hygiene; network runtime work paused
Default branch: `main`

## Role
`ardosia-network` is the game-agnostic async connection/payload layer between `ardosia-server` and `ardosia-raknet`. Gameplay, protocol-84 semantics, account identity, inventory, and world behavior do not belong here.

## Working
- Public connection/listener facade over the pinned RakNet hardfork.
- Server-consumed revision remains `f71da57dab3c28e6ccbcee5cbdc3376aac9f7fdd`.
- Network consumes RakNet revision `55b57787b6715ef2a931631ef4b690e3df0651e5`.
- Canonical durable documentation is centralized in `ardosia-docs`.
- `.agent/{CONTEXT,STATE,DECISIONS,NEXT}.md` is the local continuity harness.

## Branch/pin state
`cleanup/runtime-hardening-clean` remains intentionally preserved during the hygiene pass because the server consumes revision `f71da57dab3c28e6ccbcee5cbdc3376aac9f7fdd` from that maintenance line. Do not move the server pin merely because another branch head is newer.

The temporary workflow branch from the 2026-09-17 reconciliation is merged history only and is not active work.

## Validation
- Workflow/state reconciliation to `main`: **PASS**.
- S001.5 public-facade review at server-consumed revision `f71da57dab3c28e6ccbcee5cbdc3376aac9f7fdd`: **PASS**.
- Crate-root facade remains exactly `CookieMode`, `NetworkConfig`, `NetworkConfigError`, `NetworkServer`, `Connection`, `Reliability`, and `NetworkError`; no vendor RakNet types escape publicly.
- Server composition verification: **PASS** — RakNet protocol 8 is hardcoded in `ardosia-server::config` and is not a runtime/deployment option.
- Standalone network Rust validation: **NOT RUN** — no network source code changed.
- Consumer pin movement: **NOT RUN**.
- Runtime code changes: **NOT RUN**.

## Active work
S001.5 public-facade re-check is complete with no source changes. The small game-agnostic facade is intentionally retained: worker sharding, cookie mode, reliability, opaque payload delivery, typed configuration failure, connection lifecycle, backpressure, and backend lifecycle errors are transport responsibilities rather than Minecraft/game APIs.

The generic RakNet protocol-list constructor is also retained. Ardosia's fixed RakNet-8 target is enforced by server composition, not exposed as runtime configuration. Do not move the server pin during this hygiene pass.

Next cross-repository slice is the server-consumed `ardosia-protocol` facade re-check.
