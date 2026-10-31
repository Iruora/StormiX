//! What a run produced, shaped for reports and the UI.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FlowResult {
    pub name: String,
    pub steps: Vec<StepResult>,
    pub duration_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// What was sent and what came back. Kept for every step of a single
    /// run; under load, only for failed steps. Missing from runs saved
    /// before 1.1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchange: Option<Exchange>,
}

/// One request and its response, as they went over the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Exchange {
    /// Headers as sent, including the ones Stormix adds like `Content-Type`.
    pub request_headers: Vec<HeaderPair>,
    pub request_body: Option<Payload>,
    /// `None` when no response came back.
    pub response: Option<ResponseRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ResponseRecord {
    /// Like `Not Found`; empty for codes without a standard reason.
    pub status_text: String,
    pub headers: Vec<HeaderPair>,
    pub body: Option<Payload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct HeaderPair {
    pub name: String,
    pub value: String,
}

/// A body, as text. Binary bodies are described rather than kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Payload {
    pub text: String,
    /// The full size, which can be more than `text` holds.
    pub bytes: u32,
    /// `text` stops at [`Payload::LIMIT`] bytes.
    pub truncated: bool,
    /// Not UTF-8; `text` only describes it.
    pub binary: bool,
}

impl Payload {
    /// Bodies kept for the Details view stop here, so saved runs stay small.
    pub const LIMIT: usize = 64 * 1024;

    pub fn new(bytes: &[u8]) -> Self {
        let size = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        let kept = &bytes[..bytes.len().min(Self::LIMIT)];
        // A cut can land inside a multi-byte character; keep what decodes.
        let text = match std::str::from_utf8(kept) {
            Ok(text) => Some(text),
            Err(err) if kept.len() < bytes.len() && err.error_len().is_none() => {
                std::str::from_utf8(&kept[..err.valid_up_to()]).ok()
            }
            Err(_) => None,
        };
        match text {
            Some(text) => Self {
                text: text.to_owned(),
                bytes: size,
                truncated: kept.len() < bytes.len(),
                binary: false,
            },
            None => Self {
                text: format!("{} bytes of binary data", bytes.len()),
                bytes: size,
                truncated: false,
                binary: true,
            },
        }
    }
}

/// One assertion or extraction and how it went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
