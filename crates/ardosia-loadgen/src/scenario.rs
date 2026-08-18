use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Scenario {
    pub name: String,
    pub clients: usize,
    pub protocol_version: u8,
    pub ramp_up_seconds: u64,
    pub hold_seconds: u64,
    pub connect_timeout_seconds: u64,
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default)]
    pub traffic: Vec<TrafficSpec>,
    #[serde(default)]
    pub rtt: Option<RttConfig>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrafficKind {
    Unreliable,
    ReliableOrdered,
    FragmentedReliableOrdered,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    ClientToServer,
    ServerToClient,
    Bidirectional,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrafficSpec {
    pub kind: TrafficKind,
    pub direction: Direction,
    pub packets_per_second_per_client: f64,
    pub payload_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RttConfig {
    pub probes_per_second_per_client: f64,
    pub payload_bytes: usize,
}

#[derive(Debug, Error)]
pub enum ScenarioError {
    #[error("invalid TOML scenario: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("invalid scenario field {field}: {message}")]
    Invalid {
        field: &'static str,
        message: &'static str,
    },
}

impl FromStr for Scenario {
    type Err = ScenarioError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let scenario: Self = toml::from_str(input)?;
        scenario.validate()?;
        Ok(scenario)
    }
}

impl Scenario {
    pub fn validate(&self) -> Result<(), ScenarioError> {
        if self.clients == 0 {
            return invalid("clients", "must be at least 1");
        }

        if self.hold_seconds == 0 {
            return invalid("hold_seconds", "must be at least 1");
        }

        if self.connect_timeout_seconds == 0 {
            return invalid("connect_timeout_seconds", "must be at least 1");
        }

        for traffic in &self.traffic {
            if !traffic.packets_per_second_per_client.is_finite()
                || traffic.packets_per_second_per_client <= 0.0
            {
                return invalid(
                    "packets_per_second_per_client",
                    "must be finite and greater than 0",
                );
            }
            if traffic.payload_bytes == 0 {
                return invalid("payload_bytes", "must be at least 1");
            }
        }

        if let Some(rtt) = &self.rtt {
            if !rtt.probes_per_second_per_client.is_finite()
                || rtt.probes_per_second_per_client <= 0.0
            {
                return invalid(
                    "probes_per_second_per_client",
                    "must be finite and greater than 0",
                );
            }
            if rtt.payload_bytes == 0 {
                return invalid("payload_bytes", "must be at least 1");
            }
        }

        Ok(())
    }
}

fn default_seed() -> u64 {
    1
}

fn invalid<T>(field: &'static str, message: &'static str) -> Result<T, ScenarioError> {
    Err(ScenarioError::Invalid { field, message })
}
