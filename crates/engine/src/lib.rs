//! Runs a [`Flow`](stormix_flow::Flow) once, step by step: renders each
//! request from the variables, sends it, extracts new variables from the
//! response and checks its assertions.
//!
//! The load engine (M3) will drive this same step runner from many virtual
//! users; the CLI and the desktop app use it directly for functional runs.

mod checks;
mod junit;
mod result;
mod runner;
mod variables;

pub use junit::junit_report;
pub use result::{Check, FlowResult, StepResult};
pub use runner::{RunOptions, Runner, RunnerError};
pub use variables::Variables;
