# Ardosia RakNet scaling benchmarks

The scaling benchmark is designed to run the server target and load generator as separate processes so CPU and RSS can be reported independently. Linux is the preferred benchmark host because `/proc` provides the process and host resource counters used by the sampler. On other operating systems, unsupported resource fields may be absent without failing the correctness gate.

## Before a heavy run

Use a quiet machine, close unrelated high-load applications, and run from the repository root on the exact commit you want to characterize.

Verify the source tree first:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --manifest-path vendor/raknet-rust/Cargo.toml --lib
```

The report records best-effort environment information including the Git commit, Rust version, OS/kernel, architecture, logical CPU count, total memory, build profile, and the pinned RakNet vendor revision.

## 300-client steady-state gate

Run the strict 300-client mixed workload in release mode:

```bash
cargo run --release -p ardosia-loadgen -- local scenarios/steady-300.toml > steady-300.json
```

The concise human summary is written to stderr and remains visible in the terminal. The complete machine-readable report is written to stdout and redirected to `steady-300.json`.

`steady-300` runs 300 protocol-8 sessions with a 10-second connection ramp and a 60-second measured hold. Each client participates in bidirectional unreliable, reliable-ordered, fragmented reliable-ordered traffic, plus RTT probes.

The run passes only when the correctness gate is clean: all requested sessions establish, no unexpected disconnect/protocol/send failures occur, queue/backpressure drops remain zero, every configured workload class moves traffic in the required direction, RTT samples are produced, no session lifecycle churn occurs during the measured steady window, and all clients disconnect cleanly. CPU, RSS, latency, ACK/NACK, retransmission, and queue-peak values are characterization data and do not fail the run merely for being high.

## 500-client characterization

Only after inspecting a passing `steady-300` result:

```bash
cargo run --release -p ardosia-loadgen -- local scenarios/steady-500.toml > steady-500.json
```

This uses the same mixed workload and 60-second measured hold with 500 sessions.

## 1000-client ceiling characterization

Only after inspecting the 500-client result:

```bash
cargo run --release -p ardosia-loadgen -- local scenarios/ceiling-1000.toml > ceiling-1000.json
```

The 1000-client scenario uses a 20-second connection ramp and deliberately lower per-client traffic rates so it characterizes the session ceiling without simply doubling the 500-client packet rate.

The full-load 1000-client scenario is also available when throughput scaling rather than session-only scaling is the target:

```bash
cargo run --release -p ardosia-loadgen -- local scenarios/steady-1000.toml > steady-1000.json
```

## Server-only CPU profiling

Profiling is Linux-only and intentionally remains a local/manual diagnostic workflow. It does not run from GitHub Actions and it does not change kernel perf policy automatically.

Required tools are:

- `perf`
- `mkfifo`
- `inferno-collapse-perf`
- `inferno-flamegraph`

Run the canonical server profile with:

```bash
cargo run --profile profiling -p ardosia-loadgen -- \
  profile scenarios/steady-1000.toml \
  --output profiles/steady-1000
```

The parent load generator already owns the real benchmark child process and therefore knows its PID. The profiler attaches to that server PID automatically; there is no manual PID race.

The capture is restricted to the measured steady window. Ramp/handshake setup happens before `perf` is enabled, and profiling stops before measurement teardown so cleanup work does not contaminate the steady CPU profile.

A run directory is created under the requested output root, normally `profiles/<scenario>/<run-id>/`, and contains the machine-readable benchmark report plus profiling metadata and derived artifacts such as `perf.data`, the text report, folded stacks, and a flamegraph. Raw `perf.data` can be large and should remain an untracked local artifact.

Profiler overhead means CPU/resource numbers from a profiling run are diagnostic and should not replace an ordinary release-mode capacity baseline. A hotspot is evidence for investigation, not an automatic reason to patch the vendored RakNet implementation.

## Constant-population churn

The canonical churn scenario stresses session lifecycle cleanup and replacement while preserving a steady application workload:

```bash
cargo run --release -p ardosia-loadgen -- \
  local scenarios/churn-500.toml > churn-500.json
```

`churn-500` starts with 500 active protocol-8 sessions. During the 60-second measured window it schedules 25 planned replacements per second, for exactly 1500 nominal replacement ticks. The first tick occurs after one full replacement period rather than immediately at measurement start.

Every planned disconnect is followed by a new handshake using a never-reused client ID. Non-churning clients continue the normal full mixed workload, and a replacement joins that workload after it connects. The desired active population remains 500 throughout the run.

The benchmark child uses a temporary admission capacity of 625 connections for this scenario: 500 target sessions plus 125 lifecycle headroom. The extra capacity exists only to tolerate bounded overlap between session cleanup and replacement admission; it does not change the target population.

### Measured window versus bounded drain

Churn lifecycle accounting is deliberately separate from the transport delta of the measured 60-second window. The final nominal tick is at exactly 60.000 seconds, so a disconnect/replacement that starts at the boundary may complete immediately afterward in the bounded drain.

For that reason it is valid for measured transport counters to show, for example, 1499 sessions started and 1499 sessions closed while the final churn lifecycle report records all 1500 planned disconnects and 1500 replacement handshakes. The benchmark never invents post-window churn; the drain only finishes lifecycle work whose nominal tick belonged to the measured window.

After the measured window:

1. application measurement ends;
2. pending planned disconnect/replacement work is allowed to finish within the bounded drain;
3. transport state is reconciled back to exactly 500 active sessions with no timeout growth;
4. only then are the final 500 active clients shut down cleanly.

### Churn correctness gates

A canonical churn run requires all of the following:

- all 500 initial handshakes succeed;
- all 1500 nominal replacements are planned, completed, attempted, and successfully handshaken;
- replacement failures, replacement timeouts, and schedule misses are zero;
- logical population ends at 500 and post-drain transport also reports exactly 500 live sessions;
- transport timeouts do not grow;
- unexpected disconnects, protocol/decode errors, genuine send errors, queue/backpressure failures, ordering/split drops, and final shutdown failures remain zero;
- configured traffic and RTT probes continue completing;
- exactly 500 clients perform the final clean shutdown.

Population minimum, replacement latency percentiles, CPU/RSS, RTT latency, ACK/NACK counts, retransmits, congestion values, and queue peaks without drops are characterization metrics. They are recorded but are not arbitrary performance thresholds in the correctness gate.

The terminal summary adds a churn line similar to:

```text
churn: planned=1500 replaced=1500 failed=0 pop_min=<n> pop_end=500 repl_p95=<x>ms
```

The JSON report contains the full `results.churn` block, including admission headroom, lifecycle totals, population extrema, replacement latency, and the post-drain transport snapshot.

## What to send back for analysis

Keep the JSON report from each run. It contains separate ramp and steady resource windows for the server, load generator, and host; workload rates; RTT percentiles; RakNet counter deltas; queue peaks; correctness fields; measured duration; and environment metadata. Churn reports additionally contain replacement lifecycle and post-drain reconciliation data.

For profiling, `profile.json`, `perf-report.txt`, `stacks.folded`, `flamegraph.svg`, and `run.json` are normally enough for first-pass hotspot analysis; the much larger `perf.data` file is only needed for deeper stack inspection.

Compare results from the same machine/environment rather than mixing hosts. If a correctness gate fails, preserve that report and diagnose the concrete failure before increasing the load.

## GitHub Actions fallback

`.github/workflows/baseline.yml` is manual-only and exposes the benchmark scenarios as explicit choices, including `churn-500`. Heavy hosted runs are optional; local release-mode results on a controlled machine are preferred for performance characterization. Profiling is intentionally not part of the Actions workflow.
