use std::collections::BTreeMap;
use std::time::Duration;

use stormix_engine::{Check, FlowResult, LoadReport, StepResult};
use stormix_metrics::{Recorder, Sample, ThresholdResult};

use super::*;

fn step(name: &str, status: u16, passed: bool) -> StepResult {
    StepResult {
        name: name.into(),
        method: "GET".into(),
        url: "http://x.test/a?b=1&c=2".into(),
        status: Some(status),
        duration_ms: 12.5,
        time_to_headers_ms: None,
        response_bytes: 2,
        checks: vec![Check {
            name: "status is 200".into(),
            passed,
            message: (!passed).then(|| format!("got {status}")),
        }],
        extracted: BTreeMap::new(),
        warnings: Vec::new(),
        logs: vec!["hello".into()],
        error: None,
        skipped: false,
        cached: false,
        trace_id: None,
        span_id: None,
        exchange: None,
    }
}

#[test]
fn flow_report_shows_each_step_and_escapes_text() {
    let html = flow_report(&FlowResult {
        name: "Login <admin>".into(),
        steps: vec![step("Log in", 200, true), step("Read <b>", 500, false)],
        duration_ms: 25.0,
    });
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("Run failed"));
    assert!(html.contains("Login &lt;admin&gt;"));
    assert!(html.contains("Read &lt;b&gt;"));
    assert!(!html.contains("<b>"));
    assert!(html.contains("got 500"));
    assert!(html.contains("a?b=1&amp;c=2"));
    assert!(html.contains("1/2"), "steps passed tile");
    // Self-contained: no scripts, no external resources.
    assert!(!html.contains("<script") && !html.contains("http-equiv") && !html.contains("<link"));
}

#[test]
fn load_report_has_thresholds_timeline_and_steps() {
    let mut recorder = Recorder::new();
    for second in 0..3u32 {
        for i in 0..10 {
            recorder.record(&Sample {
                step: "Log in",
                duration_ms: 5.0 + f64::from(i),
                error: (second == 1 && i == 0).then_some("connection reset"),
                status: Some(200),
                checks_passed: 1,
                failed_checks: &[],
                bytes: 100,
            });
        }
        recorder.close_second(3);
    }
    let report = LoadReport {
        name: "Login".into(),
        summary: recorder.summary(Duration::from_secs(3)),
        thresholds: vec![ThresholdResult {
            threshold: "p95 < 10ms".into(),
            actual: 13.0,
            passed: false,
        }],
        samples: Vec::new(),
        setup: Vec::new(),
        slowest: Vec::new(),
        aborted: false,
        resources: Vec::new(),
    };
    let html = load_report(&report);
    assert!(html.contains("Thresholds failed"));
    assert!(html.contains("p95 &lt; 10ms"));
    assert!(html.contains("<svg"));
    assert_eq!(
        html.matches(r#"class="fail""#).count(),
        1,
        "one second had a failure"
    );
    assert!(html.contains("<td>Log in</td><td>30</td>"));
    assert!(html.contains("Most common failures"));
}

#[test]
fn load_report_shows_the_apis_resources() {
    use stormix_engine::{ResourceSample, ResourceSeries};
    let recorder = stormix_metrics::Recorder::new();
    let sample = |t: f64, cpu: Option<f64>, mem: f64| ResourceSample {
        t,
        cpu_percent: cpu,
        memory_bytes: Some(mem),
        heap_bytes: Some(mem / 2.0),
        gc_ms_per_s: cpu.map(|_| 4.0),
    };
    let report = LoadReport {
        name: "Watched".into(),
        summary: recorder.summary(Duration::from_secs(2)),
        thresholds: Vec::new(),
        samples: Vec::new(),
        setup: Vec::new(),
        slowest: Vec::new(),
        aborted: false,
        resources: vec![
            ResourceSeries {
                name: "orders-api".into(),
                url: "http://api/actuator/prometheus".into(),
                format: "prometheus".into(),
                cpu_cores: Some(4.0),
                heap_max_bytes: Some(512.0 * 1_048_576.0),
                memory_kind: Some("jvm".into()),
                samples: vec![
                    sample(0.0, None, 200.0 * 1_048_576.0),
                    sample(1.0, Some(80.0), 300.0 * 1_048_576.0),
                    sample(2.0, Some(120.0), 250.0 * 1_048_576.0),
                ],
                failed_reads: 0,
                error: None,
            },
            ResourceSeries {
                name: "<down>".into(),
                url: "http://down/metrics".into(),
                format: "prometheus".into(),
                cpu_cores: None,
                heap_max_bytes: None,
                memory_kind: None,
                samples: Vec::new(),
                failed_reads: 3,
                error: Some("401 Unauthorized".into()),
            },
        ],
    };
    let html = load_report(&report);
    assert!(html.contains("API resources · orders-api"));
    assert!(html.contains(">100%<"), "CPU average");
    assert!(html.contains(">120%<"), "CPU peak");
    assert!(html.contains("300.0 MB"));
    assert!(html.contains("150.0 MB / 512.0 MB"), "heap peak of max");
    assert!(html.contains("of 4 cores"));
    assert!(html.contains(r#"class="cpu""#));
    assert!(html.contains("API resources · &lt;down&gt;"));
    assert!(html.contains("Could not read it: 401 Unauthorized"));
}
