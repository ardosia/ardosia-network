# Next Work

## S001.5 network facade review — complete

Server-consumed revision reviewed: `f71da57dab3c28e6ccbcee5cbdc3376aac9f7fdd`.

Result: **retain the current public facade with no source changes**.

Intentional public concepts:

- `CookieMode`
- `NetworkConfig`
- `NetworkConfigError`
- `NetworkServer`
- `Connection`
- `Reliability`
- `NetworkError`

Rationale:

- the crate remains game-agnostic and transports opaque connected payloads;
- no vendor RakNet types escape the public facade;
- worker sharding/cookies/reliability/backpressure/lifecycle errors are genuine transport concerns;
- the generic supported-RakNet-protocol list is not an Ardosia runtime version switch: `ardosia-server` fixes protocol 8 in composition.

Validation:

- targeted public API/source review: **PASS**;
- source changes: **NOT RUN**;
- standalone Rust gate: **NOT RUN** because source is unchanged;
- server/network or network/RakNet pin movement: **NOT RUN**.

Cross-repository next action belongs in `ardosia-protocol`: re-check only the public facade consumed by the server for redundant helpers/re-exports.
