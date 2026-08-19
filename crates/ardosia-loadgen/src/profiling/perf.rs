use std::fs::OpenOptions;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::fs::File;
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::process::{Child, Command};
use tokio::time::timeout;

use super::tools::require_non_empty;
use super::{CallGraphMode, ProfileArtifacts, ProfileConfig, ProfileError, ProfileTools};

const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct PerfCommandSpec {
    server_pid: u32,
    frequency_hz: u32,
    call_graph: CallGraphMode,
    control_fifo: PathBuf,
    ack_fifo: PathBuf,
    output: PathBuf,
}

impl PerfCommandSpec {
    pub fn new(
        server_pid: u32,
        frequency_hz: u32,
        call_graph: CallGraphMode,
        control_fifo: PathBuf,
        ack_fifo: PathBuf,
        output: PathBuf,
    ) -> Self {
        Self {
            server_pid,
            frequency_hz,
            call_graph,
            control_fifo,
            ack_fifo,
            output,
        }
    }

    pub fn args(&self) -> Vec<String> {
        let call_graph = match self.call_graph {
            CallGraphMode::Dwarf => "dwarf",
        };
        vec![
            "record".into(),
            "-F".into(),
            self.frequency_hz.to_string(),
            "-g".into(),
            "--call-graph".into(),
            call_graph.into(),
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
            self.output.display().to_string(),
        ]
    }
}

#[derive(Debug, Clone)]
pub struct PerfCaptureSummary {
    pub server_pid: u32,
    pub perf_version: Option<String>,
    pub observed_capture_ms: u64,
}

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

impl<R, W> PerfControl<R, W>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    fn new(reader: R, writer: W) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer,
            state: PerfControlState::Disabled,
        }
    }

    async fn enable(&mut self) -> Result<(), ProfileError> {
        if self.state != PerfControlState::Disabled {
            return Err(ProfileError::Control(format!(
                "cannot enable perf from {:?}",
                self.state
            )));
        }
        self.send_and_ack("enable").await?;
        self.state = PerfControlState::Enabled;
        Ok(())
    }

    async fn disable(&mut self) -> Result<(), ProfileError> {
        if self.state != PerfControlState::Enabled {
            return Err(ProfileError::Control(format!(
                "cannot disable perf from {:?}",
                self.state
            )));
        }
        self.send_and_ack("disable").await?;
        self.state = PerfControlState::Disabled;
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), ProfileError> {
        if self.state == PerfControlState::Enabled {
            return Err(ProfileError::Control(
                "perf must be disabled before stop".into(),
            ));
        }
        if self.state == PerfControlState::Stopped {
            return Ok(());
        }
        self.send_and_ack("stop").await?;
        self.state = PerfControlState::Stopped;
        Ok(())
    }

    async fn send_and_ack(&mut self, command: &str) -> Result<(), ProfileError> {
        self.writer.write_all(command.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;

        let mut line = String::new();
        let read = timeout(CONTROL_TIMEOUT, self.reader.read_line(&mut line))
            .await
            .map_err(|_| ProfileError::Control(format!("timed out waiting for {command} ack")))??;
        if read == 0 {
            return Err(ProfileError::Control(format!(
                "perf ack fifo closed while waiting for {command}"
            )));
        }
        if line.trim() != "ack" {
            return Err(ProfileError::Control(format!(
                "unexpected perf ack for {command}: {:?}",
                line.trim()
            )));
        }
        Ok(())
    }
}

pub struct PerfSession {
    child: Child,
    control: PerfControl<File, File>,
    control_fifo: PathBuf,
    ack_fifo: PathBuf,
    perf_data: PathBuf,
    server_pid: u32,
    perf_version: Option<String>,
    capture_started: Option<Instant>,
}

