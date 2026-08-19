# Profiling and Churn Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add synchronized server-only CPU profiling for the trusted `steady-1000` workload, review that evidence, then add deterministic constant-population `churn-500` with 500 active clients and 25 planned replacements/sec.

**Architecture:** First make the benchmark's measured-window end explicit so clients and server workload stop without immediately tearing sessions down. Profiling then wraps the existing child-process local benchmark: the parent already owns the server PID, attaches `perf` disabled after ramp convergence, enables it before the steady window, disables/stops it at the measured deadline, and only then performs benchmark teardown. Churn later reuses the same explicit post-measurement phase with a deterministic slot/generation coordinator, bounded admission headroom, event-level lifecycle accounting, and post-drain transport reconciliation.

**Tech Stack:** Rust 1.88+ / Edition 2024, Tokio, Clap, Serde/TOML/JSON, existing vendored `raknet-rust`, Linux `perf`, `mkfifo`, Inferno (`inferno-collapse-perf`, `inferno-flamegraph`).

**Spec:** `docs/superpowers/specs/2026-08-18-profiling-churn-design.md`

## Global Constraints

- Keep the pinned RakNet vendor revision `3edfb4170e6cb5aeed992b09b50176fb7e5b6079` unchanged unless concrete correctness or profile-backed evidence justifies a separate vendor change.
- Keep the public `ardosia-network` API unchanged; profiling/churn belong to `ardosia-loadgen` and benchmark orchestration.
- Official capacity runs remain `--release`; diagnostic profiling uses a dedicated optimized `profiling` Cargo profile with symbols.
- Profiling is Linux-first and must never change host `perf_event_paranoid` or other kernel security settings automatically.
- Profiling targets only the benchmark server child PID and intentionally excludes ramp/handshake and final teardown samples.
- Profiling defaults are 99 Hz and DWARF call graphs; selected values must be recorded in metadata.
- `perf`, `inferno-collapse-perf`, `inferno-flamegraph`, and `mkfifo` are external prerequisites; missing/unusable tooling fails explicitly.
- Existing scenarios remain backward compatible when `[churn]` is absent.
- `clients` remains the logical churn population target; do not add a second target-client field.
- Canonical churn is 500 target clients, 25 replacements/sec, 60 measured seconds, exactly 1500 nominal replacement ticks, protocol 8, and the full `steady-500` traffic/RTT shape.
- Every replacement generation receives a monotonically unique client ID; IDs are never intentionally reused.
- Planned churn disconnects are not unexpected disconnects and do not inflate final `clean_disconnects`.
- Churn resource/workload/RTT rates cover only the measured window; replacement drain is bounded and excluded from those rates.
- Performance/resource values remain characterization-only; correctness gates stay strict.
- Heavy profiling/churn workflows remain manual-only; ordinary CI remains manual-only unless separately agreed, and any temporary focused trigger must be restored immediately.
- No automatic profiling in GitHub Actions.

---

## File Structure

New focused units:

- `crates/ardosia-loadgen/src/cli.rs` — Clap command model shared by binary and parsing tests.
- `crates/ardosia-loadgen/src/profiling.rs` — profile request/result/metadata types and artifact paths.
- `crates/ardosia-loadgen/src/profiling/perf.rs` — `perf record` process, FIFO control/ACK synchronization, capture lifecycle.
- `crates/ardosia-loadgen/src/profiling/tools.rs` — prerequisite/version probing and artifact post-processing.
- `crates/ardosia-loadgen/src/churn.rs` — deterministic churn schedule, slot selection, ID allocation, lifecycle accounting, dynamic generation cohort.
- `crates/ardosia-loadgen/tests/cli.rs` — profile CLI parsing regression.
- `crates/ardosia-loadgen/tests/profiling.rs` — path/metadata/tool-command tests without privileged `perf`.
- `crates/ardosia-loadgen/tests/churn.rs` — deterministic churn state/schedule/reconciliation tests.
- `crates/ardosia-loadgen/tests/client_lifecycle.rs` — planned/final disconnect classification tests.
- `scenarios/churn-500.toml` — canonical constant-population churn workload.

Existing units changed intentionally:

- `Cargo.toml` — optimized `profiling` Cargo profile.
- `.gitignore` — ignore `/profiles/` output.
- `crates/ardosia-loadgen/src/main.rs` — command dispatch and compact profile/churn output.
- `crates/ardosia-loadgen/src/lib.rs` — expose testable CLI/profiling/churn modules.
- `crates/ardosia-loadgen/src/scenario.rs` — optional `ChurnConfig`, validation, admission capacity.
- `crates/ardosia-loadgen/src/client_task.rs` — explicit post-measurement/final shutdown, later planned-disconnect generation control.
- `crates/ardosia-loadgen/src/child_protocol.rs` — explicit end-of-measurement command/ack.
- `crates/ardosia-loadgen/src/server_target.rs` — stop server workload at measurement end and use churn headroom.
- `crates/ardosia-loadgen/src/runner.rs` — explicit measurement boundaries, profiling hooks, churn loop/drain/reconciliation.
- `crates/ardosia-loadgen/src/report.rs` — optional churn result and steady/churn gates.
- `.github/workflows/baseline.yml` — manual `churn-500` choice only.
- `docs/benchmarks.md` — local profiling/churn instructions.

---

### Task 1: Profiling Build Profile, CLI Surface, and Metadata Model

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/ardosia-loadgen/src/cli.rs`
- Create: `crates/ardosia-loadgen/src/profiling.rs`
- Modify: `crates/ardosia-loadgen/src/main.rs`
- Modify: `crates/ardosia-loadgen/src/lib.rs`
- Modify: `.gitignore`
- Create: `crates/ardosia-loadgen/tests/cli.rs`
- Create: `crates/ardosia-loadgen/tests/profiling.rs`

**Interfaces:**
- Produces: `cli::Cli`, `cli::Command::Profile { scenario, bind, output }`.
- Produces: `ProfileConfig`, `ProfileArtifacts`, `ProfileMetadata`, `ProfileRun`, `CallGraphMode`.
- Produces: `resolve_run_dir(root, run_id) -> PathBuf`.

- [ ] **Step 1: Write failing CLI and artifact-path tests**

`tests/cli.rs`:

```rust
use ardosia_loadgen::cli::{Cli, Command};
use clap::Parser;

