# Profiling and Churn Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add synchronized server-only CPU profiling for the trusted `steady-1000` workload, review that evidence, then add deterministic constant-population `churn-500` with 500 active clients and 25 planned replacements/sec.

**Architecture:** Profiling is an execution mode layered around the existing child-process local benchmark: the parent owns the server PID, attaches `perf` with events disabled, enables it only for the measured window, then post-processes raw samples into text and flamegraph artifacts. Churn remains workload semantics: an optional scenario block drives a deterministic slot/generation coordinator while client connection generations stay isolated, server admission gets bounded lifecycle headroom, and measured-window telemetry is reconciled with a post-drain population snapshot.

**Tech Stack:** Rust 1.88+ / Edition 2024, Tokio, Clap, Serde/TOML/JSON, existing vendored `raknet-rust`, Linux `perf`, `mkfifo`, Inferno (`inferno-collapse-perf`, `inferno-flamegraph`).

**Spec:** `docs/superpowers/specs/2026-08-18-profiling-churn-design.md`

## Global Constraints

- Keep the pinned RakNet vendor revision `3edfb4170e6cb5aeed992b09b50176fb7e5b6079` unchanged unless concrete correctness or profile-backed evidence justifies a separate vendor change.
- Keep the public `ardosia-network` API unchanged; profiling/churn belong to `ardosia-loadgen` and benchmark orchestration.
- Official capacity runs remain `--release`; diagnostic profiling uses a dedicated optimized `profiling` Cargo profile with symbols.
- Profiling is Linux-first and must never change host `perf_event_paranoid` or other kernel security settings automatically.
- Profiling targets only the benchmark server child PID and intentionally excludes ramp/handshake samples.
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

- `crates/ardosia-loadgen/src/cli.rs` — Clap command model shared by the binary and parsing tests.
- `crates/ardosia-loadgen/src/profiling.rs` — profiling request/result/metadata types and artifact-directory resolution.
- `crates/ardosia-loadgen/src/profiling/perf.rs` — `perf record` process, FIFO control/ACK synchronization, capture lifecycle.
- `crates/ardosia-loadgen/src/profiling/tools.rs` — external-tool validation/version probing and post-processing pipeline.
- `crates/ardosia-loadgen/src/churn.rs` — deterministic churn schedule, slot selection, unique-ID allocation, population/churn accounting, and dynamic generation cohort.
- `crates/ardosia-loadgen/tests/cli.rs` — public CLI parsing regression.
- `crates/ardosia-loadgen/tests/profiling.rs` — profile path/metadata/tool-command behavior that does not require privileged `perf`.
- `crates/ardosia-loadgen/tests/churn.rs` — deterministic churn schedule/state/accounting tests.
- `crates/ardosia-loadgen/tests/client_lifecycle.rs` — planned-disconnect/final-shutdown classification regressions.
- `scenarios/churn-500.toml` — canonical constant-population churn workload.

Existing units changed intentionally:

- `Cargo.toml` — dedicated optimized `profiling` Cargo profile.
- `.gitignore` — ignore generated `/profiles/` output.
- `crates/ardosia-loadgen/src/main.rs` — command dispatch and compact churn/profile output only.
- `crates/ardosia-loadgen/src/lib.rs` — expose testable CLI/profiling/churn modules.
- `crates/ardosia-loadgen/src/scenario.rs` — optional `ChurnConfig`, validation, derived admission capacity.
- `crates/ardosia-loadgen/src/client_task.rs` — per-generation planned-disconnect control plus drain/final-shutdown behavior.
- `crates/ardosia-loadgen/src/child_protocol.rs` — explicit end-of-measurement command/ack.
- `crates/ardosia-loadgen/src/server_target.rs` — derived churn admission headroom and stop server-generated workload at measurement end.
- `crates/ardosia-loadgen/src/runner.rs` — profiling hooks, churn measured loop, bounded drain, post-drain telemetry reconciliation.
- `crates/ardosia-loadgen/src/report.rs` — optional churn result block and churn/steady correctness gates.
- `crates/ardosia-loadgen/tests/scenario.rs` — churn parsing/headroom/canonical shape.
- `crates/ardosia-loadgen/tests/report.rs` — churn gate and steady no-session-churn regressions.
- `.github/workflows/baseline.yml` — manual `churn-500` choice only; never add profiling.
- `docs/benchmarks.md` — local profiling/churn instructions and interpretation.

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
- Produces: `profiling::ProfileConfig`, `ProfileArtifacts`, `ProfileMetadata`, `ProfileRun`, `CallGraphMode`.
- Produces: `profiling::resolve_run_dir(root, scenario_name, run_id) -> PathBuf`.
- Later tasks consume the metadata/artifact types without changing their serialized field names.

- [ ] **Step 1: Write failing CLI and metadata tests**

Add `crates/ardosia-loadgen/tests/cli.rs`:

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
            assert_eq!(
                output.unwrap().to_string_lossy(),
                "profiles/steady-1000"
            );
        }
        other => panic!("expected profile command, got {other:?}"),
    }
}
```

Add the first test to `crates/ardosia-loadgen/tests/profiling.rs`:

```rust
use std::path::Path;

use ardosia_loadgen::profiling::resolve_run_dir;

