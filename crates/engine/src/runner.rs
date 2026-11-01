//! Sends a flow's requests in order.

use std::collections::BTreeMap;
use std::error::Error as _;
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, CONTENT_TYPE};
use reqwest::redirect::Policy;
use stormix_flow::{Body, Flow, KeyValue, Request, Step};
use stormix_script::{
    self as script, Phase, ScriptInput, ScriptRequest, ScriptResponse, TestOutcome,
};

use crate::cache::{Key, ResponseCache};
use crate::checks::{self, Response};
use crate::result::{Exchange, HeaderPair, Payload, ResponseRecord};
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
    /// Which steps keep the request and response they exchanged.
    pub exchanges: Exchanges,
}

/// Keeping headers and bodies costs a copy per request, which a load test
/// only pays for the requests worth looking at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exchanges {
    All,
    FailedOnly,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            insecure: false,
            follow_redirects: true,
            bail: false,
            script_timeout: Duration::from_secs(5),
            exchanges: Exchanges::All,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("could not set up the HTTP client: {0}")]
    Client(#[from] reqwest::Error),
}

/// Holds one HTTP client, so connections are reused across steps and runs,
/// and the responses kept for steps marked `cache: true`.
#[derive(Debug, Clone)]
pub struct Runner {
    client: reqwest::Client,
    options: RunOptions,
    cache: Arc<ResponseCache>,
}

/// A step about to run, as [`Runner::run_with_progress`] reports it.
#[derive(Debug, Clone, Copy)]
pub struct StepStart<'a> {
    /// From 0, among the steps that run.
    pub index: usize,
    pub total: usize,
    pub name: &'a str,
}

impl Runner {
    pub fn new(options: RunOptions) -> Result<Self, RunnerError> {
        let client = build_client(&options)?;
        Ok(Self {
            client,
            options,
            cache: Arc::default(),
        })
    }

    /// The responses kept for cached steps, shared by every clone of this
    /// runner. Clear it to make those steps send their requests again.
    pub fn cache(&self) -> &ResponseCache {
        &self.cache
    }

