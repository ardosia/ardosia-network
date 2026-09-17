# Current State

Last updated: 2026-09-17
Current milestone: stable lower-layer transport facade; no current identity/map/login runtime delta
Default branch: `main`

## Role
`ardosia-network` remains the game-agnostic async connection/payload layer between the server and `ardosia-raknet`. Gameplay, protocol-84 semantics, account identity, inventory, and world behavior do not belong here.

## Working
- Public connection/listener facade over the pinned RakNet hardfork.
- Server-consumed revision remains `f71da57dab3c28e6ccbcee5cbdc3376aac9f7fdd`.
- Network consumes RakNet revision `55b57787b6715ef2a931631ef4b690e3df0651e5`.
- Canonical durable documentation has been centralized in `ardosia-docs`; docs migration is no longer active work.
- `.agent/{CONTEXT,STATE,DECISIONS,NEXT}.md` is the local continuity harness under the Project workflow.

## Current convergence impact
The completed 2026-09-17 identity/map/login research does not currently require changes in this repository. Identity/authentication/gameplay semantics remain above the network boundary.

If future evidence changes connection/payload lifecycle expectations, the corresponding network change must be implemented and validated here or left as an explicit `.agent/NEXT.md` delta. Evidence completion alone is not a network runtime PASS.

## Branch/pin state
A maintenance branch may be ahead of the exact server-consumed revision. Do not move the server's network pin merely because branch HEAD is newer; reconcile it only as a coherent validated network slice.

## Validation status
- This agent-state migration changes no network Rust/runtime behavior: executable network validation **NOT RUN**.
- Historical tests/checks attached to prior implementation heads remain historical evidence only.
- Consumer pin movement: **NOT RUN**.

## Active work
No runtime work is required from the current identity/map/login convergence pass.

Keep this repo stable while server/protocol conversion proceeds. Revisit only for a concrete game-agnostic transport defect, a consumer-required network slice, or deliberate branch/pin reconciliation with executable validation.
