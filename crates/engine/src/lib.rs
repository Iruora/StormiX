//! Runs a [`Flow`](stormix_flow::Flow) once, step by step: renders each
//! request from the variables, sends it, extracts new variables from the
//! response and checks its assertions.
//!
//! [`run_load`] drives the same runner from many concurrent iterations,
//! following a test file's load profile, and measures the result.

mod cache;
mod checks;
mod junit;
mod load;
mod result;
mod runner;
mod variables;

pub use cache::{CachedStep, ResponseCache};
pub use junit::{junit_load_report, junit_report};
pub use load::{run_load, LoadOptions, LoadReport, Progress};
pub use result::{Check, Exchange, FlowResult, HeaderPair, Payload, ResponseRecord, StepResult};
pub use runner::{Exchanges, RunOptions, Runner, RunnerError};
pub use variables::Variables;
