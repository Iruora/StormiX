//! How much traffic a load test sends.

use serde::{Deserialize, Serialize};

use crate::Duration;

/// Open models (`constant_rate`, `ramping`, `find_limit`) start flow
/// iterations on a clock, so a slow API does not lower the load. The closed model (`virtual_users`)
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
    /// Raises the rate step by step until a threshold fails, to find how
    /// much one instance of the API sustains. Each step is checked on its
    /// own: the last one where every threshold held is the limit.
    FindLimit {
        /// The rate of the first step.
        start_rate: u32,
        /// Added to the rate at each new step.
        increase: u32,
        /// How long each step lasts.
        #[cfg_attr(feature = "specta", specta(type = String))]
        every: Duration,
        /// The highest rate tried; the test ends after that step.
        max_rate: u32,
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
            Self::FindLimit { every, .. } => {
                Duration::from_millis(every.as_millis() * u64::from(self.limit_steps()))
            }
        }
    }

    /// `find_limit` only: how many steps it takes at most, from the start
    /// rate to the max rate. 0 for the other profiles.
    pub fn limit_steps(&self) -> u32 {
        match self {
            Self::FindLimit {
                start_rate,
                increase,
                max_rate,
                ..
            } => {
                if *increase == 0 || max_rate <= start_rate {
                    1
                } else {
                    (max_rate - start_rate).div_ceil(*increase) + 1
                }
            }
            _ => 0,
        }
    }

    /// `find_limit` only: the rate of step `index`, counted from 0, never
    /// above the max rate.
    pub fn limit_rate(&self, index: u32) -> u32 {
        match self {
            Self::FindLimit {
                start_rate,
                increase,
                max_rate,
                ..
            } => start_rate
                .saturating_add(increase.saturating_mul(index))
                .min((*max_rate).max(*start_rate)),
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_limit_steps_end_at_the_max_rate() {
        let p = LoadProfile::FindLimit {
            start_rate: 10,
            increase: 20,
            every: Duration::from_secs(30),
            max_rate: 75,
        };
        assert_eq!(p.limit_steps(), 5);
        let rates: Vec<u32> = (0..5).map(|i| p.limit_rate(i)).collect();
        assert_eq!(rates, [10, 30, 50, 70, 75]);
        assert_eq!(p.total_duration(), Duration::from_secs(150));
    }
}
