//! Flows, steps and requests.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// An ordered list of requests run one after the other by one virtual user.
/// Values move between steps through variables: `{{name}}` in a request is
/// replaced at send time, and a step's `extract` sets variables for the steps
/// after it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct Flow {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Flow-level variables. Environment and CLI values override them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, String>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub name: String,
    pub request: Request,
    /// Variables to set from the response, by variable name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extract: BTreeMap<String, ExtractSource>,
    /// Declarative checks on the response. Script tests are reported alongside.
    #[serde(default, rename = "assert", skip_serializing_if = "Vec::is_empty")]
    pub assertions: Vec<Assertion>,
    #[serde(default, skip_serializing_if = "Scripts::is_empty")]
    pub scripts: Scripts,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub method: Method,
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<Header>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Body>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }
}

/// Headers are a list, not a map: HTTP allows repeats and order can matter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Body {
    /// Sent as `application/json` unless a `Content-Type` header says otherwise.
    Json(
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Unknown))]
        serde_json::Value,
    ),
    /// Sent as-is, with the step's own `Content-Type` header.
    Raw(String),
    /// Sent as `application/x-www-form-urlencoded`.
    Form(Vec<KeyValue>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum ExtractSource {
    /// A JSONPath into the response body, like `$.data.token`.
    Json(String),
    /// A response header, matched case-insensitively.
    Header(String),
    /// A regex over the response body; the first capture group, else the whole match.
    Regex(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Assertion {
    /// The status code equals this value.
    Status(u16),
    /// The response arrives within this many milliseconds.
    MaxDurationMs(u32),
    Header(HeaderAssertion),
    Json(JsonAssertion),
    /// The response body contains this text.
    BodyContains(String),
}

/// With neither `equals` nor `contains`, checks the header is present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct HeaderAssertion {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equals: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,
}

/// With neither `equals` nor `exists`, checks the path matches something.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct JsonAssertion {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Unknown>))]
    pub equals: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exists: Option<bool>,
}

/// JavaScript carried over from Postman or Bruno, run in an embedded engine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct Scripts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_request: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<String>,
}

impl Scripts {
    pub fn is_empty(&self) -> bool {
        self.pre_request.is_none() && self.test.is_none()
    }
}