    /// Sends a request for `bru.sendRequest` / `pm.sendRequest`, with the
    /// same client settings, taking no longer than the script has left.
    /// Scripts run on a blocking thread, so this blocks too.
    fn send_for_script(
        &self,
        call: &ScriptRequest,
        left: Duration,
    ) -> Result<ScriptResponse, String> {
        let timeout = left.min(self.options.timeout);
        match tokio::runtime::Handle::try_current() {
            // Inside `block_in_place`, so the runtime can drive this.
            Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
                handle.block_on(script_exchange(&self.client, call, timeout))
            }
            // A single-threaded runtime is busy running the script, so use
            // a runtime and client of our own on another thread.
            _ => std::thread::scope(|scope| {
                scope
                    .spawn(|| {
                        let runtime = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .map_err(|e| e.to_string())?;
                        let client = build_client(&self.options).map_err(|e| e.to_string())?;
                        runtime.block_on(script_exchange(&client, call, timeout))
                    })
                    .join()
                    .unwrap_or_else(|_| Err("the request thread failed".into()))
            }),
        }
    }

    /// The same client and connection pool, with other exchange keeping.
    pub(crate) fn keeping(&self, exchanges: Exchanges) -> Self {
        let mut runner = self.clone();
        runner.options.exchanges = exchanges;
        runner
    }

    /// Runs every enabled step once. `overrides` win over the flow's own
    /// variables. Disabled steps are left out of the result entirely.
    pub async fn run(&self, flow: &Flow, overrides: &Variables) -> FlowResult {
        self.run_with_progress(flow, overrides, |_| {}).await
    }

    /// Like [`Runner::run`], calling `on_step` as each step starts.
    pub async fn run_with_progress(
        &self,
        flow: &Flow,
        overrides: &Variables,
        mut on_step: impl FnMut(StepStart<'_>),
    ) -> FlowResult {
        let mut vars = Variables::new();
        vars.extend(flow.variables.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        vars.extend(overrides.entries());

        let started = Instant::now();
        let mut steps = Vec::with_capacity(flow.steps.len());
        let mut stopped = false;
        let total = flow.enabled_steps().count();
        for (index, step) in flow.enabled_steps().enumerate() {
            if stopped {
                steps.push(skipped(step));
                continue;
            }
            on_step(StepStart {
                index,
                total,
                name: &step.name,
            });
            let result = self.run_step(&flow.name, step, &mut vars).await;
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
    /// A step marked `cache: true` takes a kept response for the same request
    /// instead of sending it; `flow` names the flow it is kept under.
    pub async fn run_step(&self, flow: &str, step: &Step, vars: &mut Variables) -> StepResult {
        let mut checks = Vec::new();
        let mut logs = Vec::new();
        let mut outgoing = Outgoing::from(&step.request);

        // Postman and Bruno run the pre-request script before filling in
        // variables, so what it sets is already used by this request.
        if let Some(source) = &step.scripts.pre_request {
            let before = outgoing.script_view();
            let snapshot = vars.to_map();
            let send = |call: &ScriptRequest, left: Duration| self.send_for_script(call, left);
            let out = blocking(|| {
                script::run_with(
                    source,
                    &ScriptInput {
                        phase: Phase::PreRequest,
                        step_name: &step.name,
                        variables: &snapshot,
                        request: &before,
                        response: None,
                    },
                    self.options.script_timeout,
                    Some(&send),
                )
            });
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
            cached: false,
            exchange: None,
        };
        let method = match reqwest::Method::from_bytes(outgoing.method.as_bytes()) {
            Ok(method) => method,
            Err(_) => {
                result.error = Some(format!("`{}` is not an HTTP method", outgoing.method));
                return result;
            }
        };

        let mut builder = self.client.request(method, &url);
        let mut multipart_text = None;
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
                let mut shown = Vec::with_capacity(fields.len());
                for (k, v) in fields {
                    let (k, v) = (render(k, &mut missing), render(v, &mut missing));
                    shown.push(format!("{k}={v}"));
                    form = form.text(k, v);
                }
                // The form is streamed, so it cannot be read back once built.
                multipart_text = Some(shown.join("\n"));
                builder.multipart(form)
            }
        };
        for name in &missing {
            result.warnings.push(format!(
                "`{{{{{name}}}}}` has no value, so it was sent as written"
            ));
        }

        let request = match builder.build() {
            Ok(request) => request,
            Err(err) => {
                result.error = Some(describe(&err, self.options.timeout));
                return result;
            }
        };
        let cache_key = step
            .cache
            .then(|| Key::new(flow, &step.name, &request, multipart_text.as_deref()));
        let sent = Sent::of(&request, multipart_text);

        let started = Instant::now();
        let kept = cache_key.as_ref().and_then(|key| self.cache.get(key));
        let (status, headers, body) = if let Some(kept) = kept {
            result.cached = true;
            result.time_to_headers_ms = Some(millis(started.elapsed()));
            (kept.status, kept.headers.clone(), kept.body.clone())
        } else {
            let response = match self.client.execute(request).await {
                Ok(response) => response,
                Err(err) => {
                    result.duration_ms = millis(started.elapsed());
                    result.error = Some(describe(&err, self.options.timeout));
                    result.exchange = Some(sent.into_exchange(None));
                    return result;
                }
            };
            result.time_to_headers_ms = Some(millis(started.elapsed()));
            let status = response.status();
            let headers = response.headers().clone();
            match response.bytes().await {
                Ok(body) => (status, headers, body),
                Err(err) => {
                    result.duration_ms = millis(started.elapsed());
                    result.status = Some(status.as_u16());
                    result.error = Some(describe(&err, self.options.timeout));
                    result.exchange = Some(sent.into_exchange(Some(ResponseRecord {
                        status_text: status.canonical_reason().unwrap_or_default().to_owned(),
                        headers: header_pairs(&headers),
                        body: None,
                    })));
                    return result;
                }
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
                    .map(|(k, v)| {
                        (
                            k.as_str().to_owned(),
                            String::from_utf8_lossy(v.as_bytes()).into_owned(),
                        )
                    })
                    .collect(),
                body: String::from_utf8_lossy(&body).into_owned(),
                time_ms: result.duration_ms,
            };
            let mut request = outgoing.script_view();
            request.url = result.url.clone();
            let snapshot = vars.to_map();
            let send = |call: &ScriptRequest, left: Duration| self.send_for_script(call, left);
            let out = blocking(|| {
                script::run_with(
                    source,
                    &ScriptInput {
                        phase: Phase::Test,
                        step_name: &step.name,
                        variables: &snapshot,
                        request: &request,
                        response: Some(&response),
                    },
                    self.options.script_timeout,
                    Some(&send),
                )
            });
            for (name, value) in &out.variables {
                if let Some(value) = value {
                    result.extracted.insert(name.clone(), value.clone());
                }
            }
            apply_variables(vars, &out.variables);
            result
                .checks
                .extend(script_checks("test script", out.tests, out.error));
            result.logs.extend(out.logs);
        }
        // Only a response that passed is worth reusing: a failed login kept
        // would fail every run after it until cleared. As under load, a 4xx
        // or 5xx nothing checked counts as failed.
        if let Some(key) = cache_key {
            let unchecked_error = result.checks.is_empty() && status.as_u16() >= 400;
            if !result.cached && result.passed() && !unchecked_error {
                self.cache.put(key, status, headers.clone(), body.clone());
            }
        }
        // Under load an unchecked 4xx or 5xx counts as failed too.
        if self.options.exchanges == Exchanges::All || !result.passed() || status.as_u16() >= 400 {
            result.exchange = Some(sent.into_exchange(Some(ResponseRecord {
                status_text: status.canonical_reason().unwrap_or_default().to_owned(),
                headers: header_pairs(&headers),
                body: (!body.is_empty()).then(|| Payload::new(&body)),
            })));
        }
        result
    }
}

/// The request as built, held until the step knows whether to keep it.
/// Cloning a request built from bytes shares the body rather than copying it.
struct Sent {
    copy: Option<reqwest::Request>,
    /// Streamed bodies cannot be cloned; their headers and text are kept instead.
    headers: Option<HeaderMap>,
    multipart: Option<String>,
}

impl Sent {
    fn of(request: &reqwest::Request, multipart: Option<String>) -> Self {
        let copy = request.try_clone();
        Self {
            headers: copy.is_none().then(|| request.headers().clone()),
            copy,
            multipart,
        }
    }

    fn into_exchange(self, response: Option<ResponseRecord>) -> Exchange {
        let empty = HeaderMap::new();
        let headers = match (&self.copy, &self.headers) {
            (Some(copy), _) => copy.headers(),
            (None, Some(headers)) => headers,
            (None, None) => &empty,
        };
        let body = self
            .copy
            .as_ref()
            .and_then(|c| c.body())
            .and_then(|b| b.as_bytes());
        let mut request_headers = header_pairs(headers);
        // The client adds these to every request.
        for (name, value) in [("user-agent", USER_AGENT), ("accept", "*/*")] {
            if !headers.contains_key(name) {
                request_headers.push(HeaderPair {
                    name: name.to_owned(),
                    value: value.to_owned(),
                });
            }
        }
        let request_body = match (body, self.multipart) {
            (Some(body), _) if !body.is_empty() => Some(Payload::new(body)),
            (_, Some(text)) => Some(Payload::new(text.as_bytes())),
            _ => None,
        };
        Exchange {
            request_headers,
            request_body,
            response,
        }
    }
}

fn header_pairs(headers: &HeaderMap) -> Vec<HeaderPair> {
    headers
        .iter()
        .map(|(k, v)| HeaderPair {
            name: k.as_str().to_owned(),
            value: String::from_utf8_lossy(v.as_bytes()).into_owned(),
        })
        .collect()
}

const USER_AGENT: &str = concat!("stormix/", env!("CARGO_PKG_VERSION"));

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

/// Scripts are CPU work that holds the thread until done. Under load, on
/// the multi-threaded runtime, the scheduler moves other tasks off this
/// thread meanwhile; a current-thread runtime only runs one flow anyway.
fn blocking<T>(f: impl FnOnce() -> T) -> T {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(f)
        }
        _ => f(),
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
        cached: false,
        exchange: None,
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

fn build_client(options: &RunOptions) -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(options.timeout)
        .redirect(if options.follow_redirects {
            Policy::limited(10)
        } else {
            Policy::none()
        })
        .tls_danger_accept_invalid_certs(options.insecure)
        .build()
}

async fn script_exchange(
    client: &reqwest::Client,
    call: &ScriptRequest,
    timeout: Duration,
) -> Result<ScriptResponse, String> {
    let method = reqwest::Method::from_bytes(call.method.as_bytes())
        .map_err(|_| format!("`{}` is not an HTTP method", call.method))?;
    let mut builder = client.request(method, &call.url).timeout(timeout);
    for (name, value) in &call.headers {
        builder = builder.header(name, value);
    }
    if let Some(body) = &call.body {
        builder = builder.body(body.clone());
    }
    let started = Instant::now();
    let response = builder.send().await.map_err(|e| describe(&e, timeout))?;
    let status = response.status();
    let headers = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_owned(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let body = response.bytes().await.map_err(|e| describe(&e, timeout))?;
    Ok(ScriptResponse {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or_default().to_owned(),
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
        time_ms: millis(started.elapsed()),
    })
}

fn millis(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
