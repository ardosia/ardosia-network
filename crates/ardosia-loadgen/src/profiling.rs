use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::report::RunReport;

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

pub fn resolve_run_dir(root: &Path, run_id: &str) -> PathBuf {
    root.join(run_id)
}
