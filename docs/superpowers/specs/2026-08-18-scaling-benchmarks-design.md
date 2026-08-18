# Ardosia Network Scaling Benchmark Design

Date: 2026-08-18
Status: Design approved in chat; written spec pending user review

## Context

Phase 1 established a transport-only Ardosia networking API over a pinned `mcbe-rs/raknet-rust` snapshot and passed the `connect-300` correctness gate: 300 protocol-8 RakNet clients established, held for the required window, and disconnected cleanly without unexpected disconnects or protocol/decode errors.

Phase 2 extends the benchmark from connection-count correctness into repeatable mixed-traffic scaling measurements. The goal is to understand how the transport behaves under realistic per-connection pressure and how much CPU and memory the server, load generator, and host consume while doing so.

This phase remains transport-focused. MCPE packet codecs, game login, world simulation, entity logic, and game broadcast semantics stay outside `ardosia-network`.

## Goals

1. Add deterministic, declarative mixed RakNet workloads for steady-state tests.
2. Measure client-to-server and server-to-client transport behavior independently of game protocol logic.
3. Measure RTT percentiles with bounded memory.
4. Surface useful RakNet reliability, queue, ordering, fragmentation, RTT, and congestion telemetry through Ardosia-owned metric types.
5. Measure server-process, loadgen-process, and whole-host CPU/memory separately.
6. Keep one-command local benchmark ergonomics while separating server and clients into distinct OS processes.
7. Establish `steady-300` as the first strict mixed-traffic correctness gate.
8. Use `steady-500` and `ceiling-1000` for capacity characterization before setting hard performance/resource thresholds.
9. Keep all heavy benchmark workflows manual-only.

## Non-goals

Phase 2 will not:

- add MCPE packet definitions or game protocol behavior;
- emulate a complete MCPE 0.15.10 player;
- benchmark world simulation, chunk generation, entity ticks, inventory, or game broadcasts;
- define CPU, memory, throughput, or RTT pass/fail thresholds before real-host data exists;
- add packet loss/jitter injection yet;
- patch RakNet internals preemptively;
- treat localhost results as a production capacity guarantee.

Fanout/broadcast pressure and churn are explicitly separate scenarios after the steady-state path is trustworthy.

## High-Level Architecture

The existing `local` command currently runs the benchmark server as a Tokio task inside the same process as the fake clients. That was sufficient for Phase 1 correctness, but it cannot produce meaningful server-vs-loadgen CPU/RSS measurements.

Phase 2 changes `local` into a process orchestrator:

```text
ardosia-loadgen local <scenario>
              |
              +-- child process: benchmark server
              |      +-- ardosia-network
              |      +-- RakNet
              |      +-- UDP
              |
              +-- parent process: synthetic RakNet clients
              |
              +-- resource sampler
                     +-- server PID CPU/RSS
                     +-- loadgen PID CPU/RSS
                     +-- whole-host CPU/memory
```

The user-facing command remains one-shot and reproducible. Internally, server and client execution are separated so process-level resource measurements are real rather than blended.

## Child-Process Benchmark Server

`ardosia-loadgen local` launches the same binary in an internal server mode. The child process binds the requested address through the public `ardosia-network` API and owns all benchmark-server connection tasks.

The parent and child use a minimal control protocol over redirected stdin/stdout or equivalent local pipes:

1. child starts and binds the server;
2. child emits a machine-readable `ready` message only after the UDP target is usable;
3. parent starts the client ramp;
4. parent samples process/host resources during ramp and the measured hold window;
5. parent requests shutdown after the scenario completes or aborts;
6. child returns a final machine-readable server report containing transport metrics and benchmark-server workload counters;
7. parent combines child data, client data, environment data, and resource summaries into the final report.

Human-readable logs must use stderr so stdout can remain machine-readable for parent/child control and final JSON output.

Unexpected child exit is a benchmark failure. The parent must terminate or reap the child on all normal and error paths so repeated runs do not leave orphan benchmark servers.

## Scenario Model

The existing connection/ramp/hold fields remain. Phase 2 adds deterministic workload configuration and a seed.

Representative shape:

```toml
name = "steady-300"
clients = 300
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
```

Exact serialization names may be refined during implementation, but the following properties are required:

- traffic is declarative and checked into the repository;
- workload generation is deterministic for a given seed;
- rates may be fractional where useful;
- direction is explicit;
- payload size is explicit;
- reliability class is explicit through the workload kind;
- `connect-300` remains valid without workload sections.

## Initial `steady-300` Workload

The initial workload intentionally avoids broadcast/fanout. Every established connection independently carries bidirectional traffic:

