# Ardosia Network Profiling and Churn Design

Date: 2026-08-18
Status: Design approved in chat; written spec pending user review

## Context

Phase 2 established a trustworthy mixed-load scaling harness around `ardosia-network` and the pinned `mcbe-rs/raknet-rust` vendor snapshot. The current trusted progression is:

- `steady-300`: 300 protocol-8 sessions under the full mixed workload, clean correctness result;
- `steady-500`: 500 sessions under the same full workload, clean correctness result;
- `ceiling-1000`: 1000 sessions at half the per-client workload, clean correctness result;
- `steady-1000`: 1000 sessions under the full mixed workload, clean correctness result.

The trusted `steady-1000` run sustained roughly 48.4k application frames/sec in each direction and roughly 5.22 MiB/sec of application payload in each direction. It completed with 1000/1000 handshakes, zero unexpected disconnects, protocol errors, send errors, retransmits, NACKs, queue/backpressure drops, ordering drops, and timeouts. RTT remained approximately 5 ms p50, 10 ms p95, and 11 ms p99. The server used about 181% process CPU and about 22 MiB peak RSS on the test host.

That result changes the next question. Connection-count capacity is no longer the primary unknown. The useful next work is:

1. identify where server CPU is actually spent before changing transport internals; and
2. exercise connection lifecycle pressure while normal traffic continues, rather than merely adding more simultaneously stable sessions.

This design therefore introduces two related but separately measured capabilities: **steady-window CPU profiling** and **constant-population churn**.

The transport/application boundary remains unchanged. `ardosia-network` continues to own UDP and RakNet transport only. MCPE packets, world simulation, entity logic, chunk work, and game semantics remain outside this phase.

## Goals

1. Add a one-command Linux profiling mode that profiles only the benchmark server child process.
2. Attach profiling automatically using the server PID already owned by the local benchmark orchestrator; the user must never need to discover or race to capture a PID manually.
3. Keep ramp/handshake activity out of the CPU profile and align profiling with the steady measurement window as closely and deterministically as practical.
4. Produce useful `perf` and flamegraph artifacts without adding instrumentation to the RakNet hot path or patching the vendor.
5. Add a declarative constant-population churn mode to the existing scenario model.
6. Establish `churn-500` with a target of 500 active clients and 25 planned replacements/sec for 60 seconds.
7. Keep the normal mixed workload running on active clients while churn occurs.
8. Measure churn lifecycle correctness, replacement latency, population behavior, transport telemetry, RTT, throughput, and resources together.
9. Preserve strict correctness semantics while keeping performance/resource values record-only until enough representative evidence exists.
10. Keep heavy profiling and churn execution manual-only.

## Non-goals

This phase will not:

- optimize any hotspot before a profile demonstrates it;
- patch vendored RakNet merely because a function appears in a flamegraph;
- add packet-loss or jitter injection yet;
- add broadcast/fanout semantics yet;
- emulate full MCPE player login or gameplay;
- make flamegraph generation portable beyond Linux in the first implementation;
- establish production hardware capacity guarantees from localhost results;
- add automatic heavy GitHub Actions runs;
- turn CPU, RSS, RTT, replacement latency, or queue depth into arbitrary performance gates before representative data exists.

Loss/jitter and fanout remain later, separate stress dimensions.

## Sequence

The work is implemented and used in this order:

1. add profiling support and profile the existing trusted `steady-1000` workload;
2. inspect the profile and record the dominant server CPU paths;
3. do not optimize yet unless the profile reveals an obvious correctness bug or pathological harness artifact;
4. add constant-population churn and run `churn-500`;
5. compare churn behavior against the steady-state baseline;
6. only then decide whether profiling evidence justifies transport/harness optimization.

Profiling and churn share the child-process harness but produce separate evidence. A churn run is not simultaneously treated as the canonical CPU profile in this first slice.

# Part A: Server CPU Profiling

## User Experience

Profiling is a dedicated loadgen subcommand rather than a scenario flag because profiling is an execution mode, not workload semantics.

Representative invocation:

