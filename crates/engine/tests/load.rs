//! Load profiles against a small local API. Kept short: each test runs for
//! a second or two.

use std::time::Duration;

use axum::routing::get;
use axum::Router;
use stormix_engine::{run_load, LoadOptions, RunOptions, Runner, Variables};
use stormix_flow::{TestFile, Threshold};

async fn serve() -> String {
    let app = Router::new()
        .route("/fast", get(|| async { "ok" }))
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_millis(300)).await;
                "late"
            }),
        )
        .route(
            "/missing",
            get(|| async { (axum::http::StatusCode::NOT_FOUND, "no") }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

fn test_file(yaml: &str) -> TestFile {
    TestFile::from_yaml(&format!("version: 1\n{yaml}")).unwrap()
}

fn base(url: &str) -> Variables {
    let mut vars = Variables::new();
    vars.set("base", url);
    vars
}

fn thresholds(list: &[&str]) -> Vec<Threshold> {
    list.iter().map(|t| t.parse().unwrap()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn constant_rate_sends_the_planned_number_of_iterations() {
    let url = serve().await;
    let test = test_file(
        r#"
flow:
  name: fast
  steps:
    - name: fast
      request: { method: GET, url: "{{base}}/fast" }
      assert: [{ status: 200 }]
      scripts: { test: 'pm.test("says ok", () => pm.expect(pm.response.text()).to.eql("ok"))' }
load:
  constant_rate: { rate: 50, duration: 2s }
"#,
    );
    let runner = Runner::new(RunOptions::default()).unwrap();
    let mut seconds = Vec::new();
    let report = run_load(
        &runner,
        &test.flow,
        &base(&url),
        test.load.as_ref().unwrap(),
        &thresholds(&["errors < 1%", "p95 < 500ms", "checks > 99%"]),
        &LoadOptions::default(),
        |p| seconds.push((p.second.t, p.second.requests, p.target)),
    )
    .await;

    let s = &report.summary;
    assert_eq!(s.iterations, 100);
    assert_eq!(s.requests, s.iterations);
    assert_eq!(s.failed_requests, 0);
    assert_eq!(s.checks_failed, 0);
    assert_eq!(s.checks_passed, 2 * s.requests);
    assert_eq!(s.dropped_iterations, 0);
    assert!(report.passed(), "{:?}", report.thresholds);
    assert_eq!(seconds.len(), 2, "{seconds:?}");
    assert!(
        seconds
            .iter()
            .all(|&(_, requests, target)| requests >= 35 && target == 50.0),
        "{seconds:?}"
    );
    assert!((40.0..=60.0).contains(&s.rps), "{} rps", s.rps);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_slow_api_does_not_slow_an_open_model_but_drops_beyond_the_cap() {
    let url = serve().await;
    let test = test_file(
        r#"
flow:
  name: slow
  steps:
    - name: slow
      request: { method: GET, url: "{{base}}/slow" }
load:
  ramping:
    start_rate: 40
    stages: [{ duration: 1s, target: 40 }]
"#,
    );
    let runner = Runner::new(RunOptions::default()).unwrap();
    let options = LoadOptions {
        max_in_flight: 5,
        ..LoadOptions::default()
    };
    let report = run_load(
        &runner,
        &test.flow,
        &base(&url),
        test.load.as_ref().unwrap(),
        &thresholds(&["p50 < 100ms"]),
        &options,
        |_| {},
    )
    .await;
    let s = &report.summary;
    // About 40 were due; at most 5 run at a time and each takes 300 ms.
    assert_eq!(s.iterations + s.dropped_iterations, 40, "{s:?}");
    assert!(s.dropped_iterations >= 20, "{s:?}");
    assert!(s.latency.p50 >= 300.0);
    assert!(!report.passed());
    assert_eq!(report.thresholds[0].threshold, "p50 < 100ms");
}

#[tokio::test(flavor = "multi_thread")]
async fn virtual_users_loop_and_failures_are_counted() {
    let url = serve().await;
    let test = test_file(
        r#"
flow:
  name: users
  steps:
    - name: fast
      request: { method: GET, url: "{{base}}/fast" }
    - name: missing
      request: { method: GET, url: "{{base}}/missing" }
load:
  virtual_users: { count: 3, duration: 1s, think_time: 100ms }
"#,
    );
    let runner = Runner::new(RunOptions::default()).unwrap();
    let report = run_load(
        &runner,
        &test.flow,
        &base(&url),
        test.load.as_ref().unwrap(),
        &thresholds(&["errors < 10%"]),
        &LoadOptions::default(),
        |p| assert_eq!(p.target, 3.0),
    )
    .await;
    let s = &report.summary;
    // Each user does an iteration about every 100 ms.
    assert!((15..=33).contains(&s.iterations), "{s:?}");
    assert_eq!(s.requests, 2 * s.iterations);
    // Unchecked 404s count as failures.
    assert_eq!(s.failed_requests, s.iterations);
    assert_eq!(s.steps[1].failed, s.iterations);
    assert_eq!(s.failures[0].reason, "missing: status 404");
    assert!(!report.passed());
    // One sample of the failing step, with its exchange; none of the passing one.
    assert_eq!(report.samples.len(), 1);
    let sample = &report.samples[0];
    assert_eq!(sample.name, "missing");
    let response = sample.exchange.as_ref().unwrap().response.as_ref().unwrap();
    assert_eq!(response.status_text, "Not Found");
    assert!((report.thresholds[0].actual - 50.0).abs() < 0.01);
}

#[tokio::test(flavor = "multi_thread")]
async fn iterations_past_the_grace_period_are_interrupted() {
    let url = serve().await;
    let test = test_file(
        r#"
flow:
  name: slow users
  steps:
    - name: slow
      request: { method: GET, url: "{{base}}/slow" }
load:
  virtual_users: { count: 2, duration: 100ms }
"#,
    );
    let runner = Runner::new(RunOptions::default()).unwrap();
    let options = LoadOptions {
        grace: Duration::from_millis(50),
        ..LoadOptions::default()
    };
    let report = run_load(
        &runner,
        &test.flow,
        &base(&url),
        test.load.as_ref().unwrap(),
        &[],
        &options,
        |_| {},
    )
    .await;
    assert_eq!(report.summary.interrupted_iterations, 2);
    assert_eq!(report.summary.iterations, 0);
    assert!(report.passed(), "no thresholds, so nothing to fail");
}

#[tokio::test(flavor = "multi_thread")]
async fn cached_steps_are_counted_apart_from_requests() {
    let url = serve().await;
    let test = test_file(
        r#"
flow:
  name: cached
  steps:
    - name: setup
      cache: true
      request: { method: GET, url: "{{base}}/fast" }
      assert: [{ status: 200 }]
    - name: fast
      request: { method: GET, url: "{{base}}/fast" }
load:
  virtual_users: { count: 1, duration: 1s }
"#,
    );
    let runner = Runner::new(RunOptions::default()).unwrap();
    let report = run_load(
        &runner,
        &test.flow,
        &base(&url),
        test.load.as_ref().unwrap(),
        &[],
        &LoadOptions::default(),
        |_| {},
    )
    .await;

    let s = &report.summary;
    assert!(s.iterations > 2, "{s:?}");
    // The setup step ran once before the load, so no iteration sent it and
    // it is not in the request count or the latency figures.
    assert_eq!(report.setup.len(), 1);
    assert!(report.setup[0].passed() && !report.setup[0].cached);
    assert_eq!(s.cached_responses, s.iterations);
    assert_eq!(s.requests, s.iterations);
    let setup = &s.steps[0];
    assert_eq!(
        (setup.name.as_str(), setup.requests, setup.cached),
        ("setup", 0, s.iterations)
    );
    assert_eq!(s.failed_iterations, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_open_model_sends_a_cached_prerequisite_once() {
    let url = serve().await;
    let test = test_file(
        r#"
flow:
  name: login first
  steps:
    - name: login
      cache: true
      request: { method: GET, url: "{{base}}/slow" }
      assert: [{ status: 200 }]
    - name: fast
      request: { method: GET, url: "{{base}}/fast" }
load:
  constant_rate: { rate: 50, duration: 1s }
"#,
    );
    let runner = Runner::new(RunOptions::default()).unwrap();
    let report = run_load(
        &runner,
        &test.flow,
        &base(&url),
        test.load.as_ref().unwrap(),
        &[],
        &LoadOptions::default(),
        |_| {},
    )
    .await;

    // Without the setup pass, the iterations started during the first slow
    // login would each have sent one of their own.
    let s = &report.summary;
    assert_eq!(report.setup.len(), 1);
    assert_eq!(s.iterations, 50);
    assert_eq!(s.cached_responses, 50);
    assert_eq!(s.requests, 50);
    assert!(s
        .steps
        .iter()
        .all(|st| st.name != "login" || st.requests == 0));
}
