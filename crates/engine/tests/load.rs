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
            "/quick",
            get(|| async { "process_resident_memory_bytes 1\n" }),
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
  - prometheus: { url: "{{base}}/quick", interval: 250ms }
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
    // A fast monitor does not pile up baseline readings while it waits.
    let quick = &report.resources[1].samples;
    assert_eq!(quick.iter().filter(|s| s.t == 0.0).count(), 1, "{quick:?}");
    assert!(quick.len() > 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn dynatrace_gives_minutes_from_before_the_load_and_reads_once_more_at_the_end() {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use axum::http::{HeaderMap, Uri};

    let now_ms = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    };
    // Each query's Authorization header and parameters.
    type Seen = Vec<(String, HashMap<String, String>)>;
    let seen: Arc<Mutex<Seen>> = Arc::default();
    let log = seen.clone();
    // Two processes; the minute still going ends in the future, as with
    // Dynatrace, and a minute with no data yet is null.
    let query = move |headers: HeaderMap, uri: Uri| {
        let q: HashMap<String, String> = reqwest::Url::parse(&format!("http://dt{uri}"))
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect();
        let auth = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let now = now_ms();
        let (timestamps, cpu, memory) = match q.get("from").map(String::as_str) {
            Some("now-5m") => (vec![now - 30_000], vec![5.0], vec![100e6]),
            from => {
                let from: i64 = from.unwrap().parse().unwrap();
                (
                    vec![from + 60_000, from + 120_000, now + 30_000],
                    vec![4.0, f64::NAN, 80.0],
                    vec![90e6, f64::NAN, 300e6],
                )
            }
        };
        log.lock().unwrap().push((auth, q));
        let values = |list: &[f64], share: f64| {
            list.iter()
                .map(|v| {
                    if v.is_nan() {
                        serde_json::Value::Null
                    } else {
                        (v * share).into()
                    }
                })
                .collect::<Vec<_>>()
        };
        let body = serde_json::json!({
            "totalCount": 2,
            "resolution": "1m",
            "result": [
                { "metricId": "builtin:tech.generic.cpu.usage:splitBy():sum",
                  "data": [{ "dimensionMap": {}, "timestamps": timestamps, "values": values(&cpu, 1.0) }] },
                { "metricId": "builtin:tech.generic.mem.workingSetSize:splitBy():sum",
                  "data": [
                    { "dimensionMap": {}, "timestamps": timestamps, "values": values(&memory, 0.5) },
                    { "dimensionMap": {}, "timestamps": timestamps, "values": values(&memory, 0.5) }
                  ] }
            ]
        });
        async move { axum::Json(body) }
    };
    let app = Router::new()
        .route("/fast", get(|| async { "ok" }))
        .route("/e/env1/api/v2/metrics/query", get(query));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let test = test_file(
        r#"
flow:
  name: dynatrace
  variables: { dt_token: dt0c01.secret }
  steps:
    - name: fast
      request: { method: GET, url: "{{base}}/fast" }
load:
  virtual_users: { count: 1, duration: 500ms }
monitor:
  - dynatrace:
      url: "{{base}}/e/env1"
      entity: 'type("PROCESS_GROUP_INSTANCE"),entityName.contains("orders")'
      token: "{{dt_token}}"
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
    let seen = seen.lock().unwrap();
    // The baseline, then one last read once the load ended: 30 s apart
    // otherwise.
    assert_eq!(seen.len(), 2, "{seen:?}");
    for (auth, q) in seen.iter() {
        assert_eq!(auth, "Api-Token dt0c01.secret");
        assert_eq!(
            q["entitySelector"],
            r#"type("PROCESS_GROUP_INSTANCE"),entityName.contains("orders")"#
        );
        assert_eq!(q["resolution"], "1m");
        assert!(q["metricSelector"].contains("builtin:tech.generic.cpu.usage"));
    }
    assert!(seen[1].1["from"].parse::<i64>().is_ok());
    let series = &report.resources[0];
    assert_eq!(series.format, "dynatrace");
    assert_eq!(series.memory_kind.as_deref(), Some("working set"));
    assert_eq!(series.failed_reads, 0, "{:?}", series.error);
    assert!(!series.url.contains("secret"));
    let samples = &series.samples;
    assert_eq!(samples.len(), 2, "{samples:?}");
    // The minute that ended before the load is its baseline...
    assert_eq!(samples[0].t, 0.0);
    assert_eq!(samples[0].cpu_percent, Some(4.0));
    assert_eq!(samples[0].memory_bytes, Some(90e6));
    // ...and the minute still going sits at the end of the test, not
    // in the future.
    assert!(samples[1].t > 0.0 && samples[1].t < 5.0, "{samples:?}");
    assert_eq!(samples[1].cpu_percent, Some(80.0));
    assert_eq!(samples[1].memory_bytes, Some(300e6));
}

/// An API that serves one request at a time in 20 ms: about 50 a second,
/// with a queue building beyond that.
async fn serve_capped() -> String {
    let permit = std::sync::Arc::new(tokio::sync::Semaphore::new(1));
    let app = Router::new().route(
        "/capped",
        get(move || {
            let permit = permit.clone();
            async move {
                let _held = permit.acquire().await.unwrap();
                tokio::time::sleep(Duration::from_millis(20)).await;
                "ok"
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

#[tokio::test(flavor = "multi_thread")]
async fn find_limit_stops_at_the_first_step_that_breaks_a_threshold() {
    let url = serve_capped().await;
    let test = test_file(
        r#"
flow:
  name: capped
  steps:
    - name: capped
      request: { method: GET, url: "{{base}}/capped" }
load:
  find_limit: { start_rate: 10, increase: 20, every: 1s, max_rate: 150 }
capacity: { peak_rps: 100, growth: 0%, target_cpu: 50%, spare_pods: 1 }
"#,
    );
    let runner = Runner::new(RunOptions::default()).unwrap();
    let mut live = Vec::new();
    let report = run_load(
        &runner,
        &test.flow,
        &base(&url),
        test.load.as_ref().unwrap(),
        &thresholds(&["p95 < 150ms", "errors < 1%", "rps > 1"]),
        &LoadOptions {
            capacity: test.capacity.clone(),
            ..LoadOptions::default()
        },
        |p| live.push(p.limit_steps.len()),
    )
    .await;

    let limit = report.limit.as_ref().expect("a find_limit report");
    let rates: Vec<u32> = limit.steps.iter().map(|s| s.rate).collect();
    assert!(limit.reached, "{rates:?}");
    // 10 and 30 a second are well under the cap; 90 and above cannot hold.
    let sustained = limit.sustained.expect("a step held") as usize;
    assert!(
        (1..=3).contains(&sustained) && rates.len() == sustained + 2,
        "{rates:?}, sustained {sustained}"
    );
    assert!(rates.len() < 8, "it went on after a step broke: {rates:?}");
    assert!(!limit.steps.last().unwrap().passed);
    assert!(limit.steps[..=sustained].iter().all(|s| s.passed));
    // The test ended at the limit, not at 150/s, and counts as passed with
    // the sustained step's checks plus the run's own rps.
    assert!(!report.aborted);
    assert!(report.passed(), "{:?}", report.thresholds);
    assert_eq!(report.thresholds.len(), 3);
    assert!(report.summary.duration_ms < 7000.0);
    assert!(live.windows(2).all(|w| w[0] <= w[1]));

    // No monitor, so the sustained step is taken as saturated: half of
    // its rate is usable at a 50% target.
    let sizing = limit
        .sizing
        .as_ref()
        .expect("sized from the capacity section");
    assert_eq!(sizing.basis, "assumed");
    let held = &limit.steps[sustained];
    assert!((sizing.usable_rps_per_pod - held.rps / 2.0).abs() < 1e-9);
    assert_eq!(
        sizing.pods,
        (100.0 / sizing.usable_rps_per_pod).ceil() as u32 + 1
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cpu_threshold_without_a_monitor_breaks_the_first_find_limit_step() {
    let url = serve().await;
    let test = test_file(
        r#"
flow:
  name: fast
  steps:
    - name: fast
      request: { method: GET, url: "{{base}}/fast" }
load:
  find_limit: { start_rate: 5, increase: 5, every: 1s, max_rate: 20 }
"#,
    );
    let runner = Runner::new(RunOptions::default()).unwrap();
    let report = run_load(
        &runner,
        &test.flow,
        &base(&url),
        test.load.as_ref().unwrap(),
        &thresholds(&["cpu < 90%"]),
        &LoadOptions::default(),
        |_| {},
    )
    .await;
    let limit = report.limit.as_ref().unwrap();
    assert_eq!(limit.steps.len(), 1);
    let step = &limit.steps[0];
    // The step served requests, so the CPU threshold alone broke it.
    assert!(step.requests > 0);
    assert!(!step.passed);
    assert_eq!(step.thresholds.len(), 1);
    assert!(!step.thresholds[0].passed);
    assert_eq!(limit.sustained, None);
    assert!(!report.passed());
}

/// Serves `/user/{id}`, counting the requests for each ID.
async fn serve_users() -> (
    String,
    std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<String, u32>>>,
) {
    use std::sync::{Arc, Mutex};
    let hits: Arc<Mutex<std::collections::BTreeMap<String, u32>>> = Arc::default();
    let seen = hits.clone();
    let app = Router::new().route(
        "/user/{id}",
        get(
            move |axum::extract::Path(id): axum::extract::Path<String>| {
                let seen = seen.clone();
                async move {
                    *seen.lock().unwrap().entry(id).or_default() += 1;
                    "ok"
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), hits)
}

#[tokio::test(flavor = "multi_thread")]
async fn each_virtual_user_keeps_the_row_it_picked() {
    let (url, hits) = serve_users().await;
    let test = test_file(
        r#"
flow:
  name: users
  steps:
    - name: user
      request: { method: GET, url: "{{base}}/user/{{userId}}" }
load:
  virtual_users: { count: 5, duration: 1s, think_time: 100ms }
data:
  - variable: userId
    values: [user-1, user-2]
    pick: sequential
    per: user
"#,
    );
    let runner = Runner::new(RunOptions::default()).unwrap();
    let options = LoadOptions {
        data: std::sync::Arc::new(stormix_engine::DataFeed::load(&test.data, None).unwrap()),
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
    // Users 1, 3 and 5 take user-1; users 2 and 4 take user-2.
    let data = &report.data[0];
    assert_eq!(data.picks, 5);
    let counts: Vec<u64> = data.counts.iter().map(|c| c.count).collect();
    assert_eq!(counts, [3, 2]);
    let hits = hits.lock().unwrap().clone();
    assert_eq!(hits.keys().collect::<Vec<_>>(), ["user-1", "user-2"]);
    // Each user loops on its own ID, so user-1 gets about 3/5 of the traffic.
    let total = hits.values().sum::<u32>() as f64;
    let share = f64::from(hits["user-1"]) / total;
    assert!((0.5..0.7).contains(&share), "{hits:?}");
    assert_eq!(report.summary.failed_requests, 0);
    assert!(!report.data_ran_out);
}

#[tokio::test(flavor = "multi_thread")]
async fn unique_data_ends_the_test_when_every_row_is_used() {
    let (url, hits) = serve_users().await;
    let test = test_file(
        r#"
flow:
  name: users
  steps:
    - name: user
      request: { method: GET, url: "{{base}}/user/{{userId}}" }
load:
  constant_rate: { rate: 50, duration: 5s }
data:
  - variable: userId
    values: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    pick: unique
"#,
    );
    let runner = Runner::new(RunOptions::default()).unwrap();
    let options = LoadOptions {
        data: std::sync::Arc::new(stormix_engine::DataFeed::load(&test.data, None).unwrap()),
        ..LoadOptions::default()
    };
    let report = run_load(
        &runner,
        &test.flow,
        &base(&url),
        test.load.as_ref().unwrap(),
        &thresholds(&["errors < 1%"]),
        &options,
        |_| {},
    )
    .await;
    assert_eq!(report.summary.iterations, 10);
    assert!(report.data_ran_out);
    assert!(report.data[0].ran_out);
    assert!(!report.aborted);
    assert!(report.passed(), "{:?}", report.thresholds);
    assert!(
        report.summary.duration_ms < 2000.0,
        "{}",
        report.summary.duration_ms
    );
    let hits = hits.lock().unwrap().clone();
    assert_eq!(hits.len(), 10);
    assert!(hits.values().all(|&n| n == 1), "{hits:?}");
}
