use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ardosia_network::{NetworkError, NetworkMetrics};
use serde::Serialize;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, interval, sleep, sleep_until};

use crate::child_protocol::{ChildCommand, ChildEvent, ServerRunReport};
use crate::client_task::{ClientTaskResult, ConnectOutcome, Phase, run_client_task};
use crate::environment::collect_environment;
use crate::latency::LatencyHistogram;
use crate::profiling::{
    PerfCaptureSummary, PerfSession, ProfileArtifacts, ProfileConfig, ProfileError,
    ProfileMetadata, ProfileRun, ProfileTools, resolve_run_dir,
};
use crate::report::{
    ResourceWindowsReport, RunCounts, RunReport, RunReportInput, TransportMetricsReport,
    TransportWindowReport,
};
use crate::resource::{ResourceAccumulator, ResourcePoint, ResourceSampler};
use crate::sampling::steady_interval;
use crate::scenario::Scenario;
use crate::server_target;
use crate::workload::WorkloadCounts;

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error(transparent)]
    Network(#[from] NetworkError),

    #[error(transparent)]
    Profile(#[from] ProfileError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("benchmark task failed: {0}")]
    Task(String),

    #[error("child benchmark protocol failed: {0}")]
    ChildProtocol(String),

    #[error("child benchmark process exited unexpectedly: {0}")]
    ChildExited(String),
}

struct ProfileRequest {
    config: ProfileConfig,
    tools: ProfileTools,
    artifacts: ProfileArtifacts,
}

pub async fn run_clients(target: SocketAddr, scenario: &Scenario) -> RunReport {
    let total_started = Instant::now();
    let mut cohort = ClientCohort::spawn(target, scenario);
    let mut loadgen_sampler = ResourceSampler::for_pid(std::process::id());
    let mut ramp_loadgen = ResourceAccumulator::default();
    let mut ramp_host = ResourceAccumulator::default();

    collect_handshakes(
        &mut cohort,
        &mut loadgen_sampler,
        &mut ramp_loadgen,
        &mut ramp_host,
        None,
    )
    .await;

    let successful =
        cohort.successful_handshakes == scenario.clients && cohort.failed_handshakes == 0;
    let mut steady_loadgen = ResourceAccumulator::default();
    let mut steady_host = ResourceAccumulator::default();
    let measured_duration_ms;

    if successful {
        let _ = loadgen_sampler.sample();
        let measured_started = Instant::now();
        let deadline = measured_started + Duration::from_secs(scenario.hold_seconds);
        cohort.measure(deadline);
        sample_steady_without_server(
            deadline,
            &mut loadgen_sampler,
            &mut steady_loadgen,
            &mut steady_host,
        )
        .await;
        measured_duration_ms = millis(measured_started.elapsed());
        cohort.drain();
        cohort.shutdown();
    } else {
        cohort.abort();
        measured_duration_ms = 0;
    }

    let aggregate = cohort.finish().await;
    RunReport::assemble(
        collect_environment(),
        scenario.clone(),
        RunReportInput {
            correctness: aggregate.counts,
            workload: aggregate.workload,
            latency: aggregate.latency.summary(),
            transport: TransportWindowReport::default(),
            churn: None,
            resources: ResourceWindowsReport {
                ramp_server: None,
                ramp_loadgen: Some(ramp_loadgen.finish()),
                ramp_host: Some(ramp_host.finish()),
                steady_server: None,
                steady_loadgen: successful.then(|| steady_loadgen.finish()),
                steady_host: successful.then(|| steady_host.finish()),
            },
            total_duration_ms: millis(total_started.elapsed()),
            measured_duration_ms,
        },
    )
}

pub async fn run_local(
    bind_addr: SocketAddr,
    scenario: &Scenario,
) -> Result<RunReport, RunnerError> {
    let (report, _) = run_local_internal(bind_addr, scenario, None).await?;
    Ok(report)
}

pub async fn run_profile(
    bind_addr: SocketAddr,
    scenario_path: &Path,
    scenario: &Scenario,
    output_root: Option<&Path>,
) -> Result<ProfileRun, RunnerError> {
    let root = output_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("profiles").join(&scenario.name));
    let epoch_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| RunnerError::Task(error.to_string()))?
        .as_millis();
    let run_id = format!("{epoch_ms}-{}", std::process::id());
    let output_dir = resolve_run_dir(&root, &run_id);
    std::fs::create_dir_all(&output_dir)?;
    let artifacts = ProfileArtifacts::in_dir(&output_dir);
    let config = ProfileConfig::new(scenario_path.to_path_buf(), root);

    let tools = match ProfileTools::detect() {
        Ok(tools) => tools,
        Err(error) => {
            write_early_profile_failure(scenario, &config, &artifacts, None, &error.to_string());
            return Err(error.into());
        }
    };
    let validated_perf_version = match tools.validate().await {
        Ok(version) => version,
        Err(error) => {
            write_early_profile_failure(scenario, &config, &artifacts, None, &error.to_string());
            return Err(error.into());
        }
    };

    let request = ProfileRequest {
        config: config.clone(),
        tools: tools.clone(),
        artifacts: artifacts.clone(),
    };

    let (report, capture) = match run_local_internal(bind_addr, scenario, Some(request)).await {
        Ok(result) => result,
        Err(error) => {
            write_early_profile_failure(
                scenario,
                &config,
                &artifacts,
                validated_perf_version,
                &error.to_string(),
            );
            return Err(error);
        }
    };

    write_pretty_json(&artifacts.run_json, &report)?;

    let requested_capture_ms = scenario.hold_seconds.saturating_mul(1_000);
    let Some(capture) = capture else {
        let metadata = ProfileMetadata::from_failure(
            &scenario.name,
            scenario_path.to_path_buf(),
            report.environment.git_commit.clone(),
            report.environment.vendor_revision.clone(),
            "profiling".into(),
            0,
            validated_perf_version,
            config.frequency_hz,
            config.call_graph,
            requested_capture_ms,
            0,
            artifacts.clone(),
            "benchmark did not reach the profiling capture window",
        );
        write_pretty_json(&artifacts.profile_json, &metadata)?;
        if report.results.passed {
            return Err(RunnerError::Task(
                "profile run passed benchmark correctness without producing a capture".into(),
            ));
        }
        return Ok(ProfileRun {
            report,
            metadata,
            output_dir,
        });
    };

    if let Err(error) = tools.post_process(&artifacts).await {
        let metadata = ProfileMetadata::from_failure(
            &scenario.name,
            scenario_path.to_path_buf(),
            report.environment.git_commit.clone(),
            report.environment.vendor_revision.clone(),
            "profiling".into(),
            capture.server_pid,
            capture.perf_version.clone(),
            config.frequency_hz,
            config.call_graph,
            requested_capture_ms,
            capture.observed_capture_ms,
            artifacts.clone(),
            error.to_string(),
        );
        let _ = write_pretty_json(&artifacts.profile_json, &metadata);
        return Err(error.into());
    }

    let metadata = ProfileMetadata::from_capture(
        &scenario.name,
        scenario_path.to_path_buf(),
        report.environment.git_commit.clone(),
        report.environment.vendor_revision.clone(),
        "profiling".into(),
        capture.server_pid,
        capture.perf_version,
        config.frequency_hz,
        config.call_graph,
        requested_capture_ms,
        capture.observed_capture_ms,
        artifacts.clone(),
    );
    write_pretty_json(&artifacts.profile_json, &metadata)?;

    Ok(ProfileRun {
        report,
        metadata,
        output_dir,
    })
}