```bash
cargo run --profile profiling -p ardosia-loadgen -- \
  profile scenarios/steady-1000.toml
```

Optional output selection may be exposed as:

```bash
cargo run --profile profiling -p ardosia-loadgen -- \
  profile scenarios/steady-1000.toml \
  --output profiles/steady-1000
```

The command remains one-shot. It:

1. validates profiling prerequisites;
2. launches the same internal benchmark-server child used by `local`;
3. obtains the child PID programmatically;
4. ramps and validates all scenario sessions;
5. waits for transport telemetry to converge;
6. prepares `perf` attached only to the server PID, initially disabled;
7. enables CPU sampling at the steady boundary;
8. runs the normal scenario measured window;
9. disables/stops profiling at the steady boundary;
10. completes normal benchmark drain/shutdown;
11. emits the normal run report plus profiling artifacts.

The terminal may print the server PID for observability, but the PID is never an input the user must provide.

## Why External `perf`

The first profiling implementation uses Linux `perf` externally rather than adding an in-process Rust profiler or broad tracing instrumentation.

Reasons:

- the measured server process remains the same server process used by the benchmark;
- no profiler crate is linked into the RakNet hot path;
- no vendor changes are required;
- profiling can be enabled only during the measured phase;
- raw `perf.data` remains available for later inspection beyond one flamegraph;
- the implementation can be removed or changed without affecting the public `ardosia-network` API.

`tracing` may still be useful for semantic event debugging, but it is not the primary CPU-hotspot tool for this phase. Embedded `pprof` is intentionally deferred.

## Profiling Build Profile

A dedicated Cargo profile should inherit release optimization while retaining useful symbols, conceptually:

```toml
[profile.profiling]
inherits = "release"
debug = 1
strip = false
```

Exact Cargo settings may be adjusted only if required for usable Rust symbols, but the following properties are mandatory:

- optimized code comparable to release behavior;
- debug/symbol information sufficient for useful call stacks;
- no debug-build benchmark presented as a performance result.

Normal official capacity runs continue to use `--release`. A profiling run is diagnostic evidence and is not compared numerically to release CPU as though profiler overhead were zero.

## Profiling Prerequisites

Linux is the first-class profiling platform.

Before launching the benchmark, the profiling command must verify that required external tooling is available and usable enough to start:

- `perf`;
- `inferno-collapse-perf`;
- `inferno-flamegraph`;
- a FIFO creation mechanism such as `mkfifo` if FIFO control is used.

The command must fail early with a specific diagnostic when a prerequisite is missing.

`perf` permissions vary by Linux host and kernel policy. Permission denial, unsupported events, or unusable call-graph configuration must fail explicitly rather than silently producing an empty profile.

The harness must never change host `perf_event_paranoid` or other kernel security settings automatically. It may explain that host policy prevented profiling, but changing host policy remains a user/admin action.

## Exact Steady-Window Control

The profiling process attaches to the existing server PID with events initially disabled. The preferred mechanism is `perf record` control/ack FIFOs so the parent can synchronize enable/disable operations with the benchmark phase boundary.

Conceptual command:

```text
perf record
  -F 99
  -g
  --call-graph dwarf
  -p <server-pid>
  --delay=-1
  --control=fifo:<control>,<ack>
  -o <output>/perf.data
```

The exact command may include a bounded companion command/lifetime mechanism required by the installed `perf`, but it must remain attached to the server PID rather than profiling the load generator.

Steady transition:

```text
all clients connected
        |
transport telemetry converged
        |
perf process attached, events disabled
        |
parent -> perf: enable
        |
perf -> parent: ack
        |
server BeginMeasurement + transport start snapshot
        |
clients enter measured workload
```

End transition:

```text
measured deadline
        |
parent -> perf: disable/stop
        |
perf -> parent: ack / clean exit
        |
clients finish/drain
        |
transport end snapshot + server stop
```

No ramp samples are intentionally included. Small control-boundary skew may exist because process control is not instantaneous. If measurable, the profile metadata should record capture duration and boundary timing rather than pretending nanosecond-perfect alignment.

