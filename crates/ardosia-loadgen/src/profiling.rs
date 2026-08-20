use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::report::RunReport;

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

impl ProfileMetadata {
    #[allow(clippy::too_many_arguments)]
    pub fn from_capture(
        scenario_name: impl Into<String>,
        scenario_path: PathBuf,
        git_commit: Option<String>,
        vendor_revision: String,
        build_profile: String,
        server_pid: u32,
        perf_version: Option<String>,
        frequency_hz: u32,
        call_graph: CallGraphMode,
        requested_capture_ms: u64,
        observed_capture_ms: u64,
        artifacts: ProfileArtifacts,
    ) -> Self {
        Self {
            scenario_name: scenario_name.into(),
            scenario_path,
            git_commit,
            vendor_revision,
            build_profile,
            server_pid,
            profiler: "perf".into(),
            perf_version,
            frequency_hz,
            call_graph,
            requested_capture_ms,
            observed_capture_ms,
            artifacts,
            success: true,
            failure: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_failure(
        scenario_name: impl Into<String>,
        scenario_path: PathBuf,
        git_commit: Option<String>,
        vendor_revision: String,
        build_profile: String,
        server_pid: u32,
        perf_version: Option<String>,
        frequency_hz: u32,
        call_graph: CallGraphMode,
        requested_capture_ms: u64,
        observed_capture_ms: u64,
        artifacts: ProfileArtifacts,
        failure: impl Into<String>,
    ) -> Self {
        let mut metadata = Self::from_capture(
            scenario_name,
            scenario_path,
            git_commit,
            vendor_revision,
            build_profile,
            server_pid,
            perf_version,
            frequency_hz,
            call_graph,
            requested_capture_ms,
            observed_capture_ms,
            artifacts,
        );
        metadata.success = false;
        metadata.failure = Some(failure.into());
        metadata
    }
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
