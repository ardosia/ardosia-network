# Security policy

`ardosia-network` is pre-release transport-facade software. Security fixes are evaluated against the current `main` branch; historical benchmark harnesses and removed development branches are not supported release lines.

## Reporting a vulnerability

Please do not open a public issue containing vulnerability details.

When GitHub private vulnerability reporting is enabled for this repository, use the repository's **Security → Report a vulnerability** flow. If private reporting is not available, open a non-sensitive issue asking maintainers for a private security contact **without including exploit details**, then continue the report privately.

A useful private report includes:

- affected commit or revision;
- operating system and Rust version;
- relevant `NetworkConfig` values with secrets or private addresses removed;
- a minimal reproducer or packet/lifecycle sequence;
- expected and observed behavior;
- security impact and whether the issue is remotely triggerable;
- logs or backtraces with credentials/tokens removed.

## In scope

Examples of security-relevant issues in this repository include:

- remotely triggerable panics or process termination through facade-level input/lifecycle handling;
- resource exhaustion caused by incorrect queue, admission, or connection-lifecycle behavior in the facade;
- backpressure or bounded-queue bypasses;
- peer lifecycle races that can be triggered to cause denial of service or state corruption;
- configuration-validation bypasses that create unsafe transport states;
- leakage of internal transport implementation details that defeats the intended isolation boundary;
- unsafe handling of data returned from the pinned transport dependency.

## Usually belongs elsewhere

Report the issue in the component that owns the behavior when possible:

- RakNet handshake, reliability, retransmission, fragmentation, congestion, sharding, pacing, and low-level abuse controls belong to `ardosia-raknet`;
- Minecraft packet parsing, protocol sequencing, login claims, and game-protocol semantics belong to the protocol layer;
- authentication policy, player/game/world state, persistence, and application policy belong to the application layer.

If the correct boundary is unclear, report privately here rather than disclosing the issue publicly; maintainers can route it.

## Disclosure

Please allow maintainers time to reproduce, triage, and coordinate a fix before public disclosure. Do not include credentials, private keys, access tokens, or proprietary game assets in reports.

## Supported status

There are no stable releases yet. Only the current pre-release development line is considered for security fixes. Historical benchmark results are evidence snapshots, not supported distributions or security baselines.