The profiling process must be terminated/reaped on all benchmark error paths. No orphan `perf` process or FIFO should remain after a normal or failed run.

## Sampling Configuration

Initial defaults:

- frequency: 99 Hz;
- call graph: DWARF;
- target: benchmark server PID only;
- window: scenario steady/measured window only.

These are diagnostic defaults, not stable public protocol constants. They should be represented in profile metadata so later runs remain comparable.

If DWARF unwinding proves unusable on the target build, the implementation may add an explicit alternative such as frame-pointer profiling, but it must not silently switch modes. The selected mode must be recorded.

## Profiling Artifacts

Each profiling run writes to an isolated output directory. A representative layout is:

```text
profiles/steady-1000/<run-id>/
├── run.json
├── profile.json
├── perf.data
├── perf-report.txt
├── stacks.folded
└── flamegraph.svg
```

`run.json` is the normal `RunReport` for the workload.

`profile.json` records at least:

- scenario name/path;
- Ardosia git commit;
- vendor revision;
- build profile;
- server PID;
- profiling tool/mode;
- `perf` version when obtainable;
- sample frequency;
- call-graph mode;
- requested capture duration;
- observed profiler duration;
- artifact paths;
- profiler success/failure information.

`perf-report.txt` is a deterministic non-interactive text report suitable for quick inspection and archival.

`stacks.folded` is the collapsed stack form used for the flamegraph.

`flamegraph.svg` is the primary visual hotspot artifact.

Intermediate artifacts may be optional only if storage becomes a concrete issue. `perf.data`, `perf-report.txt`, and `flamegraph.svg` are required for the first implementation.

## Flamegraph Pipeline

After a successful capture, the harness performs the equivalent of:

```text
perf script -i perf.data
    -> inferno-collapse-perf
    -> stacks.folded
    -> inferno-flamegraph
    -> flamegraph.svg
```

The tool output and exit statuses must be checked. An empty or failed stack conversion must not be reported as a successful flamegraph.

Profiler stderr belongs in logs/diagnostics, not in the machine-readable benchmark JSON stream.

## Profiling Pass/Fail Semantics

The underlying workload still uses the scenario's normal correctness gate. Separately, the `profile` command succeeds only if:

- the benchmark itself completes successfully;
- `perf` attaches to the intended server PID;
- the capture contains samples;
- post-processing completes;
- required artifacts are produced and non-empty.

A profiling-tool failure is not reclassified as a RakNet transport failure. The diagnostic must distinguish benchmark correctness from profiler failure.

# Part B: Constant-Population Churn

## Scenario Model

Churn is workload semantics and therefore belongs in the scenario file.

The existing top-level `clients` field remains the population target. There is no separate `target_clients` field because two population fields could contradict each other.

Representative scenario:

```toml
name = "churn-500"
clients = 500
protocol_version = 8
ramp_up_seconds = 10
hold_seconds = 60
connect_timeout_seconds = 5
seed = 1

[[traffic]]
kind = "unreliable"
direction = "bidirectional"
packets_per_second_per_client = 20.0
payload_bytes = 64

[[traffic]]
kind = "reliable_ordered"
direction = "bidirectional"
packets_per_second_per_client = 2.0
payload_bytes = 256

[[traffic]]
kind = "fragmented_reliable_ordered"
direction = "bidirectional"
packets_per_second_per_client = 0.2
payload_bytes = 4096

[rtt]
probes_per_second_per_client = 2.0
payload_bytes = 32

[churn]
replacements_per_second = 25.0
```

`[churn]` is optional. Existing scenarios remain backward compatible when it is absent.

Validation requires `replacements_per_second` to be finite and greater than zero when churn is configured.

## Canonical `churn-500`

Initial canonical values:

- target active population: 500;
- ramp: 10 seconds;
- measured churn window: 60 seconds;
- planned replacement rate: 25/sec;
- expected planned replacements over the measured window: 1500;
- protocol: RakNet 8;
- workload: the same full per-active-client traffic shape used by `steady-500`;
- RTT: 2 probes/sec per active client.

