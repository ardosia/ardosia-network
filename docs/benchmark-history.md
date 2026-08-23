# Historical benchmark record

This document preserves the useful conclusions from the removed `ardosia-loadgen` benchmark harness. It is historical engineering evidence, not a production SLA or a universal player-capacity claim.

The complete pre-purge harness, scenarios, benchmark workflow, and reporting code remain recoverable from Git history at:

```text
0fe4e0bf908a08ac8e558ddd06bef84e90304e52
```

That commit is the last `main` revision before the load generator was removed from the active workspace.

## Evidence rules

All measurements below are specific to the recorded machine, commit, build profile, workload, and benchmark driver. In particular, localhost source-IP sharing and process resource limits materially affected high-client-count experiments.

Do not weaken production RakNet abuse-control defaults based on localhost benchmark artifacts.

## 300-session connection baseline

Historical report: [`results/2026-08-18-connect-300.md`](results/2026-08-18-connect-300.md)

Environment:

- GitHub-hosted Ubuntu 24.04
- Rust 1.88.0
- RakNet protocol 8
- upstream/vendor revision `3edfb4170e6cb5aeed992b09b50176fb7e5b6079`
- 300 clients, 10-second ramp, 60-second hold

Observed result:

- 300/300 successful handshakes
- 0 failed handshakes
- 0 unexpected disconnects
- 0 protocol/decode errors
- 300 clean disconnects
- total duration: 69,995 ms

Conclusion: the protocol-8 transport could establish and hold 300 localhost sessions cleanly under that connection-only scenario. This was not a throughput or capacity ceiling test.

## 500-session steady/churn characterization

The trusted steady-500-v2 comparison run recorded approximately:

- TX: 24,199.90 frames/s
- RX: 24,199.47 frames/s
- RTT p50/p95/p99: 4 / 10 / 11 ms
- server CPU average: 95.5167%
- server CPU peak: 100.8065%
- server RSS peak: 15.61 MiB
- loadgen CPU average: 102.2276%
- loadgen RSS peak: 20.79 MiB
- host CPU average: 8.6049%
- queue peak: 781 bytes
- retransmissions: 0
- NACKs: 0

The canonical churn run is preserved in [`results/2026-08-18-churn-500.md`](results/2026-08-18-churn-500.md).

Churn configuration and result:

- Ardosia commit `e0b501288486d0c837eee9f18a4f41518382637b`
- 500 target sessions
- 25 planned replacements/s
- 60-second measured window
- 1,500 planned disconnects
- 1,500 completed planned disconnects
- 1,500 replacement attempts
- 1,500 replacement handshakes
- 0 replacement failures
- 0 replacement timeouts
- 0 schedule misses
- population range: 499..500, ending at 500
- 0 transport timeout growth
- 0 queue/backpressure/order/split drops
- 0 retransmissions
- 0 NACKs

Measured workload:

- TX: 24,197.12 frames/s
- RX: 24,192.18 frames/s
- TX payload: 2,673,669.33 bytes/s
- RX payload: 2,673,102.40 bytes/s
- RTT samples: 59,972
- RTT p50/p95/p99: 4 / 10 / 11 ms
- RTT max: 12.162341 ms

Measured resources:

- server CPU average/peak: 100.0198% / 102.5641%
- server RSS average/peak: ~17.24 / ~18.66 MiB
- loadgen CPU average/peak: 111.1278% / 114.7208%
- loadgen RSS peak: ~71.71 MiB
- host CPU average/peak: 9.3103% / 14.7784%

Conclusion: continuous turnover at 25 replacements/s did not expose lifecycle correctness failures, and application throughput/RTT remained essentially unchanged from the trusted steady-500 baseline. The load generator itself became substantially more memory-hungry under churn, which is benchmark-driver behavior rather than server transport capacity.

## 1,000-session steady CPU profile

Historical report: [`results/2026-08-18-steady-1000-profile.md`](results/2026-08-18-steady-1000-profile.md)

Identity:

- Ardosia commit `43f68c7fb70987e8d941bfa7f3fff390797d118e`
- optimized profiling build
- `perf` 99 Hz, DWARF call graph
- 60,012 ms observed capture
- x86_64 Linux `7.1.8-1-cachyos-bore-lto`
- 40 logical CPUs, ~15.4 GiB RAM

