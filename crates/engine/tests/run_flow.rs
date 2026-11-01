//! Runs flows against a small local API.

use std::time::Duration;

use axum::extract::Json;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use serde_json::{json, Value};
use stormix_engine::{Exchanges, Payload, RunOptions, Runner, Variables};
use stormix_flow::TestFile;

async fn login(Json(body): Json<Value>) -> (StatusCode, Json<Value>) {
    if body["password"] == "secret" {
        let token = format!("tok-{}", body["user"].as_str().unwrap_or_default());
        (StatusCode::OK, Json(json!({ "token": token })))
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "bad password" })),
        )
    }
}

async fn me(headers: HeaderMap) -> (StatusCode, Json<Value>) {
    match headers.get("authorization").and_then(|v| v.to_str().ok()) {
        Some("Bearer tok-demo") => (StatusCode::OK, Json(json!({ "user": "demo", "id": 7 }))),
        _ => (StatusCode::UNAUTHORIZED, Json(json!({}))),
    }
}

async fn echo(body: String) -> String {
    body
}

/// Echoes the request back as `data`, with an error when the query asks.
async fn graphql(headers: HeaderMap, Json(body): Json<Value>) -> (StatusCode, Json<Value>) {
    let query = body["query"].as_str().unwrap_or_default();
    let content_type = headers.get("content-type").and_then(|v| v.to_str().ok());
    let data = json!({ "got": body, "contentType": content_type });
    if query.contains("broken") {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "data": null })),
        )
    } else if query.contains("fail") {
        let errors = json!([{ "message": "nope", "path": ["user", 0] }, { "message": "also" }]);
        (
            StatusCode::OK,
            Json(json!({ "data": data, "errors": errors })),
        )
    } else {
        (StatusCode::OK, Json(json!({ "data": data, "errors": [] })))
    }
}

/// Answers with the `traceparent` it got, or `none`.
async fn traced(headers: HeaderMap) -> String {
    headers
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("none")
        .to_owned()
}

async fn slow() -> &'static str {
    tokio::time::sleep(Duration::from_millis(400)).await;
    "late"
}