The objective is not to flood connection setup. The objective is to keep meaningful steady traffic active while continuously creating and destroying RakNet session state.

## Churn Lifecycle

The churn coordinator owns a fixed number of logical population slots equal to `scenario.clients`.

Each slot is in one of these conceptual states:

```text
Connecting -> Active -> PlannedDisconnect -> Connecting -> Active
```

The initial ramp fills all 500 slots. The measured churn scheduler then produces deterministic replacement ticks at 25/sec.

At each tick:

1. choose an eligible active slot deterministically;
2. request a planned clean disconnect for the current client generation;
3. when that generation reaches the planned-disconnect completion path, start a replacement connection for the same logical slot;
4. assign the replacement a globally unique client ID;
5. when its handshake succeeds, mark the slot active again and begin its normal measured workload;
6. continue scheduling future replacements independently of other in-flight replacements, subject to bounded state and normal timeout handling.

Multiple replacement handshakes may therefore be in flight at once. This is expected at 25 replacements/sec.

A planned churn disconnect must not increment `unexpected_disconnects` merely because the benchmark intentionally asked that client to leave.

## Deterministic Selection

The scheduler must be reproducible for a given scenario seed.

A simple deterministic round-robin over currently active logical slots is preferred unless testing demonstrates a reason for a seeded permutation. The important requirements are:

- no synchronized mass-wave disconnects;
- no starvation of a subset of slots;
- reproducible scheduling;
- no dependence on hash-map iteration order.

The churn rate uses the same deterministic scheduling philosophy as existing traffic lanes. Over a 60-second 25/sec canonical run, the scheduler must plan exactly 1500 replacements unless the benchmark fails first.

If the runtime cannot keep up with the configured schedule, that condition must be surfaced as churn scheduling/backlog failure rather than silently reducing the realized replacement rate.

## Unique Client IDs

Every replacement generation receives a new monotonically unique client ID for the run.

IDs are never intentionally recycled during churn.

This matters because ID reuse could hide stale state, late packets, incorrect cleanup, or generation-mixing bugs. Benchmark frames continue to carry the actual unique client ID, so a response from a previous generation cannot be mistaken for the replacement occupying the same logical slot.

Logical slot identity is internal orchestration state and is not substituted for the benchmark frame client ID.

## Workload During Churn

All currently active clients use the scenario's ordinary traffic and RTT configuration.

Initial clients begin measured workload at the measured boundary. Replacement clients begin workload after their replacement handshake succeeds.

A client in planned disconnect or connecting state contributes no application workload until it is active.

Traffic schedules for a replacement are derived deterministically from the scenario seed and its unique client ID. Replacement connection time may act as the local lane start, preventing all replacements from aligning to the original benchmark start instant.

The benchmark still aggregates workload and RTT across all client generations that were active during the measured window.

A planned disconnect near the measurement deadline must not create the same false send-error behavior already corrected for normal teardown. `ConnectionClosed` caused by an intentional lifecycle boundary remains benign where appropriate; genuine backpressure/backend/I/O failures remain failures.

## Client Control Model

The current shared phase watch channel is sufficient for global ramp/measure/abort but not for selecting one client for churn.

The churn design adds per-client or per-slot lifecycle control owned by the cohort/churn coordinator. The implementation should keep responsibilities separated:

- **global phase control**: ramp, measured window, global abort/shutdown;
- **slot lifecycle control**: active generation, planned disconnect, replacement spawn;
- **client task**: one RakNet connection generation, traffic/RTT work, and final result;
- **coordinator**: replacement scheduling, unique ID allocation, population accounting, result aggregation.

The public `ardosia-network` API does not change for churn.

## Admission Headroom

The logical target remains exactly 500 active clients, but the benchmark server must not enforce `max_connections = 500` during churn.

Transport cleanup and a replacement handshake can overlap briefly. If the server admission cap equals the logical population exactly, a healthy lifecycle can produce a false connection-capacity failure while the old session is still being removed internally.

For churn scenarios, server admission capacity therefore receives derived temporary headroom. The initial rule is:

