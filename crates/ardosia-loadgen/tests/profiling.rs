use std::path::Path;

use ardosia_loadgen::profiling::{ProfileArtifacts, resolve_run_dir};

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
