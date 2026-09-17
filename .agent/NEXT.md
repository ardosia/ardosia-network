# Next Work

Network runtime work is **paused** while repository hygiene is active.

1. Keep the server-consumed network revision and RakNet pin unchanged during cleanup.
2. Preserve `cleanup/runtime-hardening-clean` while it remains the source line for the server-consumed revision.
3. Classify temporary/superseded workflow branches separately from the consumed maintenance line.
4. After branch cleanup, verify `STATE.md` reflects only live branch/pin facts.

## Parked runtime work
When explicitly resumed, reconcile newer network maintenance only as a bounded reviewed slice with the Rust 1.98 network gate actually run. Move the network -> RakNet pin only for a concrete validated transport change.
