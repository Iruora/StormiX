use super::*;

fn ok(step: &str, ms: f64) -> Sample<'_> {
    Sample {
        step,
        duration_ms: ms,
        error: None,
        status: Some(200),
        checks_passed: 1,
        failed_checks: &[],
        bytes: 100,
    }
}

#[test]
fn what_counts_as_failed() {
    assert!(!ok("a", 1.0).failed());
    let refused = Sample {
        error: Some("connection refused"),
        status: None,
        checks_passed: 0,
        ..ok("a", 1.0)
    };
    assert!(refused.failed());
    let unchecked_500 = Sample {
        status: Some(500),
        checks_passed: 0,
        ..ok("a", 1.0)
    };
    assert!(unchecked_500.failed());
    // A step that checks for 404 and gets it is fine.
    let expected_404 = Sample {
        status: Some(404),
        ..ok("a", 1.0)
    };
    assert!(!expected_404.failed());
    let failed_check = Sample {
        failed_checks: &["status is 200"],
        ..ok("a", 1.0)
    };
    assert!(failed_check.failed());
}

#[test]
fn summarizes_latency_counts_and_steps() {
    let mut r = Recorder::new();
    for i in 1..=100 {
        r.record(&ok("login", f64::from(i)));
    }
    r.record(&Sample {
        error: Some("connection refused"),
        status: None,
        checks_passed: 0,
        ..ok("me", 9999.0)
    });
    r.record(&Sample {
        failed_checks: &["status is 200"],
        status: Some(401),
        checks_passed: 0,
        ..ok("me", 5.0)
    });
    r.iteration(true);
    r.iteration(false);
    r.dropped(3);

    let s = r.summary(Duration::from_secs(2));
    assert_eq!(s.requests, 102);
    assert_eq!(s.failed_requests, 2);
    assert_eq!(s.rps, 51.0);
    assert_eq!(
        (s.iterations, s.failed_iterations, s.dropped_iterations),
        (2, 1, 3)
    );
    assert_eq!((s.checks_passed, s.checks_failed), (100, 1));
    // The refused request's time is not a latency.
    assert_eq!(s.latency.max, 100.0);
    assert!((s.latency.p95 - 95.0).abs() < 0.1, "{:?}", s.latency);
    assert_eq!(
        s.steps.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        ["login", "me"]
    );
    assert_eq!(s.steps[1].failed, 2);
    assert_eq!(
        s.failures,
        [
            Failure {
                reason: "connection refused".into(),
                count: 1
            },
            Failure {
                reason: "me: status is 200".into(),
                count: 1
            }
        ]
    );
}

#[test]
fn closes_seconds_for_the_timeline() {
    let mut r = Recorder::new();
    r.record(&ok("a", 10.0));
    r.record(&ok("a", 30.0));
    assert_eq!(r.close_second(4).requests, 2);
    let quiet = r.close_second(0).clone();
    assert_eq!((quiet.t, quiet.requests, quiet.p95_ms), (2, 0, 0.0));
    let s = r.summary(Duration::from_secs(2));
    assert_eq!(s.timeline.len(), 2);
    assert_eq!(s.timeline[0].active, 4);
    assert!((s.timeline[0].p95_ms - 30.0).abs() < 0.1);
}

#[test]
fn checks_thresholds() {
    let mut r = Recorder::new();
    for i in 1..=100 {
        r.record(&ok("a", f64::from(i)));
    }
    r.record(&Sample {
        status: Some(503),
        checks_passed: 0,
        ..ok("a", 1.0)
    });
    let s = r.summary(Duration::from_secs(10));
    let thresholds: Vec<Threshold> = [
        "p95 < 100ms",
        "p99 < 50ms",
        "errors < 1%",
        "rps >= 10",
        "checks > 99%",
    ]
    .iter()
    .map(|t| t.parse().unwrap())
    .collect();
    let results = check_thresholds(&thresholds, &s, &r);
    let verdicts: Vec<_> = results
        .iter()
        .map(|r| (r.threshold.as_str(), r.passed))
        .collect();
    assert_eq!(
        verdicts,
        [
            ("p95 < 100ms", true),
            ("p99 < 50ms", false),
            ("errors < 1%", true),
            ("rps >= 10", true),
            ("checks > 99%", true),
        ]
    );
    assert!((results[2].actual - 100.0 / 101.0).abs() < 1e-9);
}

#[test]
fn an_empty_run_has_zeros() {
    let s = Recorder::new().summary(Duration::ZERO);
    assert_eq!((s.requests, s.rps, s.latency.p99), (0, 0.0, 0.0));
    assert_eq!(s.error_rate(), 0.0);
    assert_eq!(s.check_rate(), 100.0);
}
