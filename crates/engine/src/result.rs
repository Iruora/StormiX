//! What a run produced, shaped for reports and the UI.

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FlowResult {
    pub name: String,
    pub steps: Vec<StepResult>,
    pub duration_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    pub name: String,
    pub method: String,
    /// The URL as sent, after variables were filled in.
    pub url: String,
    pub status: Option<u16>,
    /// From sending the request to the last byte of the body.
    pub duration_ms: f64,
    /// From sending the request to the response headers.
    pub time_to_headers_ms: Option<f64>,
    pub response_bytes: u32,
    pub checks: Vec<Check>,
    /// Variables this step set, with their values.
    pub extracted: BTreeMap<String, String>,
    /// Things that did not fail the step but may explain a surprise.
    pub warnings: Vec<String>,
    /// What scripts printed with `console.log`.
    pub logs: Vec<String>,
    /// Set when no response came back: bad URL, connection refused, timeout.
    pub error: Option<String>,
    /// Not run because an earlier step failed and the run stops at the first failure.
    pub skipped: bool,
}

/// One assertion or extraction and how it went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub name: String,
    pub passed: bool,
    /// Why it failed, like `got 404`.
    pub message: Option<String>,
}

impl StepResult {
    pub fn passed(&self) -> bool {
        !self.skipped && self.error.is_none() && self.checks.iter().all(|c| c.passed)
    }
}

impl FlowResult {
    pub fn passed(&self) -> bool {
        self.steps.iter().all(StepResult::passed)
    }

    pub fn failed_steps(&self) -> usize {
        self.steps
            .iter()
            .filter(|s| !s.skipped && !s.passed())
            .count()
    }

    /// (passed, total) over every check of every step that ran.
    pub fn check_counts(&self) -> (usize, usize) {
        let checks = self.steps.iter().flat_map(|s| &s.checks);
        let total = checks.clone().count();
        (checks.filter(|c| c.passed).count(), total)
    }
}