```text
client <-------> server

small unreliable frames       high rate
medium reliable-ordered frames lower rate
large fragmented frames        occasional
RTT probe / echo               low rate
```

Recommended starting values are scenario data, not production constants:

- unreliable: 20 packets/sec/client, 64-byte payload, each direction;
- reliable ordered: 2 packets/sec/client, 256-byte payload, each direction;
- fragmented reliable ordered: 0.2 packets/sec/client, 4096-byte payload, each direction;
- RTT probes: 2 probes/sec/client.

The design deliberately starts below pathological packet rates. The first objective is to establish a trustworthy mixed-load baseline and instrumentation before increasing pressure.

## Benchmark Frame Format

Synthetic benchmark payloads use a small internal frame format owned by `ardosia-loadgen`, not by `ardosia-network` and not by `ardosia-protocol`.

Conceptual fields:

```text
magic/version
frame kind
client id
sequence number
probe id / timestamp fields where applicable
payload bytes
```

Required frame kinds include:

- one-way workload data;
- echo request;
- echo response.

The codec must reject malformed frames cleanly and must not panic on arbitrary bytes.

`ardosia-network` continues to receive and send opaque `Bytes`; benchmark framing never becomes part of its stable public API.

## Server Workload Behavior

The benchmark server performs only the minimum application work required to exercise transport:

- receive synthetic frames;
- validate/decode the benchmark header;
- count received workload bytes/frames by kind;
- echo RTT probes;
- generate configured server-to-client workload frames independently per connection;
- track successful/failed benchmark sends.

It must not implement MCPE packets or game semantics.

Server-to-client workload generation must run per connection and not through a global broadcast loop for `steady-300`. Broadcast/fanout is a later scenario because its cost model is materially different.

## Client Workload Behavior

After the ramp succeeds, each client enters the measured hold phase while continuing to poll RakNet events. Workload timers and incoming-event processing run concurrently.

Each client maintains deterministic sequence counters and schedules workload classes at their scenario-configured rates. Timing should avoid synchronized bursts where practical; deterministic per-client phase offsets derived from the scenario seed are preferred so all 300 clients do not send every traffic class on the same instant.

Client task results aggregate:

- workload frames/bytes sent by kind;
- workload frames/bytes received by kind;
- send errors;
- protocol/decode errors;
- unexpected disconnects;
- RTT samples/summary;
- clean disconnect status.

## RTT Measurement

Application-level RTT probes provide end-to-end transport latency for the synthetic benchmark path.

The client records a monotonic send instant indexed by probe ID. The server echoes the probe ID and any opaque timestamp bytes unchanged. When the response arrives, the same client process calculates elapsed time from its own monotonic clock.

No cross-process wall-clock comparison is used.

The report must expose at least:

- sample count;
- p50 RTT;
- p95 RTT;
- p99 RTT;
- maximum RTT.

Aggregation must be bounded-memory. The implementation may use a fixed histogram or another bounded summary structure, but must not retain an unbounded vector of every RTT sample for long-running tests.

Missing or insufficient samples are reported explicitly rather than converted to zero latency.

## Ardosia Transport Metrics Facade

The vendored RakNet implementation already exposes rich telemetry including ACK/NACK counters, retransmission counts, queue pressure, split/drop information, and RTT/congestion values. Phase 2 should translate the useful subset into Ardosia-owned metric structs instead of exposing vendor structs publicly.

The stable facade should group metrics conceptually as follows.

### Sessions

- current sessions;
- sessions started total;
- sessions closed total;
- timed-out sessions;
- local/remote disconnect categories where useful.

### Traffic

- ingress datagrams;
- ingress frames;
- forwarded packets;
- forwarded bytes.

### Reliability

- reliable datagrams sent;
- retransmitted datagrams;
- ACKs emitted;
- NACKs emitted;
- datagrams ACKed;
- datagrams NACKed.

### Queue and Backpressure

- current pending outgoing frames;
- current pending outgoing bytes;
- peak pending outgoing frames during the measured window;
- peak pending outgoing bytes during the measured window;
- queue drops;
- queue defers;
- backpressure delays/drops/disconnects.

### Ordering and Fragmentation

- duplicate reliable drops;
- stale ordered/sequenced drops;
- ordered-buffer-full drops;
- split TTL drops;
- relevant unhandled-frame pressure where useful.

### RakNet Timing / Congestion

Where meaningful in the vendor snapshot:

- smoothed RTT;
- RTT variance;
- resend RTO;
- congestion window.

Vendor-specific field names remain private. If a vendor field cannot be aggregated into a meaningful public value, it should not be exposed merely because it exists.

## Metric Sampling and Deltas

Counters are not useful if a run reports only lifetime totals from process startup. Benchmark reporting therefore records:

