//! How much traffic a load test sends.

use serde::{Deserialize, Serialize};

use crate::Duration;

/// Open models (`constant_rate`, `ramping`) start flow iterations on a clock,
/// so a slow API does not lower the load. The closed model (`virtual_users`)
/// keeps a fixed number of users looping the flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum LoadProfile {
    ConstantRate {
        /// Flow iterations started per second.
        rate: u32,
        #[cfg_attr(feature = "specta", specta(type = String))]
        duration: Duration,
    },
    Ramping {
        /// The rate at the start of the first stage.
        #[serde(default)]
        start_rate: u32,
        stages: Vec<Stage>,
    },
    VirtualUsers {
        count: u32,
        #[cfg_attr(feature = "specta", specta(type = String))]
        duration: Duration,
        /// Pause between iterations of one user.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "specta", specta(type = Option<String>))]
        think_time: Option<Duration>,
    },
}

/// Moves the rate linearly to `target` over `duration`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct Stage {
    #[cfg_attr(feature = "specta", specta(type = String))]
    pub duration: Duration,
    pub target: u32,
}

impl LoadProfile {
    /// How long the whole test runs.
    pub fn total_duration(&self) -> Duration {
        match self {
            Self::ConstantRate { duration, .. } | Self::VirtualUsers { duration, .. } => *duration,
            Self::Ramping { stages, .. } => {
                Duration::from_millis(stages.iter().map(|s| s.duration.as_millis()).sum::<u64>())
            }
        }
    }
}