#[test]
fn profile_run_directory_is_isolated_under_requested_root() {
    let path = resolve_run_dir(Path::new("profiles/custom"), "steady-1000", "1724020000000");
    assert_eq!(
        path,
        Path::new("profiles/custom").join("steady-1000-1724020000000")
    );
}
```

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```bash
cargo test -p ardosia-loadgen --test cli --test profiling
```

Expected: FAIL because `ardosia_loadgen::cli` and `ardosia_loadgen::profiling` do not exist.

- [ ] **Step 3: Add the profiling Cargo profile and public command/model types**

Append to root `Cargo.toml`:

```toml
[profile.profiling]
inherits = "release"
debug = 1
strip = false
```

Add `/profiles/` to `.gitignore`.

Move the existing Clap structs from `main.rs` into `src/cli.rs` and add:

```rust
#[derive(Debug, Subcommand)]
pub enum Command {
    Local {
        scenario: PathBuf,
        #[arg(long, default_value = "127.0.0.1:19132")]
        bind: SocketAddr,
    },
    Profile {
        scenario: PathBuf,
        #[arg(long, default_value = "127.0.0.1:19132")]
        bind: SocketAddr,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Run {
        scenario: PathBuf,
        #[arg(long)]
        target: SocketAddr,
    },
    Serve {
        #[arg(long, default_value = "0.0.0.0:19132")]
        bind: SocketAddr,
        #[arg(long, default_value_t = 8)]
        protocol: u8,
        #[arg(long, default_value_t = 1024)]
        max_connections: usize,
    },
    #[command(hide = true)]
    ServeChild,
}
```

Create `src/profiling.rs` with stable serialized names:

```rust
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::report::RunReport;

mod perf;
mod tools;

pub use perf::{PerfCaptureSummary, PerfSession};
pub use tools::ProfileTools;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CallGraphMode {
    Dwarf,
}

#[derive(Debug, Clone)]
pub struct ProfileConfig {
    pub scenario_path: PathBuf,
    pub output_root: PathBuf,
    pub frequency_hz: u32,
    pub call_graph: CallGraphMode,
}

impl ProfileConfig {
    pub fn new(scenario_path: PathBuf, output_root: PathBuf) -> Self {
        Self {
            scenario_path,
            output_root,
            frequency_hz: 99,
            call_graph: CallGraphMode::Dwarf,
        }
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

pub fn resolve_run_dir(root: &Path, scenario_name: &str, run_id: &str) -> PathBuf {
    root.join(format!("{scenario_name}-{run_id}"))
}
```

Export `pub mod cli;` and `pub mod profiling;` from `lib.rs`, and make `main.rs` import `ardosia_loadgen::cli::{Cli, Command}`.

Do not dispatch `Command::Profile` yet; return a clear temporary error in the binary only until Task 3 wires it. The library/test surface must compile now.

- [ ] **Step 4: Run focused tests and workspace formatting**

Run:

```bash
cargo fmt --all -- --check
cargo test -p ardosia-loadgen --test cli --test profiling
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml .gitignore crates/ardosia-loadgen/src crates/ardosia-loadgen/tests/cli.rs crates/ardosia-loadgen/tests/profiling.rs
git commit -m "feat: add profiling command model"
```

---

### Task 2: `perf` Prerequisites, FIFO Control, and Artifact Post-Processing

**Files:**
- Create: `crates/ardosia-loadgen/src/profiling/perf.rs`
- Create: `crates/ardosia-loadgen/src/profiling/tools.rs`
- Modify: `crates/ardosia-loadgen/src/profiling.rs`
- Modify: `crates/ardosia-loadgen/tests/profiling.rs`

**Interfaces:**
- Consumes: `ProfileConfig`, `ProfileArtifacts`, `CallGraphMode` from Task 1.
- Produces: `ProfileTools::detect() -> Result<ProfileTools, ProfileError>` and injectable `ProfileTools::from_paths(...)`.
- Produces: `PerfSession::attach_disabled(server_pid, config, tools, artifacts)`, `enable()`, `disable_and_stop()`, `abort()`.
- Produces: `ProfileTools::post_process(artifacts) -> Result<(), ProfileError>`.
- Produces: `ProfileError`, kept distinct from benchmark transport/correctness failures.

- [ ] **Step 1: Write RED tests for exact server PID, disabled start, ACK control, and post-process failure classification**

Extend `tests/profiling.rs` with pure command assertions:

```rust
use ardosia_loadgen::profiling::{CallGraphMode, PerfCommandSpec};

#[test]
fn perf_command_targets_only_server_pid_and_starts_disabled() {
    let spec = PerfCommandSpec::new(
        4242,
        99,
        CallGraphMode::Dwarf,
        "control.fifo".into(),
        "ack.fifo".into(),
        "perf.data".into(),
    );
    let args = spec.args();
    assert!(args.windows(2).any(|w| w == ["-p", "4242"]));
    assert!(args.windows(2).any(|w| w == ["--delay", "-1"]));
    assert!(args.windows(2).any(|w| w == ["-F", "99"]));
    assert!(args.windows(2).any(|w| w == ["--call-graph", "dwarf"]));
    assert!(args.iter().any(|arg| arg.starts_with("--control=fifo:")));
}
```

Inside `profiling/perf.rs`, add a unit test using `tokio::io::duplex` for the control protocol:

```rust
#[tokio::test]
async fn control_waits_for_ack_before_advancing_state() {
    let (client, mut peer) = tokio::io::duplex(128);
    let (read_half, write_half) = tokio::io::split(client);
    let mut control = PerfControl::new(read_half, write_half);

    let peer_task = tokio::spawn(async move {
        let mut buf = [0u8; 64];
        let n = peer.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"enable\n");
        peer.write_all(b"ack\n").await.unwrap();
    });

    control.enable().await.unwrap();
    peer_task.await.unwrap();
    assert_eq!(control.state(), PerfControlState::Enabled);
}
```

Add a tool-validation test using fake executable paths (Unix test helper writes `#!/bin/sh\necho fake-tool 1.0\n` and mode `0o755`) and a failing fake tool returning exit 7; assert the error names the failed prerequisite rather than reporting a RakNet error.

- [ ] **Step 2: Run focused tests and verify RED**

Run:

```bash
cargo test -p ardosia-loadgen --test profiling
cargo test -p ardosia-loadgen profiling::perf::tests::control_waits_for_ack_before_advancing_state
```

Expected: FAIL because `PerfCommandSpec`, `PerfControl`, and tool validation do not exist.

- [ ] **Step 3: Implement tool detection and exact `perf record` command construction**

In `profiling/tools.rs`, define:

```rust
#[derive(Debug, Clone)]
pub struct ProfileTools {
    pub perf: PathBuf,
    pub collapse_perf: PathBuf,
    pub flamegraph: PathBuf,
    pub mkfifo: PathBuf,
}

impl ProfileTools {
    pub fn from_paths(perf: PathBuf, collapse_perf: PathBuf, flamegraph: PathBuf, mkfifo: PathBuf) -> Self {
        Self { perf, collapse_perf, flamegraph, mkfifo }
    }

    pub async fn detect() -> Result<Self, ProfileError> {
        let tools = Self::from_paths(
            "perf".into(),
            "inferno-collapse-perf".into(),
            "inferno-flamegraph".into(),
            "mkfifo".into(),
        );
        tools.validate().await?;
        Ok(tools)
    }
}
```

`validate()` runs each executable with `--version`, redirects stdout/stderr to pipes, and returns `ProfileError::MissingTool { tool, detail }` for spawn failure/non-zero exit. Capture `perf --version` for metadata.

In `profiling/perf.rs`, define the pure command model:

```rust
#[derive(Debug, Clone)]
pub struct PerfCommandSpec {
    pub server_pid: u32,
    pub frequency_hz: u32,
    pub call_graph: CallGraphMode,
    pub control_fifo: PathBuf,
    pub ack_fifo: PathBuf,
    pub perf_data: PathBuf,
}

impl PerfCommandSpec {
    pub fn args(&self) -> Vec<String> {
        vec![
            "record".into(),
            "-F".into(),
            self.frequency_hz.to_string(),
            "-g".into(),
            "--call-graph".into(),
            "dwarf".into(),
            "-p".into(),
            self.server_pid.to_string(),
            "--delay".into(),
            "-1".into(),
            format!(
                "--control=fifo:{},{}",
                self.control_fifo.display(),
                self.ack_fifo.display()
            ),
            "-o".into(),
            self.perf_data.display().to_string(),
        ]
    }
}
```

Use `mkfifo` to create control/ACK FIFOs. Open both FIFOs read+write on the harness side before spawning `perf` so FIFO open does not deadlock. Spawn `perf` with `kill_on_drop(true)` and stderr piped for diagnostics.

- [ ] **Step 4: Implement ACK-synchronized control state and cleanup**

Implement a generic internal control channel:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PerfControlState {
    Disabled,
    Enabled,
    Stopped,
}

struct PerfControl<R, W> {
    reader: BufReader<R>,
    writer: W,
    state: PerfControlState,
}
```

`send_and_ack("enable")`, `send_and_ack("disable")`, and `send_and_ack("stop")` must write `<command>\n`, flush, read one ACK line, and require exactly `ack`. `PerfSession::enable()` records monotonic capture start after the enable ACK. `disable_and_stop()` receives disable ACK, records capture duration, receives stop ACK, waits for `perf` to exit successfully, closes/unlinks FIFOs, and verifies `perf.data` is non-empty.

`abort()` kills/reaps `perf` if still alive and removes both FIFOs. `Drop` may remove leftover FIFO paths synchronously as a final filesystem guard, but explicit async abort remains the primary cleanup path.

- [ ] **Step 5: Implement post-processing without shell pipelines**

In `profiling/tools.rs`, execute commands directly and check every exit status:

1. `perf report --stdio -i <perf.data>` -> `perf-report.txt`.
2. `perf script -i <perf.data>` stdout piped into `inferno-collapse-perf`; collapsed stdout -> `stacks.folded`.
3. `inferno-flamegraph <stacks.folded>` stdout -> `flamegraph.svg`.

Use `tokio::process::Command`, `Stdio::piped`, and file handles; do not invoke `sh -c`. Reject empty `perf.data`, `perf-report.txt`, `stacks.folded`, or `flamegraph.svg` with `ProfileError::EmptyArtifact(PathBuf)`.

- [ ] **Step 6: Run focused tests and formatting**

Run:

```bash
cargo fmt --all -- --check
cargo test -p ardosia-loadgen --test profiling
cargo test -p ardosia-loadgen profiling::perf::tests
```

Expected: PASS without invoking real privileged `perf`.

- [ ] **Step 7: Commit**

```bash
git add crates/ardosia-loadgen/src/profiling.rs crates/ardosia-loadgen/src/profiling crates/ardosia-loadgen/tests/profiling.rs
git commit -m "feat: add perf profiling control"
```

---

### Task 3: Integrate Profiling with the Existing Local Benchmark Window

**Files:**
- Modify: `crates/ardosia-loadgen/src/runner.rs`
- Modify: `crates/ardosia-loadgen/src/main.rs`
- Modify: `crates/ardosia-loadgen/src/profiling.rs`
- Modify: `crates/ardosia-loadgen/tests/profiling.rs`

**Interfaces:**
- Consumes: `PerfSession`, `ProfileTools`, `ProfileConfig`.
- Produces: `runner::run_profile(bind_addr, scenario_path, scenario, output_root) -> Result<ProfileRun, RunnerError>`.
- `run_local` retains its current user-facing behavior.
- Profiling attach happens only after initial transport convergence and before `BeginMeasurement`; enable ACK happens before the measured workload starts.

- [ ] **Step 1: Write RED tests for profile metadata construction and requested/observed window separation**

Add a public pure constructor so metadata can be tested without real profiling:

```rust
#[test]
fn profile_metadata_records_server_pid_and_diagnostic_build_profile() {
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
        artifacts(),
    );
    assert_eq!(metadata.server_pid, 4242);
    assert_eq!(metadata.requested_capture_ms, 60_000);
    assert_eq!(metadata.observed_capture_ms, 59_998);
    assert_eq!(metadata.build_profile, "profiling");
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cargo test -p ardosia-loadgen --test profiling profile_metadata_records_server_pid_and_diagnostic_build_profile
```

Expected: FAIL because `ProfileMetadata::from_capture` and runner profile integration do not exist.

- [ ] **Step 3: Refactor local execution into one private core with optional profiling**

Keep `run_local` as a thin wrapper:

```rust
pub async fn run_local(bind_addr: SocketAddr, scenario: &Scenario) -> Result<RunReport, RunnerError> {
    let (report, _) = run_local_internal(bind_addr, scenario, None).await?;
    Ok(report)
}
```

Add:

```rust
pub async fn run_profile(
    bind_addr: SocketAddr,
    scenario_path: &Path,
    scenario: &Scenario,
    output_root: Option<&Path>,
) -> Result<ProfileRun, RunnerError>
```

`run_profile` must:

1. reject non-Linux hosts with `RunnerError::Profile(ProfileError::UnsupportedPlatform)`;
2. call `ProfileTools::detect().await` **before** spawning the server child;
3. resolve/create an isolated run directory;
4. call `run_local_internal(..., Some(ProfileRequest { ... }))`;
5. post-process captured samples only after the server child is stopped/reaped;
6. write pretty JSON `run.json` and `profile.json` into the run directory;
7. return `ProfileRun` for normal stdout/stderr handling.

Inside `run_local_internal`, after `wait_for_transport_ready` and CPU sampler priming, create `PerfSession::attach_disabled(child.pid, ...)`. Then use this order:

```text
transport converged
perf attached disabled
perf enable -> ACK
child BeginMeasurement -> ACK
transport start snapshot
client measured phase starts
... measured window ...
perf disable -> ACK
perf stop -> ACK / process exit
normal client/server shutdown
post-processing outside measured server lifetime
```

Every error branch after a profiler exists must call `profiler.abort().await` before aborting/reaping the benchmark child.

- [ ] **Step 4: Wire `Command::Profile` in `main.rs`**

Dispatch:

```rust
Command::Profile {
    scenario,
    bind,
    output,
} => {
    let loaded = load_scenario(&scenario)?;
    let profile = run_profile(bind, &scenario, &loaded, output.as_deref()).await?;
    eprintln!("profile: {}", profile.output_dir.display());
    emit_report(&profile.report)?;
}
```

`emit_report` remains the only normal stdout JSON writer. Profiler diagnostics/artifact path go to stderr.

- [ ] **Step 5: Verify non-heavy regression gates**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --manifest-path vendor/raknet-rust/Cargo.toml --lib
```

Expected: PASS. Do not run `profile` in ordinary tests.

- [ ] **Step 6: Commit**

```bash
git add crates/ardosia-loadgen/src/runner.rs crates/ardosia-loadgen/src/main.rs crates/ardosia-loadgen/src/profiling.rs crates/ardosia-loadgen/tests/profiling.rs
git commit -m "feat: profile steady server window"
```

---

### Task 4: Manual `steady-1000` Profile Evidence Checkpoint

**Files:**
- Create after evidence exists: `docs/results/2026-08-18-steady-1000-profile.md`
- Do not modify vendor or optimize code in this task.

**Interfaces:**
- Consumes the new `profile` command from Task 3.
- Produces a reviewed list of dominant server CPU paths and raw artifacts supplied by the benchmark host.
- Churn implementation starts only after this checkpoint is reviewed.

- [ ] **Step 1: Verify host tools**

Run on the same Linux host used for trusted local scaling results:

```bash
perf --version
inferno-collapse-perf --version
inferno-flamegraph --version
mkfifo --version
```

Expected: all commands exit 0. If `perf` is blocked by host policy, report the exact diagnostic; do not change kernel policy automatically.

- [ ] **Step 2: Build and run the canonical profile**

Run:

```bash
cargo run --profile profiling -p ardosia-loadgen -- \
  profile scenarios/steady-1000.toml \
  --output profiles/steady-1000
```

Expected benchmark correctness: 1000/1000 initial sessions, zero unexpected disconnect/protocol/send/backpressure failures. Expected profiling artifacts inside the emitted run directory: non-empty `run.json`, `profile.json`, `perf.data`, `perf-report.txt`, `stacks.folded`, and `flamegraph.svg`.

- [ ] **Step 3: Inspect the profile before proposing any optimization**

Use:

```bash
head -n 80 <profile-dir>/perf-report.txt
```

and inspect `<profile-dir>/flamegraph.svg`. Record the top CPU paths with percentages/symbol names exactly as the profile reports them. Separate:

- Ardosia wrapper/orchestrator code;
- Tokio/runtime/syscall work;
- vendored RakNet work;
- allocator/memory work;
- profiling/unwinding artifacts if visible.

Do **not** interpret a single hot symbol as permission to patch vendor code.

- [ ] **Step 4: Write and commit the evidence note**

`docs/results/2026-08-18-steady-1000-profile.md` must contain the exact git commit, profile tool versions, artifact directory, benchmark correctness summary, top observed CPU paths, and the explicit conclusion `optimization deferred pending churn evidence` unless a correctness bug/pathological harness artifact is discovered.

Commit:

```bash
git add docs/results/2026-08-18-steady-1000-profile.md
git commit -m "docs: record steady-1000 CPU profile"
```

---

### Task 5: Churn Scenario Model, Exact Schedule, Slot Selection, and Admission Headroom

**Files:**
- Modify: `crates/ardosia-loadgen/src/scenario.rs`
- Create: `crates/ardosia-loadgen/src/churn.rs`
- Modify: `crates/ardosia-loadgen/src/lib.rs`
- Modify: `crates/ardosia-loadgen/tests/scenario.rs`
- Create: `crates/ardosia-loadgen/tests/churn.rs`
- Create: `scenarios/churn-500.toml`

**Interfaces:**
- Produces: `scenario::ChurnConfig { replacements_per_second: f64 }` and `Scenario::churn: Option<ChurnConfig>`.
- Produces: `Scenario::benchmark_max_connections() -> usize` and `Scenario::churn_admission_headroom() -> usize`.
- Produces: `ChurnSchedule::new(rate, hold)`, `planned_ticks()`, `period()`, `due_offset(index)`.
- Produces: deterministic `SlotSelector` and `ClientIdAllocator`.

- [ ] **Step 1: Write RED parsing/headroom/canonical scenario tests**

Extend `tests/scenario.rs`:

```rust
#[test]
fn parses_churn_and_derives_canonical_admission_headroom() {
    let scenario = load_checked_in("churn-500.toml");
    let churn = scenario.churn.as_ref().expect("churn config");
    assert_eq!(scenario.clients, 500);
    assert_eq!(churn.replacements_per_second, 25.0);
    assert_eq!(scenario.churn_admission_headroom(), 125);
    assert_eq!(scenario.benchmark_max_connections(), 625);
}

#[test]
fn rejects_non_positive_or_non_finite_churn_rate() {
    for rate in ["0.0", "-1.0", "inf", "nan"] {
        let input = format!("{BASE}\n[churn]\nreplacements_per_second = {rate}\n");
        let error = Scenario::from_str(&input).unwrap_err();
        assert!(error.to_string().contains("replacements_per_second"));
    }
}
```

Add `tests/churn.rs`:

```rust
use std::time::Duration;
use ardosia_loadgen::churn::{ChurnSchedule, ClientIdAllocator, SlotSelector};

#[test]
fn canonical_schedule_has_exact_deadline_inclusive_1500_ticks() {
    let schedule = ChurnSchedule::new(25.0, Duration::from_secs(60)).unwrap();
    assert_eq!(schedule.planned_ticks(), 1500);
    assert_eq!(schedule.due_offset(0), Some(Duration::from_millis(40)));
    assert_eq!(schedule.due_offset(1499), Some(Duration::from_secs(60)));
    assert_eq!(schedule.due_offset(1500), None);
}

#[test]
fn unique_ids_start_after_initial_population_and_never_reuse() {
    let mut ids = ClientIdAllocator::after_initial_population(500);
    assert_eq!(ids.next_id(), 500);
    assert_eq!(ids.next_id(), 501);
    assert_eq!(ids.next_id(), 502);
}
```

Add a selector test with availability `[false, true, true]` and verify repeated selections deterministically rotate among eligible indices without depending on hash iteration.

- [ ] **Step 2: Run focused tests and verify RED**

Run:

```bash
cargo test -p ardosia-loadgen --test scenario --test churn
```

Expected: FAIL because churn scenario/config/schedule types and `churn-500.toml` do not exist.

- [ ] **Step 3: Add optional scenario config and safe derived capacity**

Add to `Scenario`:

```rust
#[serde(default)]
pub churn: Option<ChurnConfig>,

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChurnConfig {
    pub replacements_per_second: f64,
}
```

Validation mirrors traffic/RTT finite-positive checks.

Derived capacity must preserve the existing non-churn `clients + 64` behavior and use the spec formula for churn:

```rust
pub fn churn_admission_headroom(&self) -> usize {
    self.churn.as_ref().map_or(0, |churn| {
        (churn.replacements_per_second * self.connect_timeout_seconds as f64)
            .ceil()
            .min(usize::MAX as f64) as usize
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

- [ ] **Step 4: Implement deterministic schedule/selector/ID helpers**

`ChurnSchedule` computes:

```rust
planned_ticks = floor(replacements_per_second * hold_seconds)
period = Duration::from_secs_f64(1.0 / replacements_per_second)
due_offset(i) = period * (i + 1), for i < planned_ticks
```

Reject values that produce zero planned ticks or non-finite/zero periods with a typed churn configuration error.

`SlotSelector` stores only `next_index`; selection scans at most `slots.len()` entries, picks the first eligible active slot, then advances `next_index` to the following slot.

`ClientIdAllocator::after_initial_population(clients)` starts at `u64::try_from(clients).unwrap_or(u64::MAX)` and uses checked/saturating failure reporting rather than wrapping to a reused ID.

- [ ] **Step 5: Add canonical `scenarios/churn-500.toml`**

Use exactly:

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

- [ ] **Step 6: Run focused tests and commit**

Run:

```bash
cargo fmt --all -- --check
cargo test -p ardosia-loadgen --test scenario --test churn
```

Expected: PASS.

Commit:

```bash
git add crates/ardosia-loadgen/src/scenario.rs crates/ardosia-loadgen/src/churn.rs crates/ardosia-loadgen/src/lib.rs crates/ardosia-loadgen/tests/scenario.rs crates/ardosia-loadgen/tests/churn.rs scenarios/churn-500.toml
git commit -m "feat: define deterministic churn scenarios"
```

---

### Task 6: Client Generation Lifecycle and Explicit End-of-Measurement Control

**Files:**
- Modify: `crates/ardosia-loadgen/src/client_task.rs`
- Modify: `crates/ardosia-loadgen/src/runner.rs`
- Modify: `crates/ardosia-loadgen/src/child_protocol.rs`
- Modify: `crates/ardosia-loadgen/src/server_target.rs`
- Create: `crates/ardosia-loadgen/tests/client_lifecycle.rs`
- Modify: `crates/ardosia-loadgen/tests/child_protocol.rs`

**Interfaces:**
- Produces global `Phase::{Ramp, Measure { deadline }, Drain, Shutdown, Abort}`.
- Produces per-generation `GenerationDirective::{Continue, PlannedDisconnect}`.
- `ClientTaskResult` gains `client_id` and `completed_planned_disconnects` while final `clean_disconnects` keeps its old meaning.
- `ConnectOutcome` becomes tagged by `client_id` and distinguishes timeout from connection error.
- Child IPC gains `EndMeasurement` / `MeasurementEnded`.

- [ ] **Step 1: Write RED lifecycle classification tests**

Create `tests/client_lifecycle.rs` around a small public pure classifier in `client_task` or a new testable helper exported from `churn`:

```rust
use ardosia_loadgen::churn::{DisconnectIntent, DisconnectOutcome, classify_disconnect};

#[test]
fn planned_clean_disconnect_is_not_final_clean_or_unexpected() {
    let counts = classify_disconnect(DisconnectIntent::PlannedChurn, DisconnectOutcome::Clean);
    assert_eq!(counts.completed_planned_disconnects, 1);
    assert_eq!(counts.clean_disconnects, 0);
    assert_eq!(counts.unexpected_disconnects, 0);
}

#[test]
fn final_clean_disconnect_keeps_existing_clean_disconnect_semantics() {
    let counts = classify_disconnect(DisconnectIntent::FinalShutdown, DisconnectOutcome::Clean);
    assert_eq!(counts.completed_planned_disconnects, 0);
    assert_eq!(counts.clean_disconnects, 1);
    assert_eq!(counts.unexpected_disconnects, 0);
}
```

Extend `tests/child_protocol.rs` to send `BeginMeasurement`, `EndMeasurement`, then `Stop`, requiring `MeasurementStarted` then `MeasurementEnded` ACKs.

- [ ] **Step 2: Run focused tests and verify RED**

Run:

```bash
cargo test -p ardosia-loadgen --test client_lifecycle --test child_protocol
```

Expected: FAIL because lifecycle classification and `EndMeasurement` do not exist.

- [ ] **Step 3: Refactor one client task into a reusable connection generation**

Change the task signature to include a generation directive receiver:

```rust
pub(crate) async fn run_client_task(
    target: SocketAddr,
    client_id: u64,
    scenario: Scenario,
    stagger: Duration,
    mut phase_rx: watch::Receiver<Phase>,
    mut directive_rx: watch::Receiver<GenerationDirective>,
    outcome_tx: mpsc::Sender<ConnectOutcome>,
) -> ClientTaskResult
```

`ConnectOutcome` becomes:

```rust
pub(crate) enum ConnectOutcome {
    Ready { client_id: u64 },
    Failed { client_id: u64, timed_out: bool },
}
```

At a planned directive, use a biased `tokio::select!` so the explicit lifecycle request wins over a simultaneous remote-disconnect event. A successful planned `client.disconnect(None)` increments only `completed_planned_disconnects`. A successful `Phase::Shutdown` disconnect increments only `clean_disconnects`.

Wrap planned/final `disconnect()` with `tokio::time::timeout(Duration::from_secs(scenario.connect_timeout_seconds), ...)` so a broken client close cannot make churn drain unbounded.

At the measured deadline, stop traffic/RTT scheduling but **do not automatically disconnect**. Wait for `Phase::Drain`, `Phase::Shutdown`, `Phase::Abort`, or a planned directive. This makes final shutdown explicit and allows churn replacements to remain connected during recovery.

For existing steady runs, update `ClientCohort` so runner sends `Phase::Shutdown` immediately after the measured sampler reaches its deadline; final steady behavior remains `clean_disconnects == scenario.clients`.

- [ ] **Step 4: Add child/server measurement-end control**

Extend IPC:

```rust
pub enum ChildCommand {
    Start { bind_addr: String, scenario: Scenario },
    BeginMeasurement,
    EndMeasurement,
    Snapshot,
    Stop,
}

pub enum ChildEvent {
    Ready { pid: u32 },
    MeasurementStarted,
    MeasurementEnded,
    Snapshot { metrics: TransportMetricsReport },
    Stopped { report: Box<ServerRunReport> },
    Error { message: String },
}
```

Change the server measurement watch from one-way boolean semantics into true/false measurement state. On `EndMeasurement`, child sets the watch false and ACKs `MeasurementEnded`.

In each server connection task, when measurement becomes false, clear scheduled server traffic lanes. Do not generate server workload during churn drain. Existing receive handling remains alive so disconnect/session cleanup is normal.

Add `ChildTarget::end_measurement()` in `runner.rs` and call it at the steady deadline before final client shutdown. This keeps server workload counters bounded to the same measured interval even after client tasks stop self-disconnecting at the deadline.

- [ ] **Step 5: Run focused and existing workload regressions**

Run:

```bash
cargo fmt --all -- --check
cargo test -p ardosia-loadgen --test client_lifecycle --test child_protocol --test workload --test bidirectional
cargo test -p ardosia-loadgen --test send_error_policy
```

Expected: PASS, including the existing `ConnectionClosed` teardown policy.

- [ ] **Step 6: Commit**

```bash
git add crates/ardosia-loadgen/src/client_task.rs crates/ardosia-loadgen/src/runner.rs crates/ardosia-loadgen/src/child_protocol.rs crates/ardosia-loadgen/src/server_target.rs crates/ardosia-loadgen/tests/client_lifecycle.rs crates/ardosia-loadgen/tests/child_protocol.rs
git commit -m "feat: add controllable client lifecycle"
```

---

### Task 7: Constant-Population Churn Cohort and Event-Level Accounting

**Files:**
- Modify: `crates/ardosia-loadgen/src/churn.rs`
- Modify: `crates/ardosia-loadgen/src/client_task.rs`
- Modify: `crates/ardosia-loadgen/tests/churn.rs`

**Interfaces:**
- Consumes: `run_client_task`, `Phase`, `GenerationDirective`, `ConnectOutcome`, `ChurnSchedule`, selector, ID allocator.
- Produces: `ChurnCohort::spawn_initial(target, scenario)`, `next_event()`, `begin_measurement(deadline)`, `run_planned_tick()`, `enter_drain()`, `shutdown()`, `finish()`.
- Produces: `ChurnRunMetrics` with exact lifecycle/population/replacement latency accounting.

- [ ] **Step 1: Write RED pure-state tests for overlapping replacements and population accounting**

Extend `tests/churn.rs` with an event-state model test:

```rust
#[test]
fn overlapping_replacements_reduce_active_population_then_recover() {
    let mut state = ChurnRunMetrics::for_target(500, 125, 625, 1500);
    state.observe_initial_population(500);

    state.planned_disconnect_started();
    state.planned_disconnect_started();
    assert_eq!(state.population_current(), 498);
    assert_eq!(state.population_min(), 498);

    state.replacement_attempt_started();
    state.replacement_attempt_started();
    assert_eq!(state.replacement_inflight_peak(), 2);

    state.replacement_connected(Duration::from_millis(8));
    state.replacement_connected(Duration::from_millis(11));
    assert_eq!(state.population_current(), 500);
    assert_eq!(state.population_end(), 500);
}
```

Add tests that a replacement timeout increments `replacement_failures` and `replacement_timeouts`, and that a tick with no eligible active slot increments `schedule_misses` rather than reducing `planned_disconnects`.

- [ ] **Step 2: Run focused tests and verify RED**

Run:

```bash
cargo test -p ardosia-loadgen --test churn
```

Expected: FAIL because event-level churn accounting/cohort APIs do not exist.

- [ ] **Step 3: Implement slot/generation state and unified result events**

Use fixed logical slots:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotState {
    ConnectingInitial,
    Active,
    PlannedDisconnect,
    ConnectingReplacement,
    Failed,
}

struct Slot {
    state: SlotState,
    client_id: u64,
    directive_tx: watch::Sender<GenerationDirective>,
    replacement_started: Option<Instant>,
}
```

Spawn generation tasks through one helper that wraps `run_client_task` and sends the completed `ClientTaskResult` to a cohort result channel. `ChurnCohort::next_event()` selects between tagged connect outcomes and finished-generation results so all state changes happen in one owner task; do not mutate slot state from detached client tasks.

Initial clients use IDs `0..clients-1`. Replacement IDs come only from `ClientIdAllocator`.

- [ ] **Step 4: Implement planned-tick lifecycle**

At every nominal churn tick:

1. increment/retain the precomputed planned schedule count;
2. choose an eligible active slot deterministically;
3. set slot state to `PlannedDisconnect` and decrement active population immediately;
4. send `GenerationDirective::PlannedDisconnect`;
5. on a finished result with `completed_planned_disconnects == 1`, increment completed planned disconnects and spawn the replacement for the same slot;
6. increment replacement attempts and in-flight count at spawn;
7. on `ConnectOutcome::Ready`, record monotonic replacement latency, mark active, increment replacement handshakes, decrement in-flight;
8. on replacement connect failure/timeout, record failure and leave the slot failed so final drain cannot falsely pass.

If no active slot is eligible or a directive cannot be delivered, increment `schedule_misses`; never silently decrement the configured planned total.

- [ ] **Step 5: Implement bounded drain predicates without network sleeps hidden inside the state model**

Add pure queries:

```rust
pub(crate) fn ready_for_post_drain_verification(&self) -> bool {
    self.metrics.completed_planned_disconnects == self.metrics.planned_disconnects
        && self.metrics.replacement_attempts == self.metrics.planned_disconnects
        && self.metrics.replacement_handshakes == self.metrics.replacement_attempts
        && self.metrics.replacement_inflight == 0
        && self.metrics.population_current == self.target_clients
}
```

The runner owns the wall-clock drain deadline in Task 8. `ChurnCohort` only exposes state/event processing; it must not wait indefinitely internally.

- [ ] **Step 6: Run focused tests and commit**

Run:

```bash
cargo fmt --all -- --check
cargo test -p ardosia-loadgen --test churn --test client_lifecycle
```

Expected: PASS.

Commit:

```bash
git add crates/ardosia-loadgen/src/churn.rs crates/ardosia-loadgen/src/client_task.rs crates/ardosia-loadgen/tests/churn.rs
git commit -m "feat: orchestrate constant population churn"
```

---

### Task 8: Run Churn During the Measured Window and Reconcile Post-Drain Transport

**Files:**
- Modify: `crates/ardosia-loadgen/src/runner.rs`
- Modify: `crates/ardosia-loadgen/src/server_target.rs`
- Modify: `crates/ardosia-loadgen/tests/churn.rs`

**Interfaces:**
- Consumes: `ChurnCohort`, `ChurnSchedule`, `Scenario::benchmark_max_connections`, child `EndMeasurement`.
- Produces: churn path inside `run_local`; non-churn path remains current steady behavior.
- Produces: measured transport start/end plus `post_drain_transport` snapshot.

- [ ] **Step 1: Write RED headroom usage and drain-reconciliation tests**

Add a server-target unit/pure test proving canonical churn uses 625 while steady-500 retains 564 (`500 + 64`). Add churn reconciliation tests around a helper:

```rust
#[test]
fn post_drain_population_accepts_nonzero_measured_session_churn() {
    let baseline_timeouts = 0;
    let post = TransportMetricsReport {
        sessions_current: 500,
        sessions_started_total: 2000,
        sessions_closed_total: 1500,
        timed_out_sessions: 0,
        ..TransportMetricsReport::default()
    };
    assert!(post_drain_transport_is_healthy(post, 500, baseline_timeouts));
}
```

The helper must reject `sessions_current != 500` and any timeout growth.

- [ ] **Step 2: Run focused tests and verify RED**

Run:

```bash
cargo test -p ardosia-loadgen --test churn
```

Expected: FAIL because churn runner/drain reconciliation does not exist.

- [ ] **Step 3: Make server target use scenario-derived admission capacity**

Replace:

```rust
max_connections: scenario.clients.saturating_add(64).max(1),
```

with:

```rust
max_connections: scenario.benchmark_max_connections(),
```

Do not change `serve_until` explicit `--max-connections` behavior.

- [ ] **Step 4: Add a churn-specific measured loop that samples resources/transport and executes all nominal ticks**

When `scenario.churn.is_some()`, initial ramp still reaches exactly `scenario.clients` and uses the existing initial correctness counters. After initial transport convergence:

1. `child.begin_measurement()`;
2. capture `transport_start`;
3. set global phase `Measure { deadline }`;
4. drive churn ticks and cohort events while continuing the existing ~1 Hz server/loadgen/host and transport sampling;
5. execute every nominal `ChurnSchedule::due_offset(i) <= hold_seconds` tick, including tick 1499 at 60,000 ms;
6. if a nominal tick is serviced at least one full churn period late, increment `schedule_misses` but still service it when it belongs to the configured window; if the runtime reaches end-of-window with unprocessed nominal ticks, mark each remaining tick missed and do not invent post-window replacements.

Keep the measured duration anchored to the original measurement start/deadline, not drain completion.

- [ ] **Step 5: End measurement, drain replacements, and converge transport before final shutdown**

At the measured boundary:

1. `child.end_measurement().await?` so server traffic lanes stop;
2. capture `transport_end` for the measured-window delta;
3. send global `Phase::Drain` so connected clients remain alive without generating measured workload;
4. continue processing cohort events until `ready_for_post_drain_verification()` or a global derived drain deadline expires;
5. derive the worst-case global drain bound as `2 * connect_timeout_seconds` from the measured deadline (one bounded planned disconnect plus one bounded replacement connect for the final tick);
6. once loadgen population is recovered, poll child snapshots up to `connect_timeout_seconds.max(2)` for cached transport convergence;
7. require `sessions_current == scenario.clients` and `timed_out_sessions == transport_start.timed_out_sessions`;
8. store the converged `post_drain_transport` snapshot;
9. send global `Phase::Shutdown` and wait for exactly `scenario.clients` final active generations to disconnect cleanly;
10. stop/reap the child normally.

Measured transport `sessions_started/sessions_closed` are not required to equal churn event totals because the final replacement can complete in drain.

- [ ] **Step 6: Run non-heavy workspace tests**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: PASS. Do not run canonical churn yet.

- [ ] **Step 7: Commit**

```bash
git add crates/ardosia-loadgen/src/runner.rs crates/ardosia-loadgen/src/server_target.rs crates/ardosia-loadgen/tests/churn.rs
git commit -m "feat: run measured churn with bounded drain"
```

---

### Task 9: Churn Report Schema, Correctness Gates, and Terminal Summary

**Files:**
- Modify: `crates/ardosia-loadgen/src/report.rs`
- Modify: `crates/ardosia-loadgen/src/churn.rs`
- Modify: `crates/ardosia-loadgen/src/runner.rs`
- Modify: `crates/ardosia-loadgen/src/main.rs`
- Modify: `crates/ardosia-loadgen/tests/report.rs`

**Interfaces:**
- Produces: `ResultsReport::churn: Option<ChurnReport>`.
- Produces exact serialized `ChurnReport` fields from the spec.
- Non-churn reports serialize `churn: null` (or absent only if `skip_serializing_if` is deliberately applied consistently); existing fields retain names/types.

- [ ] **Step 1: Write RED report/gate tests**

Extend `tests/report.rs` with a passing canonical churn fixture and one failure per lifecycle dimension. The passing assertions include:

```rust
assert_eq!(report.results.churn.as_ref().unwrap().planned_disconnects, 1500);
assert_eq!(report.results.churn.as_ref().unwrap().replacement_handshakes, 1500);
assert_eq!(report.results.churn.as_ref().unwrap().population_end, 500);
assert!(report.results.passed);
```

Add a steady regression:

```rust
#[test]
fn steady_scenario_fails_if_sessions_start_or_close_inside_measured_window() {
    let mut input = valid_input_for(&steady_500());
    input.transport.delta.sessions_started = 1;
    let report = RunReport::assemble(EnvironmentReport::default(), steady_500(), input);
    assert!(!report.results.passed);
    assert!(report.results.failure_reasons.iter().any(|r| r.contains("session churn")));
}
```

Add churn failures for `replacement_failures=1`, `schedule_misses=1`, `population_end=499`, `post_drain_transport.sessions_current=499`, and timeout growth.

- [ ] **Step 2: Run report tests and verify RED**

Run:

```bash
cargo test -p ardosia-loadgen --test report
```

Expected: FAIL because churn report/gates do not exist and steady measured-session churn is not currently gated.

- [ ] **Step 3: Add exact churn report type**

Define:

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

Add `churn: Option<ChurnReport>` to `ResultsReport` and `RunReportInput`. Existing steady callers pass `None`; churn runner converts `ChurnRunMetrics` to `ChurnReport` and passes `Some(...)`.

- [ ] **Step 4: Implement gate semantics without performance thresholds**

For non-churn scenarios, add measured-window correctness checks:

```text
transport.delta.sessions_started == 0
transport.delta.sessions_closed == 0
transport.delta.timed_out_sessions == 0
```

For churn scenarios, retain initial-ramp semantics in `RunCounts` (`successful_handshakes == clients`, `failed_handshakes == 0`) and require:

```text
planned_disconnects == floor(rate * hold_seconds)
completed_planned_disconnects == planned_disconnects
replacement_attempts == planned_disconnects
replacement_handshakes == replacement_attempts
replacement_failures == 0
replacement_timeouts == 0
schedule_misses == 0
population_end == clients
post_drain_transport.sessions_current == clients
post_drain_transport.timed_out_sessions == transport.start.timed_out_sessions
transport.delta.timed_out_sessions == 0
```

Continue requiring zero unexpected disconnects, protocol errors, genuine send errors, queue/backpressure drops/disconnects, required workload traffic, RTT completion, and final `clean_disconnects == clients`.

Do **not** fail solely on replacement latency, RTT percentiles, CPU/RSS, retransmits/NACKs, population minimum, or queue peaks without drops.

Failure strings must name the category, e.g. `churn replacement`, `churn schedule`, `churn drain`, `transport timeout`, rather than a generic failure.

- [ ] **Step 5: Add one compact terminal churn line**

After the normal RakNet line, print when `results.churn` exists:

```text
churn: planned=1500 replaced=1500 failed=0 pop_min=<n> pop_end=500 repl_p95=<x>ms
```

Use the existing `fmt_ms` helper for replacement p95.

- [ ] **Step 6: Run report + workspace tests and commit**

Run:

```bash
cargo fmt --all -- --check
cargo test -p ardosia-loadgen --test report --test churn
cargo test --workspace
```

Expected: PASS.

Commit:

```bash
git add crates/ardosia-loadgen/src/report.rs crates/ardosia-loadgen/src/churn.rs crates/ardosia-loadgen/src/runner.rs crates/ardosia-loadgen/src/main.rs crates/ardosia-loadgen/tests/report.rs
git commit -m "feat: report churn lifecycle health"
```

---

### Task 10: Manual Workflow, Documentation, and Full Non-Heavy Verification

**Files:**
- Modify: `.github/workflows/baseline.yml`
- Modify: `docs/benchmarks.md`
- Modify if needed: `crates/ardosia-loadgen/tests/scenario.rs`

**Interfaces:**
- Manual benchmark selector gains `churn-500` only.
- Profiling remains local/manual and is never added to GitHub Actions.
- Docs provide one-command profiling and churn invocations.

- [ ] **Step 1: Add a RED workflow/scenario regression if the checked-in selector is testable; otherwise verify by exact diff**

The final workflow options must be exactly the current choices plus:

```yaml
          - churn-500
```

There must be no `profile` job, no `push`, no `pull_request`, and no schedule trigger.

- [ ] **Step 2: Update benchmark documentation**

Add sections documenting:

Profiling:

```bash
cargo run --profile profiling -p ardosia-loadgen -- \
  profile scenarios/steady-1000.toml \
  --output profiles/steady-1000
```

List prerequisites (`perf`, Inferno tools, `mkfifo`), explain that the harness gets the server PID automatically, and state that host perf security policy is never changed automatically.

Churn:

```bash
cargo run --release -p ardosia-loadgen -- \
  local scenarios/churn-500.toml > churn-500.json
```

Explain 500 target clients, 25 replacements/sec, 1500 nominal replacements, 625 transport admission capacity as temporary lifecycle headroom, measured-window vs drain semantics, and which churn values are correctness gates vs record-only.

- [ ] **Step 3: Run the full non-heavy quality gate**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --manifest-path vendor/raknet-rust/Cargo.toml --lib
```

Expected: all PASS.

If execution must use a temporary GitHub Actions trigger because local execution is unavailable, trigger only the focused/non-heavy gate, capture RED/GREEN evidence, then restore `.github/workflows/ci.yml` to `workflow_dispatch` only immediately afterward. Do not run `churn-500` automatically.

- [ ] **Step 4: Verify manual-only workflow state**

Inspect:

```bash
git diff main...HEAD -- .github/workflows/ci.yml .github/workflows/baseline.yml
```

Expected: `ci.yml` remains manual-only; `baseline.yml` remains `workflow_dispatch` only and merely adds `churn-500` as a choice.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/baseline.yml docs/benchmarks.md crates/ardosia-loadgen/tests/scenario.rs
git commit -m "docs: add profiling and churn workflows"
```

---

### Task 11: Manual `churn-500` Evidence and Phase Completion

**Files:**
- Create after successful run: `docs/results/2026-08-18-churn-500.md`
- No vendor patch or optimization in this task unless a separate diagnosed change is explicitly approved.

**Interfaces:**
- Consumes canonical `scenarios/churn-500.toml` and completed report/gates.
- Produces the first trusted churn characterization evidence.

- [ ] **Step 1: Run canonical churn locally in release mode**

Run on the same controlled host used for prior baselines:

```bash
cargo run --release -p ardosia-loadgen -- \
  local scenarios/churn-500.toml > churn-500.json
```

- [ ] **Step 2: Check strict lifecycle correctness before performance interpretation**

Require:

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

If any item fails, stop scaling and use systematic debugging on that concrete failure; do not optimize from the profile or patch vendor as a reflex.

- [ ] **Step 3: Characterize record-only churn behavior**

Compare against trusted `steady-500` on the same host:

- server/loadgen/host CPU average/peak;
- server/loadgen RSS;
- RTT p50/p95/p99/max;
- replacement handshake p50/p95/p99/max;
- population minimum;
- replacement in-flight peak;
- measured traffic rates;
- retransmits/NACKs;
- queue peaks;
- measured-window session start/close deltas;
- post-drain lifetime/current session state.

Do not invent pass/fail thresholds from one run.

- [ ] **Step 4: Write and commit the churn result note**

Create `docs/results/2026-08-18-churn-500.md` containing environment/commit, scenario, strict correctness result, lifecycle counts, replacement latency, population behavior, transport telemetry, resources, and comparison to `steady-500`.

Commit:

```bash
git add docs/results/2026-08-18-churn-500.md
git commit -m "docs: record churn-500 baseline"
```

- [ ] **Step 5: Final verification before claiming the slice complete**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --manifest-path vendor/raknet-rust/Cargo.toml --lib
```

Confirm:

```bash
git status --short
```

Expected: quality gates PASS; only intentionally untracked local profile/result artifacts remain ignored; no unexplained vendor changes; heavy workflows remain manual-only.
