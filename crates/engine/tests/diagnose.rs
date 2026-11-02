//! Diagnoses requests to a small local server.

use axum::http::{header, StatusCode};
use axum::routing::get;
use axum::Router;
use stormix_engine::{RunOptions, Runner, StageOutcome, Variables};
use stormix_flow::TestFile;

async fn serve() -> String {
    let app = Router::new().route("/ok", get(|| async { "fine" })).route(
        "/blocked",
        get(|| async {
            (
                StatusCode::FORBIDDEN,
                [
                    (header::SERVER, "Zscaler/6.2"),
                    (header::CONTENT_TYPE, "text/html"),
                ],
                "<html><head><title>Website blocked</title></head></html>",
            )
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

fn flow(base: &str, paths: &[&str]) -> stormix_flow::Flow {
    let steps: String = paths
        .iter()
        .map(|p| format!("    - name: {p}\n      request: {{ method: GET, url: \"{base}{p}\" }}\n"))
        .collect();
    TestFile::from_yaml(&format!("version: 1\nflow:\n  name: f\n  steps:\n{steps}"))
        .unwrap()
        .flow
}

fn outcome(d: &stormix_engine::Diagnosis, name: &str) -> StageOutcome {
    d.stages
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no {name} stage in {:?}", d.stages))
        .outcome
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_that_goes_through() {
    let base = serve().await;
    let runner = Runner::new(RunOptions::default()).unwrap();
    let d = runner
        .diagnose(&flow(&base, &["/ok"]), &Variables::new(), "/ok")
        .await
        .unwrap();
    assert!(d.ok, "{d:?}");
    assert_eq!(outcome(&d, "Name lookup"), StageOutcome::Passed);
    assert_eq!(outcome(&d, "Connection"), StageOutcome::Passed);
    assert_eq!(outcome(&d, "Response"), StageOutcome::Passed);
    assert!(
        d.verdict.starts_with("The request went through: 200"),
        "{}",
        d.verdict
    );
    let connection = d.step.exchange.unwrap().connection.unwrap();
    assert_eq!(connection.http_version.as_deref(), Some("HTTP/1.1"));
    assert_eq!(
        connection.remote_address.as_deref(),
        Some(base.trim_start_matches("http://"))
    );
    assert_eq!(connection.proxy, None, "127.0.0.1 goes direct");
}

#[tokio::test(flavor = "multi_thread")]
async fn names_who_answered_a_403() {
    let base = serve().await;
    let runner = Runner::new(RunOptions::default()).unwrap();
    let d = runner
        .diagnose(
            &flow(&base, &["/ok", "/blocked"]),
            &Variables::new(),
            "/blocked",
        )
        .await
        .unwrap();
    assert!(!d.ok);
    assert_eq!(d.stages[0].name, "Earlier steps");
    assert_eq!(d.stages[0].outcome, StageOutcome::Passed);
    assert_eq!(outcome(&d, "Response"), StageOutcome::Failed);
    let response = d.stages.iter().find(|s| s.name == "Response").unwrap();
    assert!(
        response.summary.contains("403 Forbidden"),
        "{}",
        response.summary
    );
    assert!(
        response.summary.contains("Website blocked"),
        "{}",
        response.summary
    );
    assert!(d.verdict.contains("Zscaler"), "{}", d.verdict);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_closed_port_fails_at_the_connection() {
    // Bound then dropped, so nothing listens there.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let base = format!("http://127.0.0.1:{port}");
    let runner = Runner::new(RunOptions::default()).unwrap();
    let d = runner
        .diagnose(&flow(&base, &["/ok"]), &Variables::new(), "/ok")
        .await
        .unwrap();
    assert!(!d.ok);
    assert_eq!(outcome(&d, "Connection"), StageOutcome::Failed);
    assert_eq!(outcome(&d, "Response"), StageOutcome::NotReached);
    assert!(
        d.verdict.starts_with("Could not connect to 127.0.0.1"),
        "{}",
        d.verdict
    );
    // The step's own exchange still says how it was routed.
    assert!(d.step.exchange.unwrap().connection.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_an_unknown_step() {
    let runner = Runner::new(RunOptions::default()).unwrap();
    let err = runner
        .diagnose(&flow("http://x", &["/ok"]), &Variables::new(), "nope")
        .await
        .unwrap_err();
    assert!(err.contains("`nope`"));
}
