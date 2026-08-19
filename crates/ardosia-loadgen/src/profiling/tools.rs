use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::{ProfileArtifacts, ProfileError};

#[derive(Debug, Clone)]
pub struct ProfileTools {
    pub perf: PathBuf,
    pub collapse_perf: PathBuf,
    pub flamegraph: PathBuf,
    pub mkfifo: PathBuf,
}

impl ProfileTools {
    pub fn detect() -> Result<Self, ProfileError> {
        if !cfg!(target_os = "linux") {
            return Err(ProfileError::UnsupportedPlatform);
        }
        Ok(Self::from_paths(
            "perf".into(),
            "inferno-collapse-perf".into(),
            "inferno-flamegraph".into(),
            "mkfifo".into(),
        ))
    }

    pub fn from_paths(
        perf: PathBuf,
        collapse_perf: PathBuf,
        flamegraph: PathBuf,
        mkfifo: PathBuf,
    ) -> Self {
        Self {
            perf,
            collapse_perf,
            flamegraph,
            mkfifo,
        }
    }

    pub async fn validate(&self) -> Result<Option<String>, ProfileError> {
        if !cfg!(target_os = "linux") {
            return Err(ProfileError::UnsupportedPlatform);
        }
        let perf_output = validate_tool("perf", &self.perf).await?;
        validate_tool("inferno-collapse-perf", &self.collapse_perf).await?;
        validate_tool("inferno-flamegraph", &self.flamegraph).await?;
        validate_tool("mkfifo", &self.mkfifo).await?;

        let version = String::from_utf8_lossy(&perf_output.stdout)
            .lines()
            .next()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned);
        Ok(version)
    }

    pub async fn post_process(&self, artifacts: &ProfileArtifacts) -> Result<(), ProfileError> {
        require_non_empty(&artifacts.perf_data)?;

        let report = Command::new(&self.perf)
            .arg("report")
            .arg("--stdio")
            .arg("-i")
            .arg(&artifacts.perf_data)
            .output()
            .await
            .map_err(|error| command_spawn_error("perf report", error))?;
        if !report.status.success() {
            return Err(command_status_error("perf report", &report.stderr));
        }
        std::fs::write(&artifacts.perf_report, &report.stdout)?;
        require_non_empty(&artifacts.perf_report)?;

        let script = Command::new(&self.perf)
            .arg("script")
            .arg("-i")
            .arg(&artifacts.perf_data)
            .output()
            .await
            .map_err(|error| command_spawn_error("perf script", error))?;
        if !script.status.success() {
            return Err(command_status_error("perf script", &script.stderr));
        }
        if script.stdout.is_empty() {
            return Err(ProfileError::EmptyArtifact(artifacts.perf_data.clone()));
        }

        let mut collapse = Command::new(&self.collapse_perf)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| command_spawn_error("inferno-collapse-perf", error))?;
        let mut collapse_stdin = collapse.stdin.take().ok_or_else(|| ProfileError::CommandFailed {
            tool: "inferno-collapse-perf".into(),
            detail: "stdin was not piped".into(),
        })?;
        collapse_stdin.write_all(&script.stdout).await?;
        drop(collapse_stdin);
        let collapsed = collapse
            .wait_with_output()
            .await
            .map_err(|error| command_spawn_error("inferno-collapse-perf", error))?;
        if !collapsed.status.success() {
            return Err(command_status_error(
                "inferno-collapse-perf",
                &collapsed.stderr,
            ));
        }
        std::fs::write(&artifacts.stacks_folded, &collapsed.stdout)?;
        require_non_empty(&artifacts.stacks_folded)?;

        let folded = std::fs::read(&artifacts.stacks_folded)?;
        let mut flamegraph = Command::new(&self.flamegraph)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| command_spawn_error("inferno-flamegraph", error))?;
        let mut flamegraph_stdin =
            flamegraph
                .stdin
                .take()
                .ok_or_else(|| ProfileError::CommandFailed {
                    tool: "inferno-flamegraph".into(),
                    detail: "stdin was not piped".into(),
                })?;
        flamegraph_stdin.write_all(&folded).await?;
        drop(flamegraph_stdin);
        let rendered = flamegraph
            .wait_with_output()
            .await
            .map_err(|error| command_spawn_error("inferno-flamegraph", error))?;
        if !rendered.status.success() {
            return Err(command_status_error(
                "inferno-flamegraph",
                &rendered.stderr,
            ));
        }
        std::fs::write(&artifacts.flamegraph_svg, &rendered.stdout)?;
        require_non_empty(&artifacts.flamegraph_svg)?;
        Ok(())
    }
}

async fn validate_tool(tool: &str, path: &Path) -> Result<std::process::Output, ProfileError> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .await
        .map_err(|error| ProfileError::MissingTool {
            tool: tool.into(),
            detail: error.to_string(),
        })?;
    if !output.status.success() {
        return Err(ProfileError::MissingTool {
            tool: tool.into(),
            detail: format!(
                "exit status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    Ok(output)
}

pub(crate) fn require_non_empty(path: &Path) -> Result<(), ProfileError> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() == 0 {
        return Err(ProfileError::EmptyArtifact(path.to_path_buf()));
    }
    Ok(())
}

fn command_spawn_error(tool: &str, error: std::io::Error) -> ProfileError {
    ProfileError::CommandFailed {
        tool: tool.into(),
        detail: error.to_string(),
    }
}

fn command_status_error(tool: &str, stderr: &[u8]) -> ProfileError {
    ProfileError::CommandFailed {
        tool: tool.into(),
        detail: String::from_utf8_lossy(stderr).trim().to_owned(),
    }
}