1. a start snapshot near the beginning of the measured window;
2. periodic snapshots during the measured window;
3. an end snapshot;
4. counter deltas between start and end;
5. peaks for gauge-like values such as queue depth.

The final report distinguishes counters, gauges, deltas, and peaks clearly.

A final queue depth of zero must not hide a large transient queue spike.

## Resource Sampling

Resource measurements are observational in Phase 2. They do not affect the pass/fail result unless sampling itself exposes a benchmark-control failure such as losing the server child process.

Sampling cadence defaults to approximately once per second.

Linux is the first-class implementation target. `/proc`-based accounting is acceptable for the initial implementation because Ardosia's intended server environment is Linux. Platform-specific collection must be isolated behind an internal sampler interface so unsupported platforms can report unavailable fields instead of fabricated values.

### Server Process

Record at least:

- average CPU utilization;
- peak CPU utilization;
- average RSS bytes;
- peak RSS bytes.

Optional process fields may include virtual memory and thread count if they are reliable and inexpensive.

### Load Generator Process

Record the same CPU/RSS summary separately for the parent/client process.

This separation is mandatory: a localhost run must not report one combined process number and imply it is server cost.

### Whole Host

Record at least:

- average host CPU utilization;
- peak host CPU utilization;
- average used memory;
- peak used memory;
- minimum available memory;
- total physical memory;
- logical CPU count.

Load average and host network counters may be added if they can be collected consistently, but are not required for the first Phase 2 implementation.

CPU percentages must document their normalization. Process CPU should allow values above 100% when one process consumes more than one logical core, unless the implementation explicitly chooses and documents a different normalization.

## Report Schema

Every benchmark run emits one structured JSON report and a concise terminal summary.

The JSON report contains three top-level domains:

```text
environment
scenario
results
```

### Environment

Include, when available:

- Ardosia git commit;
- pinned vendor revision;
- Rust version;
- OS and kernel;
- architecture;
- logical CPU count;
- total physical RAM.

Unavailable environment fields are represented as optional/unavailable values, not invented defaults.

### Scenario

Embed the full resolved scenario configuration used for the run, including:

- client count;
- protocol version;
- ramp/hold durations;
- traffic definitions;
- RTT settings;
- deterministic seed.

This prevents a result file from depending on an external scenario file that may later change.

### Results

Include:

- handshake/disconnect/protocol correctness;
- workload tx/rx frames and bytes by kind/direction;
- workload rates over the measured window;
- RTT summary;
- Ardosia/RakNet transport telemetry deltas and peaks;
- server-process resources;
- loadgen-process resources;
- host resources;
- total run duration and measured-window duration;
- pass/fail and explicit failure reasons.

The terminal summary should surface the most useful values without requiring manual JSON inspection: establishment count, correctness failures, throughput, RTT p50/p95/p99, retransmits, queue peak, server CPU/RSS peak, loadgen CPU/RSS peak, and host memory pressure.

## Pass / Fail Semantics

Performance and resource values are record-only initially. The benchmark must not invent CPU, memory, throughput, or latency gates before representative hardware data exists.

### `steady-300` strict correctness gate

`steady-300` passes only when:

- 300/300 requested sessions establish;
- all 300 remain healthy throughout the measured window;
- zero unexpected disconnects occur;
- zero RakNet/benchmark protocol-decode errors occur;
- zero Ardosia queue/backpressure drops occur under the baseline workload;
- benchmark control completes normally;
- required workload classes actually send/receive frames rather than silently producing no traffic;
- clients disconnect cleanly at the end.

RTT, CPU, RSS, throughput, ACK/NACK counts, retransmits, and other performance values are recorded but do not fail the run solely for being high.

A scenario must distinguish transport-health failures from resource observations in the failure reason.

## Scenario Progression

### `connect-300`

Retain the existing Phase 1 scenario as a low-traffic connection correctness regression.

### `steady-300`

First Phase 2 milestone and strict mixed-traffic correctness gate. Uses the initial bidirectional workload and 60-second measured window.

### `steady-500`

Uses the same workload shape as `steady-300`, scaled to 500 sessions. Correctness failures remain failures; CPU/RAM/RTT/throughput remain characterization data rather than hard capacity gates.

### `ceiling-1000`

Diagnostic capacity test using a moderate mixed workload. Its purpose is to locate architectural cliffs and degradation points rather than establish a production requirement. The report must remain useful even if 1000/1000 cannot be established.

### Later: `fanout-300`

A separate server-send amplification scenario where one logical event fans out to many/all clients. It is deliberately excluded from `steady-300` because fanout introduces a different server/application cost model.

### Later: `churn-500`

