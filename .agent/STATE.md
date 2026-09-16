# Current State

Last updated: 2026-09-16
Current milestone: centralized-documentation cleanup
Default branch: `main`
Default head after workflow harness installation: `08cb69313ce4fcb8728aab99bdc3d0582dc300f8`
Active maintenance branch: `cleanup/runtime-hardening-clean` at `9b8342447a73879c9507b2689233c492350e0571`
Current server-consumed revision: `f71da57dab3c28e6ccbcee5cbdc3376aac9f7fdd`

## Working
- Game-agnostic network facade over the pinned RakNet hardfork.
- Opaque payload transport, connection lifecycle, bounded delivery/backpressure, and graceful shutdown surfaces are implemented.
- Current server integration reaches real-client protocol-84 gameplay through the pinned network revision.
- `.agent/` continuity harness is installed on `main`.

## Partially working / branch state
- `cleanup/runtime-hardening-clean` remains ahead of `main` and contains runtime hardening plus status/docs changes.
- The server intentionally pins `f71da57...` on that maintenance line, not current `main`.

## Broken / failing
- No standalone network defect is established by the current cleanup evidence.
- Durable network documentation/results remain local pending central migration.

## Test status
- Full standalone network gate: **NOT RUN** in this workflow migration round.
- GitHub Actions current status: **NOT RUN** / not queried in this round.
- Recent server runtime integration: **PASS**, but not a substitute for the standalone crate gate.
- Historical benchmark reports are evidence only; no current capacity/performance claim is made.

## Current blocker
- Central docs migration is **BLOCKED** until `ardosia/ardosia-docs` exists; the available connector cannot create repositories.

## Active work
- Centralize README/contribution/security/results/status documentation after the docs repository exists.
- Preserve code, CI, Cargo files, toolchain, license, and `.agent/` locally.

## Important temporary facts
- Do not change the RakNet exact-SHA dependency pin as unrelated cleanup.
- Do not merge the maintenance branch or move the server pin without a dedicated review/validation round.