async fn run_local_internal(
    bind_addr: SocketAddr,
    scenario: &Scenario,
    profile: Option<ProfileRequest>,
) -> Result<(RunReport, Option<PerfCaptureSummary>), RunnerError> {
    let total_started = Instant::now();
    let mut child = ChildTarget::spawn(bind_addr, scenario).await?;
    let mut cohort = ClientCohort::spawn(bind_addr, scenario);

    let mut server_sampler = ResourceSampler::for_pid(child.pid);
    let mut loadgen_sampler = ResourceSampler::for_pid(std::process::id());
    let mut ramp_server = ResourceAccumulator::default();
    let mut ramp_loadgen = ResourceAccumulator::default();
    let mut ramp_host = ResourceAccumulator::default();

    collect_handshakes(
        &mut cohort,
        &mut loadgen_sampler,
        &mut ramp_loadgen,
        &mut ramp_host,
        Some((&mut server_sampler, &mut ramp_server)),
    )
    .await;

    let successful =
        cohort.successful_handshakes == scenario.clients && cohort.failed_handshakes == 0;

    if !successful {
        cohort.abort();
        let aggregate = cohort.finish().await;
        let server_report = match child.stop().await {
            Ok(report) => report,
            Err(error) => {
                child.abort().await;
                return Err(error);
            }
        };
        child.reap().await?;

        let mut counts = aggregate.counts;
        add_server_errors(&mut counts, &server_report);
        let report = RunReport::assemble(
            collect_environment(),
            scenario.clone(),
            RunReportInput {
                correctness: counts,
                workload: aggregate.workload,
                latency: aggregate.latency.summary(),
                transport: TransportWindowReport::default(),
                churn: None,
                resources: ResourceWindowsReport {
                    ramp_server: Some(ramp_server.finish()),
                    ramp_loadgen: Some(ramp_loadgen.finish()),
                    ramp_host: Some(ramp_host.finish()),
                    ..ResourceWindowsReport::default()
                },
                total_duration_ms: millis(total_started.elapsed()),
                measured_duration_ms: 0,
            },
        );
        return Ok((report, None));
    }

    if let Err(error) = wait_for_transport_ready(
        &mut child,
        scenario.clients,
        Duration::from_secs(scenario.connect_timeout_seconds.max(2)),
    )
    .await
    {
        cohort.abort();
        let _ = cohort.finish().await;
        child.abort().await;
        return Err(error);
    }

    let _ = server_sampler.sample();
    let _ = loadgen_sampler.sample();

    let mut perf = if let Some(request) = profile.as_ref() {
        match PerfSession::attach_disabled(
            child.pid,
            &request.config,
            &request.tools,
            &request.artifacts,
        )
        .await
        {
            Ok(session) => Some(session),
            Err(error) => {
                cohort.abort();
                let _ = cohort.finish().await;
                child.abort().await;
                return Err(error.into());
            }
        }
    } else {
        None
    };

    if let Some(session) = perf.as_mut()
        && let Err(error) = session.enable().await
    {
        abort_perf(&mut perf).await;
        cohort.abort();
        let _ = cohort.finish().await;
        child.abort().await;
        return Err(error.into());
    }

    if let Err(error) = child.begin_measurement().await {
        abort_perf(&mut perf).await;
        cohort.abort();
        let _ = cohort.finish().await;
        child.abort().await;
        return Err(error);
    }

    let transport_start = match child.snapshot().await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            abort_perf(&mut perf).await;
            cohort.abort();
            let _ = cohort.finish().await;
            child.abort().await;
            return Err(error);
        }
    };

    let measured_started = Instant::now();
    let deadline = measured_started + Duration::from_secs(scenario.hold_seconds);
    cohort.measure(deadline);

    let mut steady_server = ResourceAccumulator::default();
    let mut steady_loadgen = ResourceAccumulator::default();
    let mut steady_host = ResourceAccumulator::default();
    let mut transport_samples = Vec::new();

    if let Err(error) = sample_steady_local(
        deadline,
        &mut child,
        &mut server_sampler,
        &mut loadgen_sampler,
        &mut steady_server,
        &mut steady_loadgen,
        &mut steady_host,
        &mut transport_samples,
    )
    .await
    {
        abort_perf(&mut perf).await;
        cohort.abort();
        let _ = cohort.finish().await;
        child.abort().await;
        return Err(error);
    }

    let measured_duration_ms = millis(measured_started.elapsed());
    let capture = match perf.take() {
        Some(session) => match session.disable_and_stop().await {
            Ok(summary) => Some(summary),
            Err(error) => {
                cohort.abort();
                let _ = cohort.finish().await;
                child.abort().await;
                return Err(error.into());
            }
        },
        None => None,
    };

    cohort.drain();
    if let Err(error) = child.end_measurement().await {
        cohort.abort();
        let _ = cohort.finish().await;
        child.abort().await;
        return Err(error);
    }

    let transport_end = match child.snapshot().await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            cohort.abort();
            let _ = cohort.finish().await;
            child.abort().await;
            return Err(error);
        }
    };

    cohort.shutdown();
    let aggregate = cohort.finish().await;

    let server_report = match child.stop().await {
        Ok(report) => report,
        Err(error) => {
            child.abort().await;
            return Err(error);
        }
    };
    child.reap().await?;

    let mut counts = aggregate.counts;
    add_server_errors(&mut counts, &server_report);
    let mut workload = aggregate.workload;
    workload.merge(server_report.workload);

    let report = RunReport::assemble(
        collect_environment(),
        scenario.clone(),
        RunReportInput {
            correctness: counts,
            workload,
            latency: aggregate.latency.summary(),
            transport: TransportWindowReport::from_snapshots(
                transport_start,
                transport_end,
                transport_samples,
            ),
            churn: None,
            resources: ResourceWindowsReport {
                ramp_server: Some(ramp_server.finish()),
                ramp_loadgen: Some(ramp_loadgen.finish()),
                ramp_host: Some(ramp_host.finish()),
                steady_server: Some(steady_server.finish()),
                steady_loadgen: Some(steady_loadgen.finish()),
                steady_host: Some(steady_host.finish()),
            },
            total_duration_ms: millis(total_started.elapsed()),
            measured_duration_ms,
        },
    );
    Ok((report, capture))
}

