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

#[tokio::test(flavor = "multi_thread")]
async fn traced_load_tests_keep_the_slowest_requests_of_each_step() {
    let url = serve().await;
    let test = test_file(
        r#"
flow:
  name: traced
  trace: request
  steps:
    - name: fast
      request: { method: GET, url: "{{base}}/fast" }
    - name: missing
      request: { method: GET, url: "{{base}}/missing" }
load:
  constant_rate: { rate: 20, duration: 1s }
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
    assert_eq!(report.slowest.len(), 10, "{:#?}", report.slowest);
    for step in ["fast", "missing"] {
        assert_eq!(report.slowest.iter().filter(|r| r.step == step).count(), 5);
    }
    assert!(report
        .slowest
        .windows(2)
        .all(|w| w[0].duration_ms >= w[1].duration_ms));
    let missing = report.slowest.iter().find(|r| r.step == "missing").unwrap();
    assert!(!missing.passed);
    assert_eq!(missing.status, Some(404));
    assert_eq!(missing.trace_id.len(), 32);
    // The failure sample carries its trace ID too.
    assert!(report.samples[0].trace_id.is_some());

    let untraced = test_file(&format!(
        "flow:\n  name: plain\n  steps:\n    - name: fast\n      request: {{ method: GET, url: \"{url}/fast\" }}\nload:\n  constant_rate: {{ rate: 5, duration: 1s }}\n"
    ));
    let report = run_load(
        &runner,
        &untraced.flow,
        &Variables::new(),
        untraced.load.as_ref().unwrap(),
        &[],
        &LoadOptions::default(),
        |_| {},
    )
    .await;
    assert!(report.slowest.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn virtual_users_run_the_whole_profile_however_short_the_grace() {
    let url = serve().await;
    let test = test_file(
        r#"
flow:
  name: fast
  steps:
    - name: fast
      request: { method: GET, url: "{{base}}/fast" }
load:
  virtual_users: { count: 1, duration: 1s, think_time: 100ms }
"#,
    );
    let runner = Runner::new(RunOptions::default()).unwrap();
    let options = LoadOptions {
        grace: Duration::from_millis(200),
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
    let s = &report.summary;
    // The grace counts from the end of the profile, not from the start.
    assert!(s.iterations >= 7, "{s:?}");
    assert!(s.duration_ms >= 1000.0, "{s:?}");
    assert!(!report.aborted);
}

/// Starts `test`, stops it after `after`, and returns the report and how
/// long the whole run took.
async fn stopped_after(
    test: &TestFile,
    url: &str,
    after: Duration,
) -> (stormix_engine::LoadReport, Duration) {
    let runner = Runner::new(RunOptions::default()).unwrap();
    let options = LoadOptions::default();
    let stop = options.stop.clone();
    tokio::spawn(async move {
        tokio::time::sleep(after).await;
        stop.stop();
    });
    let started = std::time::Instant::now();
    let report = run_load(
        &runner,
        &test.flow,
        &base(url),
        test.load.as_ref().unwrap(),
        &thresholds(&["errors < 1%"]),
        &options,
        |_| {},
    )
    .await;
    (report, started.elapsed())
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_virtual_users_keeps_what_was_measured() {
    let url = serve().await;
    let test = test_file(
        r#"
flow:
  name: fast
  steps:
    - name: fast
      request: { method: GET, url: "{{base}}/fast" }
load:
  virtual_users: { count: 3, duration: 60s, think_time: 5s }
"#,
    );
    let (report, took) = stopped_after(&test, &url, Duration::from_millis(500)).await;
    // Users waiting out their think time stop at once.
    assert!(took < Duration::from_secs(2), "took {took:?}");
    assert!(report.aborted);
    assert!(!report.passed(), "a stopped test does not pass");
    assert!(
        report.thresholds[0].passed,
        "thresholds still cover what ran"
    );
    assert_eq!(report.summary.iterations, 3, "{:?}", report.summary);
    assert_eq!(report.summary.interrupted_iterations, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_an_open_model_lets_running_iterations_finish() {
    let url = serve().await;
    let test = test_file(
        r#"
flow:
  name: slow
  steps:
    - name: slow
      request: { method: GET, url: "{{base}}/slow" }
load:
  constant_rate: { rate: 20, duration: 60s }
"#,
    );
    let (report, took) = stopped_after(&test, &url, Duration::from_millis(1000)).await;
    assert!(took < Duration::from_secs(2), "took {took:?}");
    assert!(report.aborted);
    let s = &report.summary;
    // About 20 started in the first second; those in flight finished.
    assert!((15..=25).contains(&s.iterations), "{s:?}");
    assert_eq!(s.interrupted_iterations, 0, "{s:?}");
    assert!(s.duration_ms < 2000.0, "{s:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_test_stopped_before_it_starts_sends_nothing() {
    let url = serve().await;
    let test = test_file(
        r#"
flow:
  name: slow
  steps:
    - name: slow
      request: { method: GET, url: "{{base}}/slow" }
load:
  virtual_users: { count: 2, duration: 60s }
"#,
    );
    let runner = Runner::new(RunOptions::default()).unwrap();
    let options = LoadOptions {
        stop_grace: Duration::ZERO,
        ..LoadOptions::default()
    };
    options.stop.stop();
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
    // Stopped before it began: nothing ran.
    assert!(report.aborted);
    assert_eq!(report.summary.iterations, 0);
    assert_eq!(report.summary.requests, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_cuts_off_iterations_past_the_stop_grace() {
    let url = serve().await;
    let test = test_file(
        r#"
flow:
  name: slow
  steps:
    - name: slow
      request: { method: GET, url: "{{base}}/slow" }
load:
  virtual_users: { count: 2, duration: 60s }
"#,
    );
    let runner = Runner::new(RunOptions::default()).unwrap();
    let options = LoadOptions {
        stop_grace: Duration::from_millis(50),
        ..LoadOptions::default()
    };
    let stop = options.stop.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        stop.stop();
    });
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
    // Both users were 100 ms into a 300 ms request.
    assert!(report.aborted);
    assert_eq!(report.summary.iterations, 0);
    assert_eq!(report.summary.interrupted_iterations, 2);
}

/// An API serving Prometheus text whose CPU counter grows half a second per
/// second of wall time, behind a bearer token.
async fn serve_metrics() -> String {
    use axum::http::{HeaderMap, StatusCode};
    let started = std::time::Instant::now();
    let app = Router::new()
        .route("/fast", get(|| async { "ok" }))
        .route(
            "/metrics",
            get(move |headers: HeaderMap| async move {
                if headers.get("authorization").and_then(|v| v.to_str().ok())
                    != Some("Bearer s3cret")
                {
                    return (StatusCode::UNAUTHORIZED, String::new());
                }
                let cpu = started.elapsed().as_secs_f64() / 2.0;
                (
                    StatusCode::OK,
                    format!(
                        "# TYPE process_cpu_seconds_total counter\nprocess_cpu_seconds_total {cpu}\nprocess_resident_memory_bytes 1.5e8\n"
                    ),
                )
            }),
        )
        .route(
            "/health",
            // Down from 0.8 s to 1.6 s into the test, with its database.
            get(move || async move {
                let t = started.elapsed().as_secs_f64();
                if (0.8..1.6).contains(&t) {
                    (
                        StatusCode::SERVICE_UNAVAILABLE,
                        r#"{"status":"DOWN","components":{"db":{"status":"DOWN"},"ping":{"status":"UP"}}}"#,
                    )
                } else {
                    (StatusCode::OK, r#"{"status":"UP"}"#)
                }
            }),
        )
        .route(
            "/actuator/metrics/{name}",
            get(
                |axum::extract::Path(name): axum::extract::Path<String>| async move {
                    let value = match name.as_str() {
                        "process.cpu.usage" => 0.1,
                        "system.cpu.count" => 8.0,
                        "jvm.memory.used" => 1e8,
                        _ => return (StatusCode::NOT_FOUND, String::new()),
                    };
                    (
                        StatusCode::OK,
                        format!(r#"{{"name":"{name}","measurements":[{{"statistic":"VALUE","value":{value}}}]}}"#),
                    )
                },
            ),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

#[tokio::test(flavor = "multi_thread")]
async fn monitors_read_the_apis_cpu_and_memory_throughout_the_test() {
    let url = serve_metrics().await;
    let test = test_file(
        r#"
flow:
  name: watched
  steps:
    - name: fast
      request: { method: GET, url: "{{base}}/fast" }
load:
  constant_rate: { rate: 20, duration: 2s }
monitor:
  - prometheus:
      url: "{{base}}/metrics?debug=1"
      name: orders-api
      headers: [{ name: Authorization, value: "Bearer {{token}}" }]
      interval: 200ms
      health: "{{base}}/health"
  - actuator: { url: "{{base}}/actuator/" }
  - prometheus: { url: "{{base}}/metrics" }
  - prometheus: { url: "{{nowhere}}/metrics" }
  - prometheus: { url: "{{closing}}/metrics?token=s3cret" }
"#,
    );
    // Hangs up on every connection, so reads fail at once on every OS (a
    // closed port can take seconds to refuse on Windows).
    let closing = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let closing_url = format!("http://{}", closing.local_addr().unwrap());
    tokio::spawn(async move {
        while let Ok((socket, _)) = closing.accept().await {
            drop(socket);
        }
    });
    let mut vars = base(&url);
    vars.set("closing", &closing_url);
    vars.set("token", "s3cret");
    let runner = Runner::new(RunOptions::default()).unwrap();
    let options = LoadOptions {
        monitor: test.monitor.clone(),
        ..LoadOptions::default()
    };
    let mut live = Vec::new();
    let report = run_load(
        &runner,
        &test.flow,
        &vars,
        test.load.as_ref().unwrap(),
        &[],
        &options,
        |p| {
            live.push(
                p.resources
                    .iter()
                    .map(|r| r.samples.len())
                    .collect::<Vec<_>>(),
            )
        },
    )
    .await;
    assert!(report.passed());
    let [prometheus, actuator, unauthorized, unset, refused] = &report.resources[..] else {
        panic!("{:?}", report.resources);
    };

    assert_eq!(prometheus.name, "orders-api");
    // The query string may hold a token, so reports leave it out.
    assert_eq!(prometheus.url, format!("{url}/metrics"));
    assert_eq!(prometheus.format, "prometheus");
    assert_eq!(prometheus.memory_kind.as_deref(), Some("resident"));
    assert!(prometheus.samples.len() >= 8, "{prometheus:?}");
    assert_eq!(prometheus.samples[0].t, 0.0);
    assert_eq!(prometheus.samples[0].cpu_percent, None);
    for sample in &prometheus.samples[1..] {
        let cpu = sample.cpu_percent.unwrap();
        assert!((35.0..=65.0).contains(&cpu), "{cpu}");
        assert_eq!(sample.memory_bytes, Some(1.5e8));
    }
    assert!(live.last().unwrap()[0] >= 3, "{live:?}");

    // The health check went down and came back.
    assert_eq!(prometheus.health_url, Some(format!("{url}/health")));
    let statuses: Vec<&str> = prometheus
        .health
        .iter()
        .map(|h| h.status.as_str())
        .collect();
    assert_eq!(statuses, ["UP", "DOWN", "UP"], "{:?}", prometheus.health);
    assert_eq!(prometheus.health[1].detail.as_deref(), Some("db: DOWN"));
    let periods = prometheus.unhealthy_periods();
    assert_eq!(periods.len(), 1);
    let lasted = periods[0].to.unwrap() - periods[0].from;
    assert!((0.4..=1.2).contains(&lasted), "{periods:?}");

    // No Prometheus registry: Actuator's JSON instead.
    assert_eq!(actuator.format, "actuator");
    assert_eq!(actuator.cpu_cores, Some(8.0));
    assert_eq!(actuator.samples[0].cpu_percent, Some(80.0));
    assert_eq!(actuator.samples[0].memory_bytes, Some(2e8));
    assert_eq!(actuator.memory_kind.as_deref(), Some("jvm"));
    // Actuator's /health is read without being asked; this API has none.
    assert!(actuator.health.is_empty());
    assert_eq!(actuator.health_error.as_deref(), Some("404 Not Found"));

    assert!(unauthorized.samples.is_empty());
    assert!(unauthorized.failed_reads >= 2);
    assert_eq!(unauthorized.error.as_deref(), Some("401 Unauthorized"));

    assert_eq!(unset.error.as_deref(), Some("`{{nowhere}}` has no value"));
    // A URL that does not parse is not copied into the report.
    assert_eq!(unset.url, "(not a valid URL)");
    assert_eq!(unset.name, "monitor 4");
    assert!(unset.failing);

    // Errors leave the URL, and the token in it, out.
    assert_eq!(refused.url, format!("{closing_url}/metrics"));
    let error = refused.error.as_deref().unwrap();
    assert!(!error.contains("s3cret"), "{error}");
    assert!(refused.failing);
    // A later success clears `failing` but keeps the last reason.
    assert!(!prometheus.failing);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_first_reading_is_taken_before_any_load() {
    use std::sync::atomic::{AtomicU64, Ordering};
    let hits = std::sync::Arc::new(AtomicU64::new(0));
    let counted = hits.clone();
    // A slow metrics endpoint whose "memory" is the number of requests
    // the API has had so far.
    let app = Router::new()
        .route(
            "/fast",
            get(move || {
                counted.fetch_add(1, Ordering::Relaxed);
                async { "ok" }
            }),
        )
        .route(
            "/metrics",
            get(move || {
                let seen = hits.load(Ordering::Relaxed);
                async move {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    format!("process_resident_memory_bytes {seen}\n")
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let test = test_file(
        r#"
flow:
  name: baseline
  steps:
    - name: fast
      request: { method: GET, url: "{{base}}/fast" }
load:
  virtual_users: { count: 2, duration: 500ms }
monitor:
  - prometheus: { url: "{{base}}/metrics" }
"#,
    );
    let runner = Runner::new(RunOptions::default()).unwrap();
    let options = LoadOptions {
        monitor: test.monitor.clone(),
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
    let first = &report.resources[0].samples[0];
    assert_eq!(first.t, 0.0);
    assert_eq!(first.memory_bytes, Some(0.0), "the baseline saw load");
    assert!(report.summary.requests > 0);
}
