use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ChurnError {
    #[error("invalid churn rate")]
    InvalidRate,
    #[error("client id space exhausted")]
    ClientIdExhausted,
    #[error("initial population does not fit in u64 client ids")]
    InitialPopulationTooLarge,
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
