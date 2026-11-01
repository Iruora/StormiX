//! What a load test measures: latency histograms (HDR, so percentiles stay
//! exact to three significant digits at any volume), request and check
//! counts, a per-second timeline for live charts, and the threshold verdicts.
//!
//! A [`Recorder`] is fed one [`Sample`] per request and asked to close each
//! second; [`Recorder::summary`] gives the [`Summary`] that reports and
//! [`check_thresholds`] work from.

use std::collections::BTreeMap;
use std::time::Duration;

use hdrhistogram::Histogram;
use serde::{Deserialize, Serialize};
use stormix_flow::{Metric, Threshold};

/// Latencies are kept in microseconds, up to an hour.
const MAX_MICROS: u64 = 3_600_000_000;

/// How many distinct failure reasons a summary lists.
const TOP_FAILURES: usize = 10;

/// One request, as the load engine saw it.
#[derive(Debug, Clone, Copy)]
pub struct Sample<'a> {
    pub step: &'a str,
    pub duration_ms: f64,
    /// Set when no response came back.
    pub error: Option<&'a str>,
    pub status: Option<u16>,
    pub checks_passed: u32,
    /// Names of the checks that failed.
    pub failed_checks: &'a [&'a str],
    pub bytes: u32,
}

impl Sample<'_> {
    /// No response, a failed check, or (when nothing checks it) a 4xx or 5xx
    /// status. A step that asserts `status: 404` and gets it has not failed.
    pub fn failed(&self) -> bool {
        self.error.is_some()
            || !self.failed_checks.is_empty()
            || (self.checks_passed == 0 && self.status.is_some_and(|s| s >= 400))
    }
}

#[derive(Debug, Clone)]
struct Stats {
    latency: Histogram<u64>,
    /// Exact, where the histogram rounds to its bucket.
    min_ms: f64,
    max_ms: f64,
    sum_ms: f64,
    requests: u64,
    failed: u64,
    checks_passed: u64,
    checks_failed: u64,
    bytes: u64,
    /// Responses reused from the cache, not sent for, so not requests.
    cached: u64,
}

impl Stats {
    fn new() -> Self {
        Self {
            latency: Histogram::new_with_bounds(1, MAX_MICROS, 3).expect("valid histogram bounds"),
            min_ms: f64::INFINITY,
            max_ms: 0.0,
            sum_ms: 0.0,
            requests: 0,
            failed: 0,
            checks_passed: 0,
            checks_failed: 0,
            bytes: 0,
            cached: 0,
        }
    }

    fn record(&mut self, sample: &Sample<'_>) {
        // Failed connections still count as requests, but their time says
        // nothing about the API, so it stays out of the latency figures.
        if sample.error.is_none() {
            let micros = (sample.duration_ms * 1000.0)
                .round()
                .clamp(1.0, MAX_MICROS as f64) as u64;
            self.latency.saturating_record(micros);
            self.min_ms = self.min_ms.min(sample.duration_ms);
            self.max_ms = self.max_ms.max(sample.duration_ms);
            self.sum_ms += sample.duration_ms;
        }
        self.requests += 1;
        self.failed += u64::from(sample.failed());
        self.checks_passed += u64::from(sample.checks_passed);
        self.checks_failed += sample.failed_checks.len() as u64;
        self.bytes += u64::from(sample.bytes);
    }

    fn latency(&self) -> Latency {
        let h = &self.latency;
        let ms = |micros: u64| micros as f64 / 1000.0;
        if h.is_empty() {
            return Latency::default();
        }
        Latency {
            min: self.min_ms,
            avg: self.sum_ms / h.len() as f64,
            p50: ms(h.value_at_quantile(0.50)),
            p90: ms(h.value_at_quantile(0.90)),
            p95: ms(h.value_at_quantile(0.95)),
            p99: ms(h.value_at_quantile(0.99)),
            max: self.max_ms,
        }
    }

    fn percentile(&self, p: f64) -> f64 {
        if self.latency.is_empty() {
            0.0
        } else {
            self.latency.value_at_quantile(p / 100.0) as f64 / 1000.0
        }
    }
}

