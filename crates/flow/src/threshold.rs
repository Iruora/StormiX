//! Pass/fail rules such as `p95 < 300ms` or `errors < 1%`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Metric {
    /// A latency percentile, 0 to 100 (`p95`).
    Percentile(u8),
    AvgLatency,
    MaxLatency,
    /// Share of failed requests, in percent.
    ErrorRate,
    /// Share of passed assertions and script tests, in percent.
    CheckRate,
    /// Requests per second over the whole run.
    Throughput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Comparison {
    Lt,
    Le,
    Gt,
    Ge,
}

/// Latencies are kept in milliseconds, rates in percent, throughput in
/// requests per second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Threshold {
    pub metric: Metric,
    pub comparison: Comparison,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid threshold `{input}`: {reason}")]
pub struct ThresholdError {
    input: String,
    reason: &'static str,
}

impl Metric {
    fn is_latency(self) -> bool {
        matches!(
            self,
            Self::Percentile(_) | Self::AvgLatency | Self::MaxLatency
        )
    }

    fn is_rate(self) -> bool {
        matches!(self, Self::ErrorRate | Self::CheckRate)
    }

    /// Only measured under load, not on a single functional run.
    pub fn needs_load(self) -> bool {
        matches!(self, Self::Throughput)
    }
}

impl Comparison {
    pub fn holds(self, actual: f64, limit: f64) -> bool {
        match self {
            Self::Lt => actual < limit,
            Self::Le => actual <= limit,
            Self::Gt => actual > limit,
            Self::Ge => actual >= limit,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
        }
    }
}

impl Threshold {
    pub fn passes(&self, actual: f64) -> bool {
        self.comparison.holds(actual, self.value)
    }
}

impl FromStr for Threshold {
    type Err = ThresholdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = |reason| ThresholdError {
            input: s.to_owned(),
            reason,
        };
        let op_at = s
            .find(['<', '>'])
            .ok_or_else(|| err("expected <, <=, > or >="))?;
        let (lhs, rest) = s.split_at(op_at);
        let (comparison, rhs) = if let Some(r) = rest.strip_prefix("<=") {
            (Comparison::Le, r)
        } else if let Some(r) = rest.strip_prefix(">=") {
            (Comparison::Ge, r)
        } else if let Some(r) = rest.strip_prefix('<') {
            (Comparison::Lt, r)
        } else {
            (Comparison::Gt, &rest[1..])
        };

        let metric = match lhs.trim() {
            "avg" => Metric::AvgLatency,
            "max" => Metric::MaxLatency,
            "errors" => Metric::ErrorRate,
            "checks" => Metric::CheckRate,
            "rps" => Metric::Throughput,
            other => {
                let p = other
                    .strip_prefix('p')
                    .and_then(|n| n.parse::<u8>().ok())
                    .filter(|p| (1..=100).contains(p))
                    .ok_or_else(|| {
                        err("unknown metric; use p50, p95, p99, avg, max, errors, checks or rps")
                    })?;
                Metric::Percentile(p)
            }
        };

        let rhs = rhs.trim();
        let number_end = rhs
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rhs.len());
        let (number, unit) = rhs.split_at(number_end);
        let number: f64 = number.parse().map_err(|_| err("expected a number"))?;
        let value = match (unit.trim(), metric) {
            ("ms", m) if m.is_latency() => number,
            ("s", m) if m.is_latency() => number * 1000.0,
            (_, m) if m.is_latency() => return Err(err("latency needs a unit, ms or s")),
            ("%", m) if m.is_rate() => number,
            (_, m) if m.is_rate() => return Err(err("rates are in percent, like 1%")),
            ("", Metric::Throughput) => number,
            _ => return Err(err("rps takes a plain number")),
        };
        Ok(Self {
            metric,
            comparison,
            value,
        })
    }
}

impl fmt::Display for Threshold {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.metric {
            Metric::Percentile(p) => {
                return write!(f, "p{p} {} {}ms", self.comparison.as_str(), self.value)
            }
            Metric::AvgLatency => "avg",
            Metric::MaxLatency => "max",
            Metric::ErrorRate => "errors",
            Metric::CheckRate => "checks",
            Metric::Throughput => "rps",
        };
        let unit = match self.metric {
            m if m.is_latency() => "ms",
            m if m.is_rate() => "%",
            _ => "",
        };
        write!(
            f,
            "{name} {} {}{unit}",
            self.comparison.as_str(),
            self.value
        )
    }
}

impl Serialize for Threshold {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Threshold {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Threshold {
        s.parse().unwrap()
    }

    #[test]
    fn parses_latency_with_units() {
        assert_eq!(
            t("p95 < 300ms"),
            Threshold {
                metric: Metric::Percentile(95),
                comparison: Comparison::Lt,
                value: 300.0
            }
        );
        assert_eq!(t("max<=1.5s").value, 1500.0);
        assert_eq!(t("avg >= 10ms").comparison, Comparison::Ge);
    }

    #[test]
    fn parses_rates_and_throughput() {
        assert_eq!(t("errors < 1%").metric, Metric::ErrorRate);
        assert_eq!(t("checks > 99.5%").value, 99.5);
        assert_eq!(t("rps >= 100").metric, Metric::Throughput);
    }

    #[test]
    fn rejects_wrong_units_and_metrics() {
        for bad in [
            "p95 < 300",
            "errors < 1",
            "rps > 10ms",
            "p0 < 1ms",
            "latency < 1ms",
            "p95 = 1ms",
            "p95 < fast",
        ] {
            assert!(bad.parse::<Threshold>().is_err(), "{bad} should not parse");
        }
    }

    #[test]
    fn round_trips_through_display() {
        for s in ["p95 < 300ms", "max <= 1500ms", "errors < 1%", "rps >= 100"] {
            assert_eq!(t(s).to_string(), s);
        }
    }

    #[test]
    fn compares() {
        assert!(t("p95 < 300ms").passes(299.9));
        assert!(!t("p95 < 300ms").passes(300.0));
        assert!(t("rps >= 100").passes(100.0));
    }
}