```text
admission_headroom = ceil(replacements_per_second * connect_timeout_seconds)
server_max_connections = clients + admission_headroom
```

For canonical `churn-500`:

```text
ceil(25 * 5) = 125 headroom
server max_connections = 625
```

This does **not** redefine the target population as 625. The load generator still targets 500 active clients. The extra capacity exists only to avoid confusing cleanup overlap with capacity failure.

The derived headroom must be visible in the resolved scenario/run metadata or equivalent diagnostic output.

Transport telemetry remains the guard against lifecycle leakage. A server that accumulates stale sessions will show session/current/closed behavior and eventually fail the bounded headroom or final drain checks rather than being hidden by unlimited capacity.

## Churn Metrics

The report adds a dedicated optional churn result block. At minimum it contains:

- `planned_disconnects`;
- `completed_planned_disconnects`;
- `replacement_attempts`;
- `replacement_handshakes`;
- `replacement_failures`;
- `replacement_timeouts` if distinguished;
- `schedule_misses` or equivalent backlog indicator;
- `population_min`;
- `population_max`;
- `population_end`;
- `replacement_inflight_peak`;
- replacement handshake latency sample count;
- replacement handshake latency p50;
- replacement handshake latency p95;
- replacement handshake latency p99;
- replacement handshake latency max.

Replacement latency is measured on the loadgen's monotonic clock from replacement connection attempt start until successful RakNet handshake completion. No cross-process clock comparison is used.

Population metrics are based on event-level loadgen slot state, not only 1 Hz `/proc` or transport sampling, so short deficits are not invisible. Periodic transport `sessions_current` remains complementary server-side evidence.

## Population Semantics

A transient population dip is expected:

```text
500 active
-> planned disconnect
-> 499 active + 1 replacement connecting
-> 500 active
```

With 25 replacements/sec, multiple slots may be connecting concurrently.

Therefore `population_min == 500` is **not** a pass requirement.

Instead, sustained population health is defined by lifecycle completion:

- every planned disconnect must complete through the planned path;
- every required replacement must be attempted;
- every replacement must handshake within the existing `connect_timeout_seconds` bound;
- no replacement backlog may remain unresolved after the bounded drain;
- final active population before global shutdown must return to exactly `scenario.clients`.

`population_min`, `population_max`, and replacement latency are recorded for characterization. They become hard performance gates only if later evidence justifies explicit thresholds.

## Measurement and Drain Phases

A churn run extends the existing phase model:

1. startup/server readiness;
2. initial ramp to the full target population;
3. telemetry convergence;
4. measured churn window;
5. replacement drain/recovery;
6. final steady population verification;
7. global clean disconnect and server shutdown.

The measured window is still exactly `hold_seconds` for workload/resource/RTT rate calculations.

At the measured deadline:

- no new churn replacement ticks are scheduled;
- replacements already required by planned disconnects are allowed a bounded drain up to their existing connection timeout/deadline;
- final population must recover to the target;
- then all active final-generation clients disconnect cleanly.

The drain must be bounded. The harness must not wait indefinitely for a broken replacement.

Resource and workload rates for the canonical report remain based on the measured churn window. Drain-phase resource observations may be recorded separately later but are not mixed into steady/churn averages.

## Correctness Counters and Planned Disconnects

General correctness counters retain their existing meaning as much as possible:

- `unexpected_disconnects` counts unplanned connection loss only;
- `protocol_errors` counts malformed/invalid benchmark behavior;
- `send_errors` counts genuine benchmark send failures according to the existing send-error policy;
- `clean_disconnects` should remain useful for final benchmark shutdown rather than becoming ambiguous with thousands of planned churn exits.

Planned churn exits therefore belong primarily in the churn result block instead of being folded indistinguishably into `clean_disconnects`.

If implementation constraints require a total clean-disconnect counter, the report must still expose planned churn disconnects separately so old steady-scenario interpretation remains clear.

## Churn Pass/Fail Gate

`churn-500` passes only when all normal applicable correctness requirements pass and churn-specific requirements also pass.