#[test]
fn parses_profile_command_without_manual_pid() {
    let cli = Cli::try_parse_from([
        "ardosia-loadgen",
        "profile",
        "scenarios/steady-1000.toml",
        "--output",
        "profiles/steady-1000",
    ])
    .unwrap();

    match cli.command {
        Command::Profile { scenario, output, .. } => {
            assert_eq!(scenario.to_string_lossy(), "scenarios/steady-1000.toml");
            assert_eq!(output.unwrap().to_string_lossy(), "profiles/steady-1000");
        }
        other => panic!("expected profile command, got {other:?}"),
    }
}
```

`tests/profiling.rs`:

```rust
use std::path::Path;
use ardosia_loadgen::profiling::{ProfileArtifacts, resolve_run_dir};

#[test]
fn profile_run_directory_is_isolated_under_requested_root() {
    let path = resolve_run_dir(Path::new("profiles/custom"), "1724020000000-4242");
    assert_eq!(path, Path::new("profiles/custom/1724020000000-4242"));
}

#[test]
fn artifact_layout_is_stable() {
    let dir = Path::new("profiles/steady-1000/run-1");
    let artifacts = ProfileArtifacts::in_dir(dir);
    assert_eq!(artifacts.run_json, dir.join("run.json"));
    assert_eq!(artifacts.profile_json, dir.join("profile.json"));
    assert_eq!(artifacts.perf_data, dir.join("perf.data"));
    assert_eq!(artifacts.perf_report, dir.join("perf-report.txt"));
    assert_eq!(artifacts.stacks_folded, dir.join("stacks.folded"));
    assert_eq!(artifacts.flamegraph_svg, dir.join("flamegraph.svg"));
}
```

- [ ] **Step 2: Run RED**

```bash
cargo test -p ardosia-loadgen --test cli --test profiling
```

Expected: FAIL because CLI/profiling modules do not exist.

- [ ] **Step 3: Add profile build and models**

Root `Cargo.toml`:

```toml
[profile.profiling]
inherits = "release"
debug = 1
strip = false
```

Add `/profiles/` to `.gitignore`.

Move current Clap structs into `src/cli.rs`; preserve Local/Run/Serve/ServeChild and add:

```rust
Profile {
    scenario: PathBuf,
    #[arg(long, default_value = "127.0.0.1:19132")]
    bind: SocketAddr,
    #[arg(long)]
    output: Option<PathBuf>,
},
```

Create `profiling.rs`:

```rust
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::report::RunReport;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CallGraphMode { Dwarf }

#[derive(Debug, Clone)]
pub struct ProfileConfig {
    pub scenario_path: PathBuf,
    pub output_root: PathBuf,
    pub frequency_hz: u32,
    pub call_graph: CallGraphMode,
}

