# Current State

Last updated: 2026-09-16
Current milestone: centralized-documentation migration complete; runtime branch reconciliation next
Default branch: `main`
Active maintenance branch: `cleanup/runtime-hardening-clean`
Current server-consumed revision: `f71da57dab3c28e6ccbcee5cbdc3376aac9f7fdd`

## Working
- Game-agnostic network facade over the pinned RakNet hardfork.
- Opaque payload transport, connection lifecycle, bounded delivery/backpressure, and graceful shutdown surfaces are implemented.
- Current server integration reaches real-client protocol-84 gameplay through the pinned network revision.
- `.agent/` continuity harness remains local and authoritative for execution state.
- Durable repository overview and all three historical benchmark reports have been migrated to `ardosia/ardosia-docs`.
- The centrally migrated benchmark reports were re-fetched with blob SHAs identical to their source copies before the local duplicates were removed.
- Repository-local README is now intentionally minimal/operational; contribution, security, license, build, CI, source, and toolchain material remain with this repository.

## Partially working / branch state
- `cleanup/runtime-hardening-clean` remains ahead of `main` and contains runtime hardening plus maintenance state.
- The server intentionally pins `f71da57...` on that maintenance line, not current `main`.

## Broken / failing
- No standalone network defect is established by the documentation cleanup evidence.

## Test status
- Central documentation copy verification: **PASS** — migrated benchmark blobs matched the original source blob SHAs before deletion.
- Source duplicate cleanup on `main` and `cleanup/runtime-hardening-clean`: **PASS** — repository-local historical benchmark Markdown was removed after verification.
- Full standalone network Rust gate: **NOT RUN** in this documentation-only migration round.
- GitHub Actions current status: **NOT RUN** / not queried in this round.
- Recent wider server runtime integration: **PASS**, but not a substitute for the standalone crate gate.

## Current blocker
None.

## Active work
- Organization-wide documentation migration continues in sibling repositories.
- Network-specific next engineering work is to review/reconcile `cleanup/runtime-hardening-clean` against `main` through a normal validated PR if that line remains canonical.

## Important temporary facts
- Do not change the RakNet exact-SHA dependency pin as unrelated cleanup.
- Do not merge the maintenance branch or move the server pin without a dedicated review/validation round.
- Historical benchmark reports are durable evidence in `ardosia-docs`; do not recreate repository-local copies as current operational guidance.