pub async fn serve_until(
    bind_addr: SocketAddr,
    protocol_version: u8,
    max_connections: usize,
    stop_rx: watch::Receiver<bool>,
) -> Result<NetworkMetrics, RunnerError> {
    server_target::serve_until(bind_addr, protocol_version, max_connections, stop_rx).await
}

struct ClientCohort {
    phase_tx: watch::Sender<Phase>,
    outcome_rx: mpsc::Receiver<ConnectOutcome>,
    tasks: Vec<JoinHandle<ClientTaskResult>>,
    successful_handshakes: usize,
    failed_handshakes: usize,
}

impl ClientCohort {
    fn spawn(target: SocketAddr, scenario: &Scenario) -> Self {
        let (phase_tx, phase_rx) = watch::channel(Phase::Ramp);
        let (outcome_tx, outcome_rx) = mpsc::channel(scenario.clients.max(1));
        let mut tasks = Vec::with_capacity(scenario.clients);

        for index in 0..scenario.clients {
            tasks.push(tokio::spawn(run_client_task(
                target,
                u64::try_from(index).unwrap_or(u64::MAX),
                scenario.clone(),
                stagger_delay(index, scenario.clients, scenario.ramp_up_seconds),
                phase_rx.clone(),
                outcome_tx.clone(),
            )));
        }
        drop(outcome_tx);

        Self {
            phase_tx,
            outcome_rx,
            tasks,
            successful_handshakes: 0,
            failed_handshakes: 0,
        }
    }

