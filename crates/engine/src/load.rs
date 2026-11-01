//! Runs a flow many times at once, following a load profile, and measures it.
//!
//! Open models (`constant_rate`, `ramping`) start iterations on a clock, so
//! a slow API does not slow the arrivals: iterations that would exceed
//! [`LoadOptions::max_in_flight`] are dropped and counted. The closed model
//! (`virtual_users`) keeps a fixed number of users looping the flow.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use stormix_flow::{Flow, LoadProfile, Threshold};
use stormix_metrics::{check_thresholds, Recorder, Sample, Second, Summary, ThresholdResult};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::{Exchanges, FlowResult, Runner, StepResult, Variables};

#[derive(Debug, Clone)]
pub struct LoadOptions {
    /// Open models: the most iterations in flight at once. Iterations due
    /// beyond it are dropped, which the summary reports.
    pub max_in_flight: u32,
    /// How long iterations still running at the end may take to finish.
    pub grace: Duration,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            max_in_flight: 1000,
            grace: Duration::from_secs(30),
        }
    }
}

/// Sent once a second while the test runs.
#[derive(Debug)]
pub struct Progress<'a> {
    pub elapsed: Duration,
    pub total: Duration,
    /// The second that just ended.
    pub second: &'a Second,
    /// Requests so far.
    pub requests: u64,
    /// The rate (open models) or user count (closed model) aimed for now.
    pub target: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LoadReport {
    pub name: String,
    pub summary: Summary,
    pub thresholds: Vec<ThresholdResult>,
    /// The first failure of each step, with the request and response, so
    /// a failure count can be traced to a real exchange. Empty in reports
    /// saved before 1.1.
    #[serde(default)]
    pub samples: Vec<StepResult>,
    /// Steps run once before the load started, to fill the cache: every
    /// step up to the last one marked `cache: true`. Prerequisites like a
    /// login, so they are not in the summary or the thresholds. Empty when
    /// no step is cached.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setup: Vec<StepResult>,
}

/// Enough to show what each failing step did without keeping thousands.
const MAX_SAMPLES: usize = 20;

impl LoadReport {
    /// Every threshold held. A test without thresholds passes.
    pub fn passed(&self) -> bool {
        self.thresholds.iter().all(|t| t.passed)
    }
}

