use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Scenario {
    pub name: String,
    pub clients: usize,
    pub protocol_version: u8,
    pub ramp_up_seconds: u64,
    pub hold_seconds: u64,
    pub connect_timeout_seconds: u64,
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
            return Err(ScenarioError::Invalid {
                field: "clients",
                message: "must be at least 1",
            });
        }

        if self.hold_seconds == 0 {
            return Err(ScenarioError::Invalid {
                field: "hold_seconds",
                message: "must be at least 1",
            });
        }

        if self.connect_timeout_seconds == 0 {
            return Err(ScenarioError::Invalid {
                field: "connect_timeout_seconds",
                message: "must be at least 1",
            });
        }

        Ok(())
    }
}
