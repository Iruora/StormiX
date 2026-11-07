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
    /// A Dynatrace environment, like `https://abc12345.live.dynatrace.com`
    /// (or `https://host/e/{environment-id}` for Managed), read through its
    /// Metrics API v2 for the processes `entity` selects. Needs a `token`
    /// with the `metrics.read` scope.
    Dynatrace(MonitorTarget),
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
    /// Time between reads. 1s when left out (30s for Dynatrace).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "specta", specta(type = Option<String>))]
    pub interval: Option<Duration>,
    /// A health check read at the same time, like `/actuator/health` or
    /// `/healthz`, to spot the API going down during the test. Actuator
    /// reads `/health` under its URL when this is left out. `{{variables}}`
    /// work, and the headers above are sent with it too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<String>,
    /// An API token, sent as `Authorization: Api-Token …` to Dynatrace and
    /// as `Authorization: Bearer …` to the other endpoints. Use a
    /// `{{variable}}` so the token stays a secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Dynatrace only: an entity selector for the API's processes, like
    /// `type("PROCESS_GROUP_INSTANCE"),entityName.contains("orders")`.
    /// Their CPU and memory are summed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
}

impl Monitor {
    /// The endpoint settings, whatever the kind.
    pub fn target(&self) -> &MonitorTarget {
        match self {
            Self::Prometheus(t) | Self::Actuator(t) | Self::Dynatrace(t) => t,
        }
    }

    /// `prometheus` or `actuator`, as written in the file.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Prometheus(_) => "prometheus",
            Self::Actuator(_) => "actuator",
            Self::Dynatrace(_) => "dynatrace",
        }
    }
}

impl Monitor {
    /// The time between reads: 1 s by default and at least 250 ms, or for
    /// Dynatrace (whose data comes by the minute) 30 s and at least 10 s.
    pub fn every(&self) -> std::time::Duration {
        let (default, least) = match self {
            Self::Dynatrace(_) => (30_000, 10_000),
            _ => (1000, 250),
        };
        let millis = self
            .target()
            .interval
            .map_or(default, |d| d.as_millis())
            .max(least);
        std::time::Duration::from_millis(millis)
    }
}