/// Runs `flow` under `profile` and checks `thresholds` at the end.
/// `on_second` is called once a second with live figures.
pub async fn run_load(
    runner: &Runner,
    flow: &Flow,
    overrides: &Variables,
    profile: &LoadProfile,
    thresholds: &[Threshold],
    options: &LoadOptions,
    mut on_second: impl FnMut(&Progress<'_>),
) -> LoadReport {
    let setup = match setup_flow(flow) {
        Some(prefix) => runner.run(&prefix, overrides).await.steps,
        None => Vec::new(),
    };
    let shared = Arc::new(Shared {
        runner: runner.keeping(Exchanges::FailedOnly),
        flow: flow.clone(),
        overrides: overrides.clone(),
        active: AtomicU32::new(0),
    });
    let total: Duration = profile.total_duration().into();
    let started = Instant::now();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut driver = tokio::spawn(drive(
        shared.clone(),
        profile.clone(),
        options.clone(),
        started,
        tx,
    ));

    let mut recorder = Recorder::new();
    let mut samples = Vec::new();
    let mut tick = tokio::time::interval_at(
        tokio::time::Instant::from_std(started) + Duration::from_secs(1),
        Duration::from_secs(1),
    );
    let outcome = loop {
        tokio::select! {
            Some(result) = rx.recv() => record(&mut recorder, &mut samples, result),
            _ = tick.tick() => {
                let elapsed = started.elapsed();
                let target = target(profile, elapsed.min(total));
                let requests = recorder.requests();
                let second = recorder.close_second(shared.active.load(Ordering::Relaxed));
                on_second(&Progress { elapsed, total, second, requests, target });
            }
            outcome = &mut driver => break outcome.unwrap_or_default(),
        }
    };
    let elapsed = started.elapsed();
    while let Ok(result) = rx.try_recv() {
        record(&mut recorder, &mut samples, result);
    }
    // The last, partial second, so its requests reach the timeline too.
    recorder.close_second(0);
    recorder.dropped(outcome.dropped);
    recorder.interrupted(outcome.interrupted);

    let summary = recorder.summary(elapsed);
    LoadReport {
        name: flow.name.clone(),
        thresholds: check_thresholds(thresholds, &summary, &recorder),
        summary,
        samples,
        setup,
    }
}

/// The steps to run once before the load, so cached steps are sent once
/// and iterations only reuse them: the enabled steps up to and including
/// the last cached one, since a cached step may need variables from the
/// steps before it. `None` when no enabled step is cached.
fn setup_flow(flow: &Flow) -> Option<Flow> {
    let enabled: Vec<_> = flow.enabled_steps().cloned().collect();
    let last = enabled.iter().rposition(|s| s.cache)?;
    let mut prefix = flow.clone();
    prefix.steps = enabled.into_iter().take(last + 1).collect();
    prefix.set_aside.clear();
    Some(prefix)
}

struct Shared {
    runner: Runner,
    flow: Flow,
    overrides: Variables,
    /// Iterations running right now.
    active: AtomicU32,
}

impl Shared {
    async fn iteration(&self) -> FlowResult {
        self.active.fetch_add(1, Ordering::Relaxed);
        let result = self.runner.run(&self.flow, &self.overrides).await;
        self.active.fetch_sub(1, Ordering::Relaxed);
        result
    }
}

#[derive(Debug, Default)]
struct Outcome {
    dropped: u32,
    interrupted: u32,
}

/// Starts iterations until the profile ends, then gives the ones still
/// running `grace` to finish.
async fn drive(
    shared: Arc<Shared>,
    profile: LoadProfile,
    options: LoadOptions,
    started: Instant,
    tx: mpsc::UnboundedSender<FlowResult>,
) -> Outcome {
    let total: Duration = profile.total_duration().into();
    let deadline = started + total;
    let mut tasks = JoinSet::new();
    let mut dropped = 0;

    match &profile {
        LoadProfile::VirtualUsers {
            count, think_time, ..
        } => {
            let think: Duration = think_time.map(Into::into).unwrap_or_default();
            for _ in 0..*count {
                let (shared, tx) = (shared.clone(), tx.clone());
                tasks.spawn(async move {
                    while Instant::now() < deadline {
                        let result = shared.iteration().await;
                        if tx.send(result).is_err() {
                            return;
                        }
                        let left = deadline.saturating_duration_since(Instant::now());
                        if !think.is_zero() {
                            tokio::time::sleep(think.min(left)).await;
                        }
                    }
                });
            }
        }
        open => {
            let schedule = Schedule::new(open);
            let mut due_started = 0u64;
            loop {
                let t = started.elapsed().min(total);
                while tasks.try_join_next().is_some() {}
                // Rounded a hair up, so float error cannot lose the last one.
                let due = (schedule.iterations_by(t.as_secs_f64()) + 1e-9) as u64;
                while due_started < due {
                    due_started += 1;
                    if tasks.len() >= options.max_in_flight as usize {
                        dropped += 1;
                        continue;
                    }
                    let (shared, tx) = (shared.clone(), tx.clone());
                    tasks.spawn(async move {
                        let _ = tx.send(shared.iteration().await);
                    });
                }
                if t >= total {
                    break;
                }
                // Wake about once per due iteration, within 1 to 50 ms.
                let rate = schedule.rate_at(t.as_secs_f64());
                let wait = if rate > 0.0 {
                    (1.0 / rate).clamp(0.001, 0.05)
                } else {
                    0.05
                };
                tokio::time::sleep(Duration::from_secs_f64(wait)).await;
            }
        }
    }
    drop(tx);

    let finish = async { while tasks.join_next().await.is_some() {} };
    let interrupted = match tokio::time::timeout(options.grace, finish).await {
        Ok(()) => 0,
        Err(_) => {
            let left = tasks.len() as u32;
            tasks.abort_all();
            // Virtual users between iterations stop on their own; those
            // still in the set were mid-iteration.
            left
        }
    };
    Outcome {
        dropped,
        interrupted,
    }
}

fn record(recorder: &mut Recorder, samples: &mut Vec<StepResult>, result: FlowResult) {
    for step in result.steps.iter().filter(|s| !s.skipped) {
        if step.cached {
            recorder.cached(&step.name);
            continue;
        }
        let failed: Vec<&str> = step
            .checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| c.name.as_str())
            .collect();
        recorder.record(&Sample {
            step: &step.name,
            duration_ms: step.duration_ms,
            error: step.error.as_deref(),
            status: step.status,
            checks_passed: step.checks.iter().filter(|c| c.passed).count() as u32,
            failed_checks: &failed,
            bytes: step.response_bytes,
        });
    }
    recorder.iteration(result.passed());
    for step in result.steps {
        if samples.len() < MAX_SAMPLES
            && !step.skipped
            && (!step.passed() || step.status.is_some_and(|s| s >= 400))
            && !samples.iter().any(|s| s.name == step.name)
        {
            samples.push(step);
        }
    }
}