impl ProfileConfig {
    pub fn new(scenario_path: PathBuf, output_root: PathBuf) -> Self {
        Self { scenario_path, output_root, frequency_hz: 99, call_graph: CallGraphMode::Dwarf }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileArtifacts {
    pub run_json: PathBuf,
    pub profile_json: PathBuf,
    pub perf_data: PathBuf,
    pub perf_report: PathBuf,
    pub stacks_folded: PathBuf,
    pub flamegraph_svg: PathBuf,
}

impl ProfileArtifacts {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            run_json: dir.join("run.json"),
            profile_json: dir.join("profile.json"),
            perf_data: dir.join("perf.data"),
            perf_report: dir.join("perf-report.txt"),
            stacks_folded: dir.join("stacks.folded"),
            flamegraph_svg: dir.join("flamegraph.svg"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProfileMetadata {
    pub scenario_name: String,
    pub scenario_path: PathBuf,
    pub git_commit: Option<String>,
    pub vendor_revision: String,
    pub build_profile: String,
    pub server_pid: u32,
    pub profiler: String,
    pub perf_version: Option<String>,
    pub frequency_hz: u32,
    pub call_graph: CallGraphMode,
    pub requested_capture_ms: u64,
    pub observed_capture_ms: u64,
    pub artifacts: ProfileArtifacts,
    pub success: bool,
    pub failure: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProfileRun {
    pub report: RunReport,
    pub metadata: ProfileMetadata,
    pub output_dir: PathBuf,
}

pub fn resolve_run_dir(root: &Path, run_id: &str) -> PathBuf { root.join(run_id) }
```

Export `pub mod cli; pub mod profiling;`. `main.rs` imports CLI types. Until Task 4 wires execution, `Command::Profile` returns `Err("profile command is not wired yet".into())` so the match remains exhaustive.

- [ ] **Step 4: GREEN and commit**

```bash
cargo fmt --all -- --check
cargo test -p ardosia-loadgen --test cli --test profiling
git add Cargo.toml .gitignore crates/ardosia-loadgen/src crates/ardosia-loadgen/tests/cli.rs crates/ardosia-loadgen/tests/profiling.rs
git commit -m "feat: add profiling command model"
```

---

### Task 2: `perf` Prerequisites, FIFO Control, and Post-Processing

**Files:**
- Create: `crates/ardosia-loadgen/src/profiling/perf.rs`
- Create: `crates/ardosia-loadgen/src/profiling/tools.rs`
- Modify: `crates/ardosia-loadgen/src/profiling.rs`
- Modify: `crates/ardosia-loadgen/tests/profiling.rs`

**Interfaces:**
- Produces `ProfileError`, `ProfileTools`, `PerfCommandSpec`, `PerfSession`, `PerfCaptureSummary`.
- `PerfSession` starts attached to only the server PID with events disabled; `enable`, `disable_and_stop`, and `abort` are ACK-synchronized/cleanup-safe.

- [ ] **Step 1: Write RED command/control/tool tests**

Add command test:

```rust
use ardosia_loadgen::profiling::{CallGraphMode, PerfCommandSpec};

#[test]
fn perf_command_targets_only_server_pid_and_starts_disabled() {
    let spec = PerfCommandSpec::new(
        4242, 99, CallGraphMode::Dwarf,
        "control.fifo".into(), "ack.fifo".into(), "perf.data".into(),
    );
    let args = spec.args();
    assert!(args.windows(2).any(|w| w == ["-p", "4242"]));
    assert!(args.windows(2).any(|w| w == ["--delay", "-1"]));
    assert!(args.windows(2).any(|w| w == ["-F", "99"]));
    assert!(args.windows(2).any(|w| w == ["--call-graph", "dwarf"]));
    assert!(args.iter().any(|arg| arg.starts_with("--control=fifo:")));
}
```

Inside `perf.rs`, unit-test control ACK with `tokio::io::duplex`, importing `AsyncReadExt`/`AsyncWriteExt`; fake peer must observe `enable\n`, reply `ack\n`, and state must become `Enabled` only after ACK.

For tool validation, write Unix fake executables with:

```rust
std::fs::write(path, format!("#!/bin/sh\necho fake-tool 1.0\nexit {exit_code}\n"))?;
```

set mode `0o755`, validate four exit-0 paths, then assert an exit-7 fake `perf` returns `ProfileError::MissingTool { tool: "perf", .. }`.

- [ ] **Step 2: Run RED**

```bash
cargo test -p ardosia-loadgen --test profiling
cargo test -p ardosia-loadgen profiling::perf::tests
```

Expected: FAIL because perf/tool types do not exist.

- [ ] **Step 3: Define exact error and tool surface**

In `profiling.rs`:

```rust
use thiserror::Error;
mod perf;
mod tools;
pub use perf::{PerfCaptureSummary, PerfCommandSpec, PerfSession};
pub use tools::ProfileTools;

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("profiling is supported only on Linux")]
    UnsupportedPlatform,
    #[error("profiling prerequisite {tool} is unavailable: {detail}")]
    MissingTool { tool: String, detail: String },
    #[error("profiling command {tool} failed: {detail}")]
    CommandFailed { tool: String, detail: String },
    #[error("perf control protocol failed: {0}")]
    Control(String),
    #[error("profiling artifact is empty: {0}")]
    EmptyArtifact(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
```

`ProfileTools`:

```rust
#[derive(Debug, Clone)]
pub struct ProfileTools {
    pub perf: PathBuf,
    pub collapse_perf: PathBuf,
    pub flamegraph: PathBuf,
    pub mkfifo: PathBuf,
}
```

`from_paths(...)` is injectable. `detect()` uses names `perf`, `inferno-collapse-perf`, `inferno-flamegraph`, `mkfifo`; `validate()` runs each with `--version`, captures output/status, and preserves successful `perf --version` text.

- [ ] **Step 4: Implement exact perf command and FIFO control**

`PerfCommandSpec` stores PID/frequency/callgraph/control FIFO/ACK FIFO/output. Constructor accepts those fields. `args()` yields:

```text
record -F 99 -g --call-graph dwarf -p <pid> --delay -1 --control=fifo:<ctl>,<ack> -o <perf.data>
```

Use `mkfifo` for both FIFOs, open each read+write on the harness side before spawning `perf`, spawn with `kill_on_drop(true)`, and pipe stderr.

Generic internal control:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PerfControlState { Disabled, Enabled, Stopped }

struct PerfControl<R, W> {
    reader: BufReader<R>,
    writer: W,
    state: PerfControlState,
}
```

`send_and_ack` writes `<command>\n`, flushes, reads one line, requires exactly `ack`. `enable` records capture start after ACK. `disable_and_stop` ACKs `disable`, records observed duration, ACKs `stop`, waits successful perf exit, unlinks FIFOs, and verifies non-empty `perf.data`.

```rust
#[derive(Debug, Clone)]
pub struct PerfCaptureSummary {
    pub server_pid: u32,
    pub perf_version: Option<String>,
    pub observed_capture_ms: u64,
}
```

`abort()` kills/reaps live perf and unlinks FIFOs. Drop may synchronously unlink leftover FIFO paths as a final guard.

- [ ] **Step 5: Implement post-processing without a shell**

`ProfileTools::post_process` executes and checks:

1. `perf report --stdio -i <perf.data>` -> `perf-report.txt`.
2. `perf script -i <perf.data>` stdout -> stdin of `inferno-collapse-perf`; collapse stdout -> `stacks.folded`.
3. open `stacks.folded` as stdin of `inferno-flamegraph`; flamegraph stdout -> `flamegraph.svg`.

Use `tokio::process::Command`/`Stdio`, not `sh -c`. Require non-empty `perf.data`, `perf-report.txt`, `stacks.folded`, `flamegraph.svg`.

- [ ] **Step 6: GREEN and commit**

```bash
cargo fmt --all -- --check
cargo test -p ardosia-loadgen --test profiling
cargo test -p ardosia-loadgen profiling::perf::tests
git add crates/ardosia-loadgen/src/profiling.rs crates/ardosia-loadgen/src/profiling crates/ardosia-loadgen/tests/profiling.rs
git commit -m "feat: add perf profiling control"
```

---

### Task 3: Make the Measurement End Explicit Before Profiling

**Files:**
- Modify: `crates/ardosia-loadgen/src/client_task.rs`
- Modify: `crates/ardosia-loadgen/src/runner.rs`
- Modify: `crates/ardosia-loadgen/src/child_protocol.rs`
- Modify: `crates/ardosia-loadgen/src/server_target.rs`
- Modify: `crates/ardosia-loadgen/tests/child_protocol.rs`
- Modify: `crates/ardosia-loadgen/tests/bidirectional.rs`

**Interfaces:**
- Global phase becomes `Phase::{Ramp, Measure { deadline }, Drain, Shutdown, Abort}`.
- Child IPC gains `EndMeasurement` / `MeasurementEnded`.
- Clients stop measured workload at deadline but stay connected until explicit shutdown.
- Server scheduled workload stops on `EndMeasurement` while connection tasks remain alive.

- [ ] **Step 1: Write RED child-protocol measurement-end test**

Extend `tests/child_protocol.rs` to start a tiny target, then issue:

```rust
ChildCommand::BeginMeasurement
ChildCommand::EndMeasurement
ChildCommand::Stop
```

and require exact events:

```rust
ChildEvent::MeasurementStarted
ChildEvent::MeasurementEnded
ChildEvent::Stopped { .. }
```

Add/extend the small bidirectional test so measured traffic completes, then the client remains connected until runner-driven final shutdown and still yields one final clean disconnect.

- [ ] **Step 2: Run RED**

```bash
cargo test -p ardosia-loadgen --test child_protocol --test bidirectional
```

Expected: FAIL because `EndMeasurement`/explicit post-measurement shutdown do not exist.

- [ ] **Step 3: Refactor client deadline behavior**

Change:

```rust
pub(crate) enum Phase {
    Ramp,
    Measure { deadline: Instant },
    Drain,
    Shutdown,
    Abort,
}
```

At the `Measure` deadline, `run_client_task` exits traffic/RTT scheduling but does **not** call `disconnect`. It enters a post-measurement loop that continues polling RakNet events and phase changes. `Phase::Drain` keeps the session alive with no benchmark workload. `Phase::Shutdown` performs the existing clean disconnect and increments `clean_disconnects`. `Phase::Abort` performs best-effort cleanup without turning a prior benchmark failure into a new correctness signal.

Add `ClientCohort::shutdown()` which sends `Phase::Shutdown`.

- [ ] **Step 4: Add explicit server measurement end**

IPC additions:

```rust
ChildCommand::EndMeasurement
ChildEvent::MeasurementEnded
```

`ServerTargetHandle` gains `end_measurement()` which sends `false` on the existing measurement watch. In `run_connection_task`, if measurement becomes false, `lanes.clear()`; receive/session handling stays alive.

`ChildTarget::end_measurement()` sends command and requires `MeasurementEnded`.

For normal `run_local`, deadline order becomes:

```text
measured sampler reaches deadline
child EndMeasurement -> ACK
transport end snapshot
cohort Shutdown
cohort finish
server Stop
```

This preserves measured workload bounds while making teardown explicitly later than the measured deadline.

- [ ] **Step 5: GREEN and full non-heavy regression**

```bash
cargo fmt --all -- --check
cargo test -p ardosia-loadgen --test child_protocol --test bidirectional --test workload --test send_error_policy
cargo test --workspace
```

Expected: PASS with existing steady correctness semantics unchanged.

- [ ] **Step 6: Commit**

```bash
git add crates/ardosia-loadgen/src/client_task.rs crates/ardosia-loadgen/src/runner.rs crates/ardosia-loadgen/src/child_protocol.rs crates/ardosia-loadgen/src/server_target.rs crates/ardosia-loadgen/tests/child_protocol.rs crates/ardosia-loadgen/tests/bidirectional.rs
git commit -m "refactor: make measurement teardown explicit"
```

---

### Task 4: Integrate Profiling Around the Explicit Steady Window

**Files:**
- Modify: `crates/ardosia-loadgen/src/runner.rs`
- Modify: `crates/ardosia-loadgen/src/main.rs`
- Modify: `crates/ardosia-loadgen/src/profiling.rs`
- Modify: `crates/ardosia-loadgen/tests/profiling.rs`

**Interfaces:**
- Produces `run_profile(bind_addr, scenario_path, scenario, output_root) -> Result<ProfileRun, RunnerError>`.
- Private `run_local_internal` accepts optional `ProfileRequest` and returns optional `PerfCaptureSummary`.

- [ ] **Step 1: Write RED metadata constructor test**

```rust
#[test]
fn profile_metadata_records_server_pid_and_diagnostic_build_profile() {
    let dir = Path::new("profiles/steady-1000/run-1");
    let metadata = ProfileMetadata::from_capture(
        "steady-1000",
        "scenarios/steady-1000.toml".into(),
        Some("abc123".into()),
        "3edfb4170e6cb5aeed992b09b50176fb7e5b6079".into(),
        "profiling".into(),
        4242,
        Some("perf version 6.x".into()),
        99,
        CallGraphMode::Dwarf,
        60_000,
        59_998,
        ProfileArtifacts::in_dir(dir),
    );
    assert_eq!(metadata.server_pid, 4242);
    assert_eq!(metadata.observed_capture_ms, 59_998);
    assert!(metadata.success);
    assert!(metadata.failure.is_none());
}
```

- [ ] **Step 2: Run RED**

```bash
cargo test -p ardosia-loadgen --test profiling profile_metadata_records_server_pid_and_diagnostic_build_profile
```

Expected: FAIL because constructor/integration do not exist.

- [ ] **Step 3: Add exact internal profile request and output rules**

```rust
struct ProfileRequest {
    config: ProfileConfig,
    tools: ProfileTools,
    artifacts: ProfileArtifacts,
}
```

Refactor private core to:

```rust
async fn run_local_internal(
    bind_addr: SocketAddr,
    scenario: &Scenario,
    profile: Option<ProfileRequest>,
) -> Result<(RunReport, Option<PerfCaptureSummary>), RunnerError>
```

`run_local` calls it with `None`.

`run_profile` validates tools before server spawn, then resolves:

```rust
let root = output_root
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("profiles").join(&scenario.name));
let epoch_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
let run_id = format!("{epoch_ms}-{}", std::process::id());
let output_dir = resolve_run_dir(&root, &run_id);
let artifacts = ProfileArtifacts::in_dir(&output_dir);
```

Add `RunnerError::Profile(#[from] ProfileError)` and map `SystemTimeError` to `RunnerError::Task(error.to_string())` rather than using `?` directly.

- [ ] **Step 4: Place perf exactly around measurement, not teardown**

After initial transport convergence and CPU sampler priming:

```text
PerfSession::attach_disabled(child.pid)
perf enable -> ACK
child BeginMeasurement -> ACK
transport start snapshot
cohort Measure
... hold_seconds ...
perf disable -> ACK
perf stop -> ACK / wait
child EndMeasurement -> ACK
transport end snapshot
cohort Shutdown
server stop/reap
post-process artifacts
```

Thus ramp and final disconnect cleanup are intentionally outside the profile. Every error branch after `PerfSession` creation calls `abort().await` before benchmark child abort/reap.

If an output directory exists and profiling fails, write best-effort `profile.json` with `success=false` and `failure=Some(error.to_string())`; do not classify it as transport failure.

`ProfileMetadata::from_capture` fills Task 1 fields, sets `profiler="perf"`, `success=true`, `failure=None`.

- [ ] **Step 5: Wire CLI and files**

```rust
Command::Profile { scenario, bind, output } => {
    let loaded = load_scenario(&scenario)?;
    let profile = run_profile(bind, &scenario, &loaded, output.as_deref()).await?;
    eprintln!("profile: {}", profile.output_dir.display());
    emit_report(&profile.report)?;
}
```

`run_profile` writes pretty `run.json` and `profile.json`; stdout remains only normal `RunReport` JSON via `emit_report`.

- [ ] **Step 6: GREEN and commit**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --manifest-path vendor/raknet-rust/Cargo.toml --lib
git add crates/ardosia-loadgen/src/runner.rs crates/ardosia-loadgen/src/main.rs crates/ardosia-loadgen/src/profiling.rs crates/ardosia-loadgen/tests/profiling.rs
git commit -m "feat: profile steady server window"
```

---

### Task 5: Manual `steady-1000` Profile Evidence Checkpoint

**Files:**
- Create after evidence: `docs/results/2026-08-18-steady-1000-profile.md`
- No vendor/optimization change in this task.

- [ ] **Step 1: Verify tools on the trusted Linux host**

```bash
perf --version
inferno-collapse-perf --version
inferno-flamegraph --version
mkfifo --version
```

All must exit 0. On perf permission failure, preserve exact diagnostic; never change kernel policy automatically.

- [ ] **Step 2: Run canonical profile**

```bash
cargo run --profile profiling -p ardosia-loadgen -- \
  profile scenarios/steady-1000.toml \
  --output profiles/steady-1000
```

Require benchmark correctness plus non-empty `run.json`, `profile.json`, `perf.data`, `perf-report.txt`, `stacks.folded`, `flamegraph.svg`.

- [ ] **Step 3: Review evidence before churn or optimization**

```bash
head -n 80 <profile-dir>/perf-report.txt
```

Inspect flamegraph and record top CPU paths/percentages exactly. Separate Ardosia wrapper, Tokio/runtime/syscalls, vendored RakNet, allocator/memory, and profiler/unwinding artifacts. A hot vendor symbol alone does not authorize a patch.

- [ ] **Step 4: Record and commit profile evidence**

`docs/results/2026-08-18-steady-1000-profile.md` records commit, tool versions, artifact directory, correctness, dominant paths, and conclusion `optimization deferred pending churn evidence` unless the profile proves a correctness bug or harness artifact.

```bash
git add docs/results/2026-08-18-steady-1000-profile.md
git commit -m "docs: record steady-1000 CPU profile"
```

---

### Task 6: Churn Scenario, Exact Scheduler, IDs, and Admission Headroom

**Files:**
- Modify: `crates/ardosia-loadgen/src/scenario.rs`
- Create: `crates/ardosia-loadgen/src/churn.rs`
- Modify: `crates/ardosia-loadgen/src/lib.rs`
- Modify: `crates/ardosia-loadgen/tests/scenario.rs`
- Create: `crates/ardosia-loadgen/tests/churn.rs`
- Create: `scenarios/churn-500.toml`

**Interfaces:**
- `Scenario::churn: Option<ChurnConfig>`.
- `churn_admission_headroom()`, `benchmark_max_connections()`.
- `ChurnSchedule`, `SlotSelector`, `ClientIdAllocator`, `ChurnError`.

- [ ] **Step 1: Write RED scenario/schedule/ID tests**

Scenario tests:

```rust
#[test]
fn parses_churn_and_derives_canonical_admission_headroom() {
    let scenario = load_checked_in("churn-500.toml");
    assert_eq!(scenario.clients, 500);
    assert_eq!(scenario.churn.as_ref().unwrap().replacements_per_second, 25.0);
    assert_eq!(scenario.churn_admission_headroom(), 125);
    assert_eq!(scenario.benchmark_max_connections(), 625);
}
```

Reject `0.0`, `-1.0`, `inf`, `nan` for `replacements_per_second`.

Churn tests:

```rust
#[test]
fn canonical_schedule_has_deadline_inclusive_1500_ticks() {
    let schedule = ChurnSchedule::new(25.0, Duration::from_secs(60)).unwrap();
    assert_eq!(schedule.planned_ticks(), 1500);
    assert_eq!(schedule.due_offset(0), Some(Duration::from_millis(40)));
    assert_eq!(schedule.due_offset(1499), Some(Duration::from_secs(60)));
    assert_eq!(schedule.due_offset(1500), None);
}

#[test]
fn ids_never_reuse() {
    let mut ids = ClientIdAllocator::after_initial_population(500).unwrap();
    assert_eq!(ids.next_id().unwrap(), 500);
    assert_eq!(ids.next_id().unwrap(), 501);
    assert_eq!(ids.next_id().unwrap(), 502);
}
```

Add deterministic selector test over `[false, true, true]`.

- [ ] **Step 2: Run RED**

```bash
cargo test -p ardosia-loadgen --test scenario --test churn
```

- [ ] **Step 3: Implement config/headroom**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChurnConfig { pub replacements_per_second: f64 }
```

Add optional serde-default field to `Scenario`; finite-positive validation.

```rust
pub fn churn_admission_headroom(&self) -> usize {
    self.churn.as_ref().map_or(0, |churn| {
        (churn.replacements_per_second * self.connect_timeout_seconds as f64)
            .ceil().min(usize::MAX as f64) as usize
    })
}

pub fn benchmark_max_connections(&self) -> usize {
    if self.churn.is_some() {
        self.clients.saturating_add(self.churn_admission_headroom()).max(1)
    } else {
        self.clients.saturating_add(64).max(1)
    }
}
```

- [ ] **Step 4: Implement exact churn helpers**

```rust
#[derive(Debug, thiserror::Error)]
pub enum ChurnError {
    #[error("invalid churn rate")]
    InvalidRate,
    #[error("client id space exhausted")]
    ClientIdExhausted,
    #[error("initial population does not fit in u64 client ids")]
    InitialPopulationTooLarge,
}
```

Schedule stores rate/hold. `planned_ticks = floor(rate * hold_seconds)`. `period = Duration::from_secs_f64(1.0/rate)`. `due_offset(i) = Duration::from_secs_f64((i+1) as f64/rate)` when `i < planned_ticks`; positive low rates yielding zero ticks remain valid.

`SlotSelector` stores `next_index` and scans at most N availability entries. `ClientIdAllocator`:

```rust
pub struct ClientIdAllocator { next: u64 }
impl ClientIdAllocator {
    pub fn after_initial_population(clients: usize) -> Result<Self, ChurnError> {
        Ok(Self { next: u64::try_from(clients).map_err(|_| ChurnError::InitialPopulationTooLarge)? })
    }
    pub fn next_id(&mut self) -> Result<u64, ChurnError> {
        let id = self.next;
        self.next = self.next.checked_add(1).ok_or(ChurnError::ClientIdExhausted)?;
        Ok(id)
    }
}
```

- [ ] **Step 5: Add canonical scenario**

`scenarios/churn-500.toml` is exactly steady-500 traffic/RTT plus:

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

- [ ] **Step 6: GREEN and commit**

```bash
cargo fmt --all -- --check
cargo test -p ardosia-loadgen --test scenario --test churn
git add crates/ardosia-loadgen/src/scenario.rs crates/ardosia-loadgen/src/churn.rs crates/ardosia-loadgen/src/lib.rs crates/ardosia-loadgen/tests/scenario.rs crates/ardosia-loadgen/tests/churn.rs scenarios/churn-500.toml
git commit -m "feat: define deterministic churn scenarios"
```

---

### Task 7: Add Per-Generation Planned Disconnect Control

**Files:**
- Modify: `crates/ardosia-loadgen/src/churn.rs`
- Modify: `crates/ardosia-loadgen/src/client_task.rs`
- Create: `crates/ardosia-loadgen/tests/client_lifecycle.rs`

**Interfaces:**
- `GenerationDirective::{Continue, PlannedDisconnect}`.
- Tagged `ConnectOutcome::{Ready { client_id }, Failed { client_id, timed_out }}`.
- `ClientTaskResult` gains `client_id`, `completed_planned_disconnects`.
- Pure disconnect classifier types below.

- [ ] **Step 1: Write RED classification tests**

```rust
use ardosia_loadgen::churn::{DisconnectIntent, DisconnectOutcome, classify_disconnect};

#[test]
fn planned_clean_disconnect_is_not_final_clean_or_unexpected() {
    let c = classify_disconnect(DisconnectIntent::PlannedChurn, DisconnectOutcome::Clean);
    assert_eq!(c.completed_planned_disconnects, 1);
    assert_eq!(c.clean_disconnects, 0);
    assert_eq!(c.unexpected_disconnects, 0);
}

#[test]
fn final_clean_disconnect_keeps_existing_semantics() {
    let c = classify_disconnect(DisconnectIntent::FinalShutdown, DisconnectOutcome::Clean);
    assert_eq!(c.completed_planned_disconnects, 0);
    assert_eq!(c.clean_disconnects, 1);
    assert_eq!(c.unexpected_disconnects, 0);
}
```

- [ ] **Step 2: Run RED**

```bash
cargo test -p ardosia-loadgen --test client_lifecycle
```

- [ ] **Step 3: Implement exact classifier**

In `churn.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisconnectIntent { PlannedChurn, FinalShutdown }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisconnectOutcome { Clean, Failed }
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DisconnectCounts {
    pub completed_planned_disconnects: usize,
    pub clean_disconnects: usize,
    pub unexpected_disconnects: usize,
}

pub fn classify_disconnect(intent: DisconnectIntent, outcome: DisconnectOutcome) -> DisconnectCounts {
    match (intent, outcome) {
        (DisconnectIntent::PlannedChurn, DisconnectOutcome::Clean) => DisconnectCounts { completed_planned_disconnects: 1, ..Default::default() },
        (DisconnectIntent::FinalShutdown, DisconnectOutcome::Clean) => DisconnectCounts { clean_disconnects: 1, ..Default::default() },
        (_, DisconnectOutcome::Failed) => DisconnectCounts { unexpected_disconnects: 1, ..Default::default() },
    }
}
```

- [ ] **Step 4: Refactor client generation control**

```rust
pub(crate) enum GenerationDirective { Continue, PlannedDisconnect }
pub(crate) enum ConnectOutcome {
    Ready { client_id: u64 },
    Failed { client_id: u64, timed_out: bool },
}
```

`run_client_task` gains `directive_rx: watch::Receiver<GenerationDirective>`. Use biased selects so a delivered planned directive wins over simultaneous remote disconnect processing. Planned disconnect calls `client.disconnect(None)` under `timeout(connect_timeout_seconds, ...)`; clean planned exit increments only `completed_planned_disconnects`. Final `Phase::Shutdown` uses `DisconnectIntent::FinalShutdown` and retains `clean_disconnects` semantics. Connect timeout and connect error are distinguished in `ConnectOutcome`.

Steady `ClientCohort` creates one `GenerationDirective::Continue` sender per initial task and never changes it.

- [ ] **Step 5: GREEN and commit**

```bash
cargo fmt --all -- --check
cargo test -p ardosia-loadgen --test client_lifecycle --test bidirectional --test send_error_policy
git add crates/ardosia-loadgen/src/churn.rs crates/ardosia-loadgen/src/client_task.rs crates/ardosia-loadgen/tests/client_lifecycle.rs
git commit -m "feat: add planned client disconnects"
```

---

### Task 8: Constant-Population Churn Cohort and Event Accounting

**Files:**
- Modify: `crates/ardosia-loadgen/src/churn.rs`
- Modify: `crates/ardosia-loadgen/src/client_task.rs`
- Modify: `crates/ardosia-loadgen/tests/churn.rs`

**Interfaces:**
- Produces `ChurnRunMetrics`, `ChurnCohort`, `ChurnEvent`.
- Cohort is sole owner of logical slot state; detached generation tasks only emit events/results.

- [ ] **Step 1: Write RED metrics/state tests**

```rust
#[test]
fn overlapping_replacements_reduce_population_then_recover() {
    let mut m = ChurnRunMetrics::for_target(500, 125, 625, 1500);
    m.observe_initial_population(500);
    m.planned_disconnect_started();
    m.planned_disconnect_started();
    assert_eq!(m.population_current(), 498);
    assert_eq!(m.population_min(), 498);
    m.replacement_attempt_started();
    m.replacement_attempt_started();
    assert_eq!(m.replacement_inflight_peak(), 2);
    m.replacement_connected(Duration::from_millis(8));
    m.replacement_connected(Duration::from_millis(11));
    assert_eq!(m.population_current(), 500);
}
```

Add timeout failure test and schedule-miss test.

- [ ] **Step 2: Run RED**

```bash
cargo test -p ardosia-loadgen --test churn
```

- [ ] **Step 3: Implement exact metrics model**

```rust
pub struct ChurnRunMetrics {
    pub(crate) target_clients: usize,
    pub(crate) admission_headroom: usize,
    pub(crate) server_max_connections: usize,
    pub(crate) planned_disconnects: u64,
    pub(crate) completed_planned_disconnects: u64,
    pub(crate) replacement_attempts: u64,
    pub(crate) replacement_handshakes: u64,
    pub(crate) replacement_failures: u64,
    pub(crate) replacement_timeouts: u64,
    pub(crate) schedule_misses: u64,
    pub(crate) population_current: usize,
    pub(crate) population_min: usize,
    pub(crate) population_max: usize,
    pub(crate) replacement_inflight: usize,
    pub(crate) replacement_inflight_peak: usize,
    pub(crate) replacement_latency: LatencyHistogram,
}
```

`for_target(target, headroom, max_connections, planned)` initializes planned count. `observe_initial_population` sets current/min/max. `planned_disconnect_started` decrements current and updates min. `completed_planned_disconnect` increments completion. `replacement_attempt_started` increments attempts/inflight/peak. `replacement_connected(duration)` increments handshakes, decrements inflight, increments active population/max, records latency. `replacement_failed(timed_out)` increments failures, optional timeout, decrements inflight. `schedule_miss()` increments misses. Query methods used by tests return current/min/end/peak.

- [ ] **Step 4: Implement slots and cohort events**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotState { ConnectingInitial, Active, PlannedDisconnect, ConnectingReplacement, Failed }

struct Slot {
    state: SlotState,
    client_id: u64,
    directive_tx: watch::Sender<GenerationDirective>,
    replacement_started: Option<Instant>,
}

pub(crate) enum ChurnEvent {
    Connect(ConnectOutcome),
    Finished(ClientTaskResult),
}
```

Generation spawn wrapper sends task result to a result channel. `ChurnCohort::next_event()` selects connect outcome/result channels; all slot mutation occurs in the cohort owner. Initial IDs are `0..clients-1`; replacements use allocator.

At planned tick: deterministic active slot -> state PlannedDisconnect -> population decremented -> directive sent. On planned finished result: completion metric -> allocate unique ID -> replacement attempt/inflight -> spawn same slot. On Ready: latency from slot `replacement_started` -> Active. On failed replacement: Failed and failure metrics. No eligible slot or failed directive delivery -> `schedule_miss`.

Expose:

```rust
pub(crate) fn ready_for_post_drain_verification(&self) -> bool {
    self.metrics.completed_planned_disconnects == self.metrics.planned_disconnects
        && self.metrics.replacement_attempts == self.metrics.planned_disconnects
        && self.metrics.replacement_handshakes == self.metrics.replacement_attempts
        && self.metrics.replacement_inflight == 0
        && self.metrics.population_current == self.metrics.target_clients
}
```

No wall-clock waits inside this predicate/cohort model.

- [ ] **Step 5: GREEN and commit**

```bash
cargo fmt --all -- --check
cargo test -p ardosia-loadgen --test churn --test client_lifecycle
git add crates/ardosia-loadgen/src/churn.rs crates/ardosia-loadgen/src/client_task.rs crates/ardosia-loadgen/tests/churn.rs
git commit -m "feat: orchestrate constant population churn"
```

---

### Task 9: Execute Churn, Bounded Drain, and Post-Drain Transport Reconciliation

**Files:**
- Modify: `crates/ardosia-loadgen/src/churn.rs`
- Modify: `crates/ardosia-loadgen/src/runner.rs`
- Modify: `crates/ardosia-loadgen/src/server_target.rs`
- Modify: `crates/ardosia-loadgen/tests/churn.rs`

**Interfaces:**
- `server_target` uses `Scenario::benchmark_max_connections()`.
- Produces `post_drain_transport_is_healthy(sample, target, baseline_timeouts)`.
- Churn path keeps measured window/delta separate from drain snapshot.

- [ ] **Step 1: Write RED headroom/reconciliation tests**

```rust
#[test]
fn post_drain_population_accepts_lifecycle_totals_but_requires_target_current() {
    let post = TransportMetricsReport {
        sessions_current: 500,
        sessions_started_total: 2000,
        sessions_closed_total: 1500,
        timed_out_sessions: 0,
        ..TransportMetricsReport::default()
    };
    assert!(post_drain_transport_is_healthy(post, 500, 0));
}
```

Also assert same helper rejects current 499 and timeout growth 1. Scenario test already proves 625 churn vs 564 steady-500.

- [ ] **Step 2: Run RED**

```bash
cargo test -p ardosia-loadgen --test churn
```

- [ ] **Step 3: Implement exact reconciliation helper and server capacity**

In `churn.rs`:

```rust
pub fn post_drain_transport_is_healthy(
    sample: TransportMetricsReport,
    target_clients: usize,
    baseline_timeouts: u64,
) -> bool {
    sample.sessions_current == u64::try_from(target_clients).unwrap_or(u64::MAX)
        && sample.timed_out_sessions == baseline_timeouts
}
```

Benchmark target binding uses:

```rust
max_connections: scenario.benchmark_max_connections(),
```

Passive `serve_until --max-connections` is unchanged.

- [ ] **Step 4: Add churn initial ramp collection**

`collect_churn_initial_handshakes(...)` consumes `ChurnCohort::next_event()` and existing ~1 Hz process/host samplers until all initial `scenario.clients` outcomes are known. Initial outcomes alone populate `RunCounts.successful_handshakes/failed_handshakes`; replacement outcomes never alter those initial counters.

- [ ] **Step 5: Add measured churn loop**

After initial transport convergence:

```text
child BeginMeasurement
transport_start snapshot
cohort Measure(deadline)
```

Drive concurrently: next churn nominal tick, cohort events, ~1 Hz server/loadgen/host sampling, periodic transport snapshots. Execute every nominal `due_offset(i) <= hold_seconds`, including tick 1499 at 60,000 ms. A tick serviced at least one full churn period late increments `schedule_misses` but is still serviced if its nominal due time belongs to the window. Any nominal ticks unprocessed when the window is closed are each marked missed and are not invented as post-window churn.

Measured duration stays tied to original start/deadline.

- [ ] **Step 6: End measurement and bounded drain**

At deadline:

```text
child EndMeasurement -> ACK
transport_end snapshot
cohort Drain
```

Continue processing cohort events until readiness or global deadline `measured_deadline + 2 * connect_timeout_seconds` (one bounded final planned disconnect + one bounded replacement connect). On loadgen recovery, poll child snapshots for up to `connect_timeout_seconds.max(2)` seconds until reconciliation helper passes. Retain that post-drain snapshot.

Then:

```text
cohort Shutdown
wait final active generations
child Stop/reap
```

Require final shutdown to yield exactly `scenario.clients` clean final disconnects. Measured `sessions_started/sessions_closed` are explicitly not equated to churn totals.

- [ ] **Step 7: GREEN and commit**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
git add crates/ardosia-loadgen/src/churn.rs crates/ardosia-loadgen/src/runner.rs crates/ardosia-loadgen/src/server_target.rs crates/ardosia-loadgen/tests/churn.rs
git commit -m "feat: run measured churn with bounded drain"
```

---

### Task 10: Churn Report, Gates, and Terminal Summary

**Files:**
- Modify: `crates/ardosia-loadgen/src/report.rs`
- Modify: `crates/ardosia-loadgen/src/churn.rs`
- Modify: `crates/ardosia-loadgen/src/runner.rs`
- Modify: `crates/ardosia-loadgen/src/main.rs`
- Modify: `crates/ardosia-loadgen/tests/report.rs`

**Interfaces:**
- `ResultsReport::churn: Option<ChurnReport>`.
- Existing report fields keep names/types.

- [ ] **Step 1: Write RED report tests using existing test helpers**

First update every existing `RunReportInput` in `tests/report.rs` with `churn: None` as the new field is introduced.

Add helper:

```rust
fn churn_scenario() -> Scenario {
    Scenario::from_str(include_str!("../../../scenarios/churn-500.toml")).unwrap()
}

fn clean_churn_counts() -> RunCounts {
    RunCounts {
        successful_handshakes: 500,
        failed_handshakes: 0,
        unexpected_disconnects: 0,
        protocol_errors: 0,
        send_errors: 0,
        clean_disconnects: 500,
    }
}
```

Add passing churn input with complete nonzero workload/RTT, measured transport delta with no timeout/drop failures, and churn block values 1500/1500/1500/1500, zero failures/misses, population end 500, post-drain current 500. Assert PASS.

Add steady session-churn regression by constructing the same `RunReportInput` shape currently used in `steady_report_fails_on_queue_or_backpressure_drop`, set `churn: None`, then set `input.transport.delta.sessions_started = 1`; assert a failure reason containing `session churn`.

Add churn mutations for replacement failure, schedule miss, population end 499, post-drain current 499, timeout growth.

- [ ] **Step 2: Run RED**

```bash
cargo test -p ardosia-loadgen --test report
```

- [ ] **Step 3: Add exact report type**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChurnReport {
    pub admission_headroom: usize,
    pub server_max_connections: usize,
    pub planned_disconnects: u64,
    pub completed_planned_disconnects: u64,
    pub replacement_attempts: u64,
    pub replacement_handshakes: u64,
    pub replacement_failures: u64,
    pub replacement_timeouts: u64,
    pub schedule_misses: u64,
    pub population_min: usize,
    pub population_max: usize,
    pub population_end: usize,
    pub replacement_inflight_peak: usize,
    pub replacement_latency: LatencySummary,
    pub post_drain_transport: TransportMetricsReport,
}
```

Add `churn: Option<ChurnReport>` to `ResultsReport` and `RunReportInput`. Steady callers pass `None`; churn metrics convert to Some.

- [ ] **Step 4: Implement gates exactly**

Non-churn measured window requires zero `sessions_started`, `sessions_closed`, `timed_out_sessions`.

Churn requires:

```text
planned == floor(rate * hold_seconds)
completed_planned == planned
replacement_attempts == planned
replacement_handshakes == replacement_attempts
replacement_failures == 0
replacement_timeouts == 0
schedule_misses == 0
population_end == clients
post_drain.sessions_current == clients
post_drain.timed_out_sessions == transport.start.timed_out_sessions
transport.delta.timed_out_sessions == 0
```

Both modes continue zero unexpected/protocol/genuine-send/queue-backpressure failures, configured traffic/RTT completion, and final clean disconnect count equal to `clients`.

CPU/RSS, RTT values, replacement latency, retransmits/NACKs, population minimum, and queue peaks without drops remain record-only. Failure reasons name category (`churn replacement`, `churn schedule`, `churn drain`, `transport timeout`).

- [ ] **Step 5: Terminal summary**

When churn exists, add:

```text
churn: planned=1500 replaced=1500 failed=0 pop_min=<n> pop_end=500 repl_p95=<x>ms
```

Use existing `fmt_ms`.

- [ ] **Step 6: GREEN and commit**

```bash
cargo fmt --all -- --check
cargo test -p ardosia-loadgen --test report --test churn
cargo test --workspace
git add crates/ardosia-loadgen/src/report.rs crates/ardosia-loadgen/src/churn.rs crates/ardosia-loadgen/src/runner.rs crates/ardosia-loadgen/src/main.rs crates/ardosia-loadgen/tests/report.rs
git commit -m "feat: report churn lifecycle health"
```

---

### Task 11: Manual Workflow, Docs, and Full Non-Heavy Verification

**Files:**
- Modify: `.github/workflows/baseline.yml`
- Modify: `docs/benchmarks.md`
- Modify: `crates/ardosia-loadgen/tests/scenario.rs`

- [ ] **Step 1: Add only `churn-500` to manual choices**

```yaml
          - churn-500
```

No profile job/step and no automatic trigger.

- [ ] **Step 2: Document exact local commands**

Profiling:

```bash
cargo run --profile profiling -p ardosia-loadgen -- \
  profile scenarios/steady-1000.toml \
  --output profiles/steady-1000
```

Document tools, automatic PID, default `profiles/<scenario>/<run-id>/`, steady-only capture, no automatic kernel policy changes.

Churn:

```bash
cargo run --release -p ardosia-loadgen -- \
  local scenarios/churn-500.toml > churn-500.json
```

Document 500 target, 25/s, 1500 planned, 625 temporary admission cap, measured vs drain semantics, correctness vs record-only metrics.

- [ ] **Step 3: Verify workflow policy deterministically**

```bash
python - <<'PY'
from pathlib import Path
baseline = Path('.github/workflows/baseline.yml').read_text()
assert '          - churn-500\n' in baseline
assert '\npush:' not in baseline
assert '\npull_request:' not in baseline
assert '\nschedule:' not in baseline
assert 'profiling' not in baseline.lower()
ci = Path('.github/workflows/ci.yml').read_text()
assert 'workflow_dispatch:' in ci
assert '\npush:' not in ci
assert '\npull_request:' not in ci
assert '\nschedule:' not in ci
PY
```

- [ ] **Step 4: Full non-heavy gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --manifest-path vendor/raknet-rust/Cargo.toml --lib
```

If a temporary Actions trigger is required for focused RED/GREEN evidence, restore `ci.yml` to `workflow_dispatch` immediately. Never run profiling/canonical churn automatically.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/baseline.yml docs/benchmarks.md crates/ardosia-loadgen/tests/scenario.rs
git commit -m "docs: add profiling and churn workflows"
```

---

### Task 12: Manual `churn-500` Evidence and Phase Completion

**Files:**
- Create after run: `docs/results/2026-08-18-churn-500.md`
- No vendor/optimization change unless separately diagnosed and approved.

- [ ] **Step 1: Run canonical churn**

```bash
cargo run --release -p ardosia-loadgen -- \
  local scenarios/churn-500.toml > churn-500.json
```

- [ ] **Step 2: Require strict lifecycle correctness first**

```text
initial sessions: 500/500
planned_disconnects: 1500
completed_planned_disconnects: 1500
replacement_attempts: 1500
replacement_handshakes: 1500
replacement_failures: 0
replacement_timeouts: 0
schedule_misses: 0
population_end: 500
post_drain_transport.sessions_current: 500
unexpected_disconnects: 0
protocol_errors: 0
send_errors: 0
transport timeouts: 0
queue/backpressure drops/disconnects: 0
final clean_disconnects: 500
result: PASS
```

Any failure -> systematic debugging on that concrete signal; no reflexive scaling/vendor patch.

- [ ] **Step 3: Characterize record-only behavior vs trusted `steady-500`**

Compare server/loadgen/host CPU, RSS, RTT, replacement latency p50/p95/p99/max, population minimum, replacement in-flight peak, traffic rates, retransmits/NACKs, queue peaks, measured session start/close deltas, post-drain state. Do not invent thresholds from one run.

- [ ] **Step 4: Record result**

`docs/results/2026-08-18-churn-500.md` contains environment/commit, scenario, strict result, lifecycle counts, replacement latency, population, transport, resources, and steady-500 comparison.

```bash
git add docs/results/2026-08-18-churn-500.md
git commit -m "docs: record churn-500 baseline"
```

- [ ] **Step 5: Verification before completion claim**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --manifest-path vendor/raknet-rust/Cargo.toml --lib
git status --short
```

Expected: all quality gates PASS; generated profile/results artifacts are intentionally ignored/untracked; no unexplained vendor change; heavy workflows manual-only.
