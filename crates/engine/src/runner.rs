//! Sends a flow's requests in order.

use std::collections::BTreeMap;
use std::error::Error as _;
use std::time::{Duration, Instant};

use reqwest::header::CONTENT_TYPE;
use reqwest::redirect::Policy;
use stormix_flow::{Body, Flow, Step};

use crate::checks::{self, Response};
use crate::{FlowResult, StepResult, Variables};

#[derive(Debug, Clone)]
pub struct RunOptions {
    /// For the whole request, connect to last byte.
    pub timeout: Duration,
    /// Accept invalid or self-signed TLS certificates.
    pub insecure: bool,
    pub follow_redirects: bool,
    /// Stop at the first failed step; the rest are reported as skipped.
    pub bail: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            insecure: false,
            follow_redirects: true,
            bail: false,
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
    /// extracts are set in `vars` for the steps after it.
    pub async fn run_step(&self, step: &Step, vars: &mut Variables) -> StepResult {
        let mut missing = Vec::new();
        let mut warnings = Vec::new();
        let render = |text: &str, missing: &mut Vec<String>| vars.render_collecting(text, missing);

        let request = &step.request;
        let url = render(&request.url, &mut missing);
        let method = reqwest::Method::from_bytes(request.method.as_str().as_bytes())
            .expect("every Method is a valid HTTP method");
        let mut builder = self.client.request(method, &url);
        let mut has_content_type = false;
        for header in &request.headers {
            has_content_type |= header.name.eq_ignore_ascii_case("content-type");
            builder = builder.header(&header.name, render(&header.value, &mut missing));
        }
        builder = match &request.body {
            None => builder,
            Some(Body::Raw(text)) => builder.body(render(text, &mut missing)),
            Some(Body::Json(value)) => {
                let value = vars.render_json(value, &mut missing);
                if !has_content_type {
                    builder = builder.header(CONTENT_TYPE, "application/json");
                }
                builder.body(serde_json::to_vec(&value).expect("a JSON value always serializes"))
            }
            Some(Body::Form(fields)) => {
                let pairs: Vec<(String, String)> = fields
                    .iter()
                    .map(|f| (render(&f.key, &mut missing), render(&f.value, &mut missing)))
                    .collect();
                builder.form(&pairs)
            }
        };
        for name in &missing {
            warnings.push(format!(
                "`{{{{{name}}}}}` has no value, so it was sent as written"
            ));
        }
        if !step.scripts.is_empty() {
            warnings.push("pre-request and test scripts do not run yet; they arrive in M2".into());
        }

        let mut result = StepResult {
            name: step.name.clone(),
            method: request.method.as_str().to_owned(),
            url,
            status: None,
            duration_ms: 0.0,
            time_to_headers_ms: None,
            response_bytes: 0,
            checks: Vec::new(),
            extracted: BTreeMap::new(),
            warnings,
            error: None,
            skipped: false,
        };

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
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let body = match response.bytes().await {
            Ok(body) => body,
            Err(err) => {
                result.duration_ms = millis(started.elapsed());
                result.status = Some(status);
                result.error = Some(describe(&err, self.options.timeout));
                return result;
            }
        };
        result.duration_ms = millis(started.elapsed());
        result.status = Some(status);
        result.response_bytes = u32::try_from(body.len()).unwrap_or(u32::MAX);

        let res = Response::new(status, &headers, &body, result.duration_ms);
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
        result
    }
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
