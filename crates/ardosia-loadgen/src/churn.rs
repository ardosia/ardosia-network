use std::net::SocketAddr;
use std::time::Duration;

use thiserror::Error;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use crate::client_task::{
    ClientTaskResult, GenerationConnectOutcome, GenerationDirective, Phase, run_client_generation,
};
use crate::latency::{LatencyHistogram, LatencySummary};
use crate::report::TransportMetricsReport;
use crate::scenario::Scenario;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ChurnError {
    #[error("invalid churn rate")]
    InvalidRate,
    #[error("client id space exhausted")]
    ClientIdExhausted,
    #[error("initial population does not fit in u64 client ids")]
    InitialPopulationTooLarge,
    #[error("churn scenario is missing [churn] configuration")]
    MissingConfig,
    #[error("churn event referenced unknown client id {0}")]
    UnknownClient(u64),
    #[error("invalid churn slot state for client id {0}")]
    InvalidSlotState(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisconnectIntent {
    PlannedChurn,
    FinalShutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisconnectOutcome {
    Clean,
    Failed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DisconnectCounts {
    pub completed_planned_disconnects: usize,
    pub clean_disconnects: usize,
    pub unexpected_disconnects: usize,
}

pub fn classify_disconnect(
    intent: DisconnectIntent,
    outcome: DisconnectOutcome,
) -> DisconnectCounts {
    match (intent, outcome) {
        (DisconnectIntent::PlannedChurn, DisconnectOutcome::Clean) => DisconnectCounts {
            completed_planned_disconnects: 1,
            ..DisconnectCounts::default()
        },
        (DisconnectIntent::FinalShutdown, DisconnectOutcome::Clean) => DisconnectCounts {
            clean_disconnects: 1,
            ..DisconnectCounts::default()
        },
        (_, DisconnectOutcome::Failed) => DisconnectCounts {
            unexpected_disconnects: 1,
            ..DisconnectCounts::default()
        },
    }
}

pub fn post_drain_transport_is_healthy(
    sample: TransportMetricsReport,
    target_clients: usize,
    baseline_timeouts: u64,
) -> bool {
    sample.sessions_current == u64::try_from(target_clients).unwrap_or(u64::MAX)
        && sample.timed_out_sessions == baseline_timeouts
}

#[derive(Debug, Clone)]
pub struct ChurnSchedule {
    replacements_per_second: f64,
    planned_ticks: usize,
}

impl ChurnSchedule {
    pub fn new(replacements_per_second: f64, hold: Duration) -> Result<Self, ChurnError> {
        if !replacements_per_second.is_finite() || replacements_per_second <= 0.0 {
            return Err(ChurnError::InvalidRate);
        }

        let planned = (replacements_per_second * hold.as_secs_f64()).floor();
        let planned_ticks = if planned >= usize::MAX as f64 {
            usize::MAX
        } else {
            planned as usize
        };

        Ok(Self {
            replacements_per_second,
            planned_ticks,
        })
    }

    pub fn planned_ticks(&self) -> usize {
        self.planned_ticks
    }

    pub fn due_offset(&self, tick: usize) -> Option<Duration> {
        if tick >= self.planned_ticks {
            return None;
        }
        Some(Duration::from_secs_f64(
            (tick as f64 + 1.0) / self.replacements_per_second,
        ))
    }
}

#[derive(Debug, Clone)]
pub struct ClientIdAllocator {
    next: u64,
}

impl ClientIdAllocator {
    pub fn after_initial_population(clients: usize) -> Result<Self, ChurnError> {
        let next = u64::try_from(clients).map_err(|_| ChurnError::InitialPopulationTooLarge)?;
        Ok(Self { next })
    }

    pub fn next_id(&mut self) -> Result<u64, ChurnError> {
        let id = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or(ChurnError::ClientIdExhausted)?;
        Ok(id)
    }
}

#[derive(Debug, Default, Clone)]
pub struct SlotSelector {
    next_index: usize,
}

impl SlotSelector {
    pub fn select(&mut self, eligible: &[bool]) -> Option<usize> {
        if eligible.is_empty() {
            return None;
        }

        for offset in 0..eligible.len() {
            let index = (self.next_index + offset) % eligible.len();
            if eligible[index] {
                self.next_index = (index + 1) % eligible.len();
                return Some(index);
            }
        }
        None
    }
}

#[derive(Debug, Clone)]
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

impl ChurnRunMetrics {
    pub fn for_target(
        target_clients: usize,
        admission_headroom: usize,
        server_max_connections: usize,
        planned_disconnects: usize,
    ) -> Self {
        Self {
            target_clients,
            admission_headroom,
            server_max_connections,
            planned_disconnects: u64::try_from(planned_disconnects).unwrap_or(u64::MAX),
            completed_planned_disconnects: 0,
            replacement_attempts: 0,
            replacement_handshakes: 0,
            replacement_failures: 0,
            replacement_timeouts: 0,
            schedule_misses: 0,
            population_current: 0,
            population_min: 0,
            population_max: 0,
            replacement_inflight: 0,
            replacement_inflight_peak: 0,
            replacement_latency: LatencyHistogram::default(),
        }
    }

    pub fn observe_initial_population(&mut self, population: usize) {
        self.population_current = population;
        self.population_min = population;
        self.population_max = population;
    }

    pub fn planned_disconnect_started(&mut self) {
        self.population_current = self.population_current.saturating_sub(1);
        self.population_min = self.population_min.min(self.population_current);
    }

    pub fn completed_planned_disconnect(&mut self) {
        self.completed_planned_disconnects = self.completed_planned_disconnects.saturating_add(1);
    }

    pub fn replacement_attempt_started(&mut self) {
        self.replacement_attempts = self.replacement_attempts.saturating_add(1);
        self.replacement_inflight = self.replacement_inflight.saturating_add(1);
        self.replacement_inflight_peak = self
            .replacement_inflight_peak
            .max(self.replacement_inflight);
    }

    pub fn replacement_connected(&mut self, latency: Duration) {
        self.replacement_handshakes = self.replacement_handshakes.saturating_add(1);
        self.replacement_inflight = self.replacement_inflight.saturating_sub(1);
        self.population_current = self.population_current.saturating_add(1);
        self.population_max = self.population_max.max(self.population_current);
        self.replacement_latency.record(latency);
    }

    pub fn replacement_failed(&mut self, timed_out: bool) {
        self.replacement_failures = self.replacement_failures.saturating_add(1);
        if timed_out {
            self.replacement_timeouts = self.replacement_timeouts.saturating_add(1);
        }
        self.replacement_inflight = self.replacement_inflight.saturating_sub(1);
    }

    pub fn schedule_miss(&mut self) {
        self.schedule_misses = self.schedule_misses.saturating_add(1);
    }

    pub fn target_clients(&self) -> usize {
        self.target_clients
    }

    pub fn admission_headroom(&self) -> usize {
        self.admission_headroom
    }

    pub fn server_max_connections(&self) -> usize {
        self.server_max_connections
    }

    pub fn planned_disconnects(&self) -> u64 {
        self.planned_disconnects
    }

    pub fn completed_planned_disconnects(&self) -> u64 {
        self.completed_planned_disconnects
    }

    pub fn replacement_attempts(&self) -> u64 {
        self.replacement_attempts
    }

    pub fn replacement_handshakes(&self) -> u64 {
        self.replacement_handshakes
    }

    pub fn population_current(&self) -> usize {
        self.population_current
    }

    pub fn population_min(&self) -> usize {
        self.population_min
    }

    pub fn population_max(&self) -> usize {
        self.population_max
    }

    pub fn population_end(&self) -> usize {
        self.population_current
    }

    pub fn replacement_inflight(&self) -> usize {
        self.replacement_inflight
    }

    pub fn replacement_inflight_peak(&self) -> usize {
        self.replacement_inflight_peak
    }

    pub fn replacement_failures(&self) -> u64 {
        self.replacement_failures
    }

    pub fn replacement_timeouts(&self) -> u64 {
        self.replacement_timeouts
    }

    pub fn schedule_misses(&self) -> u64 {
        self.schedule_misses
    }

    pub fn replacement_latency_summary(&self) -> LatencySummary {
        self.replacement_latency.summary()
    }
}

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

pub(crate) enum ChurnEvent {
    Connect(GenerationConnectOutcome),
    Finished(ClientTaskResult),
}

pub(crate) struct ChurnCohort {
    target: SocketAddr,
    scenario: Scenario,
    phase_tx: watch::Sender<Phase>,
    connect_tx: mpsc::Sender<GenerationConnectOutcome>,
    connect_rx: mpsc::Receiver<GenerationConnectOutcome>,
    result_tx: mpsc::Sender<ClientTaskResult>,
    result_rx: mpsc::Receiver<ClientTaskResult>,
    slots: Vec<Slot>,
    selector: SlotSelector,
    ids: ClientIdAllocator,
    metrics: ChurnRunMetrics,
    spawned_generations: usize,
    finished_generations: usize,
}

impl ChurnCohort {
    pub(crate) fn spawn(target: SocketAddr, scenario: &Scenario) -> Result<Self, ChurnError> {
        let churn = scenario.churn.as_ref().ok_or(ChurnError::MissingConfig)?;
        let schedule = ChurnSchedule::new(
            churn.replacements_per_second,
            Duration::from_secs(scenario.hold_seconds),
        )?;
        let capacity = scenario.benchmark_max_connections().max(1);
        let (phase_tx, _) = watch::channel(Phase::Ramp);
        let (connect_tx, connect_rx) = mpsc::channel(capacity);
        let (result_tx, result_rx) = mpsc::channel(capacity);
        let ids = ClientIdAllocator::after_initial_population(scenario.clients)?;
        let metrics = ChurnRunMetrics::for_target(
            scenario.clients,
            scenario.churn_admission_headroom(),
            scenario.benchmark_max_connections(),
            schedule.planned_ticks(),
        );

        let mut cohort = Self {
            target,
            scenario: scenario.clone(),
            phase_tx,
            connect_tx,
            connect_rx,
            result_tx,
            result_rx,
            slots: Vec::with_capacity(scenario.clients),
            selector: SlotSelector::default(),
            ids,
            metrics,
            spawned_generations: 0,
            finished_generations: 0,
        };

        for index in 0..scenario.clients {
            let client_id =
                u64::try_from(index).map_err(|_| ChurnError::InitialPopulationTooLarge)?;
            let (directive_tx, directive_rx) = watch::channel(GenerationDirective::Continue);
            cohort.slots.push(Slot {
                state: SlotState::ConnectingInitial,
                client_id,
                directive_tx,
                replacement_started: None,
            });
            cohort.spawn_generation_task(
                client_id,
                stagger_delay(index, scenario.clients, scenario.ramp_up_seconds),
                directive_rx,
            );
        }

        Ok(cohort)
    }

    pub(crate) fn measure(&self, deadline: Instant) {
        let _ = self.phase_tx.send(Phase::Measure { deadline });
    }

    pub(crate) fn drain(&self) {
        let _ = self.phase_tx.send(Phase::Drain);
    }

    pub(crate) fn shutdown(&self) {
        let _ = self.phase_tx.send(Phase::Shutdown);
    }

    pub(crate) fn abort(&self) {
        let _ = self.phase_tx.send(Phase::Abort);
    }

    pub(crate) fn observe_initial_population(&mut self, population: usize) {
        self.metrics.observe_initial_population(population);
    }

    pub(crate) fn note_schedule_miss(&mut self) {
        self.metrics.schedule_miss();
    }

    pub(crate) fn schedule_replacement(&mut self) -> Result<bool, ChurnError> {
        let eligible: Vec<bool> = self
            .slots
            .iter()
            .map(|slot| slot.state == SlotState::Active)
            .collect();
        let Some(index) = self.selector.select(&eligible) else {
            self.metrics.schedule_miss();
            return Ok(false);
        };

        let slot = &mut self.slots[index];
        if slot
            .directive_tx
            .send(GenerationDirective::PlannedDisconnect)
            .is_err()
        {
            self.metrics.schedule_miss();
            return Ok(false);
        }

        slot.state = SlotState::PlannedDisconnect;
        self.metrics.planned_disconnect_started();
        Ok(true)
    }

    pub(crate) async fn next_event(&mut self) -> Result<ChurnEvent, ChurnError> {
        tokio::select! {
            biased;
            outcome = self.connect_rx.recv() => {
                let outcome = outcome.expect("churn connect channel is held by cohort");
                self.handle_connect(outcome)?;
                Ok(ChurnEvent::Connect(outcome))
            }
            result = self.result_rx.recv() => {
                let result = result.expect("churn result channel is held by cohort");
                self.finished_generations = self.finished_generations.saturating_add(1);
                self.handle_finished(&result)?;
                Ok(ChurnEvent::Finished(result))
            }
        }
    }

    pub(crate) fn ready_for_post_drain_verification(&self) -> bool {
        self.metrics.completed_planned_disconnects == self.metrics.planned_disconnects
            && self.metrics.replacement_attempts == self.metrics.planned_disconnects
            && self.metrics.replacement_handshakes == self.metrics.replacement_attempts
            && self.metrics.replacement_inflight == 0
            && self.metrics.population_current == self.metrics.target_clients
    }

    pub(crate) fn metrics(&self) -> &ChurnRunMetrics {
        &self.metrics
    }

    pub(crate) fn spawned_generations(&self) -> usize {
        self.spawned_generations
    }

    pub(crate) fn finished_generations(&self) -> usize {
        self.finished_generations
    }

    fn handle_connect(&mut self, outcome: GenerationConnectOutcome) -> Result<(), ChurnError> {
        let client_id = match outcome {
            GenerationConnectOutcome::Ready { client_id }
            | GenerationConnectOutcome::Failed { client_id, .. } => client_id,
        };
        let index = self
            .slot_index(client_id)
            .ok_or(ChurnError::UnknownClient(client_id))?;
        let slot = &mut self.slots[index];

        match outcome {
            GenerationConnectOutcome::Ready { .. } => match slot.state {
                SlotState::ConnectingInitial => {
                    slot.state = SlotState::Active;
                }
                SlotState::ConnectingReplacement => {
                    let latency = slot
                        .replacement_started
                        .take()
                        .map_or(Duration::ZERO, |started| started.elapsed());
                    slot.state = SlotState::Active;
                    self.metrics.replacement_connected(latency);
                }
                _ => return Err(ChurnError::InvalidSlotState(client_id)),
            },
            GenerationConnectOutcome::Failed { timed_out, .. } => match slot.state {
                SlotState::ConnectingInitial => {
                    slot.state = SlotState::Failed;
                }
                SlotState::ConnectingReplacement => {
                    slot.state = SlotState::Failed;
                    slot.replacement_started = None;
                    self.metrics.replacement_failed(timed_out);
                }
                _ => return Err(ChurnError::InvalidSlotState(client_id)),
            },
        }
        Ok(())
    }

    fn handle_finished(&mut self, result: &ClientTaskResult) -> Result<(), ChurnError> {
        let client_id = result.client_id;
        let index = self
            .slot_index(client_id)
            .ok_or(ChurnError::UnknownClient(client_id))?;

        match self.slots[index].state {
            SlotState::PlannedDisconnect if result.completed_planned_disconnects == 1 => {
                self.metrics.completed_planned_disconnect();
                if matches!(*self.phase_tx.borrow(), Phase::Shutdown | Phase::Abort) {
                    self.slots[index].state = SlotState::Failed;
                    self.slots[index].replacement_started = None;
                    return Ok(());
                }
                let replacement_id = self.ids.next_id()?;
                self.metrics.replacement_attempt_started();
                let replacement_started = Instant::now();
                let (directive_tx, directive_rx) = watch::channel(GenerationDirective::Continue);
                self.slots[index] = Slot {
                    state: SlotState::ConnectingReplacement,
                    client_id: replacement_id,
                    directive_tx,
                    replacement_started: Some(replacement_started),
                };
                self.spawn_generation_task(replacement_id, Duration::ZERO, directive_rx);
            }
            SlotState::PlannedDisconnect => {
                self.slots[index].state = SlotState::Failed;
                self.slots[index].replacement_started = None;
            }
            SlotState::Active
            | SlotState::ConnectingInitial
            | SlotState::ConnectingReplacement
            | SlotState::Failed => {
                self.slots[index].state = SlotState::Failed;
                self.slots[index].replacement_started = None;
            }
        }

        Ok(())
    }

    fn slot_index(&self, client_id: u64) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| slot.client_id == client_id)
    }

    fn spawn_generation_task(
        &mut self,
        client_id: u64,
        stagger: Duration,
        directive_rx: watch::Receiver<GenerationDirective>,
    ) {
        let target = self.target;
        let scenario = self.scenario.clone();
        let phase_rx = self.phase_tx.subscribe();
        let outcome_tx = self.connect_tx.clone();
        let result_tx = self.result_tx.clone();
        self.spawned_generations = self.spawned_generations.saturating_add(1);

        tokio::spawn(async move {
            let result = run_client_generation(
                target,
                client_id,
                scenario,
                stagger,
                phase_rx,
                directive_rx,
                outcome_tx,
            )
            .await;
            let _ = result_tx.send(result).await;
        });
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
