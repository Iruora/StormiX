//! Compares two runs: the regression view.

use serde::{Deserialize, Serialize};
use stormix_engine::{FlowResult, LoadReport, StepResult};

use crate::RunData;

/// A change of more than this, in percent, counts as better or worse.
const NOISE_PERCENT: f64 = 10.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum RunDiff {
    Functional {
        /// In the newer run's order; steps only the older run had come last.
        steps: Vec<StepDiff>,
        broke: u32,
        fixed: u32,
    },
    Load(LoadDiff),
    /// A functional run against a load test: nothing to line up.
    Mismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum StepChange {
    Broke,
    Fixed,
    StillFailing,
    StillPassing,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct StepDiff {
    pub name: String,
    pub change: StepChange,
    pub before_status: Option<u16>,
    pub after_status: Option<u16>,
    pub before_ms: Option<f64>,
    pub after_ms: Option<f64>,
    /// Only checks that changed, appeared or disappeared.
    pub checks: Vec<CheckChange>,
    /// The newer run's error, when it has one.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CheckChange {
    pub name: String,
    pub before: Option<bool>,
    pub after: Option<bool>,
    /// Why it fails now, if it does.
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LoadDiff {
    pub metrics: Vec<Metric>,
    pub steps: Vec<StepLoadDiff>,
}

/// One figure in both runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Metric {
    pub name: String,
    /// `ms`, `%` or `/s`.
    pub unit: String,
    pub before: f64,
    pub after: f64,
    /// Moved the wrong way by more than 10%.
    pub worse: bool,
    /// Moved the right way by more than 10%.
    pub better: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct StepLoadDiff {
    pub name: String,
    pub before_p95: Option<f64>,
    pub after_p95: Option<f64>,
    pub before_failed: Option<u32>,
    pub after_failed: Option<u32>,
}

/// Compares `before` with `after` (the newer run).
pub fn diff(before: &RunData, after: &RunData) -> RunDiff {
    match (before, after) {
        (RunData::Functional(a), RunData::Functional(b)) => functional(a, b),
        (RunData::Load(a), RunData::Load(b)) => RunDiff::Load(load(a, b)),
        _ => RunDiff::Mismatch,
    }
}

fn functional(before: &FlowResult, after: &FlowResult) -> RunDiff {
    let mut steps = Vec::new();
    for b in &after.steps {
        steps.push(step_diff(&b.name, find(before, &b.name), Some(b)));
    }
    for a in &before.steps {
        if find(after, &a.name).is_none() {
            steps.push(step_diff(&a.name, Some(a), None));
        }
    }
    let count = |change| steps.iter().filter(|s| s.change == change).count() as u32;
    RunDiff::Functional {
        broke: count(StepChange::Broke),
        fixed: count(StepChange::Fixed),
        steps,
    }
}

fn find<'a>(run: &'a FlowResult, name: &str) -> Option<&'a StepResult> {
    run.steps.iter().find(|s| s.name == name)
}

/// A skipped step neither passed nor failed; it counts as not run.
fn ran(step: Option<&StepResult>) -> Option<&StepResult> {
    step.filter(|s| !s.skipped)
}

fn step_diff(name: &str, before: Option<&StepResult>, after: Option<&StepResult>) -> StepDiff {
    let change = match (ran(before), ran(after)) {
        (Some(a), Some(b)) => match (a.passed(), b.passed()) {
            (true, false) => StepChange::Broke,
            (false, true) => StepChange::Fixed,
            (false, false) => StepChange::StillFailing,
            (true, true) => StepChange::StillPassing,
        },
        (None, Some(b)) if before.is_some() => {
            if b.passed() {
                StepChange::StillPassing
            } else {
                StepChange::Broke
            }
        }
        (None, Some(_)) => StepChange::Added,
        (Some(_), None) if after.is_some() => StepChange::StillFailing,
        _ => StepChange::Removed,
    };

    let mut checks = Vec::new();
    if let Some(b) = after {
        for check in &b.checks {
            let was = before.and_then(|a| a.checks.iter().find(|c| c.name == check.name));
            if was.map(|c| c.passed) != Some(check.passed) {
                checks.push(CheckChange {
                    name: check.name.clone(),
                    before: was.map(|c| c.passed),
                    after: Some(check.passed),
                    message: check.message.clone(),
                });
            }
        }
    }
    if let Some(a) = before {
        for check in &a.checks {
            if !after.is_some_and(|b| b.checks.iter().any(|c| c.name == check.name)) {
                checks.push(CheckChange {
                    name: check.name.clone(),
                    before: Some(check.passed),
                    after: None,
                    message: None,
                });
            }
        }
    }

    StepDiff {
        name: name.to_owned(),
        change,
        before_status: before.and_then(|s| s.status),
        after_status: after.and_then(|s| s.status),
        before_ms: ran(before).map(|s| s.duration_ms),
        after_ms: ran(after).map(|s| s.duration_ms),
        checks,
        error: after.and_then(|s| s.error.clone()),
    }
}

fn load(before: &LoadReport, after: &LoadReport) -> LoadDiff {
    let (a, b) = (&before.summary, &after.summary);
    let metric = |name: &str, unit: &str, before: f64, after: f64, lower_is_better: bool| {
        let change = if before.abs() < f64::EPSILON {
            if after.abs() < f64::EPSILON {
                0.0
            } else {
                100.0
            }
        } else {
            (after - before) / before * 100.0
        };
        let improved = if lower_is_better {
            change < 0.0
        } else {
            change > 0.0
        };
        let moved = change.abs() > NOISE_PERCENT;
        Metric {
            name: name.into(),
            unit: unit.into(),
            before,
            after,
            worse: moved && !improved,
            better: moved && improved,
        }
    };
    let metrics = vec![
        metric("Requests/s", "/s", a.rps, b.rps, false),
        metric("p50", "ms", a.latency.p50, b.latency.p50, true),
        metric("p95", "ms", a.latency.p95, b.latency.p95, true),
        metric("p99", "ms", a.latency.p99, b.latency.p99, true),
        metric("Failed requests", "%", a.error_rate(), b.error_rate(), true),
        metric("Checks passed", "%", a.check_rate(), b.check_rate(), false),
    ];

    let mut steps: Vec<StepLoadDiff> = b
        .steps
        .iter()
        .map(|s| {
            let was = a.steps.iter().find(|x| x.name == s.name);
            StepLoadDiff {
                name: s.name.clone(),
                before_p95: was.map(|x| x.latency.p95),
                after_p95: Some(s.latency.p95),
                before_failed: was.map(|x| x.failed),
                after_failed: Some(s.failed),
            }
        })
        .collect();
    for s in &a.steps {
        if !b.steps.iter().any(|x| x.name == s.name) {
            steps.push(StepLoadDiff {
                name: s.name.clone(),
                before_p95: Some(s.latency.p95),
                after_p95: None,
                before_failed: Some(s.failed),
                after_failed: None,
            });
        }
    }
    LoadDiff { metrics, steps }
}
