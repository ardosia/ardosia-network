use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ardosia_loadgen::profiling::{
    CallGraphMode, PerfCommandSpec, ProfileArtifacts, ProfileError, ProfileMetadata, ProfileTools,
    resolve_run_dir,
};

#[test]
fn profile_run_directory_is_isolated_under_requested_root() {
    let path = resolve_run_dir(Path::new("profiles/custom"), "1724020000000-4242");
    assert_eq!(
        path,
        Path::new("profiles/custom").join("1724020000000-4242")
    );
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

#[tokio::test]
async fn injected_profile_tools_validate_and_preserve_perf_version() {
    let dir = temp_test_dir("profile-tools-ok");
    let perf = fake_tool(&dir, "perf", 0);
    let collapse = fake_tool(&dir, "inferno-collapse-perf", 0);
    let flamegraph = fake_tool(&dir, "inferno-flamegraph", 0);
    let mkfifo = fake_tool(&dir, "mkfifo", 0);
    let tools = ProfileTools::from_paths(perf, collapse, flamegraph, mkfifo);

    let perf_version = tools.validate().await.unwrap();
    assert_eq!(perf_version.as_deref(), Some("fake-tool 1.0"));
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn inferno_tools_are_validated_without_requiring_version_flag() {
    let dir = temp_test_dir("profile-tools-inferno-help");
    let perf = fake_tool(&dir, "perf", 0);
    let collapse = fake_help_only_tool(&dir, "inferno-collapse-perf");
    let flamegraph = fake_help_only_tool(&dir, "inferno-flamegraph");
    let mkfifo = fake_tool(&dir, "mkfifo", 0);
    let tools = ProfileTools::from_paths(perf, collapse, flamegraph, mkfifo);

    let perf_version = tools.validate().await.unwrap();
    assert_eq!(perf_version.as_deref(), Some("fake-tool 1.0"));
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn failing_perf_prerequisite_is_classified_as_missing_tool() {
    let dir = temp_test_dir("profile-tools-bad-perf");
    let perf = fake_tool(&dir, "perf", 7);
    let collapse = fake_tool(&dir, "inferno-collapse-perf", 0);
    let flamegraph = fake_tool(&dir, "inferno-flamegraph", 0);
    let mkfifo = fake_tool(&dir, "mkfifo", 0);
    let tools = ProfileTools::from_paths(perf, collapse, flamegraph, mkfifo);

    match tools.validate().await.unwrap_err() {
        ProfileError::MissingTool { tool, .. } => assert_eq!(tool, "perf"),
        other => panic!("expected missing perf prerequisite, got {other}"),
    }
    let _ = fs::remove_dir_all(dir);
}

fn temp_test_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("ardosia-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn fake_tool(dir: &Path, name: &str, exit_code: i32) -> PathBuf {
    let path = dir.join(name);
    fs::write(
        &path,
        format!("#!/bin/sh\necho fake-tool 1.0\nexit {exit_code}\n"),
    )
    .unwrap();
    make_executable(&path);
    path
}

fn fake_help_only_tool(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(
        &path,
        "#!/bin/sh\nif [ \"$1\" = \"--help\" ]; then\n  echo fake-help\n  exit 0\nfi\necho unsupported >&2\nexit 2\n",
    )
    .unwrap();
    make_executable(&path);
    path
}

fn make_executable(path: &Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}