Required churn conditions:

- initial ramp establishes 500/500 sessions;
- exactly the configured replacement schedule is realized for the full measured window (1500 planned replacements for canonical `churn-500`);
- `completed_planned_disconnects == planned_disconnects`;
- `replacement_attempts == planned_disconnects`;
- `replacement_handshakes == replacement_attempts`;
- `replacement_failures == 0`;
- `replacement_timeouts == 0` when tracked separately;
- no churn schedule/backlog misses remain hidden;
- all in-flight replacements drain within the bounded recovery phase;
- population returns to exactly 500 before final shutdown;
- zero unexpected disconnects;
- zero benchmark protocol errors;
- zero genuine benchmark send errors;
- zero transport timeouts;
- zero Ardosia outgoing queue/backpressure drops or disconnects;
- required workload classes remain nonzero during the measured window;
- final active clients disconnect cleanly during global shutdown.

The following remain record-only initially:

- server/loadgen/host CPU;
- RSS;
- throughput;
- RTT percentiles;
- replacement latency percentiles;
- population minimum during replacement overlap;
- retransmit/NACK counts unless they correspond to another explicit correctness failure;
- queue peaks when no drop/disconnect occurs.

Failure reasons must say whether the cause is initial ramp, planned disconnect, replacement handshake, replacement scheduler/backlog, transport health, benchmark protocol, or final drain.

## Transport Telemetry Under Churn

Unlike steady scenarios, session start/close deltas are expected to be nonzero during the measured window.

The transport report must therefore be interpreted differently for churn:

- `sessions_started` should reflect successful replacements;
- `sessions_closed` should reflect planned churn exits, subject to transport metric timing;
- `sessions_current` should return to target after drain;
- timeouts remain unexpected;
- queue/backpressure/order/split counters retain their existing meanings.

The harness must not reuse the steady-scenario assertion that `sessions_started` and `sessions_closed` deltas are zero.

Metric-cache convergence still matters at the initial ramp boundary and final drain boundary. Final verification should allow the bounded telemetry cache to catch up before declaring a lifecycle leak, using the same principle as the existing pre-steady convergence fix.

## Resource Interpretation

Server, loadgen, and host resource sampling remains separated.

For churn, the primary resource window covers the measured churn interval. It therefore includes:

- ordinary workload processing;
- planned disconnect cleanup;
- replacement connection setup;
- temporary overlapping connection lifecycle work.

This is intentional. The purpose of churn is to characterize the combined cost of maintaining live traffic while sessions turn over.

The initial ramp remains separately summarized so initial mass connection establishment is not confused with sustained churn.

## Report and Terminal Summary

For non-churn scenarios the existing JSON schema remains backward compatible through an absent/null churn result.

For churn scenarios, the terminal summary gains one compact line similar to:

```text
churn: planned=1500 replaced=1500 failed=0 pop_min=... pop_end=500 repl_p95=...ms
```

The normal lines for sessions/errors/traffic/RTT/RakNet/resources remain.

For profiling mode, profiler diagnostics and artifact locations go to stderr. The normal run JSON remains machine-readable and is additionally written as `run.json` inside the profile output directory.

## Error Handling

### Profiling

- missing `perf`/Inferno/FIFO tooling: fail before server launch;
- `perf` permission denied: profiler failure with actionable diagnostic;
- profiler exits unexpectedly: stop/abort benchmark cleanly and reap children;
- flamegraph conversion failure: preserve `perf.data` when valid and report post-processing failure;
- benchmark correctness failure while profiler runs: stop/reap profiler, preserve useful artifacts when safely finalized, and report benchmark failure separately.

### Churn

- planned disconnect command cannot be delivered: churn lifecycle failure;
- replacement connect error/timeout: replacement failure;
- unexpected remote disconnect: ordinary unexpected-disconnect failure;
- scheduler falls behind configured rate: schedule/backlog failure;
- no active slot is eligible at a scheduled tick due to unresolved prior churn: schedule/backlog failure evidence, not a silently skipped tick;
- final population cannot recover before bounded drain deadline: churn drain failure.

