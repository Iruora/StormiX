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
use stormix_flow::{Capacity, Flow, LoadProfile, Metric, Monitor, Threshold};
use stormix_metrics::{check_thresholds, Recorder, Sample, Second, Summary, ThresholdResult};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;

use crate::capacity::{self, LimitSearch, LimitStep};
use crate::monitor::{self, ResourceSeries};
use crate::{Exchanges, FlowResult, Runner, StepResult, Variables};

#[derive(Debug, Clone)]
pub struct LoadOptions {
    /// Open models: the most iterations in flight at once. Iterations due
    /// beyond it are dropped, which the summary reports.
    pub max_in_flight: u32,
    /// How long iterations still running at the end may take to finish.
    pub grace: Duration,
    /// Stops the test early when triggered; see [`StopSignal`].
    pub stop: StopSignal,
    /// After a stop, how long iterations still running may take to finish.
    /// Shorter than `grace`, since whoever stopped the test wants it over.
    pub stop_grace: Duration,
    /// Metrics endpoints of the API, read throughout the test for its CPU
    /// and memory use: the test file's `monitor` section.
    pub monitor: Vec<Monitor>,
    /// `find_limit` only: the production traffic to size pods for, from
    /// the test file's `capacity` section.
    pub capacity: Option<Capacity>,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            max_in_flight: 1000,
            grace: Duration::from_secs(30),
            stop: StopSignal::default(),
            stop_grace: Duration::from_secs(3),
            monitor: Vec::new(),
            capacity: None,
        }
    }
}

/// Stops a running load test early, from another task or thread. No new
/// iterations start once it is triggered; those already running get
/// [`LoadOptions::stop_grace`] to finish and the rest are cut off. The
/// report keeps what was measured until then and is marked
/// [`LoadReport::aborted`]. Clones share one signal.
#[derive(Debug, Clone)]
pub struct StopSignal(Arc<watch::Sender<bool>>);

impl Default for StopSignal {
    fn default() -> Self {
        Self(Arc::new(watch::Sender::new(false)))
    }
}

impl StopSignal {
    pub fn stop(&self) {
        self.0.send_replace(true);
    }