A lifecycle-stress scenario that disconnects and reconnects a controlled portion of a ~500-client population while the remaining sessions carry traffic.

Loss/jitter profiles remain a subsequent phase after the clean-network workload and telemetry path are trusted.

## Heavy Benchmark Execution

300/500/1000-client benchmark runs are explicit operations, not ordinary test-suite work.

GitHub Actions workflows for heavy scenarios remain `workflow_dispatch`-only. Push and pull-request events must not automatically start them.

Normal correctness CI may also remain manual-only under the repository's current hosted-runner budget policy unless the user later chooses otherwise.

Local host runs are the preferred source for hardware-specific resource characterization because hosted runner hardware is shared/virtualized and should not be treated as the target production machine.

## Testing Strategy

### Unit Tests

Add focused tests for:

- scenario workload parsing and validation;
- deterministic scheduling/seed behavior;
- benchmark frame encode/decode and malformed input rejection;
- bounded RTT percentile/histogram aggregation;
- resource sample averaging/peaks/minimums;
- `/proc` parser helpers using fixture text rather than depending entirely on live procfs;
- vendor telemetry to Ardosia metric mapping;
- counter-delta and gauge-peak aggregation;
- report pass/fail logic.

### Integration Tests

Use real loopback UDP and subprocesses for:

- child benchmark server startup and `ready` handshake;
- controlled shutdown and child reaping;
- unexpected child-exit handling;
- bidirectional unreliable workload delivery;
- bidirectional reliable-ordered delivery;
- fragmented workload delivery;
- RTT request/echo response;
- final child server metric handoff;
- combined report assembly.

Integration tests should use small client counts and short durations. They must not turn `cargo test` into a 300-client load test.

### Manual Load Tests

Run and archive/inspect explicit scenario reports for:

1. `steady-300`;
2. `steady-500`;
3. `ceiling-1000`.

`fanout-300`, `churn-500`, and network impairment scenarios are later milestones.

## Error Handling

Phase 2 must make benchmark-control errors distinguishable from transport failures.

Expected categories include:

- child process spawn failure;
- child failed to become ready;
- child exited unexpectedly;
- malformed child control/report message;
- resource metric unavailable;
- scenario configuration invalid;
- workload send failure;
- malformed benchmark frame;
- RTT probe timeout/loss;
- transport disconnect/protocol error;
- queue/backpressure failure.

Resource metrics being unavailable on an unsupported platform is not itself a transport failure. The report records availability explicitly.

## Dependency and Boundary Rules

- `ardosia-network` does not depend on `ardosia-loadgen`.
- benchmark framing and resource sampling stay in `ardosia-loadgen`.
- `ardosia-network` may gain richer Ardosia-owned transport metrics but must not expose vendor telemetry structs publicly.
- `ardosia-protocol` remains completely independent of this work.
- no MCPE packet type enters a Phase 2 API.
- vendor source remains unmodified unless a failing correctness test or profile/benchmark observation later justifies a focused patch.

## Implementation Order

Phase 2 should be implemented in this order:

1. Extend scenario and report schemas with backward compatibility for `connect-300`.
2. Add benchmark frame codec and bounded RTT summary.
3. Add Ardosia-owned rich transport telemetry mapping/snapshots.
4. Add Linux resource sampler with unit-testable parsers and unavailable fallbacks.
5. Split local benchmark server into a child process with ready/stop/final-report control.
6. Add per-client and per-server mixed workload generation.
7. Assemble periodic sampling, deltas, peaks, and final JSON/terminal report.
8. Add and pass a small-process integration workload.
9. Add `steady-300` and run the strict 300-client mixed-load gate.
10. Only after `steady-300` is trusted, add/run `steady-500` and `ceiling-1000` characterization.
11. Review profiles/metrics before proposing any RakNet vendor patch.

## Success Criteria

Phase 2 is successful when:

- `local` runs server and clients as separate OS processes while preserving one-command ergonomics;
- server, loadgen, and host CPU/RAM are measured separately on Linux;
- resource/performance metrics are record-only, with explicit unavailable values where necessary;
- mixed bidirectional unreliable, reliable-ordered, fragmented, and RTT traffic is scenario-driven and deterministic;
- RTT p50/p95/p99/max are produced using bounded memory;
- Ardosia exposes meaningful RakNet telemetry without leaking vendor public types;
- reports capture transport counter deltas, queue peaks, resource summaries, scenario config, and environment metadata;
- `steady-300` passes its strict mixed-traffic correctness gate;
- `steady-500` and `ceiling-1000` produce useful characterization data;
- heavy hosted-runner scenarios remain manual-only;
- no MCPE game protocol logic enters `ardosia-network`.