    fn measure(&self, deadline: Instant) {
        let _ = self.phase_tx.send(Phase::Measure { deadline });
    }

    fn drain(&self) {
        let _ = self.phase_tx.send(Phase::Drain);
    }

    fn shutdown(&self) {
        let _ = self.phase_tx.send(Phase::Shutdown);
    }

    fn abort(&self) {
        let _ = self.phase_tx.send(Phase::Abort);
    }

    async fn finish(self) -> ClientAggregate {
        let reported = self
            .successful_handshakes
            .saturating_add(self.failed_handshakes);
        let mut aggregate = ClientAggregate {
            counts: RunCounts {
                successful_handshakes: self.successful_handshakes,
                failed_handshakes: self
                    .failed_handshakes
                    .saturating_add(self.tasks.len().saturating_sub(reported)),
                ..RunCounts::default()
            },
            ..ClientAggregate::default()
        };

        for task in self.tasks {
            match task.await {
                Ok(result) => {
                    aggregate.counts.unexpected_disconnects = aggregate
                        .counts
                        .unexpected_disconnects
                        .saturating_add(result.unexpected_disconnects);
                    aggregate.counts.protocol_errors = aggregate
                        .counts
                        .protocol_errors
                        .saturating_add(result.protocol_errors);
                    aggregate.counts.send_errors = aggregate
                        .counts
                        .send_errors
                        .saturating_add(result.send_errors);
                    aggregate.counts.clean_disconnects = aggregate
                        .counts
                        .clean_disconnects
                        .saturating_add(result.clean_disconnects);
                    aggregate.workload.merge(result.workload);
                    aggregate.latency.merge(&result.latency);
                }
                Err(_) => {
                    aggregate.counts.unexpected_disconnects =
                        aggregate.counts.unexpected_disconnects.saturating_add(1);
                }
            }
        }
        aggregate
    }
}