fn target(profile: &LoadProfile, elapsed: Duration) -> f64 {
    match profile {
        LoadProfile::VirtualUsers { count, .. } => f64::from(*count),
        open => Schedule::new(open).rate_at(elapsed.as_secs_f64()),
    }
}

/// The arrival rate of an open model over time, as linear segments.
#[derive(Debug, Clone)]
struct Schedule {
    /// (seconds, rate at start, rate at end)
    segments: Vec<(f64, f64, f64)>,
}

impl Schedule {
    fn new(profile: &LoadProfile) -> Self {
        let secs = |d: stormix_flow::Duration| d.as_millis() as f64 / 1000.0;
        let segments = match profile {
            LoadProfile::ConstantRate { rate, duration } => {
                vec![(secs(*duration), f64::from(*rate), f64::from(*rate))]
            }
            LoadProfile::Ramping { start_rate, stages } => {
                let mut from = f64::from(*start_rate);
                stages
                    .iter()
                    .map(|stage| {
                        let to = f64::from(stage.target);
                        let segment = (secs(stage.duration), from, to);
                        from = to;
                        segment
                    })
                    .collect()
            }
            LoadProfile::VirtualUsers { .. } => Vec::new(),
        };
        Self { segments }
    }

    fn rate_at(&self, t: f64) -> f64 {
        let mut start = 0.0;
        for &(len, from, to) in &self.segments {
            if t < start + len {
                return from + (to - from) * (t - start) / len;
            }
            start += len;
        }
        self.segments.last().map_or(0.0, |s| s.2)
    }

    /// How many iterations should have started by `t`: the area under the
    /// rate curve.
    fn iterations_by(&self, t: f64) -> f64 {
        let mut start = 0.0;
        let mut total = 0.0;
        for &(len, from, to) in &self.segments {
            if t <= start {
                break;
            }
            let x = (t - start).min(len);
            total += from * x + (to - from) * x * x / (2.0 * len);
            start += len;
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stormix_flow::Stage;

    fn d(secs: u64) -> stormix_flow::Duration {
        stormix_flow::Duration::from_secs(secs)
    }

    #[test]
    fn constant_rate_schedule() {
        let s = Schedule::new(&LoadProfile::ConstantRate {
            rate: 10,
            duration: d(5),
        });
        assert_eq!(s.iterations_by(0.0), 0.0);
        assert_eq!(s.iterations_by(2.5), 25.0);
        assert_eq!(s.iterations_by(9.0), 50.0);
        assert_eq!(s.rate_at(1.0), 10.0);
    }

    #[test]
    fn ramping_schedule_integrates_each_stage() {
        let s = Schedule::new(&LoadProfile::Ramping {
            start_rate: 0,
            stages: vec![
                Stage {
                    duration: d(10),
                    target: 20,
                },
                Stage {
                    duration: d(10),
                    target: 20,
                },
                Stage {
                    duration: d(5),
                    target: 0,
                },
            ],
        });
        assert_eq!(s.rate_at(5.0), 10.0);
        assert_eq!(s.iterations_by(10.0), 100.0);
        assert_eq!(s.iterations_by(20.0), 300.0);
        assert_eq!(s.iterations_by(25.0), 350.0);
        assert_eq!(s.rate_at(30.0), 0.0);
    }
}
