//! What a `find_limit` test sizes for: the traffic expected in production
//! and how much room to leave, so the report can say how many pods (or
//! containers, or servers) that traffic needs.

use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct Capacity {
    /// Requests per second at the busiest time in production, all pods
    /// together.
    pub peak_rps: f64,
    /// Traffic growth to plan for, in percent: 30 sizes for 1.3 × the peak.
    #[serde(default = "default_growth")]
    #[cfg_attr(feature = "specta", specta(type = f64))]
    pub growth: Percent,
    /// The CPU each pod should run at during the peak, in percent of what
    /// the pod may use. Below 100 leaves room for spikes, garbage
    /// collection and a pod restarting.
    #[serde(default = "default_target_cpu")]
    #[cfg_attr(feature = "specta", specta(type = f64))]
    pub target_cpu: Percent,
    /// How many pods served the test. The limit found is divided by it.
    #[serde(default = "one")]
    pub tested_pods: u32,
    /// CPU cores one pod may use (its CPU limit). Read from the monitor
    /// when it says, else taken as 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pod_cores: Option<f64>,
    /// Extra pods on top of what the traffic needs, so one can fail or be
    /// redeployed without overloading the rest.
    #[serde(default = "one")]
    pub spare_pods: u32,
    /// Pods running in production today, to compare with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_pods: Option<u32>,
}

fn default_growth() -> Percent {
    Percent(30.0)
}

fn default_target_cpu() -> Percent {
    Percent(65.0)
}

fn one() -> u32 {
    1
}

/// A percentage, written `30`, `30%` or `"30 %"`; saved as a number.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Percent(pub f64);

impl std::ops::Deref for Percent {
    type Target = f64;

    fn deref(&self) -> &f64 {
        &self.0
    }
}

impl Serialize for Percent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(self.0)
    }
}

impl<'de> Deserialize<'de> for Percent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Value {
            Number(f64),
            Text(String),
        }
        match Value::deserialize(deserializer)? {
            Value::Number(n) => Ok(Self(n)),
            Value::Text(s) => s
                .trim()
                .trim_end_matches('%')
                .trim()
                .parse()
                .map(Self)
                .map_err(|_| {
                    serde::de::Error::custom(format!("expected a percentage like 30%, not `{s}`"))
                }),
        }
    }
}