#[derive(Debug, Default)]
struct ClientAggregate {
    counts: RunCounts,
    workload: WorkloadCounts,
    latency: LatencyHistogram,
}

async fn collect_handshakes(
    cohort: &mut ClientCohort,
    loadgen_sampler: &mut ResourceSampler,
    ramp_loadgen: &mut ResourceAccumulator,
    ramp_host: &mut ResourceAccumulator,
    mut server: Option<(&mut ResourceSampler, &mut ResourceAccumulator)>,
) {
    let expected = cohort.tasks.len();
    let mut ticker = interval(Duration::from_secs(1));

    while cohort
        .successful_handshakes
        .saturating_add(cohort.failed_handshakes)
        < expected
    {
        tokio::select! {
            outcome = cohort.outcome_rx.recv() => {
                match outcome {
                    Some(ConnectOutcome::Ready) => {
                        cohort.successful_handshakes = cohort.successful_handshakes.saturating_add(1);
                    }
                    Some(ConnectOutcome::Failed) => {
                        cohort.failed_handshakes = cohort.failed_handshakes.saturating_add(1);
                    }
                    None => break,
                }
            }
            _ = ticker.tick() => {
                let loadgen = loadgen_sampler.sample();
                push_process(ramp_loadgen, loadgen);
                push_host(ramp_host, loadgen);
                if let Some((sampler, accumulator)) = server.as_mut() {
                    push_process(accumulator, sampler.sample());
                }
            }
        }
    }

    let loadgen = loadgen_sampler.sample();
    push_process(ramp_loadgen, loadgen);
    push_host(ramp_host, loadgen);
    if let Some((sampler, accumulator)) = server.as_mut() {
        push_process(accumulator, sampler.sample());
    }
}

async fn wait_for_transport_ready(
    child: &mut ChildTarget,
    expected_sessions: usize,
    timeout_duration: Duration,
) -> Result<(), RunnerError> {
    let expected = u64::try_from(expected_sessions).unwrap_or(u64::MAX);
    let deadline = Instant::now() + timeout_duration;

    loop {
        let snapshot = child.snapshot().await?;
        if snapshot.sessions_current == expected
            && snapshot.sessions_started_total == expected
            && snapshot.sessions_closed_total == 0
            && snapshot.timed_out_sessions == 0
        {
            return Ok(());
        }

        if Instant::now() >= deadline {
            return Err(RunnerError::ChildProtocol(format!(
                "transport metrics did not converge before measurement: expected {expected} sessions, current={}, started={}, closed={}, timed_out={}",
                snapshot.sessions_current,
                snapshot.sessions_started_total,
                snapshot.sessions_closed_total,
                snapshot.timed_out_sessions,
            )));
        }

        sleep(Duration::from_millis(25)).await;
    }
}

