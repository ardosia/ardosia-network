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

The run passes only when the correctness gate is clean: all requested sessions establish, no unexpected disconnect/protocol/send failures occur, queue/backpressure drops remain zero, every configured workload class moves traffic in the required direction, RTT samples are produced, and all clients disconnect cleanly. CPU, RSS, latency, ACK/NACK, retransmission, and queue-peak values are characterization data and do not fail the run merely for being high.

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

## What to send back for analysis

Keep the JSON report from each run. It contains separate ramp and steady resource windows for the server, load generator, and host; workload rates; RTT percentiles; RakNet counter deltas; queue peaks; correctness fields; measured duration; and environment metadata.

Start with `steady-300.json`. If it fails, preserve that report and diagnose the concrete failure before moving to 500 clients. If it passes, compare 500 and then 1000 against the same machine/environment rather than mixing hosts.

## GitHub Actions fallback

`.github/workflows/baseline.yml` is manual-only and provides the same scenarios as explicit choices. Heavy hosted runs are optional; local release-mode results on a controlled machine are preferred for performance characterization.
