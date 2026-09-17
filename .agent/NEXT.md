# Next Work

1. Keep the current server-consumed network revision stable while server/protocol identity and inventory convergence proceeds.
2. Reconcile newer network maintenance work with the consumed revision only as a bounded reviewed slice with the Rust 1.98 network gate actually run.
3. Move the network -> RakNet pin only when a concrete transport change requires it and both the lower-layer candidate and consuming network slice are executable-validated.
4. If future research exposes a game-agnostic connection/payload lifecycle delta, implement it here or persist the exact deferment; do not absorb protocol/gameplay semantics into this layer.

Documentation centralization is complete and is not active work.