/// Latency figures in milliseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Latency {
    pub min: f64,
    pub avg: f64,
    pub p50: f64,
    pub p90: f64,
    pub p95: f64,
    pub p99: f64,
    pub max: f64,
}

/// One second of the run, for live charts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Second {
    /// Seconds since the start; this second ended at `t`.
    pub t: u32,
    pub requests: u32,
    pub failed: u32,
    pub p50_ms: f64,
    pub p95_ms: f64,
    /// Iterations in flight when the second ended.
    pub active: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct StepSummary {
    pub name: String,
    pub requests: u32,
    pub failed: u32,
    pub latency: Latency,
    /// Times the step reused a cached response instead of sending a
    /// request. Not counted in `requests` or the latency figures.
    #[serde(default)]
    pub cached: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    /// The connection error, or `step: check` for a failed check.
    pub reason: String,
    pub count: u32,
}

/// Counts are u32 so they cross to TypeScript as numbers; a load test that
/// sends four billion requests from one machine is not on the roadmap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub duration_ms: f64,
    pub iterations: u32,
    pub failed_iterations: u32,
    /// Open models only: iterations that were due but not started because
    /// too many were already in flight.
    pub dropped_iterations: u32,
    /// Still running when the test ended and the grace period ran out.
    pub interrupted_iterations: u32,
    pub requests: u32,
    pub failed_requests: u32,
    /// Responses reused from the cache by steps marked `cache: true`; no
    /// request went out for them, so they are not in `requests`.
    #[serde(default)]
    pub cached_responses: u32,
    /// Requests per second over the whole run.
    pub rps: f64,
    pub latency: Latency,
    pub checks_passed: u32,
    pub checks_failed: u32,
    pub bytes_received: f64,
    pub steps: Vec<StepSummary>,
    /// The most common reasons requests failed, most frequent first.
    pub failures: Vec<Failure>,
    pub timeline: Vec<Second>,
}

impl Summary {
    /// Failed requests, in percent.
    pub fn error_rate(&self) -> f64 {
        percent(self.failed_requests, self.requests)
    }

    /// Passed checks, in percent; 100 when nothing was checked.
    pub fn check_rate(&self) -> f64 {
        let total = self.checks_passed + self.checks_failed;
        if total == 0 {
            100.0
        } else {
            percent(self.checks_passed, total)
        }
    }
}

fn percent(part: u32, whole: u32) -> f64 {
    if whole == 0 {
        0.0
    } else {
        f64::from(part) * 100.0 / f64::from(whole)
    }
}

