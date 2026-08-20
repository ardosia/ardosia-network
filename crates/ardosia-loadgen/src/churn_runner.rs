use std::net::SocketAddr;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::{Instant, interval, sleep, sleep_until};

use crate::child_protocol::{ChildCommand, ChildEvent, ServerRunReport};
use crate::churn::{ChurnCohort, ChurnEvent, ChurnSchedule, post_drain_transport_is_healthy};
use crate::client_task::{ClientTaskResult, GenerationConnectOutcome};
use crate::environment::collect_environment;
use crate::latency::LatencyHistogram;
use crate::report::{
    ChurnReport, ResourceWindowsReport, RunCounts, RunReport, RunReportInput,
    TransportMetricsReport, TransportWindowReport,
};
use crate::resource::{ResourceAccumulator, ResourcePoint, ResourceSampler};
use crate::runner::RunnerError;
use crate::sampling::steady_interval;
use crate::scenario::Scenario;
use crate::workload::WorkloadCounts;

pub async fn run_local_churn(
    bind_addr: SocketAddr,
    scenario: &Scenario,
) -> Result<RunReport, RunnerError> {
    if scenario.churn.is_none() {
        return Err(RunnerError::Task(
            "churn runner requires a scenario with [churn]".into(),
        ));
    }

    let total_started = Instant::now();
    let mut child = ChildTarget::spawn(bind_addr, scenario).await?;
    let mut cohort = ChurnCohort::spawn(bind_addr, scenario)
        .map_err(|error| RunnerError::Task(error.to_string()))?;
    let mut aggregate = ChurnAggregate::default();

    let result = run_local_churn_inner(
        scenario,
        total_started,
        &mut child,
        &mut cohort,
        &mut aggregate,
    )
    .await;

    if result.is_err() {
        cohort.abort();
        let _ = finish_all_generations(
            &mut cohort,
            &mut aggregate,
            Duration::from_secs(scenario.connect_timeout_seconds.saturating_add(2)),
        )
        .await;
        child.abort().await;
    }

    result
}

