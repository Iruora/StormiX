//! Sends a flow's requests in order.

use std::collections::BTreeMap;
use std::error::Error as _;
use std::time::{Duration, Instant};

use reqwest::header::CONTENT_TYPE;
use reqwest::redirect::Policy;
use stormix_flow::{Body, Flow, KeyValue, Request, Step};
use stormix_script::{self as script, Phase, ScriptInput, ScriptRequest, ScriptResponse, TestOutcome};

use crate::checks::{self, Response};
use crate::{Check, FlowResult, StepResult, Variables};

#[derive(Debug, Clone)]
pub struct RunOptions {
    /// For the whole request, connect to last byte.
    pub timeout: Duration,
    /// Accept invalid or self-signed TLS certificates.
    pub insecure: bool,
    pub follow_redirects: bool,
    /// Stop at the first failed step; the rest are reported as skipped.
    pub bail: bool,
    /// For each pre-request or test script.
    pub script_timeout: Duration,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            insecure: false,
            follow_redirects: true,
            bail: false,
            script_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("could not set up the HTTP client: {0}")]
    Client(#[from] reqwest::Error),
}

/// Holds one HTTP client, so connections are reused across steps and runs.
#[derive(Debug, Clone)]
pub struct Runner {
    client: reqwest::Client,
    options: RunOptions,
}

