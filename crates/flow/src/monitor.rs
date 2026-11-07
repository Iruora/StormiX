//! Where a load test reads the API's own CPU and memory use from.

use serde::{Deserialize, Serialize};

use crate::{Duration, Header};

/// A metrics endpoint the API serves, read throughout a load test so its
/// CPU and memory show beside the latency they cause.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Monitor {
    /// A Prometheus text endpoint, like `/actuator/prometheus` (Spring
    /// Boot with Micrometer), `/metrics` (prometheus-net, prom-client,
    /// client_golang) or `/q/metrics` (Quarkus).
    Prometheus(MonitorTarget),
    /// Spring Boot Actuator's base URL, like `https://api/actuator`. Reads
    /// `/prometheus` under it when exposed, else the JSON at `/metrics`.
    Actuator(MonitorTarget),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct MonitorTarget {
    /// Can use `{{variables}}`, filled in from the flow and environment.
    pub url: String,
    /// What to call it in results, like `orders-api`. The URL's host when
    /// left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Sent with every read, for an endpoint behind a token. Values can
    /// use `{{variables}}`, so the token can stay a secret.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<Header>,
    /// Time between reads. 1s when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "specta", specta(type = Option<String>))]
    pub interval: Option<Duration>,
    /// A health check read at the same time, like `/actuator/health` or
    /// `/healthz`, to spot the API going down during the test. Actuator
    /// reads `/health` under its URL when this is left out. `{{variables}}`
    /// work, and the headers above are sent with it too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<String>,
}

impl Monitor {
    pub fn target(&self) -> &MonitorTarget {
        match self {
            Self::Prometheus(t) | Self::Actuator(t) => t,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Prometheus(_) => "prometheus",
            Self::Actuator(_) => "actuator",
        }
    }
}

impl MonitorTarget {
    /// The time between reads, at least 250 ms.
    pub fn every(&self) -> std::time::Duration {
        let millis = self.interval.map_or(1000, |d| d.as_millis()).max(250);
        std::time::Duration::from_millis(millis)
    }
}