async fn sample_steady_without_server(
    deadline: Instant,
    loadgen_sampler: &mut ResourceSampler,
    steady_loadgen: &mut ResourceAccumulator,
    steady_host: &mut ResourceAccumulator,
) {
    let mut ticker = steady_interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = sleep_until(deadline) => break,
            _ = ticker.tick() => {
                let loadgen = loadgen_sampler.sample();
                push_process(steady_loadgen, loadgen);
                push_host(steady_host, loadgen);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn sample_steady_local(
    deadline: Instant,
    child: &mut ChildTarget,
    server_sampler: &mut ResourceSampler,
    loadgen_sampler: &mut ResourceSampler,
    steady_server: &mut ResourceAccumulator,
    steady_loadgen: &mut ResourceAccumulator,
    steady_host: &mut ResourceAccumulator,
    transport_samples: &mut Vec<TransportMetricsReport>,
) -> Result<(), RunnerError> {
    let mut ticker = steady_interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = sleep_until(deadline) => return Ok(()),
            _ = ticker.tick() => {
                push_process(steady_server, server_sampler.sample());
                let loadgen = loadgen_sampler.sample();
                push_process(steady_loadgen, loadgen);
                push_host(steady_host, loadgen);
                transport_samples.push(child.snapshot().await?);
            }
        }
    }
}

async fn abort_perf(perf: &mut Option<PerfSession>) {
    if let Some(mut session) = perf.take() {
        session.abort().await;
    }
}

fn push_process(accumulator: &mut ResourceAccumulator, point: ResourcePoint) {
    accumulator.push(ResourcePoint {
        process_cpu_pct: point.process_cpu_pct,
        process_rss_bytes: point.process_rss_bytes,
        ..ResourcePoint::default()
    });
}

fn push_host(accumulator: &mut ResourceAccumulator, point: ResourcePoint) {
    accumulator.push(ResourcePoint {
        host_cpu_pct: point.host_cpu_pct,
        host_memory_used_bytes: point.host_memory_used_bytes,
        host_memory_available_bytes: point.host_memory_available_bytes,
        ..ResourcePoint::default()
    });
}

fn add_server_errors(counts: &mut RunCounts, report: &ServerRunReport) {
    counts.protocol_errors = counts
        .protocol_errors
        .saturating_add(report.metrics.protocol_errors_total as usize)
        .saturating_add(report.benchmark_protocol_errors);
    counts.send_errors = counts.send_errors.saturating_add(report.send_errors);
}

fn write_pretty_json<T: Serialize>(path: &Path, value: &T) -> Result<(), RunnerError> {
    let json =
        serde_json::to_vec_pretty(value).map_err(|error| RunnerError::Task(error.to_string()))?;
    std::fs::write(path, json)?;
    Ok(())
}

fn write_early_profile_failure(
    scenario: &Scenario,
    config: &ProfileConfig,
    artifacts: &ProfileArtifacts,
    perf_version: Option<String>,
    failure: &str,
) {
    let environment = collect_environment();
    let metadata = ProfileMetadata::from_failure(
        &scenario.name,
        config.scenario_path.clone(),
        environment.git_commit,
        environment.vendor_revision,
        "profiling".into(),
        0,
        perf_version,
        config.frequency_hz,
        config.call_graph,
        scenario.hold_seconds.saturating_mul(1_000),
        0,
        artifacts.clone(),
        failure,
    );
    let _ = write_pretty_json(&artifacts.profile_json, &metadata);
}

struct ChildTarget {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    pid: u32,
}