Correctness:

- 1000/1000 successful handshakes
- 0 failed handshakes
- 0 unexpected disconnects
- 0 protocol errors
- 0 benchmark send errors
- 1000 clean disconnects
- 0 retransmissions
- 0 NACKs
- 0 queue/backpressure/order/split drops
- 0 timed-out sessions

Measured workload:

- TX: ~48,609.8 application frames/s
- RX: ~48,396.2 application frames/s
- TX payload: ~5.375 MB/s
- RX payload: ~5.350 MB/s
- RTT samples: 119,987
- RTT p50/p95/p99: 5 / 10 / 11 ms
- RTT max: 12.787626 ms
- queue peak: 13 frames / 5,084 bytes

Diagnostic profiling resources:

- server CPU average/peak: ~159.15% / ~161.48%
- server RSS peak: ~21.1 MiB
- loadgen CPU average/peak: ~278.35% / ~285.64%
- loadgen RSS peak: ~33.0 MiB
- host CPU average/peak: ~13.44% / ~16.78%

The CPU profile showed distributed runtime/session-maintenance overhead rather than one dominant packet-processing hotspot. Notable costs included Tokio worker/scheduling/notification work, `TransportServer::prune_idle_sessions`, `TransportServer::recv_and_process`, `Session::on_tick`, timekeeping, allocator activity, channels, and synchronization.

Conclusion: the profile did not justify a speculative RakNet algorithm/correctness patch.

## High-client-count diagnosis

Later shard-scaling experiments exposed two benchmark-environment ceilings that initially looked like transport capacity limits.

### ~1,012-client admission wall

The roughly 1,012-client plateau was caused by the **load-generator process** running with `RLIMIT_NOFILE=1024`. It was not evidence of a server-side 1k connection ceiling.

This distinction is important: one load-generator client consumed file descriptors, so the driver exhausted its own process limit before the server demonstrated an admission limit.

### 3,000-client localhost degradation

The severe 3,000-client degradation was traced to all localhost clients sharing the same source IP and therefore sharing a **per-IP connected processing budget**.

Raising only the experimental localhost connected-processing budget restored healthy 3,000-client runs at 8 and 10 worker shards. Production abuse-control defaults were deliberately left unchanged.

This establishes that the observed failing 3k runs were not sufficient evidence of an intrinsic 3k transport ceiling. It does **not** establish a universal production capacity above 3,000 players.

### UDP receive-buffer hypothesis

For the observed failing 3,000-client / 10-shard case, Linux UDP receive-buffer exhaustion was checked and falsified:

```text
RcvbufErrors=0
InErrors=0
```

Therefore that specific failure should not be attributed to kernel UDP receive-buffer drops.

### Scheduler finding

During the same scaling work, biased Tokio shard-worker selection was found capable of starving receive/outbound work. Fair scheduling was carried into the Ardosia RakNet hardfork.

Established connected traffic was also kept separate from the coarse offline/unknown per-IP packet window, while explicit IP blocks remained enforceable.

## Why the harness was removed

By August 2026, the main questions that justified the in-repository localhost load generator had been answered well enough to stop investing in the harness itself:

1. the ~1k wall was a driver file-descriptor limit;
2. the failing 3k localhost runs were materially distorted by one shared source-IP policy bucket;
3. a healthy 3k run could be restored without weakening production defaults;
4. the observed UDP receive-buffer hypothesis was falsified;
5. profiling pointed to distributed runtime/session-maintenance costs rather than a single obvious RakNet hot function.

The harness, scenarios, profiling tooling, benchmark workflow, and active benchmark documentation were therefore removed from the workspace so networking development could return to the transport facade, hardfork correctness, and protocol integration.

## Requirements for any future capacity harness

If capacity characterization resumes, do not resurrect the old localhost harness unchanged. A replacement should, at minimum:

- model independent source IPs or use distributed generators;
- record soft/hard process open-file limits before the run;
- record the exact network/RakNet revisions;
- make relevant abuse-control policy ceilings explicit;
- separate load-generator resource exhaustion from server resource exhaustion;
- record kernel UDP error counters;
- keep production policy defaults unchanged;
- treat results as machine/commit/workload-specific evidence rather than universal capacity.