No panic should be required for normal failure reporting.

## Testing Strategy

Implementation follows TDD.

### Profiling unit/integration tests

Tests should cover logic that does not require privileged real profiling:

- profile CLI parsing;
- profiling output-path resolution;
- prerequisite detection using injectable/fake command paths where practical;
- profiler metadata serialization;
- profiler process/control state transitions using a fake profiler process or small fixture rather than requiring real `perf` in ordinary tests;
- correct server PID passed to profiler command construction;
- profiler cleanup on benchmark abort;
- post-processing command failure classification.

A real `perf` smoke is manual-only because GitHub-hosted environments and host security policy are not reliable profiling prerequisites.

### Churn tests

Tests should cover:

- parsing and validation of optional `[churn]`;
- backward compatibility of all existing scenarios;
- canonical `churn-500` checked-in shape;
- deterministic exact tick count for 25/sec over 60 seconds;
- deterministic slot selection;
- monotonically unique client ID allocation across replacements;
- planned disconnect not counted as unexpected;
- replacement success/failure/timeout accounting;
- population min/max/end accounting from event-level state;
- multiple concurrent in-flight replacements;
- scheduler backlog/miss behavior;
- derived admission headroom calculation;
- bounded drain completion and failure;
- churn-specific pass/fail report semantics;
- existing steady scenarios still require zero measured-window session churn.

A small real localhost churn smoke may use a tiny target/rate/duration in manual or focused verification, but the canonical 500-client churn workload remains heavy/manual-only.

## CI and Workflow Policy

Automatic heavy CI remains disabled.

Ordinary correctness CI remains manual-only unless separately agreed. The implementation may temporarily use a focused workflow to obtain RED/GREEN evidence when local execution is unavailable, but it must restore the workflow to manual-only immediately afterward, following the established project practice.

Profiling must never run automatically in GitHub Actions.

Canonical `churn-500` remains a manually selected benchmark scenario.

## Vendor Policy

The pinned vendor revision remains unchanged during implementation unless one of the new tests or profiles demonstrates a concrete vendor correctness problem or a profile-backed bottleneck worth evaluating.

A flamegraph hotspot alone is not permission to rewrite vendor code. Before a vendor patch, the evidence should establish:

1. the hotspot belongs to the transport path rather than the synthetic load generator or profiling artifact;
2. it is material at a relevant workload;
3. a change can be validated with correctness regression and before/after profiling/benchmark evidence.

## Expected Deliverables

The implementation slice should leave the branch with:

1. a `profiling` Cargo profile suitable for optimized symbols;
2. `ardosia-loadgen profile <scenario>` one-command server-only profiling;
3. automatic server PID attachment;
4. synchronized `perf` steady-window capture;
5. `run.json`, `profile.json`, `perf.data`, `perf-report.txt`, and `flamegraph.svg` artifacts;
6. optional `[churn]` scenario configuration;
7. dynamic slot/generation churn orchestration with unique client IDs;
8. churn-specific metrics and correctness gates;
9. checked-in `scenarios/churn-500.toml` at 500 target / 25 replacements/sec / 60 seconds;
10. manual-only benchmark workflow support for `churn-500`;
11. documentation for running profiling and churn locally;
12. regression coverage preserving all existing steady scenarios and public network API boundaries.

## Acceptance Evidence

This slice is considered implemented only after:

- ordinary workspace formatting/lint/tests pass;
- vendored RakNet library tests still pass without an unexplained vendor change;
- focused profiling orchestration tests pass;
- a manual local profiling run of `steady-1000` produces a non-empty usable flamegraph and raw profile artifacts on a compatible Linux host;
- the profile is reviewed before any optimization is proposed;
- canonical `churn-500` is then run manually;
- the churn JSON and terminal summary are reviewed for replacement correctness, population recovery, RTT, queues, transport health, and resource behavior.

The first churn result is characterization evidence. If it exposes a failure, the next action is systematic diagnosis, not immediate capacity scaling or vendor optimization.
