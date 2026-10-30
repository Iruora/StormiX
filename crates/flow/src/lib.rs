//! The Stormix flow model and test file format.
//!
//! A [`Flow`] is an ordered list of HTTP requests that pass values to each
//! other through variables. Every importer (Postman, Bruno) produces one, and
//! the runner, the load engine and the UI only ever deal with this model.
//!
//! A [`TestFile`] is what `stormix run` and `stormix load` read: a flow, an
//! optional load profile and the thresholds that decide pass or fail.

mod duration;
mod load;
mod model;
mod template;
mod test_file;
mod threshold;
mod validate;

pub use duration::{Duration, DurationError};
pub use load::{LoadProfile, Stage};
pub use model::{
    Assertion, Body, ExtractSource, Flow, Header, HeaderAssertion, JsonAssertion, KeyValue, Method,
    Request, Scripts, Step,
};
pub use template::{render, variable_refs, Rendered};
pub use test_file::{ParseError, TestFile, CURRENT_VERSION};
pub use threshold::{Comparison, Metric, Threshold, ThresholdError};
pub use validate::{Issue, Severity};