    /// Whether `other` is a clone of this signal, not just another one.
    pub fn same_as(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub fn is_stopped(&self) -> bool {
        *self.0.borrow()
    }

    /// Resolves once [`StopSignal::stop`] has been called.
    pub async fn stopped(&self) {
        let mut rx = self.0.subscribe();
        // The sender lives in `self`, so this cannot fail.
        let _ = rx.wait_for(|stopped| *stopped).await;
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
    /// The API's CPU and memory so far, one series per `monitor` entry.
    pub resources: &'a [ResourceSeries],
    /// `find_limit` only: the steps finished so far.
    pub limit_steps: &'a [LimitStep],
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
    /// The slowest requests of each step that went out with a trace ID,
    /// slowest first, to look up in Dynatrace or another APM tool. Empty
    /// when the flow has no `trace`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slowest: Vec<SlowRequest>,
    /// Stopped before the profile ended (see [`StopSignal`]): the figures
    /// cover only the time it ran, and the test does not pass.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub aborted: bool,
    /// The API's own CPU and memory over the test, one series per
    /// `monitor` entry. Empty when the file has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<ResourceSeries>,
    /// `find_limit` only: each step's figures, the most the API sustained
    /// and, with a `capacity` section, the pods that means.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<LimitSearch>,
}

/// One traced request from a load test.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SlowRequest {
    pub step: String,
    pub trace_id: String,
    pub span_id: String,
    pub duration_ms: f64,
    pub status: Option<u16>,
    pub passed: bool,
}

/// How many of each step's slowest traced requests a report keeps.
const SLOWEST_PER_STEP: usize = 5;

/// Enough to show what each failing step did without keeping thousands.
const MAX_SAMPLES: usize = 20;

impl LoadReport {
    /// It ran to the end and every threshold held. A test without
    /// thresholds passes unless it was stopped.
    pub fn passed(&self) -> bool {
        !self.aborted && self.thresholds.iter().all(|t| t.passed)
    }
}

/// Runs `flow` under `profile` and checks `thresholds` at the end.
/// Under `find_limit` the thresholds are checked at the end of each step
/// instead, and the test ends at the first step that breaks one.
/// `on_second` is called once a second with live figures.
/// Configured monitors read a baseline before load begins, waiting up to ten
/// seconds or until stopped. Monitor failures are recorded in the returned
/// resource series and do not stop the load test.
pub async fn run_load(
    runner: &Runner,
    flow: &Flow,
    overrides: &Variables,
    profile: &LoadProfile,
    thresholds: &[Threshold],
    options: &LoadOptions,
    mut on_second: impl FnMut(&Progress<'_>),
) -> LoadReport {
    let stop = &options.stop;
    let setup = match setup_flow(flow) {
        Some(prefix) => tokio::select! {
            result = runner.run(&prefix, overrides) => result.steps,
            // The driver sees the stop too and starts nothing.
            () = stop.stopped() => Vec::new(),
        },
        None => Vec::new(),
    };
    let shared = Arc::new(Shared {
        runner: runner.keeping(Exchanges::FailedOnly),
        flow: flow.clone(),
        overrides: overrides.clone(),
        active: AtomicU32::new(0),
    });
    let total: Duration = profile.total_duration().into();
    let (set_origin, origin) = tokio::sync::watch::channel(None);
    let (set_end, end) = tokio::sync::watch::channel(false);
    let (monitor_tx, mut monitor_rx) = mpsc::unbounded_channel();
    let (mut resources, mut monitors, mut finals) = if options.monitor.is_empty() {
        (Vec::new(), JoinSet::new(), JoinSet::new())
    } else {
        let mut vars = Variables::new();
        vars.extend(flow.variables.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        vars.extend(overrides.entries());
        monitor::start(
            &options.monitor,
            &vars,
            &runner.client,
            &origin,
            &end,
            &monitor_tx,
        )
    };
    drop(monitor_tx);
    // The first reads are the baseline, so the load waits for them: every
    // running monitor's first answer or failure. A read gives up within
    // 10 s, so a slow or unreachable API cannot hold the test any longer.
    let mut waiting = monitors.len() + finals.len();
    let baseline = tokio::time::sleep(Duration::from_secs(10));
    tokio::pin!(baseline);
    let mut first = vec![false; resources.len()];
    while waiting > 0 {
        tokio::select! {
            Some(update) = monitor_rx.recv() => {
                if !std::mem::replace(&mut first[update.index], true) {
                    waiting -= 1;
                }
                resources[update.index].apply(update);
            }
            () = &mut baseline => break,
            () = stop.stopped() => break,
            else => break,
        }
    }
    let started = Instant::now();
    set_origin.send_replace(Some(started));
    let (tx, mut rx) = mpsc::unbounded_channel();
    // The driver stops on the user's stop, or once a `find_limit` step
    // breaks a threshold, which is how that test is meant to end.
    let halt = StopSignal::default();
    let forward = {
        let (user, halt) = (stop.clone(), halt.clone());
        tokio::spawn(async move {
            user.stopped().await;
            halt.stop();
        })
    };
    let mut driver = tokio::spawn(drive(
        shared.clone(),
        profile.clone(),
        LoadOptions {
            stop: halt.clone(),
            ..options.clone()
        },
        started,
        tx,
    ));
    let mut limit = LimitTracker::new(profile, thresholds);

    let mut recorder = Recorder::new();
    let mut samples = Vec::new();
    let mut slowest = Slowest::default();
    let mut tick = tokio::time::interval_at(
        tokio::time::Instant::from_std(started) + Duration::from_secs(1),
        Duration::from_secs(1),
    );
    let outcome = loop {
        tokio::select! {
            Some(result) = rx.recv() => {
                if let Some(limit) = &mut limit {
                    limit.add(&result);
                }
                record(&mut recorder, &mut samples, &mut slowest, result);
            }
            Some(update) = monitor_rx.recv() => resources[update.index].apply(update),
            _ = tick.tick() => {
                let elapsed = started.elapsed();
                if let Some(limit) = &mut limit {
                    if limit.close_due(elapsed, &resources) {
                        halt.stop();
                    }
                }
                let target = target(profile, elapsed.min(total));
                let requests = recorder.requests();
                let second = recorder.close_second(shared.active.load(Ordering::Relaxed));
                let limit_steps = limit.as_ref().map_or(&[][..], |l| &l.steps[..]);
                on_second(&Progress { elapsed, total, second, requests, target, resources: &resources, limit_steps });
            }
            outcome = &mut driver => break outcome.unwrap_or_default(),
        }
    };
    let elapsed = started.elapsed();
    forward.abort();
    monitors.abort_all();
    // Dynatrace reads once more for the minutes the test ended in, unless
    // the test was stopped. A read gives up within 10 s.
    if stop.is_stopped() {
        finals.abort_all();
    } else {
        set_end.send_replace(true);
        let last = async { while finals.join_next().await.is_some() {} };
        let _ = tokio::time::timeout(Duration::from_secs(11), last).await;
    }
    while let Ok(update) = monitor_rx.try_recv() {
        resources[update.index].apply(update);
    }
    while let Ok(result) = rx.try_recv() {
        record(&mut recorder, &mut samples, &mut slowest, result);
    }
    // The last, partial second, so its requests reach the timeline too.
    recorder.close_second(0);
    recorder.dropped(outcome.dropped);
    recorder.interrupted(outcome.interrupted);

    let summary = recorder.summary(elapsed);
    let api_cpu = capacity::average_cpu(&resources, 0.0, f64::INFINITY);
    let mut checked = check_thresholds(thresholds, &summary, &recorder, api_cpu);
    // A stop the user made; a found limit ends the test as planned.
    let aborted = outcome.aborted && stop.is_stopped();
    let limit = limit.map(|tracker| {
        let mut search = tracker.finish(elapsed, aborted, &resources);
        if let Some(step) = search
            .sustained
            .or(search.steps.first().map(|_| 0))
            .and_then(|i| search.steps.get(i as usize))
        {
            // The test passes when some step held: its checks stand for
            // the test's, since every later one was meant to break.
            checked = step.thresholds.clone();
            checked.extend(
                check_thresholds(thresholds, &summary, &recorder, api_cpu)
                    .into_iter()
                    .zip(thresholds)
                    .filter(|(_, t)| t.metric == Metric::Throughput)
                    .map(|(r, _)| r),
            );
        }
        search.sizing = options
            .capacity
            .as_ref()
            .and_then(|c| capacity::size(&search, c));
        search
    });
    LoadReport {
        name: flow.name.clone(),
        thresholds: checked,
        summary,
        samples,
        setup,
        slowest: slowest.into_list(),
        aborted,
        resources,
        limit,
    }
}

/// Measures each `find_limit` step on its own and checks the thresholds
/// against it as it ends.
struct LimitTracker<'a> {
    profile: &'a LoadProfile,
    every: Duration,
    /// Thresholds a step can break: all but `rps`, which the step sets.
    thresholds: Vec<Threshold>,
    window: Recorder,
    steps: Vec<LimitStep>,
    /// Set once a step broke a threshold, or the last one ended.
    done: bool,
}

impl<'a> LimitTracker<'a> {
    fn new(profile: &'a LoadProfile, thresholds: &[Threshold]) -> Option<Self> {
        let LoadProfile::FindLimit { every, .. } = profile else {
            return None;
        };
        Some(Self {
            profile,
            every: (*every).into(),
            thresholds: thresholds
                .iter()
                .filter(|t| t.metric != Metric::Throughput)
                .copied()
                .collect(),
            window: Recorder::new(),
            steps: Vec::new(),
            done: false,
        })
    }

    fn add(&mut self, result: &FlowResult) {
        if !self.done {
            add_samples(&mut self.window, result);
        }
    }

    /// Closes the current step once its time is up. True when it broke a
    /// threshold, so the test should end.
    fn close_due(&mut self, elapsed: Duration, resources: &[ResourceSeries]) -> bool {
        let index = self.steps.len() as u32;
        if self.done || elapsed < self.every * (index + 1) {
            return false;
        }
        let step = self.close(self.every * (index + 1), false, resources);
        let failed = !step.passed;
        self.steps.push(step);
        self.done = failed || index + 1 >= self.profile.limit_steps();
        failed
    }

    fn close(&mut self, end: Duration, partial: bool, resources: &[ResourceSeries]) -> LimitStep {
        let index = self.steps.len() as u32;
        let start = self.every * index;
        let length = end.saturating_sub(start);
        let window = std::mem::take(&mut self.window);
        let summary = window.summary(length);
        let (start_s, end_s) = (start.as_secs_f64(), end.as_secs_f64());
        let api_cpu = capacity::average_cpu(resources, start_s, end_s);
        let mut thresholds = check_thresholds(&self.thresholds, &summary, &window, api_cpu);
        // Monitors read every few seconds (Dynatrace once a minute), so a
        // step can end without a CPU reading: that decides nothing.
        if api_cpu.is_none() {
            for (result, t) in thresholds.iter_mut().zip(&self.thresholds) {
                if t.metric == Metric::ApiCpu && !partial {
                    result.passed = true;
                    result.note = Some("no CPU reading during this step".into());
                }
            }
        }
        let passed = !partial && summary.requests > 0 && thresholds.iter().all(|t| t.passed);
        LimitStep {
            rate: self.profile.limit_rate(index),
            start_s,
            end_s,
            rps: summary.rps,
            requests: summary.requests,
            failed_requests: summary.failed_requests,
            error_rate: summary.error_rate(),
            latency: summary.latency,
            api_cpu,
            thresholds,
            passed,
            partial,
        }
    }

    /// The search so far, with a step cut short by a stop kept as partial.
    fn finish(
        mut self,
        elapsed: Duration,
        aborted: bool,
        resources: &[ResourceSeries],
    ) -> LimitSearch {
        if !self.done {
            let index = self.steps.len() as u32;
            let end = self.every * (index + 1);
            if elapsed + Duration::from_millis(500) >= end {
                // The driver ended right at the step's end, before a tick
                // closed it.
                let step = self.close(end, false, resources);
                self.steps.push(step);
            } else if aborted && elapsed > self.every * index {
                let step = self.close(elapsed, true, resources);
                self.steps.push(step);
            }
        }
        let first_failed = self.steps.iter().position(|s| !s.passed && !s.partial);
        let held = first_failed.unwrap_or(self.steps.len());
        let sustained = self.steps[..held]
            .iter()
            .rposition(|s| s.passed)
            .map(|i| i as u32);
        LimitSearch {
            steps: self.steps,
            sustained,
            reached: first_failed.is_some(),
            cpu_cores: resources.iter().find_map(|r| r.cpu_cores),
            sizing: None,
        }
    }
}

/// Each step's slowest traced requests so far.
#[derive(Default)]
struct Slowest {
    /// In the order steps first appear; each list slowest first.
    by_step: Vec<(String, Vec<SlowRequest>)>,
}

impl Slowest {
    fn offer(&mut self, step: &StepResult) {
        let (Some(trace_id), Some(span_id)) = (&step.trace_id, &step.span_id) else {
            return;
        };
        let list = match self.by_step.iter().position(|(name, _)| *name == step.name) {
            Some(i) => &mut self.by_step[i].1,
            None => {
                self.by_step.push((step.name.clone(), Vec::new()));
                &mut self.by_step.last_mut().expect("just pushed").1
            }
        };
        if list.len() == SLOWEST_PER_STEP
            && list
                .last()
                .is_some_and(|l| l.duration_ms >= step.duration_ms)
        {
            return;
        }
        let at = list.partition_point(|l| l.duration_ms >= step.duration_ms);
        list.insert(
            at,
            SlowRequest {
                step: step.name.clone(),
                trace_id: trace_id.clone(),
                span_id: span_id.clone(),
                duration_ms: step.duration_ms,
                status: step.status,
                passed: step.passed() && step.status.is_none_or(|s| s < 400),
            },
        );
        list.truncate(SLOWEST_PER_STEP);
    }

    /// Every step's, slowest first overall.
    fn into_list(self) -> Vec<SlowRequest> {
        let mut all: Vec<SlowRequest> = self.by_step.into_iter().flat_map(|(_, l)| l).collect();
        all.sort_by(|a, b| b.duration_ms.total_cmp(&a.duration_ms));
        all
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
    aborted: bool,
}

/// Starts iterations until the profile ends or the test is stopped, then
/// gives the ones still running `grace` (or `stop_grace`) to finish.
async fn drive(
    shared: Arc<Shared>,
    profile: LoadProfile,
    options: LoadOptions,
    started: Instant,
    tx: mpsc::UnboundedSender<FlowResult>,
) -> Outcome {
    let total: Duration = profile.total_duration().into();
    let deadline = started + total;
    let stop = &options.stop;
    let mut tasks = JoinSet::new();
    let mut dropped = 0;

    match &profile {
        _ if stop.is_stopped() => {}
        LoadProfile::VirtualUsers {
            count, think_time, ..
        } => {
            let think: Duration = think_time.map(Into::into).unwrap_or_default();
            for _ in 0..*count {
                let (shared, tx, stop) = (shared.clone(), tx.clone(), stop.clone());
                tasks.spawn(async move {
                    while Instant::now() < deadline && !stop.is_stopped() {
                        let result = shared.iteration().await;
                        if tx.send(result).is_err() {
                            return;
                        }
                        let left = deadline.saturating_duration_since(Instant::now());
                        if !think.is_zero() {
                            tokio::select! {
                                () = tokio::time::sleep(think.min(left)) => {}
                                () = stop.stopped() => return,
                            }
                        }
                    }
                });
            }
            // Users finish their iteration and stop on their own, so the
            // grace below starts at the stop, not at the planned end.
            tokio::select! {
                () = tokio::time::sleep_until(deadline.into()) => {}
                () = stop.stopped() => {}
            }
        }
        open => {
            let schedule = Schedule::new(open);
            let mut due_started = 0u64;
            while !stop.is_stopped() {
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
                tokio::select! {
                    () = tokio::time::sleep(Duration::from_secs_f64(wait)) => {}
                    () = stop.stopped() => {}
                }
            }
        }
    }
    // A stop during the grace below still lets the test count as complete.
    let aborted = stop.is_stopped();
    drop(tx);

    let finished = {
        let finish = async { while tasks.join_next().await.is_some() {} };
        tokio::pin!(finish);
        if stop.is_stopped() {
            tokio::time::timeout(options.stop_grace, &mut finish)
                .await
                .is_ok()
        } else {
            // A stop during the grace shortens it.
            tokio::select! {
                done = tokio::time::timeout(options.grace, &mut finish) => done.is_ok(),
                () = stop.stopped() => {
                    tokio::time::timeout(options.stop_grace, &mut finish).await.is_ok()
                }
            }
        }
    };
    let interrupted = if finished {
        0
    } else {
        let left = tasks.len() as u32;
        tasks.abort_all();
        // Virtual users between iterations stop on their own; those
        // still in the set were mid-iteration.
        left
    };
    Outcome {
        dropped,
        interrupted,
        aborted,
    }
}

fn record(
    recorder: &mut Recorder,
    samples: &mut Vec<StepResult>,
    slowest: &mut Slowest,
    result: FlowResult,
) {
    for step in result.steps.iter().filter(|s| !s.skipped && !s.cached) {
        slowest.offer(step);
    }
    add_samples(recorder, &result);
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

/// Counts one iteration's requests in `recorder`.
fn add_samples(recorder: &mut Recorder, result: &FlowResult) {
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
            LoadProfile::FindLimit { every, .. } => (0..profile.limit_steps())
                .map(|i| {
                    let rate = f64::from(profile.limit_rate(i));
                    (secs(*every), rate, rate)
                })
                .collect(),
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
