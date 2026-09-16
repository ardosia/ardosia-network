# Decisions

## D-0001 — Keep `ardosia-network` transport-only
Status: Accepted
Date: 2026-09-16

### Context
The active Ardosia stack separates game protocol/application semantics from connection transport.

### Decision
`ardosia-network` remains a game-agnostic facade over `ardosia-raknet`. Minecraft packet/session/world behavior belongs above this crate; RakNet algorithms belong below it.

### Rationale
This keeps transport reusable and prevents protocol/application state from leaking into networking infrastructure.

### Consequences
Do not add protocol-84 packet semantics or player/world policy to this repository.

## D-0002 — Preserve exact RakNet pin reproducibility
Status: Accepted
Date: 2026-09-16

### Context
Ardosia consumes a reviewed RakNet hardfork revision rather than a moving branch.

### Decision
Keep the exact Git revision pin unless a dedicated transport update is reviewed and validated.

### Rationale
Compatibility work requires reproducible transport behavior.

### Consequences
Dependency-pin movement is not routine cleanup.

## D-0003 — Centralize durable documentation
Status: Accepted
Date: 2026-09-16

### Decision
Move durable architecture, benchmark/result, status, contribution, and security documentation to `ardosia/ardosia-docs` after that repository exists and the copies are verified. Keep `.agent/` as the local continuity harness.

### Consequences
Do not delete current documentation until migration is persisted and checked.

## D-0004 — PROJECT_WORKFLOW.md governs substantial work
Status: Accepted
Date: 2026-09-16

### Decision
Use the Project-provided `PROJECT_WORKFLOW.md`: inline single-agent execution, targeted state recovery, exact PASS/FAIL/BLOCKED/NOT RUN reporting, and durable `.agent/` state.
