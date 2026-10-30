//! Runs flows against a small local API.

use std::time::Duration;

use axum::extract::Json;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use serde_json::{json, Value};
use stormix_engine::{RunOptions, Runner, Variables};
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
        .route("/slow", get(slow));
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
