# steady-1000 CPU profile

Date: 2026-08-19

> **Historical evidence:** this report records the benchmark harness, vendored transport layout, toolchain, and profiler environment as they existed at measurement time. Those identifiers are preserved for provenance and are **not current setup instructions**. The active repository now uses Rust `1.98.0`, pins the public `ardosia-raknet` hardfork by exact Git revision, and no longer contains the old load generator/profiling harness. See the repository README for current development instructions.

## Run identity

- Ardosia commit: `43f68c7fb70987e8d941bfa7f3fff390797d118e`
- Vendor revision: `3edfb4170e6cb5aeed992b09b50176fb7e5b6079`
- Scenario: `steady-1000`
- Profiling build: optimized `profiling` Cargo profile
- Profiler: `perf version 7.1.8-1`
- Sampling: 99 Hz, DWARF call graph
- Requested capture: 60,000 ms
- Observed capture: 60,012 ms
- Host: x86_64 Linux 7.1.8-1-cachyos-bore-lto, 40 logical CPUs, ~15.4 GiB RAM

## Correctness

The profiled benchmark passed its strict correctness gate:

- 1000/1000 successful initial handshakes
- 0 failed handshakes
- 0 unexpected disconnects
- 0 protocol errors
- 0 benchmark send errors
- 1000 clean final disconnects
- 0 RakNet retransmissions
- 0 NACKs
- 0 outgoing queue/backpressure drops or disconnects
- 0 ordering/split drops
- 0 timed-out sessions

The transport start and end snapshots both reported 1000 current sessions during the measured window; measured session start/close deltas were zero.

## Workload and latency

Measured duration was ~60.001 s.

- TX rate: ~48,609.8 application frames/s
- RX rate: ~48,396.2 application frames/s
- TX payload: ~5.375 MB/s (~5,248.9 KiB/s)
- RX payload: ~5.350 MB/s (~5,224.6 KiB/s)
- RTT samples: 119,987
- RTT p50: 5 ms
- RTT p95: 10 ms
- RTT p99: 11 ms
- RTT max: 12.787626 ms
- Transport queue peak: 13 frames / 5,084 bytes

## Resource characterization

Because this was a diagnostic profiling build with profiler overhead, its CPU numbers are not interchangeable with release-only capacity measurements.

During the measured profile window:

- server CPU average: ~159.15%
- server CPU peak: ~161.48%
- server RSS peak: ~21.1 MiB
- load generator CPU average: ~278.35%
- load generator CPU peak: ~285.64%
- load generator RSS peak: ~33.0 MiB
- host CPU average: ~13.44%
- host CPU peak: ~16.78%

## CPU profile

`perf report --stdio` recorded 11K cycle samples with zero lost samples. The largest individual self-cost symbols were distributed rather than dominated by one transport routine:

- Tokio multi-thread worker context run: 4.21% self
- Tokio broadcast receiver `recv_ref`: 3.60% self
- then-vendored `TransportServer::prune_idle_sessions`: 3.56% self
- then-vendored `TransportServer::recv_and_process`: 2.83% self
- `Timespec::sub_timespec`: 2.61% self
- benchmark server `run_connection_task`: 2.35% self
- then-vendored `Session::on_tick`: 2.29% self
- Tokio notify/waker operations: multiple ~1-2% self entries
- contended futex mutex lock: 1.48% self
- allocator (`malloc`): 1.34% self in the flat report, with additional allocator cost visible in collapsed stacks
- clock/timekeeping paths (`clock_gettime`/VDSO/Instant arithmetic) recurring across folded stacks

Lower individual costs included RakNet payload queuing, ingress processing, frame/datagram codec work, ACK handling, rate limiting, queue operations, UDP recv/send, and Ardosia backend/channel work. No encode/decode, retransmission, fragmentation, queue-pressure, or syscall path individually dominated the profile.

## Interpretation

The evidence pointed to distributed runtime/session-maintenance overhead rather than one obvious packet-processing hotspot. At 1000 steady sessions, a meaningful fraction of CPU was spent in periodic lifecycle work, scheduler wakeups/synchronization, monotonic time queries, allocator activity, and channel/notification machinery.

There was no single transport hotspot large enough in this profile to justify a speculative RakNet patch. Correctness remained perfect under the measured workload, retransmission/NACK/drop counters remained zero, queues stayed small, and hot symbols were spread across maintenance/runtime responsibilities.

## Historical disposition

At the time of this profile, the next planned experiment was `churn-500`. That experiment was subsequently completed successfully and is preserved in [`2026-08-18-churn-500.md`](2026-08-18-churn-500.md).

The old profiling/load-generation harness has since been removed. Current development should not recreate or optimize against this profile without a new, explicitly designed benchmark environment. This report is machine/commit/workload-specific evidence, not a universal capacity claim or a current optimization roadmap.
