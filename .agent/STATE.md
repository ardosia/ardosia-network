# Current State

Last updated: 2026-09-16
Current milestone: workflow/documentation migration
Default branch: `main`
Default head before harness migration: `76a83dbda07839c20a43fdfa71938d5ed160cda6`
Active maintenance branch: `cleanup/runtime-hardening-clean` at `9b8342447a73879c9507b2689233c492350e0571`
Current server-consumed revision: `f71da57dab3c28e6ccbcee5cbdc3376aac9f7fdd`

## Working
- Game-agnostic network facade over the pinned RakNet hardfork.
- Protocol 8 can be configured at the facade boundary.
- Opaque payload transport, connection lifecycle, bounded delivery/backpressure, and graceful shutdown surfaces are implemented.
- Current server integration successfully reaches real-client protocol-84 gameplay through the pinned network revision.

## Partially working / branch state
- `cleanup/runtime-hardening-clean` is 3 commits ahead of `main` and contains runtime hardening plus documentation/status changes.
- The server intentionally pins `f71da57...`, a revision on that maintenance line, not current `main`.

## Broken / failing
- No standalone network defect is established by the current cleanup evidence.

## Test status
- Full standalone network gate: **NOT RUN** in this workflow migration round.
- GitHub Actions current status: **NOT RUN** / not queried in this round.
- Integration through current `ardosia-server`: **PASS** for the recent real-client connection/startup/shutdown smoke, but this is not a substitute for the standalone crate gate.
- Historical benchmark reports are evidence only; no current capacity/performance claim is made.

## Current blocker
- Central docs migration is **BLOCKED** until `ardosia/ardosia-docs` exists; the available connector cannot create repositories.

## Active work
- Install `.agent/` harness.
- Migrate README/contribution/security/results/status documentation into the central docs repository.
- Preserve code, CI, Cargo manifests/lockfile, toolchain, and license locally.

## Important temporary facts
- Do not change the RakNet exact-SHA dependency pin as unrelated cleanup.
- Do not treat historical load reports as present-day production capacity guarantees.