/// Serves the API on a free port and returns its base URL.
async fn serve() -> String {
    let app = Router::new()
        .route("/login", post(login))
        .route("/me", get(me))
        .route("/echo", post(echo))
        .route("/graphql", post(graphql))
        .route("/slow", get(slow))
        .route("/traced", get(traced));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

fn flow(yaml: &str) -> stormix_flow::Flow {
    TestFile::from_yaml(&format!("version: 1\nflow:\n{yaml}"))
        .unwrap()
        .flow
}

const LOGIN: &str = r#"
  name: login
  variables: { password: secret }
  steps:
    - name: Log in
      request:
        method: POST
        url: "{{base}}/login"
        body: { json: { user: demo, password: "{{password}}" } }
      extract: { token: { json: $.token } }
      assert: [{ status: 200 }]
    - name: Profile
      request:
        method: GET
        url: "{{base}}/me"
        headers: [{ name: Authorization, value: "Bearer {{token}}" }]
      assert:
        - status: 200
        - json: { path: $.user, equals: demo }
        - header: { name: content-type, contains: json }
"#;

fn base(url: &str) -> Variables {
    let mut vars = Variables::new();
    vars.set("base", url);
    vars
}

#[tokio::test]
async fn chains_a_token_through_two_steps() {
    let url = serve().await;
    let runner = Runner::new(RunOptions::default()).unwrap();
    let result = runner.run(&flow(LOGIN), &base(&url)).await;

    assert!(result.passed(), "{result:#?}");
    assert_eq!(result.steps[0].extracted["token"], "tok-demo");
    assert_eq!(result.steps[1].url, format!("{url}/me"));
    assert_eq!(result.check_counts(), (5, 5));
    assert!(result.steps[1].time_to_headers_ms.is_some());
}

#[tokio::test]
async fn disabled_and_set_aside_steps_are_not_sent() {
    let url = serve().await;
    let yaml = r#"
  name: partial
  steps:
    - name: Off
      enabled: false
      request: { method: GET, url: "{{base}}/nowhere" }
    - name: Echo
      request: { method: POST, url: "{{base}}/echo", body: { raw: hi } }
      assert: [{ status: 200 }, { body_contains: hi }]
  set_aside:
    - name: Parked
      request: { method: GET, url: "{{base}}/nowhere" }
"#;
    let result = Runner::new(RunOptions::default())
        .unwrap()
        .run(&flow(yaml), &base(&url))
        .await;

    let names: Vec<_> = result.steps.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Echo"]);
    assert!(result.passed(), "{result:#?}");
}

#[tokio::test]
async fn overrides_beat_flow_variables_and_bail_skips_the_rest() {
    let url = serve().await;
    let mut vars = base(&url);
    vars.set("password", "wrong");
    let options = RunOptions {
        bail: true,
        ..RunOptions::default()
    };
    let result = Runner::new(options).unwrap().run(&flow(LOGIN), &vars).await;

    let login = &result.steps[0];
    assert_eq!(login.status, Some(401));
    let failed: Vec<_> = login.checks.iter().filter(|c| !c.passed).collect();
    assert_eq!(failed.len(), 2, "{failed:?}");
    assert_eq!(failed[0].message.as_deref(), Some("got 401"));
    assert!(result.steps[1].skipped);
    assert_eq!(result.failed_steps(), 1);
}

#[tokio::test]
async fn without_bail_later_steps_run_and_warn_about_missing_values() {
    let url = serve().await;
    let mut vars = base(&url);
    vars.set("password", "wrong");
    let result = Runner::new(RunOptions::default())
        .unwrap()
        .run(&flow(LOGIN), &vars)
        .await;

    let profile = &result.steps[1];
    assert!(!profile.skipped);
    assert_eq!(profile.status, Some(401));
    assert_eq!(
        profile.warnings,
        ["`{{token}}` has no value, so it was sent as written"]
    );
}

#[tokio::test]
async fn sends_forms_and_raw_bodies() {
    let url = serve().await;
    let f = flow(
        r#"
  name: bodies
  steps:
    - name: form
      request:
        method: POST
        url: "{{base}}/echo"
        body: { form: [{ key: q, value: "a b&c" }] }
      assert: [{ body_contains: "q=a+b%26c" }]
    - name: raw
      request:
        method: POST
        url: "{{base}}/echo"
        body: { raw: "id={{$randomInt}}" }
      extract: { id: { regex: "id=(\\d+)" } }
"#,
    );
    let result = Runner::new(RunOptions::default())
        .unwrap()
        .run(&f, &base(&url))
        .await;
    assert!(result.passed(), "{result:#?}");
    assert!(result.steps[1].extracted["id"].parse::<u32>().is_ok());
}

#[tokio::test]
async fn reports_timeouts_and_refused_connections() {
    let url = serve().await;
    let f = flow(
        r#"
  name: failures
  steps:
    - name: slow
      request: { method: GET, url: "{{base}}/slow" }
    - name: closed port
      request: { method: GET, url: "http://127.0.0.1:9" }
"#,
    );
    let options = RunOptions {
        timeout: Duration::from_millis(100),
        ..RunOptions::default()
    };
    let result = Runner::new(options).unwrap().run(&f, &base(&url)).await;
    assert_eq!(
        result.steps[0].error.as_deref(),
        Some("timed out after 100 ms")
    );
    assert!(result.steps[1].error.is_some());
    assert_eq!(result.failed_steps(), 2);
}

#[tokio::test]
async fn scripts_set_variables_change_requests_and_add_checks() {
    let url = serve().await;
    let f = flow(
        r#"
  name: scripts
  steps:
    - name: Log in
      request:
        method: POST
        url: "{{base}}/login"
        body: { json: { user: demo, password: "{{password}}" } }
      scripts:
        pre_request: |
          pm.environment.set("password", "secret");
          console.log("logging in as", "demo");
        test: |
          pm.test("status is 200", () => pm.response.to.have.status(200));
          pm.environment.set("token", pm.response.json().token);
    - name: Profile
      request: { method: GET, url: "{{base}}/me" }
      scripts:
        pre_request: |
          req.setHeader("Authorization", "Bearer " + bru.getVar("token"));
        test: |
          test("user", () => expect(res.body.user).to.equal("demo"));
          test("wrong id", () => expect(res.body.id).to.equal(8));
          oops();
"#,
    );
    let result = Runner::new(RunOptions::default())
        .unwrap()
        .run(&f, &base(&url))
        .await;

    let login = &result.steps[0];
    assert!(login.passed(), "{login:#?}");
    assert_eq!(login.extracted["token"], "tok-demo");
    assert_eq!(login.logs, ["logging in as demo"]);

    let profile = &result.steps[1];
    assert_eq!(profile.status, Some(200));
    let checks: Vec<_> = profile
        .checks
        .iter()
        .map(|c| (c.name.as_str(), c.passed))
        .collect();
    assert_eq!(
        checks,
        [
            ("user", true),
            ("wrong id", false),
            ("test script ran", false)
        ]
    );
    assert_eq!(
        profile.checks[2].message.as_deref(),
        Some("ReferenceError: oops is not defined")
    );
}

#[tokio::test]
async fn keeps_what_was_sent_and_received() {
    let url = serve().await;
    let runner = Runner::new(RunOptions::default()).unwrap();
    let mut vars = base(&url);
    vars.set("password", "wrong");
    let result = runner.run(&flow(LOGIN), &vars).await;

    let login = &result.steps[0];
    assert!(!login.passed());
    let exchange = login.exchange.as_ref().unwrap();
    let header = |name: &str| {
        exchange
            .request_headers
            .iter()
            .find(|h| h.name == name)
            .map(|h| h.value.as_str())
    };
    assert_eq!(header("content-type"), Some("application/json"));
    assert!(header("user-agent").unwrap().starts_with("stormix/"));
    let sent = exchange.request_body.as_ref().unwrap();
    assert_eq!(sent.text, r#"{"user":"demo","password":"wrong"}"#);
    let response = exchange.response.as_ref().unwrap();
    assert_eq!(response.status_text, "Unauthorized");
    assert_eq!(
        response.body.as_ref().unwrap().text,
        r#"{"error":"bad password"}"#
    );

    // Failed-only keeping drops the exchange of steps that passed.
    let runner = Runner::new(RunOptions {
        exchanges: Exchanges::FailedOnly,
        ..RunOptions::default()
    })
    .unwrap();
    let result = runner.run(&flow(LOGIN), &base(&url)).await;
    assert!(result.passed());
    assert!(result.steps.iter().all(|s| s.exchange.is_none()));

    // A refused connection keeps the request but has no response.
    let result = runner.run(&flow(LOGIN), &base("http://127.0.0.1:9")).await;
    let exchange = result.steps[0].exchange.as_ref().unwrap();
    assert!(exchange.response.is_none());
    assert!(exchange.request_body.is_some());
}

#[test]
fn payloads_are_cut_on_a_character_boundary_and_binary_is_described() {
    let long = "é".repeat(Payload::LIMIT);
    let payload = Payload::new(long.as_bytes());
    assert!(payload.truncated && !payload.binary);
    assert_eq!(payload.text.len(), Payload::LIMIT);
    assert_eq!(payload.bytes as usize, long.len());

    let payload = Payload::new(&[0xff, 0xfe, 0x00]);
    assert!(payload.binary);
    assert_eq!(payload.text, "3 bytes of binary data");
}

const SEND_FROM_SCRIPT: &str = r#"
  name: send from script
  steps:
    - name: Token
      request: { method: GET, url: "{{base}}/slow" }
      scripts:
        test: |
          const login = await bru.sendRequest({
            url: bru.getVar("base") + "/login",
            method: "POST",
            data: { user: "demo", password: "secret" },
          })
          bru.setVar("token", login.data.token)
          try {
            await bru.sendRequest({ url: bru.getVar("base") + "/me" })
          } catch (e) {
            bru.setVar("denied", String(e.response.status))
          }
          const echo = await pm.sendRequest({
            url: bru.getVar("base") + "/echo",
            method: "POST",
            body: { mode: "raw", raw: "hi" },
          })
          test("echo", () => expect(echo.text()).to.equal("hi"))
"#;

fn check_send_from_script(result: &stormix_engine::FlowResult) {
    let step = &result.steps[0];
    assert!(step.passed(), "{step:#?}");
    assert_eq!(step.extracted["token"], "tok-demo");
    assert_eq!(step.extracted["denied"], "401");
}

#[tokio::test(flavor = "multi_thread")]
async fn scripts_send_requests_on_a_multi_threaded_runtime() {
    let url = serve().await;
    let result = Runner::new(RunOptions::default())
        .unwrap()
        .run(&flow(SEND_FROM_SCRIPT), &base(&url))
        .await;
    check_send_from_script(&result);
}

/// The CLI's `run` uses a single-threaded runtime, which is busy running
/// the script while it waits, so the API lives on a runtime of its own.
#[tokio::test]
async fn scripts_send_requests_on_a_single_threaded_runtime() {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            tx.send(serve().await).unwrap();
            std::future::pending::<()>().await
        })
    });
    let url = rx.recv().unwrap();
    let result = Runner::new(RunOptions::default())
        .unwrap()
        .run(&flow(SEND_FROM_SCRIPT), &base(&url))
        .await;
    check_send_from_script(&result);
}