impl ChildTarget {
    async fn spawn(bind_addr: SocketAddr, scenario: &Scenario) -> Result<Self, RunnerError> {
        let executable = std::env::current_exe()?;
        let mut child = Command::new(executable)
            .arg("serve-child")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()?;

        let pid = child
            .id()
            .ok_or_else(|| RunnerError::ChildExited("process has no PID".into()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| RunnerError::ChildProtocol("child stdin was not piped".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RunnerError::ChildProtocol("child stdout was not piped".into()))?;
        let mut target = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout).lines(),
            pid,
        };

        if let Err(error) = target
            .send(&ChildCommand::Start {
                bind_addr: bind_addr.to_string(),
                scenario: scenario.clone(),
                worker_shards: crate::runtime_override::worker_shards(),
            })
            .await
        {
            target.abort().await;
            return Err(error);
        }

        match target.recv().await {
            Ok(ChildEvent::Ready { pid }) if pid == target.pid => Ok(target),
            Ok(ChildEvent::Ready { pid }) => {
                target.abort().await;
                Err(RunnerError::ChildProtocol(format!(
                    "ready PID {pid} did not match spawned PID {}",
                    target.pid
                )))
            }
            Ok(ChildEvent::Error { message }) => {
                target.abort().await;
                Err(RunnerError::ChildProtocol(message))
            }
            Ok(other) => {
                target.abort().await;
                Err(RunnerError::ChildProtocol(format!(
                    "expected ready event, got {other:?}"
                )))
            }
            Err(error) => {
                target.abort().await;
                Err(error)
            }
        }
    }

    async fn begin_measurement(&mut self) -> Result<(), RunnerError> {
        self.send(&ChildCommand::BeginMeasurement).await?;
        match self.recv().await? {
            ChildEvent::MeasurementStarted => Ok(()),
            ChildEvent::Error { message } => Err(RunnerError::ChildProtocol(message)),
            other => Err(RunnerError::ChildProtocol(format!(
                "expected measurement_started event, got {other:?}"
            ))),
        }
    }

    async fn end_measurement(&mut self) -> Result<(), RunnerError> {
        self.send(&ChildCommand::EndMeasurement).await?;
        match self.recv().await? {
            ChildEvent::MeasurementEnded => Ok(()),
            ChildEvent::Error { message } => Err(RunnerError::ChildProtocol(message)),
            other => Err(RunnerError::ChildProtocol(format!(
                "expected measurement_ended event, got {other:?}"
            ))),
        }
    }

    async fn snapshot(&mut self) -> Result<TransportMetricsReport, RunnerError> {
        self.send(&ChildCommand::Snapshot).await?;
        match self.recv().await? {
            ChildEvent::Snapshot { metrics, .. } => Ok(metrics),
            ChildEvent::Error { message } => Err(RunnerError::ChildProtocol(message)),
            other => Err(RunnerError::ChildProtocol(format!(
                "expected snapshot event, got {other:?}"
            ))),
        }
    }

    async fn stop(&mut self) -> Result<ServerRunReport, RunnerError> {
        self.send(&ChildCommand::Stop).await?;
        match self.recv().await? {
            ChildEvent::Stopped { report } => Ok(*report),
            ChildEvent::Error { message } => Err(RunnerError::ChildProtocol(message)),
            other => Err(RunnerError::ChildProtocol(format!(
                "expected stopped event, got {other:?}"
            ))),
        }
    }

    async fn send(&mut self, command: &ChildCommand) -> Result<(), RunnerError> {
        let json = serde_json::to_vec(command)
            .map_err(|error| RunnerError::ChildProtocol(error.to_string()))?;
        self.stdin.write_all(&json).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<ChildEvent, RunnerError> {
        let Some(line) = self.stdout.next_line().await? else {
            let status = self.child.wait().await?;
            return Err(RunnerError::ChildExited(status.to_string()));
        };
        serde_json::from_str(&line).map_err(|error| RunnerError::ChildProtocol(error.to_string()))
    }

    async fn reap(&mut self) -> Result<(), RunnerError> {
        let status = self.child.wait().await?;
        if status.success() {
            Ok(())
        } else {
            Err(RunnerError::ChildExited(status.to_string()))
        }
    }

    async fn abort(&mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

fn stagger_delay(index: usize, clients: usize, ramp_up_seconds: u64) -> Duration {
    if clients <= 1 || ramp_up_seconds == 0 {
        return Duration::ZERO;
    }

    let total_ms = u128::from(ramp_up_seconds).saturating_mul(1_000);
    let offset_ms = total_ms
        .saturating_mul(index as u128)
        .checked_div(clients as u128)
        .unwrap_or(0)
        .min(u64::MAX as u128) as u64;
    Duration::from_millis(offset_ms)
}

fn millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}
