use std::collections::BTreeMap;

use std::time::Duration;

use stormix_engine::{Check, StepResult};
use stormix_metrics::{Recorder, Sample};

use super::*;

fn step(name: &str, status: u16, checks: &[(&str, bool)]) -> StepResult {
    StepResult {
        name: name.into(),
        method: "GET".into(),
        url: "http://x.test".into(),
        status: Some(status),
        duration_ms: 10.0,
        time_to_headers_ms: Some(9.0),
        response_bytes: 2,
        checks: checks
            .iter()
            .map(|&(name, passed)| Check {
                name: name.into(),
                passed,
                message: (!passed).then(|| format!("got {status}")),
            })
            .collect(),
        extracted: BTreeMap::new(),
        warnings: Vec::new(),
        logs: Vec::new(),
        error: None,
        skipped: false,
        cached: false,
        exchange: None,
    }
}

fn run(steps: Vec<StepResult>) -> RunData {
    RunData::Functional(FlowResult {
        name: "Login".into(),
        steps,
        duration_ms: 20.0,
    })
}

#[test]
fn saves_lists_reads_and_deletes_runs() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("runs.db")).unwrap();
    let first = store
        .save(
            &run(vec![step("a", 200, &[("status is 200", true)])]),
            Some("login.yaml"),
        )
        .unwrap();
    let second = store
        .save(
            &run(vec![step("a", 500, &[("status is 200", false)])]),
            None,
        )
        .unwrap();
    assert!(first.passed && !second.passed);
    assert_eq!(second.headline, "0/1 steps passed, 0/1 checks");

    let list = store.list(10).unwrap();
    assert_eq!(
        list.iter().map(|r| r.id).collect::<Vec<_>>(),
        [second.id, first.id]
    );
    assert_eq!(list[1].source.as_deref(), Some("login.yaml"));

    let stored = store.get(first.id).unwrap();
    assert_eq!(stored.summary, first);
    assert!(matches!(stored.data, RunData::Functional(ref r) if r.passed()));

    store.delete(first.id).unwrap();
    assert!(matches!(store.get(first.id), Err(StoreError::NotFound(_))));
    assert_eq!(store.list(10).unwrap().len(), 1);
}

#[test]
fn functional_diff_shows_what_broke_and_what_was_fixed() {
    let before = run(vec![
        step(
            "login",
            200,
            &[("status is 200", true), ("has token", true)],
        ),
        step("profile", 401, &[("status is 200", false)]),
        step("old", 200, &[]),
    ]);
    let after = run(vec![
        step(
            "login",
            500,
            &[("status is 200", false), ("has token", true)],
        ),
        step("profile", 200, &[("status is 200", true)]),
        step("new", 200, &[]),
    ]);
    let RunDiff::Functional {
        steps,
        broke,
        fixed,
    } = diff(&before, &after)
    else {
        panic!("expected a functional diff")
    };
    assert_eq!((broke, fixed), (1, 1));
    let changes: Vec<_> = steps.iter().map(|s| (s.name.as_str(), s.change)).collect();
    assert_eq!(
        changes,
        [
            ("login", StepChange::Broke),
            ("profile", StepChange::Fixed),
            ("new", StepChange::Added),
            ("old", StepChange::Removed),
        ]
    );
    // Only the check that changed is listed.
    assert_eq!(
        steps[0].checks,
        [CheckChange {
            name: "status is 200".into(),
            before: Some(true),
            after: Some(false),
            message: Some("got 500".into()),
        }]
    );
    assert_eq!(
        (steps[0].before_status, steps[0].after_status),
        (Some(200), Some(500))
    );
}

fn load(latency_ms: f64, failed: usize, seconds: u64) -> RunData {
    let mut recorder = Recorder::new();
    for i in 0..100 {
        recorder.record(&Sample {
            step: "login",
            duration_ms: latency_ms,
            error: None,
            status: Some(if i < failed { 500 } else { 200 }),
            checks_passed: 0,
            failed_checks: &[],
            bytes: 2,
        });
    }
    RunData::Load(LoadReport {
        name: "Login".into(),
        summary: recorder.summary(Duration::from_secs(seconds)),
        thresholds: Vec::new(),
        samples: Vec::new(),
    })
}

#[test]
fn load_diff_flags_regressions_beyond_noise() {
    let RunDiff::Load(d) = diff(&load(10.0, 0, 10), &load(15.0, 5, 10)) else {
        panic!("expected a load diff")
    };
    let flags: Vec<_> = d
        .metrics
        .iter()
        .map(|m| (m.name.as_str(), m.worse, m.better))
        .collect();
    assert_eq!(
        flags,
        [
            ("Requests/s", false, false),
            ("p50", true, false),
            ("p95", true, false),
            ("p99", true, false),
            ("Failed requests", true, false),
            ("Checks passed", false, false),
        ]
    );
    assert_eq!(d.steps[0].before_failed, Some(0));
    assert_eq!(d.steps[0].after_failed, Some(5));

    // Twice the throughput is better, not worse.
    let RunDiff::Load(d) = diff(&load(10.0, 0, 10), &load(10.0, 0, 5)) else {
        unreachable!()
    };
    assert!(d.metrics[0].better && !d.metrics[0].worse);
}

#[test]
fn different_kinds_do_not_compare() {
    assert_eq!(diff(&run(Vec::new()), &load(1.0, 0, 1)), RunDiff::Mismatch);
}