/// Serves `/count`, which answers with how many times it was called, and
/// `/fail`, which always answers 500 and counts too.
async fn serve_counter() -> (String, std::sync::Arc<std::sync::atomic::AtomicU32>) {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    let calls = Arc::new(AtomicU32::new(0));
    let (a, b) = (calls.clone(), calls.clone());
    let app = Router::new()
        .route(
            "/count",
            get(move || async move { (a.fetch_add(1, Ordering::SeqCst) + 1).to_string() }),
        )
        .route(
            "/fail",
            get(move || async move {
                b.fetch_add(1, Ordering::SeqCst);
                StatusCode::INTERNAL_SERVER_ERROR
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), calls)
}

const CACHED: &str = r#"
  name: cached
  steps:
    - name: Token
      cache: true
      request: { method: GET, url: "{{base}}/count?user={{user}}" }
      extract: { first: { regex: "\\d+" } }
      assert: [{ status: 200 }]
    - name: Live
      request: { method: GET, url: "{{base}}/count" }
"#;

#[tokio::test]
async fn a_cached_step_reuses_its_response_until_cleared() {
    use std::sync::atomic::Ordering;
    let (url, calls) = serve_counter().await;
    let runner = Runner::new(RunOptions::default()).unwrap();
    let mut vars = base(&url);
    vars.set("user", "a");
    let f = flow(CACHED);

    let first = runner.run(&f, &vars).await;
    assert!(!first.steps[0].cached && !first.steps[1].cached);
    assert_eq!(first.steps[0].extracted["first"], "1");
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    // A clone shares the cache, as load test iterations do.
    let second = runner.clone().run(&f, &vars).await;
    assert!(second.steps[0].cached, "{second:#?}");
    assert!(!second.steps[1].cached);
    // Checks and extraction ran on the kept response.
    assert!(second.passed(), "{second:#?}");
    assert_eq!(second.steps[0].extracted["first"], "1");
    assert_eq!(second.steps[0].status, Some(200));
    assert!(second.steps[0].exchange.is_some());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "only the live step was sent"
    );

    let steps = runner.cache().steps();
    assert_eq!(steps.len(), 1);
    assert_eq!(
        (steps[0].flow.as_str(), steps[0].step.as_str()),
        ("cached", "Token")
    );
    assert_eq!(steps[0].responses, 1);

    // Another value in the request is another request: sent and kept apart.
    vars.set("user", "b");
    let other = runner.run(&f, &vars).await;
    assert!(!other.steps[0].cached);
    assert_eq!(other.steps[0].extracted["first"], "4");
    assert_eq!(runner.cache().steps()[0].responses, 2);

    // Clearing another step leaves this one; clearing this one empties it.
    assert_eq!(runner.cache().clear(Some("cached"), Some("Live")), 0);
    assert_eq!(runner.cache().clear(Some("cached"), Some("Token")), 2);
    assert!(runner.cache().is_empty());
    let after = runner.run(&f, &vars).await;
    assert!(!after.steps[0].cached);
    assert_eq!(after.steps[0].extracted["first"], "6");

    assert_eq!(runner.cache().clear(None, None), 1);
}