#[derive(Debug, Clone)]
pub struct Recorder {
    total: Stats,
    /// In the order steps were first seen, which is flow order.
    steps: Vec<(String, Stats)>,
    second: Stats,
    timeline: Vec<Second>,
    failures: BTreeMap<String, u32>,
    iterations: u32,
    failed_iterations: u32,
    dropped_iterations: u32,
    interrupted_iterations: u32,
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            total: Stats::new(),
            steps: Vec::new(),
            second: Stats::new(),
            timeline: Vec::new(),
            failures: BTreeMap::new(),
            iterations: 0,
            failed_iterations: 0,
            dropped_iterations: 0,
            interrupted_iterations: 0,
        }
    }

    pub fn record(&mut self, sample: &Sample<'_>) {
        self.total.record(sample);
        self.second.record(sample);
        match self.steps.iter_mut().find(|(name, _)| name == sample.step) {
            Some((_, stats)) => stats.record(sample),
            None => {
                let mut stats = Stats::new();
                stats.record(sample);
                self.steps.push((sample.step.to_owned(), stats));
            }
        }
        if let Some(error) = sample.error {
            self.fail(error.to_owned());
        }
        for check in sample.failed_checks {
            self.fail(format!("{}: {check}", sample.step));
        }
        if sample.error.is_none() && sample.failed_checks.is_empty() && sample.failed() {
            if let Some(status) = sample.status {
                self.fail(format!("{}: status {status}", sample.step));
            }
        }
    }

    /// A step that reused a cached response. It shows in the step table but
    /// adds no request, latency or check counts.
    pub fn cached(&mut self, step: &str) {
        self.total.cached += 1;
        match self.steps.iter_mut().find(|(name, _)| name == step) {
            Some((_, stats)) => stats.cached += 1,
            None => {
                let mut stats = Stats::new();
                stats.cached = 1;
                self.steps.push((step.to_owned(), stats));
            }
        }
    }

    fn fail(&mut self, reason: String) {
        // Bounded, so a URL with a random id in every error cannot grow it
        // without limit; the rest still count in `failed_requests`.
        if self.failures.len() < 1000 || self.failures.contains_key(&reason) {
            *self.failures.entry(reason).or_default() += 1;
        }
    }

    pub fn iteration(&mut self, passed: bool) {
        self.iterations += 1;
        self.failed_iterations += u32::from(!passed);
    }

    pub fn dropped(&mut self, count: u32) {
        self.dropped_iterations += count;
    }

    pub fn interrupted(&mut self, count: u32) {
        self.interrupted_iterations += count;
    }

    /// Ends the current second and returns it.
    pub fn close_second(&mut self, active: u32) -> &Second {
        let second = std::mem::replace(&mut self.second, Stats::new());
        self.timeline.push(Second {
            t: self.timeline.len() as u32 + 1,
            requests: second.requests as u32,
            failed: second.failed as u32,
            p50_ms: second.percentile(50.0),
            p95_ms: second.percentile(95.0),
            active,
        });
        self.timeline.last().expect("just pushed")
    }

    pub fn requests(&self) -> u64 {
        self.total.requests
    }

    pub fn summary(&self, elapsed: Duration) -> Summary {
        let secs = elapsed.as_secs_f64();
        let mut failures: Vec<Failure> = self
            .failures
            .iter()
            .map(|(reason, &count)| Failure {
                reason: reason.clone(),
                count,
            })
            .collect();
        failures.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.reason.cmp(&b.reason)));
        failures.truncate(TOP_FAILURES);
        Summary {
            duration_ms: secs * 1000.0,
            iterations: self.iterations,
            failed_iterations: self.failed_iterations,
            dropped_iterations: self.dropped_iterations,
            interrupted_iterations: self.interrupted_iterations,
            requests: self.total.requests as u32,
            failed_requests: self.total.failed as u32,
            cached_responses: self.total.cached as u32,
            rps: if secs > 0.0 {
                self.total.requests as f64 / secs
            } else {
                0.0
            },
            latency: self.total.latency(),
            checks_passed: self.total.checks_passed as u32,
            checks_failed: self.total.checks_failed as u32,
            bytes_received: self.total.bytes as f64,
            steps: self
                .steps
                .iter()
                .map(|(name, stats)| StepSummary {
                    name: name.clone(),
                    requests: stats.requests as u32,
                    failed: stats.failed as u32,
                    latency: stats.latency(),
                    cached: stats.cached as u32,
                })
                .collect(),
            failures,
            timeline: self.timeline.clone(),
        }
    }

    /// The exact percentile, which the summary only gives for p50 to p99.
    pub fn percentile(&self, p: f64) -> f64 {
        self.total.percentile(p)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ThresholdResult {
    /// As written, like `p95 < 300ms`.
    pub threshold: String,
    /// In the threshold's unit: ms, percent or requests per second.
    pub actual: f64,
    pub passed: bool,
}

/// Checks each threshold against the run. Percentiles are read from the
/// recorder, so `p97` is as exact as `p95`.
pub fn check_thresholds(
    thresholds: &[Threshold],
    summary: &Summary,
    recorder: &Recorder,
) -> Vec<ThresholdResult> {
    thresholds
        .iter()
        .map(|t| {
            let actual = match t.metric {
                Metric::Percentile(p) => recorder.percentile(f64::from(p)),
                Metric::AvgLatency => summary.latency.avg,
                Metric::MaxLatency => summary.latency.max,
                Metric::ErrorRate => summary.error_rate(),
                Metric::CheckRate => summary.check_rate(),
                Metric::Throughput => summary.rps,
            };
            ThresholdResult {
                threshold: t.to_string(),
                actual,
                passed: t.passes(actual),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