impl Runner {
    pub fn new(options: RunOptions) -> Result<Self, RunnerError> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("stormix/", env!("CARGO_PKG_VERSION")))
            .timeout(options.timeout)
            .redirect(if options.follow_redirects {
                Policy::limited(10)
            } else {
                Policy::none()
            })
            .tls_danger_accept_invalid_certs(options.insecure)
            .build()?;
        Ok(Self { client, options })
    }

    /// Runs every step once. `overrides` win over the flow's own variables.
    pub async fn run(&self, flow: &Flow, overrides: &Variables) -> FlowResult {
        let mut vars = Variables::new();
        vars.extend(flow.variables.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        vars.extend(overrides.entries());

        let started = Instant::now();
        let mut steps = Vec::with_capacity(flow.steps.len());
        let mut stopped = false;
        for step in &flow.steps {
            if stopped {
                steps.push(skipped(step));
                continue;
            }
            let result = self.run_step(step, &mut vars).await;
            stopped = self.options.bail && !result.passed();
            steps.push(result);
        }
        FlowResult {
            name: flow.name.clone(),
            steps,
            duration_ms: millis(started.elapsed()),
        }
    }

    /// Sends one step's request and checks the response. Variables the step
    /// extracts, or its scripts set, are kept in `vars` for the steps after it.
    pub async fn run_step(&self, step: &Step, vars: &mut Variables) -> StepResult {
        let mut checks = Vec::new();
        let mut logs = Vec::new();
        let mut outgoing = Outgoing::from(&step.request);

        // Postman and Bruno run the pre-request script before filling in
        // variables, so what it sets is already used by this request.
        if let Some(source) = &step.scripts.pre_request {
            let before = outgoing.script_view();
            let snapshot = vars.to_map();
            let out = script::run(
                source,
                &ScriptInput {
                    phase: Phase::PreRequest,
                    step_name: &step.name,
                    variables: &snapshot,
                    request: &before,
                    response: None,
                },
                self.options.script_timeout,
            );
            apply_variables(vars, &out.variables);
            outgoing.apply_script(before, out.request);
            checks.extend(script_checks("pre-request script", out.tests, out.error));
            logs.extend(out.logs);
        }

        let mut missing = Vec::new();
        let render = |text: &str, missing: &mut Vec<String>| vars.render_collecting(text, missing);
        let url = render(&outgoing.url, &mut missing);
        let mut result = StepResult {
            name: step.name.clone(),
            method: outgoing.method.clone(),
            url: url.clone(),
            status: None,
            duration_ms: 0.0,
            time_to_headers_ms: None,
            response_bytes: 0,
            checks,
            extracted: BTreeMap::new(),
            warnings: Vec::new(),
            logs,
            error: None,
            skipped: false,
        };
        let method = match reqwest::Method::from_bytes(outgoing.method.as_bytes()) {
            Ok(method) => method,
            Err(_) => {
                result.error = Some(format!("`{}` is not an HTTP method", outgoing.method));
                return result;
            }
        };

        let mut builder = self.client.request(method, &url);
        let mut has_content_type = false;
        for (name, value) in &outgoing.headers {
            has_content_type |= name.eq_ignore_ascii_case("content-type");
            builder = builder.header(name, render(value, &mut missing));
        }
        builder = match &outgoing.body {
            OutBody::None => builder,
            OutBody::Raw(text) => builder.body(render(text, &mut missing)),
            OutBody::Json(value) => {
                let value = vars.render_json(value, &mut missing);
                if !has_content_type {
                    builder = builder.header(CONTENT_TYPE, "application/json");
                }
                builder.body(serde_json::to_vec(&value).expect("a JSON value always serializes"))
            }
            OutBody::Form(fields) => {
                let pairs: Vec<(String, String)> = fields
                    .iter()
                    .map(|(k, v)| (render(k, &mut missing), render(v, &mut missing)))
                    .collect();
                builder.form(&pairs)
            }
            OutBody::Multipart(fields) => {
                let mut form = reqwest::multipart::Form::new();
                for (k, v) in fields {
                    form = form.text(render(k, &mut missing), render(v, &mut missing));
                }
                builder.multipart(form)
            }
        };
        for name in &missing {
            result.warnings.push(format!(
                "`{{{{{name}}}}}` has no value, so it was sent as written"
            ));
        }

        let started = Instant::now();
        let response = match builder.send().await {
            Ok(response) => response,
            Err(err) => {
                result.duration_ms = millis(started.elapsed());
                result.error = Some(describe(&err, self.options.timeout));
                return result;
            }
        };
        result.time_to_headers_ms = Some(millis(started.elapsed()));
        let status = response.status();
        let headers = response.headers().clone();
        let body = match response.bytes().await {
            Ok(body) => body,
            Err(err) => {
                result.duration_ms = millis(started.elapsed());
                result.status = Some(status.as_u16());
                result.error = Some(describe(&err, self.options.timeout));
                return result;
            }
        };
        result.duration_ms = millis(started.elapsed());
        result.status = Some(status.as_u16());
        result.response_bytes = u32::try_from(body.len()).unwrap_or(u32::MAX);

        let res = Response::new(status.as_u16(), &headers, &body, result.duration_ms);
        for assertion in &step.assertions {
            result.checks.push(checks::assert(assertion, &res, vars));
        }
        for (name, source) in &step.extract {
            let (check, value) = checks::extract(name, source, &res);
            result.checks.push(check);
            if let Some(value) = value {
                vars.set(name.clone(), value.clone());
                result.extracted.insert(name.clone(), value);
            }
        }

        if let Some(source) = &step.scripts.test {
            let response = ScriptResponse {
                status: status.as_u16(),
                status_text: status.canonical_reason().unwrap_or_default().to_owned(),
                headers: headers
                    .iter()
                    .map(|(k, v)| (k.as_str().to_owned(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
                    .collect(),
                body: String::from_utf8_lossy(&body).into_owned(),
                time_ms: result.duration_ms,
            };
            let mut request = outgoing.script_view();
            request.url = result.url.clone();
            let snapshot = vars.to_map();
            let out = script::run(
                source,
                &ScriptInput {
                    phase: Phase::Test,
                    step_name: &step.name,
                    variables: &snapshot,
                    request: &request,
                    response: Some(&response),
                },
                self.options.script_timeout,
            );
            for (name, value) in &out.variables {
                if let Some(value) = value {
                    result.extracted.insert(name.clone(), value.clone());
                }
            }
            apply_variables(vars, &out.variables);
            result.checks.extend(script_checks("test script", out.tests, out.error));
            result.logs.extend(out.logs);
        }
        result
    }
}

/// The request being built, in the shape scripts can read and change.
struct Outgoing {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: OutBody,
}

enum OutBody {
    None,
    Json(serde_json::Value),
    Raw(String),
    Form(Vec<(String, String)>),
    Multipart(Vec<(String, String)>),
}

impl From<&Request> for Outgoing {
    fn from(request: &Request) -> Self {
        Self {
            method: request.method.as_str().to_owned(),
            url: request.url.clone(),
            headers: request
                .headers
                .iter()
                .map(|h| (h.name.clone(), h.value.clone()))
                .collect(),
            body: match &request.body {
                None => OutBody::None,
                Some(Body::Json(v)) => OutBody::Json(v.clone()),
                Some(Body::Raw(t)) => OutBody::Raw(t.clone()),
                Some(Body::Form(f)) => OutBody::Form(pairs(f)),
                Some(Body::Multipart(f)) => OutBody::Multipart(pairs(f)),
            },
        }
    }
}

fn pairs(fields: &[KeyValue]) -> Vec<(String, String)> {
    fields
        .iter()
        .map(|kv| (kv.key.clone(), kv.value.clone()))
        .collect()
}

impl Outgoing {
    fn body_text(&self) -> Option<String> {
        match &self.body {
            OutBody::None => None,
            OutBody::Json(v) => Some(v.to_string()),
            OutBody::Raw(t) => Some(t.clone()),
            OutBody::Form(f) | OutBody::Multipart(f) => Some(
                f.iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join("&"),
            ),
        }
    }

    fn script_view(&self) -> ScriptRequest {
        ScriptRequest {
            method: self.method.clone(),
            url: self.url.clone(),
            headers: self.headers.clone(),
            body: self.body_text(),
        }
    }

    /// Takes what a pre-request script changed. A body it rewrote is sent as
    /// raw text, keeping the step's `Content-Type`.
    fn apply_script(&mut self, before: ScriptRequest, after: ScriptRequest) {
        self.method = after.method;
        self.url = after.url;
        self.headers = after.headers;
        if after.body != before.body {
            self.body = after.body.map_or(OutBody::None, OutBody::Raw);
        }
    }
}

fn apply_variables(vars: &mut Variables, changes: &BTreeMap<String, Option<String>>) {
    for (name, value) in changes {
        match value {
            Some(value) => vars.set(name.clone(), value.clone()),
            None => vars.remove(name),
        }
    }
}

/// Script tests become checks under their own names; an exception that
/// stopped the script is a failed check too, so it fails the step.
fn script_checks(label: &str, tests: Vec<TestOutcome>, error: Option<String>) -> Vec<Check> {
    let mut checks: Vec<Check> = tests
        .into_iter()
        .map(|t| Check {
            name: t.name,
            passed: t.passed,
            message: t.message,
        })
        .collect();
    if let Some(error) = error {
        checks.push(Check {
            name: format!("{label} ran"),
            passed: false,
            message: Some(error),
        });
    }
    checks
}

fn skipped(step: &Step) -> StepResult {
    StepResult {
        name: step.name.clone(),
        method: step.request.method.as_str().to_owned(),
        url: step.request.url.clone(),
        status: None,
        duration_ms: 0.0,
        time_to_headers_ms: None,
        response_bytes: 0,
        checks: Vec::new(),
        extracted: BTreeMap::new(),
        warnings: Vec::new(),
        logs: Vec::new(),
        error: None,
        skipped: true,
    }
}

/// reqwest's own message is often just "error sending request"; the cause
/// (connection refused, DNS, TLS) is further down the chain.
fn describe(err: &reqwest::Error, timeout: Duration) -> String {
    if err.is_timeout() {
        return format!("timed out after {} ms", timeout.as_millis());
    }
    let mut message = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        if !message.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = cause.source();
    }
    message
}

fn millis(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