impl PerfSession {
    pub async fn attach_disabled(
        server_pid: u32,
        config: &ProfileConfig,
        tools: &ProfileTools,
        artifacts: &ProfileArtifacts,
    ) -> Result<Self, ProfileError> {
        let perf_version = tools.validate().await?;
        let parent = artifacts.perf_data.parent().ok_or_else(|| {
            ProfileError::Control("perf.data has no parent output directory".into())
        })?;
        std::fs::create_dir_all(parent)?;
        let control_fifo = parent.join("perf-control.fifo");
        let ack_fifo = parent.join("perf-ack.fifo");
        remove_if_present(&control_fifo)?;
        remove_if_present(&ack_fifo)?;
        create_fifo(tools, &control_fifo).await?;
        if let Err(error) = create_fifo(tools, &ack_fifo).await {
            let _ = std::fs::remove_file(&control_fifo);
            return Err(error);
        }

        let control_file = match open_fifo(&control_fifo) {
            Ok(file) => file,
            Err(error) => {
                cleanup_fifos(&control_fifo, &ack_fifo);
                return Err(error.into());
            }
        };
        let ack_file = match open_fifo(&ack_fifo) {
            Ok(file) => file,
            Err(error) => {
                cleanup_fifos(&control_fifo, &ack_fifo);
                return Err(error.into());
            }
        };

        let spec = PerfCommandSpec::new(
            server_pid,
            config.frequency_hz,
            config.call_graph,
            control_fifo.clone(),
            ack_fifo.clone(),
            artifacts.perf_data.clone(),
        );
        let child = match Command::new(&tools.perf)
            .args(spec.args())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                cleanup_fifos(&control_fifo, &ack_fifo);
                return Err(ProfileError::CommandFailed {
                    tool: "perf record".into(),
                    detail: error.to_string(),
                });
            }
        };

        Ok(Self {
            child,
            control: PerfControl::new(File::from_std(ack_file), File::from_std(control_file)),
            control_fifo,
            ack_fifo,
            perf_data: artifacts.perf_data.clone(),
            server_pid,
            perf_version,
            capture_started: None,
        })
    }

    pub async fn enable(&mut self) -> Result<(), ProfileError> {
        self.control.enable().await?;
        self.capture_started = Some(Instant::now());
        Ok(())
    }

    pub async fn disable_and_stop(mut self) -> Result<PerfCaptureSummary, ProfileError> {
        self.control.disable().await?;
        let observed_capture_ms = self
            .capture_started
            .take()
            .map(|started| started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0);
        self.control.stop().await?;

        let status = self.child.wait().await?;
        let mut stderr = String::new();
        if let Some(mut stream) = self.child.stderr.take() {
            stream.read_to_string(&mut stderr).await?;
        }
        cleanup_fifos(&self.control_fifo, &self.ack_fifo);
        if !status.success() {
            return Err(ProfileError::CommandFailed {
                tool: "perf record".into(),
                detail: format!("exit status {status}: {}", stderr.trim()),
            });
        }
        require_non_empty(&self.perf_data)?;

        Ok(PerfCaptureSummary {
            server_pid: self.server_pid,
            perf_version: self.perf_version.clone(),
            observed_capture_ms,
        })
    }

    pub async fn abort(&mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
        cleanup_fifos(&self.control_fifo, &self.ack_fifo);
    }
}

impl Drop for PerfSession {
    fn drop(&mut self) {
        cleanup_fifos(&self.control_fifo, &self.ack_fifo);
    }
}

async fn create_fifo(tools: &ProfileTools, path: &PathBuf) -> Result<(), ProfileError> {
    let output = Command::new(&tools.mkfifo)
        .arg(path)
        .output()
        .await
        .map_err(|error| ProfileError::CommandFailed {
            tool: "mkfifo".into(),
            detail: error.to_string(),
        })?;
    if !output.status.success() {
        return Err(ProfileError::CommandFailed {
            tool: "mkfifo".into(),
            detail: format!(
                "exit status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    Ok(())
}

fn open_fifo(path: &PathBuf) -> std::io::Result<std::fs::File> {
    OpenOptions::new().read(true).write(true).open(path)
}

fn remove_if_present(path: &PathBuf) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn cleanup_fifos(control: &PathBuf, ack: &PathBuf) {
    let _ = std::fs::remove_file(control);
    let _ = std::fs::remove_file(ack);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    use super::{PerfControl, PerfControlState};

    #[tokio::test]
    async fn control_waits_for_ack_before_advancing_state() {
        let (client, peer) = tokio::io::duplex(128);
        let (client_read, client_write) = tokio::io::split(client);
        let (peer_read, mut peer_write) = tokio::io::split(peer);
        let mut peer_read = BufReader::new(peer_read);
        let mut control = PerfControl::new(client_read, client_write);

        let peer_task = tokio::spawn(async move {
            let mut line = String::new();
            peer_read.read_line(&mut line).await.unwrap();
            assert_eq!(line, "enable\n");
            tokio::time::sleep(Duration::from_millis(10)).await;
            peer_write.write_all(b"ack\n").await.unwrap();
        });

        control.enable().await.unwrap();
        assert_eq!(control.state, PerfControlState::Enabled);
        peer_task.await.unwrap();
    }
}