#[tokio::test]
async fn steps_without_cache_and_failed_responses_are_not_kept() {
    use std::sync::atomic::Ordering;
    let (url, calls) = serve_counter().await;
    let runner = Runner::new(RunOptions::default()).unwrap();
    let f = flow(
        r#"
  name: uncached
  steps:
    - name: Plain
      request: { method: GET, url: "{{base}}/count" }
    - name: Broken
      cache: true
      request: { method: GET, url: "{{base}}/fail" }
"#,
    );
    for _ in 0..2 {
        let result = runner.run(&f, &base(&url)).await;
        assert!(result.steps.iter().all(|s| !s.cached), "{result:#?}");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    assert!(runner.cache().is_empty());
}

#[tokio::test]
async fn graphql_steps_send_typed_variables_and_fail_on_errors() {
    let url = serve().await;
    let f = flow(
        r#"
  name: graphql
  variables: { id: "42", on: "true", name: Ada }
  steps:
    - name: typed
      request:
        method: POST
        url: "{{base}}/graphql"
        body:
          graphql:
            query: "query Q($id: Int!, $on: Boolean, $name: String) { user(id: $id) { name } }"
            variables: { id: "{{id}}", on: "{{on}}", name: "{{name}}", note: "{{id}}" }
            operation_name: Q
      assert:
        - json: { path: $.data.got.variables, equals: { id: 42, on: true, name: Ada, note: "42" } }
        - json: { path: $.data.got.operationName, equals: Q }
        - json: { path: $.data.contentType, equals: application/json }
    - name: text variables
      request:
        method: POST
        url: "{{base}}/graphql"
        body:
          graphql:
            query: "{ user { name } }"
            variables: '{ "id": {{id}}, "name": "{{name}}" }'
      assert:
        - json: { path: $.data.got.variables, equals: { id: 42, name: Ada } }
    - name: errors fail the step
      request:
        method: POST
        url: "{{base}}/graphql"
        body: { graphql: { query: "{ fail }" } }
    - name: unless they are allowed
      request:
        method: POST
        url: "{{base}}/graphql"
        body: { graphql: { query: "{ fail }", allow_errors: true } }
    - name: a server error fails too
      request:
        method: POST
        url: "{{base}}/graphql"
        body: { graphql: { query: "{ broken }" } }
    - name: text that is not JSON
      request:
        method: POST
        url: "{{base}}/graphql"
        body: { graphql: { query: "{ x }", variables: '{ "id": {{id}} ' } }
"#,
    );
    let result = Runner::new(RunOptions::default())
        .unwrap()
        .run(&f, &base(&url))
        .await;
    let [typed, text, errors, allowed, broken, invalid] = &result.steps[..] else {
        panic!("{result:#?}")
    };
    assert!(typed.passed(), "{typed:#?}");
    assert!(typed.checks.iter().any(|c| c.name == "no GraphQL errors"));
    assert!(text.passed(), "{text:#?}");

    assert!(!errors.passed());
    let check = errors.checks.last().unwrap();
    assert_eq!(check.name, "no GraphQL errors");
    assert_eq!(
        check.message.as_deref(),
        Some("nope (at user.0), and 1 more")
    );

    assert!(allowed.passed(), "{allowed:#?}");
    assert!(allowed.checks.is_empty());

    assert!(!broken.passed());
    assert_eq!(
        broken.checks.last().unwrap().message.as_deref(),
        Some("status 500")
    );

    assert!(
        invalid.error.as_deref().unwrap().contains("not valid JSON"),
        "{invalid:#?}"
    );
}

const TRACED: &str = r#"
  name: traced
  trace: TRACE_MODE
  steps:
    - name: First
      request: { method: GET, url: "{{base}}/traced" }
      extract: { got: { regex: ".+" } }
    - name: Second
      request: { method: GET, url: "{{base}}/traced" }
      extract: { got: { regex: ".+" } }
    - name: Own
      request:
        method: GET
        url: "{{base}}/traced"
        headers: [{ name: TraceParent, value: "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01" }]
      extract: { got: { regex: ".+" } }
"#;

/// What the server got, and what the step recorded.
fn trace_of(step: &stormix_engine::StepResult) -> (String, String, String) {
    (
        step.extracted["got"].clone(),
        step.trace_id.clone().unwrap_or_default(),
        step.span_id.clone().unwrap_or_default(),
    )
}

#[tokio::test]
async fn traced_flows_send_a_traceparent_per_request() {
    let url = serve().await;
    let runner = Runner::new(RunOptions::default()).unwrap();
    let result = runner
        .run(&flow(&TRACED.replace("TRACE_MODE", "request")), &base(&url))
        .await;
    assert!(result.passed(), "{result:#?}");
    let (got1, trace1, span1) = trace_of(&result.steps[0]);
    let (got2, trace2, _) = trace_of(&result.steps[1]);
    assert_eq!(got1, format!("00-{trace1}-{span1}-01"));
    assert_eq!(trace1.len(), 32);
    assert_eq!(span1.len(), 16);
    assert!(got2.ends_with("-01") && got2.contains(&trace2));
    assert_ne!(trace1, trace2, "each request is its own trace");
    // The header shows in the request kept for Details.
    let sent = &result.steps[0].exchange.as_ref().unwrap().request_headers;
    assert!(sent
        .iter()
        .any(|h| h.name == "traceparent" && h.value == got1));

    // A step's own header is sent untouched, and its IDs recorded.
    let (own, own_trace, own_span) = trace_of(&result.steps[2]);
    assert_eq!(
        own,
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
    );
    assert_eq!(own_trace, "4bf92f3577b34da6a3ce929d0e0e4736");
    assert_eq!(own_span, "00f067aa0ba902b7");
}

#[tokio::test]
async fn one_trace_per_run_in_flow_mode() {
    let url = serve().await;
    let runner = Runner::new(RunOptions::default()).unwrap();
    let f = flow(&TRACED.replace("TRACE_MODE", "flow"));
    let first = runner.run(&f, &base(&url)).await;
    let (_, trace1, span1) = trace_of(&first.steps[0]);
    let (got2, trace2, span2) = trace_of(&first.steps[1]);
    assert_eq!(trace1, trace2, "one trace for the run");
    assert_ne!(span1, span2, "a span per request");
    assert_eq!(got2, format!("00-{trace2}-{span2}-01"));
    let again = runner.run(&f, &base(&url)).await;
    assert_ne!(trace_of(&again.steps[0]).1, trace1, "a new trace per run");
}

#[tokio::test]
async fn untraced_flows_send_no_traceparent() {
    let url = serve().await;
    let runner = Runner::new(RunOptions::default()).unwrap();
    let yaml = TRACED.replace("  trace: TRACE_MODE\n", "");
    let result = runner.run(&flow(&yaml), &base(&url)).await;
    assert_eq!(
        trace_of(&result.steps[0]),
        ("none".into(), String::new(), String::new())
    );
    // A header the step sets itself is still recorded.
    assert_eq!(
        result.steps[2].trace_id.as_deref(),
        Some("4bf92f3577b34da6a3ce929d0e0e4736")
    );
}

#[tokio::test]
async fn an_invalid_own_traceparent_is_kept_with_a_warning() {
    let url = serve().await;
    let runner = Runner::new(RunOptions::default()).unwrap();
    let yaml = TRACED.replace("TRACE_MODE", "request").replace(
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        "not-a-trace",
    );
    let result = runner.run(&flow(&yaml), &base(&url)).await;
    let own = &result.steps[2];
    assert_eq!(own.extracted["got"], "not-a-trace");
    assert_eq!(own.trace_id, None);
    assert!(own.warnings[0].contains("traceparent"), "{own:#?}");
}

#[tokio::test]
async fn tracing_does_not_stop_a_cached_step_from_matching() {
    use std::sync::atomic::Ordering;
    let (url, calls) = serve_counter().await;
    let runner = Runner::new(RunOptions::default()).unwrap();
    let mut vars = base(&url);
    vars.set("user", "a");
    let f = flow(&CACHED.replace("  name: cached\n", "  name: cached\n  trace: request\n"));
    let first = runner.run(&f, &vars).await;
    assert!(first.steps[0].trace_id.is_some());
    let second = runner.run(&f, &vars).await;
    assert!(second.steps[0].cached, "{second:#?}");
    assert_eq!(second.steps[0].trace_id, None, "nothing was sent");
    assert!(second.steps[1].trace_id.is_some());
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}