async fn run_local_churn_inner(
    scenario: &Scenario,
    total_started: Instant,
    child: &mut ChildTarget,
    cohort: &mut ChurnCohort,
    aggregate: &mut ChurnAggregate,
) -> Result<RunReport, RunnerError> {
    let mut server_sampler = ResourceSampler::for_pid(child.pid);
    let mut loadgen_sampler = ResourceSampler::for_pid(std::process::id());
    let mut ramp_server = ResourceAccumulator::default();
    let mut ramp_loadgen = ResourceAccumulator::default();
    let mut ramp_host = ResourceAccumulator::default();

    collect_churn_initial_handshakes(
        cohort,
        scenario.clients,
        aggregate,
        &mut server_sampler,
        &mut loadgen_sampler,
        &mut ramp_server,
        &mut ramp_loadgen,
        &mut ramp_host,
    )
    .await?;

    let initial_success = aggregate.counts.successful_handshakes == scenario.clients
        && aggregate.counts.failed_handshakes == 0;
    cohort.observe_initial_population(aggregate.counts.successful_handshakes);

    if !initial_success {
        cohort.abort();
        finish_all_generations(
            cohort,
            aggregate,
            Duration::from_secs(scenario.connect_timeout_seconds.saturating_add(2)),
        )
        .await?;
        let server_report = child.stop().await?;
        child.reap().await?;
        add_server_result(aggregate, server_report);

        return Ok(RunReport::assemble(
            collect_environment(),
            scenario.clone(),
            RunReportInput {
                correctness: aggregate.counts,
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
        ));
    }

    if cohort.metrics().admission_headroom() != scenario.churn_admission_headroom()
        || cohort.metrics().server_max_connections() != scenario.benchmark_max_connections()
        || cohort.metrics().target_clients() != scenario.clients
    {
        return Err(RunnerError::Task(
            "churn cohort capacity metadata disagrees with scenario".into(),
        ));
    }

    wait_for_transport_ready(
        child,
        scenario.clients,
        Duration::from_secs(scenario.connect_timeout_seconds.max(2)),
    )
    .await?;

    let _ = server_sampler.sample();
    let _ = loadgen_sampler.sample();

    child.begin_measurement().await?;
    let transport_start = child.snapshot().await?;
    let measured_started = Instant::now();
    let deadline = measured_started + Duration::from_secs(scenario.hold_seconds);
    cohort.measure(deadline);

    let mut steady_server = ResourceAccumulator::default();
    let mut steady_loadgen = ResourceAccumulator::default();
    let mut steady_host = ResourceAccumulator::default();
    let mut transport_samples = Vec::new();

    run_measured_churn(
        scenario,
        measured_started,
        deadline,
        child,
        cohort,
        aggregate,
        &mut server_sampler,
        &mut loadgen_sampler,
        &mut steady_server,
        &mut steady_loadgen,
        &mut steady_host,
        &mut transport_samples,
    )
    .await?;

    let measured_duration_ms = millis(deadline.duration_since(measured_started));

    child.end_measurement().await?;
    let transport_end = child.snapshot().await?;
    cohort.drain();

    let drain_deadline =
        deadline + Duration::from_secs(scenario.connect_timeout_seconds.saturating_mul(2));
    drain_replacements_until(cohort, aggregate, drain_deadline).await?;

    let post_drain_transport = reconcile_post_drain_transport(
        child,
        scenario.clients,
        transport_start.timed_out_sessions,
        Duration::from_secs(scenario.connect_timeout_seconds.max(2)),
    )
    .await?;

    let churn_report = ChurnReport {
        admission_headroom: cohort.metrics().admission_headroom(),
        server_max_connections: cohort.metrics().server_max_connections(),
        planned_disconnects: cohort.metrics().planned_disconnects(),
        completed_planned_disconnects: cohort.metrics().completed_planned_disconnects(),
        replacement_attempts: cohort.metrics().replacement_attempts(),
        replacement_handshakes: cohort.metrics().replacement_handshakes(),
        replacement_failures: cohort.metrics().replacement_failures(),
        replacement_timeouts: cohort.metrics().replacement_timeouts(),
        schedule_misses: cohort.metrics().schedule_misses(),
        population_min: cohort.metrics().population_min(),
        population_max: cohort.metrics().population_max(),
        population_end: cohort.metrics().population_end(),
        replacement_inflight_peak: cohort.metrics().replacement_inflight_peak(),
        replacement_latency: cohort.metrics().replacement_latency_summary(),
        post_drain_transport,
    };

    cohort.shutdown();
    finish_all_generations(
        cohort,
        aggregate,
        Duration::from_secs(scenario.connect_timeout_seconds.saturating_add(2)),
    )
    .await?;

    let server_report = child.stop().await?;
    child.reap().await?;
    add_server_result(aggregate, server_report);

    Ok(RunReport::assemble(
        collect_environment(),
        scenario.clone(),
        RunReportInput {
            correctness: aggregate.counts,
            workload: aggregate.workload,
            latency: aggregate.latency.summary(),
            transport: TransportWindowReport::from_snapshots(
                transport_start,
                transport_end,
                transport_samples,
            ),
            churn: Some(churn_report),
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
    ))
}

#[allow(clippy::too_many_arguments)]
async fn collect_churn_initial_handshakes(
    cohort: &mut ChurnCohort,
    initial_clients: usize,
    aggregate: &mut ChurnAggregate,
    server_sampler: &mut ResourceSampler,
    loadgen_sampler: &mut ResourceSampler,
    ramp_server: &mut ResourceAccumulator,
    ramp_loadgen: &mut ResourceAccumulator,
    ramp_host: &mut ResourceAccumulator,
) -> Result<(), RunnerError> {
    let mut initial_outcomes = 0usize;
    let mut ticker = interval(Duration::from_secs(1));

    while initial_outcomes < initial_clients {
        tokio::select! {
            event = cohort.next_event() => {
                let event = event.map_err(|error| RunnerError::Task(error.to_string()))?;
                if record_churn_event(event, initial_clients, aggregate) {
                    initial_outcomes = initial_outcomes.saturating_add(1);
                }
            }
            _ = ticker.tick() => {
                push_process(ramp_server, server_sampler.sample());
                let loadgen = loadgen_sampler.sample();
                push_process(ramp_loadgen, loadgen);
                push_host(ramp_host, loadgen);
            }
        }
    }

    push_process(ramp_server, server_sampler.sample());
    let loadgen = loadgen_sampler.sample();
    push_process(ramp_loadgen, loadgen);
    push_host(ramp_host, loadgen);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_measured_churn(
    scenario: &Scenario,
    measured_started: Instant,
    deadline: Instant,
    child: &mut ChildTarget,
    cohort: &mut ChurnCohort,
    aggregate: &mut ChurnAggregate,
    server_sampler: &mut ResourceSampler,
    loadgen_sampler: &mut ResourceSampler,
    steady_server: &mut ResourceAccumulator,
    steady_loadgen: &mut ResourceAccumulator,
    steady_host: &mut ResourceAccumulator,
    transport_samples: &mut Vec<TransportMetricsReport>,
) -> Result<(), RunnerError> {
    let churn = scenario
        .churn
        .as_ref()
        .ok_or_else(|| RunnerError::Task("churn config disappeared during run".into()))?;
    let schedule = ChurnSchedule::new(
        churn.replacements_per_second,
        Duration::from_secs(scenario.hold_seconds),
    )
    .map_err(|error| RunnerError::Task(error.to_string()))?;
    let period = Duration::from_secs_f64(1.0 / churn.replacements_per_second);
    let mut next_tick = 0usize;
    let mut ticker = steady_interval(Duration::from_secs(1));

    loop {
        let next_due = schedule
            .due_offset(next_tick)
            .map(|offset| measured_started + offset);
        let tick_deadline = next_due.unwrap_or(deadline);

        tokio::select! {
            biased;
            _ = sleep_until(tick_deadline), if next_due.is_some() => {
                let nominal = next_due.expect("guarded by is_some");
                if Instant::now().saturating_duration_since(nominal) >= period {
                    cohort.note_schedule_miss();
                }
                cohort
                    .schedule_replacement()
                    .map_err(|error| RunnerError::Task(error.to_string()))?;
                next_tick = next_tick.saturating_add(1);
            }
            event = cohort.next_event() => {
                let event = event.map_err(|error| RunnerError::Task(error.to_string()))?;
                record_churn_event(event, scenario.clients, aggregate);
            }
            _ = ticker.tick() => {
                push_process(steady_server, server_sampler.sample());
                let loadgen = loadgen_sampler.sample();
                push_process(steady_loadgen, loadgen);
                push_host(steady_host, loadgen);
                transport_samples.push(child.snapshot().await?);
            }
            _ = sleep_until(deadline) => {
                while next_tick < schedule.planned_ticks() {
                    cohort.note_schedule_miss();
                    next_tick = next_tick.saturating_add(1);
                }
                return Ok(());
            }
        }
    }
}

async fn drain_replacements_until(
    cohort: &mut ChurnCohort,
    aggregate: &mut ChurnAggregate,
    deadline: Instant,
) -> Result<(), RunnerError> {
    while !cohort.ready_for_post_drain_verification() && Instant::now() < deadline {
        tokio::select! {
            event = cohort.next_event() => {
                let event = event.map_err(|error| RunnerError::Task(error.to_string()))?;
                record_churn_event(event, cohort.metrics().target_clients(), aggregate);
            }
            _ = sleep_until(deadline) => break,
        }
    }
    Ok(())
}

async fn reconcile_post_drain_transport(
    child: &mut ChildTarget,
    target_clients: usize,
    baseline_timeouts: u64,
    timeout_duration: Duration,
) -> Result<TransportMetricsReport, RunnerError> {
    let deadline = Instant::now() + timeout_duration;
    let mut sample = child.snapshot().await?;

    while !post_drain_transport_is_healthy(sample, target_clients, baseline_timeouts)
        && Instant::now() < deadline
    {
        sleep(Duration::from_millis(25)).await;
        sample = child.snapshot().await?;
    }

    Ok(sample)
}

async fn finish_all_generations(
    cohort: &mut ChurnCohort,
    aggregate: &mut ChurnAggregate,
    timeout_duration: Duration,
) -> Result<(), RunnerError> {
    let deadline = Instant::now() + timeout_duration;

    while cohort.finished_generations() < cohort.spawned_generations() {
        tokio::select! {
            event = cohort.next_event() => {
                let event = event.map_err(|error| RunnerError::Task(error.to_string()))?;
                record_churn_event(event, cohort.metrics().target_clients(), aggregate);
            }
            _ = sleep_until(deadline) => {
                let missing = cohort
                    .spawned_generations()
                    .saturating_sub(cohort.finished_generations());
                aggregate.counts.unexpected_disconnects = aggregate
                    .counts
                    .unexpected_disconnects
                    .saturating_add(missing);
                break;
            }
        }
    }
    Ok(())
}

fn record_churn_event(
    event: ChurnEvent,
    initial_clients: usize,
    aggregate: &mut ChurnAggregate,
) -> bool {
    match event {
        ChurnEvent::Connect(outcome) => {
            let initial_limit = u64::try_from(initial_clients).unwrap_or(u64::MAX);
            match outcome {
                GenerationConnectOutcome::Ready { client_id } if client_id < initial_limit => {
                    aggregate.counts.successful_handshakes =
                        aggregate.counts.successful_handshakes.saturating_add(1);
                    true
                }
                GenerationConnectOutcome::Failed { client_id, .. } if client_id < initial_limit => {
                    aggregate.counts.failed_handshakes =
                        aggregate.counts.failed_handshakes.saturating_add(1);
                    true
                }
                GenerationConnectOutcome::Ready { .. }
                | GenerationConnectOutcome::Failed { .. } => false,
            }
        }
        ChurnEvent::Finished(result) => {
            merge_client_result(aggregate, result);
            false
        }
    }
}

fn merge_client_result(aggregate: &mut ChurnAggregate, result: ClientTaskResult) {
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
                "transport metrics did not converge before churn measurement: expected {expected} sessions, current={}, started={}, closed={}, timed_out={}",
                snapshot.sessions_current,
                snapshot.sessions_started_total,
                snapshot.sessions_closed_total,
                snapshot.timed_out_sessions,
            )));
        }

        sleep(Duration::from_millis(25)).await;
    }
}

fn add_server_result(aggregate: &mut ChurnAggregate, report: ServerRunReport) {
    aggregate.counts.protocol_errors = aggregate
        .counts
        .protocol_errors
        .saturating_add(report.metrics.protocol_errors_total as usize)
        .saturating_add(report.benchmark_protocol_errors);
    aggregate.counts.send_errors = aggregate
        .counts
        .send_errors
        .saturating_add(report.send_errors);
    aggregate.workload.merge(report.workload);
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

#[derive(Debug, Default)]
struct ChurnAggregate {
    counts: RunCounts,
    workload: WorkloadCounts,
    latency: LatencyHistogram,
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

fn millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}
