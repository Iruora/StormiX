//! `stormix load`: the flow under its load profile, with live progress and
//! thresholds deciding the exit code.

use std::fmt::Write as _;
use std::io::{IsTerminal, Write as _};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clap::Args;
use stormix_engine::{junit_load_report, run_load, LoadOptions, LoadReport, Progress};
use stormix_flow::{Duration, LoadProfile, Monitor, MonitorTarget, TestFile, Threshold};

use crate::run::{write_report, Prepared, Target};
use crate::Format;

#[derive(Args)]
pub struct LoadArgs {
    #[command(flatten)]
    target: Target,
    /// Run this many virtual users instead of the file's load profile.
    #[arg(long, value_name = "N", conflicts_with = "rate")]
    vus: Option<u32>,
    /// Start this many iterations per second instead of the file's profile.
    #[arg(long, value_name = "N")]
    rate: Option<u32>,
    /// How long to run, with --vus or --rate, or to change a constant-rate
    /// or virtual-user profile's length.
    #[arg(long)]
    duration: Option<Duration>,
    /// Pause between a virtual user's iterations, with --vus.
    #[arg(long, requires = "vus")]
    think_time: Option<Duration>,
    /// Add a threshold, like "p95 < 300ms". Repeatable; adds to the file's.
    #[arg(long = "threshold", value_name = "RULE")]
    thresholds: Vec<Threshold>,
    /// Read the API's CPU and memory from this metrics endpoint during the
    /// test: Prometheus text (like /actuator/prometheus or /metrics), or a
    /// Spring Boot Actuator base URL ending in /actuator. Repeatable; adds
    /// to the file's `monitor` section.
    #[arg(long = "monitor", value_name = "URL")]
    monitors: Vec<String>,
    /// Open models: the most iterations in flight at once; more are dropped.
    #[arg(long, default_value_t = 1000)]
    max_in_flight: u32,
    /// How long iterations still running at the end may take to finish.
    #[arg(long, default_value = "30s")]
    grace: Duration,
    /// Also write the full report as JSON here.
    #[arg(long, value_name = "FILE")]
    out: Option<PathBuf>,
    /// Also write a JUnit XML report, one test case per threshold.
    #[arg(long, value_name = "FILE")]
    junit: Option<PathBuf>,
    /// Also write a self-contained HTML report here.
    #[arg(long, value_name = "FILE")]
    html: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
    /// No live progress.
    #[arg(long, short)]
    quiet: bool,
}

/// Exit codes: 0 every threshold held, 1 one did not or the test was stopped
/// with Ctrl+C, 2 the test could not start.
pub fn load(args: LoadArgs) -> Result<ExitCode, String> {
    let overrides = Overrides {
        vus: args.vus,
        rate: args.rate,
        duration: args.duration,
        think_time: args.think_time,
        thresholds: args.thresholds,
        monitors: args.monitors,
    };
    let Prepared {
        source,
        vars,
        runner,
        data,
    } = args.target.prepare(false, |test| overrides.apply(test))?;
    let test = &source.test;
    let profile = test.load.clone().expect("apply ensures a load profile");

    let options = LoadOptions {
        max_in_flight: args.max_in_flight,
        grace: args.grace.into(),
        monitor: test.monitor.clone(),
        capacity: test.capacity.clone(),
        data,
        ..LoadOptions::default()
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("cannot start the runtime: {e}"))?;
    let live = !args.quiet && args.format == Format::Text;
    let terminal = std::io::stderr().is_terminal() && std::io::stdout().is_terminal();
    if live {
        eprintln!("{} · {}", test.flow.name, describe(&profile));
    }
    // The first Ctrl+C stops the test and still reports what it measured;
    // a second one, or any once the test is over, quits at once.
    let stop = options.stop.clone();
    let over = Arc::new(AtomicBool::new(false));
    let over_seen = over.clone();
    runtime.spawn(async move {
        if tokio::signal::ctrl_c().await.is_err() {
            return;
        }
        if over_seen.load(Ordering::Relaxed) {
            std::process::exit(130);
        }
        eprintln!("\nStopping: letting running iterations finish (Ctrl+C again to quit now)");
        stop.stop();
        if tokio::signal::ctrl_c().await.is_ok() {
            std::process::exit(130);
        }
    });
    let mut progress = ProgressLine::new(std::io::stderr().is_terminal());
    let report = runtime.block_on(run_load(
        &runner,
        &test.flow,
        &vars,
        &profile,
        &test.thresholds,
        &options,
        |p| {
            if live {
                progress.show(p);
            }
        },
    ));
    over.store(true, Ordering::Relaxed);
    progress.finish();

    if let Some(path) = &args.out {
        write_report(path, &to_json(&report))?;
    }
    if let Some(path) = &args.junit {
        write_report(path, &junit_load_report(&report))?;
    }
    if let Some(path) = &args.html {
        write_report(path, &stormix_report::load_report(&report))?;
    }
    match args.format {
        Format::Json => println!("{}", to_json(&report)),
        // On a terminal the title is already above the progress line.
        Format::Text => print!(
            "{}",
            render_text(&report, (!(live && terminal)).then(|| describe(&profile)))
        ),
    }
    Ok(if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn to_json(report: &LoadReport) -> String {
    serde_json::to_string_pretty(report).expect("a report always serializes")
}

struct Overrides {
    vus: Option<u32>,
    rate: Option<u32>,
    duration: Option<Duration>,
    think_time: Option<Duration>,
    thresholds: Vec<Threshold>,
    monitors: Vec<String>,
}

impl Overrides {
    /// Command-line load options win over the file's `load` section.
    fn apply(self, test: &mut TestFile) -> Result<(), String> {
        test.thresholds.extend(self.thresholds);
        test.monitor
            .extend(self.monitors.into_iter().map(monitor_for));
        let duration = self.duration;
        let length = |what: &str| {
            duration.ok_or_else(|| format!("--{what} needs --duration too, like --duration 30s"))
        };
        if let Some(count) = self.vus {
            test.load = Some(LoadProfile::VirtualUsers {
                count,
                duration: length("vus")?,
                think_time: self.think_time,
            });
        } else if let Some(rate) = self.rate {
            test.load = Some(LoadProfile::ConstantRate {
                rate,
                duration: length("rate")?,
            });
        } else if let Some(new) = duration {
            match &mut test.load {
                Some(
                    LoadProfile::ConstantRate { duration, .. }
                    | LoadProfile::VirtualUsers { duration, .. },
                ) => *duration = new,
                Some(LoadProfile::Ramping { .. }) => {
                    return Err(
                        "--duration cannot change a ramping profile; edit its stages".into(),
                    )
                }
                Some(LoadProfile::FindLimit { .. }) => return Err(
                    "--duration cannot change a find_limit profile; edit its `every` or `max_rate`"
                        .into(),
                ),
                None => {}
            }
        }
        if test.load.is_none() {
            return Err(
                "there is no `load` section; add one, or pass --vus 10 --duration 30s (or --rate)"
                    .into(),
            );
        }
        Ok(())
    }
}

/// `--monitor URL`: Actuator for a URL ending in `/actuator`, else
/// Prometheus text.
fn monitor_for(url: String) -> Monitor {
    let actuator = url.trim_end_matches('/').ends_with("/actuator");
    let target = MonitorTarget {
        url,
        name: None,
        headers: Vec::new(),
        interval: None,
        health: None,
        token: None,
        entity: None,
    };
    if actuator {
        Monitor::Actuator(target)
    } else {
        Monitor::Prometheus(target)
    }
}

fn describe(profile: &LoadProfile) -> String {
    match profile {
        LoadProfile::ConstantRate { rate, duration } => {
            format!("{rate} iterations/s for {duration}")
        }
        LoadProfile::Ramping { start_rate, stages } => {
            let targets: Vec<String> = stages
                .iter()
                .map(|s| format!("{}/s over {}", s.target, s.duration))
                .collect();
            format!(
                "ramping from {start_rate}/s: {} ({} in all)",
                targets.join(", "),
                profile.total_duration()
            )
        }
        LoadProfile::FindLimit {
            start_rate,
            increase,
            every,
            max_rate,
        } => format!(
            "finding the limit: {start_rate}/s, then +{increase}/s every {every} up to {max_rate}/s, until a threshold breaks"
        ),
        LoadProfile::VirtualUsers {
            count,
            duration,
            think_time,
        } => {
            let users = if *count == 1 { "user" } else { "users" };
            match think_time {
                Some(t) => {
                    format!("{count} virtual {users} for {duration}, {t} between iterations")
                }
                None => format!("{count} virtual {users} for {duration}"),
            }
        }
    }
}

/// One line a second: rewritten in place on a terminal, else every 10 s.
struct ProgressLine {
    terminal: bool,
    shown: bool,
}

impl ProgressLine {
    fn new(terminal: bool) -> Self {
        Self {
            terminal,
            shown: false,
        }
    }

    fn show(&mut self, p: &Progress<'_>) {
        let mut line = format!(
            "  {:>4}s/{}s  {:>6.1} req/s  p95 {:>7}  failed {:<4} in flight {:<4} target {}",
            p.elapsed.as_secs(),
            p.total.as_secs(),
            f64::from(p.second.requests),
            ms(p.second.p95_ms),
            p.second.failed,
            p.second.active,
            round(p.target),
        );
        // The first monitored API's latest figures.
        if let Some(latest) = p.resources.first().and_then(|r| r.samples.last()) {
            if let Some(cpu) = latest.cpu_percent {
                let _ = write!(line, "  api cpu {cpu:.0}%");
            }
            if let Some(memory) = latest.memory_bytes {
                let _ = write!(line, "  mem {}", bytes(memory));
            }
        }
        // find_limit: the last step that ended, and how it went.
        if let Some(step) = p.limit_steps.last() {
            let mark = if step.passed { "✓" } else { "✗" };
            let _ = write!(
                line,
                "  step {} {mark} {}/s",
                p.limit_steps.len(),
                step.rate
            );
        }
        // Any monitored API that is not healthy right now.
        for series in p.resources {
            if let Some(now) = series.health.last().filter(|h| h.unhealthy()) {
                let _ = write!(line, "  {} {}", series.name, now.status);
            }
        }
        let mut err = std::io::stderr();
        if self.terminal {
            let _ = write!(err, "\r\x1b[2K{line}");
            let _ = err.flush();
            self.shown = true;
        } else if p.second.t.is_multiple_of(10) {
            let _ = writeln!(err, "{line}");
        }
    }

    fn finish(&mut self) {
        if self.shown {
            eprintln!();
        }
    }
}

fn render_text(report: &LoadReport, title: Option<String>) -> String {
    let s = &report.summary;
    let mut out = String::new();
    match title {
        Some(profile) => {
            let _ = writeln!(out, "{} · {profile}\n", report.name);
        }
        None => out.push('\n'),
    }
    if report.aborted {
        let _ = writeln!(
            out,
            "  stopped     after {:.1} s, before the profile ended; figures cover that time only\n",
            s.duration_ms / 1000.0
        );
    }
    if !report.setup.is_empty() {
        let _ = writeln!(out, "  set up once, not measured");
        for step in &report.setup {
            let outcome = match (&step.error, step.status) {
                (Some(error), _) => format!("✗ {error}"),
                (None, status) => {
                    let mark = if step.passed() { "✓" } else { "✗" };
                    let status = status.map_or("---".to_owned(), |s| s.to_string());
                    format!("{mark} {status} {:.0} ms", step.duration_ms)
                }
            };
            let _ = writeln!(out, "    {}  {outcome}", step.name);
        }
        out.push('\n');
    }
    let _ = writeln!(
        out,
        "  requests    {} ({:.1}/s), {} failed ({:.2}%)",
        s.requests,
        s.rps,
        s.failed_requests,
        s.error_rate()
    );
    let mut iterations = format!(
        "  iterations  {}, {} failed",
        s.iterations, s.failed_iterations
    );
    if s.dropped_iterations > 0 {
        let _ = write!(
            iterations,
            ", {} dropped (raise --max-in-flight?)",
            s.dropped_iterations
        );
    }
    if s.interrupted_iterations > 0 {
        let _ = write!(
            iterations,
            ", {} cut off at the end",
            s.interrupted_iterations
        );
    }
    let _ = writeln!(out, "{iterations}");
    if s.cached_responses > 0 {
        let _ = writeln!(
            out,
            "  cached      {} responses reused, not sent",
            s.cached_responses
        );
    }
    let checks = s.checks_passed + s.checks_failed;
    if checks > 0 {
        let _ = writeln!(
            out,
            "  checks      {}/{checks} passed ({:.2}%)",
            s.checks_passed,
            s.check_rate()
        );
    }
    let l = &s.latency;
    let _ = writeln!(
        out,
        "  latency     min {}  avg {}  p50 {}  p90 {}  p95 {}  p99 {}  max {}",
        ms(l.min),
        ms(l.avg),
        ms(l.p50),
        ms(l.p90),
        ms(l.p95),
        ms(l.p99),
        ms(l.max)
    );

    if s.steps.len() > 1 {
        let width = s
            .steps
            .iter()
            .map(|st| st.name.chars().count())
            .max()
            .unwrap_or(0)
            .clamp(4, 40);
        // Only files with cached steps get the extra column.
        let cached = s.cached_responses > 0;
        let _ = writeln!(
            out,
            "\n  {:<width$}  {:>8}  {:>6}  {:>9}  {:>9}  {:>9}{}",
            "step",
            "requests",
            "failed",
            "p50",
            "p95",
            "p99",
            if cached { "    cached" } else { "" }
        );
        for st in &s.steps {
            let name: String = st.name.chars().take(width).collect();
            let _ = writeln!(
                out,
                "  {name:<width$}  {:>8}  {:>6}  {:>9}  {:>9}  {:>9}{}",
                st.requests,
                st.failed,
                ms(st.latency.p50),
                ms(st.latency.p95),
                ms(st.latency.p99),
                if cached {
                    format!("  {:>8}", st.cached)
                } else {
                    String::new()
                }
            );
        }
    }

    if !s.failures.is_empty() {
        let _ = writeln!(out, "\n  most common failures");
        for f in &s.failures {
            let _ = writeln!(out, "    {:>6}×  {}", f.count, f.reason);
        }
    }

    if !report.slowest.is_empty() {
        let _ = writeln!(
            out,
            "\n  slowest traced requests (look these up in your APM tool)"
        );
        let width = report
            .slowest
            .iter()
            .map(|r| r.step.chars().count())
            .max()
            .unwrap_or(0)
            .clamp(4, 40);
        for r in report.slowest.iter().take(10) {
            let name: String = r.step.chars().take(width).collect();
            let status = r.status.map_or("---".to_owned(), |s| s.to_string());
            let mark = if r.passed { " " } else { "✗" };
            let _ = writeln!(
                out,
                "    {mark} {name:<width$}  {:>9}  {status}  {}",
                ms(r.duration_ms),
                r.trace_id
            );
        }
        if report.slowest.len() > 10 {
            let _ = writeln!(
                out,
                "    and {} more in the JSON and HTML reports",
                report.slowest.len() - 10
            );
        }
    }

    for series in &report.resources {
        render_resources(&mut out, series);
    }

    if let Some(limit) = &report.limit {
        render_limit(&mut out, limit);
    }

    render_data(&mut out, report);

    if report.thresholds.is_empty() {
        let _ = writeln!(
            out,
            "\n  no thresholds, so the test passes whatever the figures"
        );
    } else {
        let _ = writeln!(out, "\n  thresholds");
        for t in &report.thresholds {
            let mark = if t.passed { "✓" } else { "✗" };
            let _ = writeln!(out, "    {mark} {:<20} {}", t.threshold, measured(t));
        }
    }
    out
}

/// How each test data set was used, with each row's share when there are few.
fn render_data(out: &mut String, report: &LoadReport) {
    for set in &report.data {
        let how = match (set.pick, set.per) {
            (stormix_flow::Pick::Random, _) => "random",
            (stormix_flow::Pick::Sequential, _) => "in order",
            (stormix_flow::Pick::Unique, _) => "each once",
        };
        let per = match set.per {
            stormix_flow::Per::Iteration => "iteration",
            stormix_flow::Per::User => "user",
        };
        let _ = writeln!(
            out,
            "\n  test data   {}: {} rows, {how} per {per}, {} picked{}",
            set.label,
            set.rows,
            set.picks,
            match (set.ran_out, report.data_ran_out) {
                (false, _) => "",
                (true, true) => ", all used so the test ended early",
                // Only `per: user` sets run out without ending the test.
                (true, false) => ", fewer rows than users so some users did not start",
            }
        );
        let shown: Vec<_> = set.counts.iter().filter(|c| c.count > 0).collect();
        for row in shown.iter().take(10) {
            let _ = writeln!(out, "    {:<24} {}×", row.values.join(", "), row.count);
        }
        if shown.len() > 10 {
            let _ = writeln!(out, "    and {} more rows", shown.len() - 10);
        }
    }
}

/// The steps of a `find_limit` test, the limit and the pods it means.
fn render_limit(out: &mut String, limit: &stormix_engine::LimitSearch) {
    let _ = writeln!(out, "\n  limit steps (each checked on its own)");
    let _ = writeln!(
        out,
        "      {:>8}  {:>9}  {:>9}  {:>9}  {:>7}  {:>8}  {:>8}",
        "rate", "req/s", "avg", "p95", "errors", "api cpu", "api mem"
    );
    for (i, step) in limit.steps.iter().enumerate() {
        let mark = match (step.partial, step.passed) {
            (true, _) => "…",
            (false, true) => "✓",
            (false, false) => "✗",
        };
        let cpu = step.api_cpu.map_or("-".to_owned(), |c| format!("{c:.0}%"));
        let mem = step.api_memory_bytes.map_or("-".to_owned(), bytes);
        let broke: Vec<String> = step
            .thresholds
            .iter()
            .filter(|t| !t.passed)
            .map(|t| format!("{} ({})", t.threshold, measured(t)))
            .collect();
        let note = if limit.sustained == Some(i as u32) {
            "  ← sustained".to_owned()
        } else if step.partial {
            "  stopped during this step".to_owned()
        } else if broke.is_empty() {
            String::new()
        } else {
            format!("  broke {}", broke.join(", "))
        };
        let _ = writeln!(
            out,
            "    {mark} {:>6}/s  {:>9.1}  {:>9}  {:>9}  {:>6.2}%  {cpu:>8}  {mem:>8}{note}",
            step.rate,
            step.rps,
            ms(step.latency.avg),
            ms(step.latency.p95),
            step.error_rate,
        );
    }
    let Some(held) = limit.sustained.and_then(|i| limit.steps.get(i as usize)) else {
        let _ = writeln!(
            out,
            "\n  limit       not found: even the first step broke a threshold; start lower"
        );
        return;
    };
    if limit.reached {
        let _ = writeln!(
            out,
            "\n  limit       {:.1} req/s sustained ({}/s iterations); the next step broke a threshold",
            held.rps, held.rate
        );
    } else {
        let _ = writeln!(
            out,
            "\n  limit       not reached: {:.1} req/s held at the max rate, so it can take at least that; raise max_rate",
            held.rps
        );
    }
    let Some(s) = &limit.sizing else {
        let _ = writeln!(
            out,
            "              add a `capacity` section (peak_rps, growth, current_pods) to size pods from it"
        );
        return;
    };
    let c = &s.capacity;
    let per_pod = if c.tested_pods > 1 {
        format!(" ({} pods served the test)", c.tested_pods)
    } else {
        String::new()
    };
    let _ = writeln!(out, "\n  pods needed {}", s.pods);
    let _ = writeln!(
        out,
        "    one pod held   {:.1} req/s{per_pod}{}",
        s.tested_rps_per_pod,
        s.tested_cpu
            .map(|cpu| format!(" at {cpu:.0}% CPU of {} core(s)", round(s.pod_cores)))
            .unwrap_or_default()
    );
    let reference = s
        .cpu_step
        .filter(|i| Some(*i) != limit.sustained)
        .and_then(|i| limit.steps.get(i as usize));
    let why = match (s.basis.as_str(), reference) {
        ("cpu", Some(r)) => format!(
            "CPU was saturated at the top, so scaled from the {}/s step at {:.0}% CPU to the {:.0}% target",
            r.rate,
            r.api_cpu.unwrap_or_default() / s.pod_cores,
            c.target_cpu.0
        ),
        ("cpu", None) => format!("scaled to the {:.0}% CPU target", c.target_cpu.0),
        ("latency", _) => format!(
            "CPU stayed under the {:.0}% target, so latency or errors set the limit",
            c.target_cpu.0
        ),
        _ => format!(
            "no CPU reading, so the step is taken as saturated and run at {:.0}%",
            c.target_cpu.0
        ),
    };
    let _ = writeln!(
        out,
        "    usable per pod {:.1} req/s ({why})",
        s.usable_rps_per_pod
    );
    let _ = writeln!(
        out,
        "    design rate    {:.1} req/s = peak {} × (1 + {:.0}% growth)",
        s.design_rps,
        round(c.peak_rps),
        c.growth.0
    );
    let _ = writeln!(
        out,
        "    cpu pods       ⌈{:.1} ÷ {:.1}⌉ = {}",
        s.design_rps, s.usable_rps_per_pod, s.pods_by_cpu
    );
    if let (Some(fixed), Some(per_request)) = (s.fixed_memory_bytes, s.memory_per_request_bytes) {
        let _ = writeln!(
            out,
            "    memory         {} fixed + {:.2} MB per request in flight (fitted across the steps)",
            bytes(fixed),
            per_request / (1024.0 * 1024.0)
        );
    }
    if let (Some(pods), Some(limit_mb)) = (s.pods_by_memory, c.pod_memory_mb) {
        let _ = writeln!(
            out,
            "    memory pods    {} (about {} requests in flight in all, within {:.0}% of {} per pod)",
            pods,
            count(s.in_flight_total),
            c.target_memory.0,
            bytes(limit_mb * 1024.0 * 1024.0)
        );
    } else if c.pod_memory_mb.is_none() {
        let _ = writeln!(
            out,
            "    memory pods    not sized; add pod_memory_mb (the pod's memory limit) to the capacity section"
        );
    }
    if let Some(note) = &s.memory_note {
        let _ = writeln!(out, "                   {note}");
    }
    let by = if s.pods_by_memory.is_some_and(|m| m > s.pods_by_cpu) {
        "memory"
    } else {
        "cpu"
    };
    let _ = writeln!(
        out,
        "    pods           {} (set by {by}) + {} spare = {}",
        s.pods_for_traffic, c.spare_pods, s.pods
    );
    if let Some(current) = c.current_pods {
        let verdict = match s.pods.cmp(&current) {
            std::cmp::Ordering::Greater => format!("add {}", s.pods - current),
            std::cmp::Ordering::Less => format!("{} more than needed", current - s.pods),
            std::cmp::Ordering::Equal => "just right".to_owned(),
        };
        let _ = writeln!(out, "    running now    {current}: {verdict}");
    }
    let _ = writeln!(
        out,
        "    in flight      about {} requests per pod, {} in all (rate × {} avg latency); size thread and connection pools above that",
        count(s.in_flight_per_pod),
        count(s.in_flight_total),
        ms(s.latency_ms)
    );
}

/// Writes one monitored API's CPU and memory summary to the report.
fn render_resources(out: &mut String, series: &stormix_engine::ResourceSeries) {
    let _ = writeln!(out, "\n  api resources  {} ({})", series.name, series.url);
    render_health(out, series);
    if series.samples.is_empty() {
        let reason = series.error.as_deref().unwrap_or("no reads");
        let _ = writeln!(out, "    could not read it: {reason}");
        return;
    }
    let stats = series.stats();
    if let (Some(avg), Some(peak)) = (stats.cpu_avg, stats.cpu_peak) {
        let scale = if series.format == "dynatrace" {
            "as Dynatrace reports it".to_owned()
        } else {
            let cores = series
                .cpu_cores
                .map(|c| format!(" of {c:.0} cores"))
                .unwrap_or_default();
            format!("100% = one core{cores}")
        };
        let _ = writeln!(
            out,
            "    cpu      avg {avg:.0}%  peak {peak:.0}%   ({scale})"
        );
    }
    if let (Some(start), Some(peak), Some(end)) =
        (stats.memory_start, stats.memory_peak, stats.memory_end)
    {
        let kind = match series.memory_kind.as_deref() {
            Some("jvm") => "   (JVM heap + non-heap)",
            Some("resident") => "   (resident)",
            Some("working set") => "   (working set)",
            _ => "",
        };
        let _ = writeln!(
            out,
            "    memory   before {}  peak {}  end {}{kind}",
            bytes(start),
            bytes(peak),
            bytes(end)
        );
    }
    if let Some(heap) = stats.heap_peak {
        let max = series
            .heap_max_bytes
            .map(|m| format!(" of {}", bytes(m)))
            .unwrap_or_default();
        let _ = writeln!(out, "    heap     peak {}{max}", bytes(heap));
    }
    if let Some(gc) = stats.gc_avg {
        let _ = writeln!(
            out,
            "    gc       {gc:.1} ms of pauses per second on average"
        );
    }
    if series.failed_reads > 0 {
        let _ = writeln!(
            out,
            "    {} reads failed, the last with: {}",
            series.failed_reads,
            series.error.as_deref().unwrap_or("?")
        );
    }
}

/// One line when the API stayed healthy, else one per unhealthy stretch.
fn render_health(out: &mut String, series: &stormix_engine::ResourceSeries) {
    if let Some(error) = &series.health_error {
        let _ = writeln!(out, "    health   not checked: {error}");
        return;
    }
    if series.health.is_empty() {
        return;
    }
    let periods = series.unhealthy_periods();
    if periods.is_empty() {
        let _ = writeln!(out, "    health   ✓ healthy throughout");
        return;
    }
    for p in &periods {
        let detail = p
            .detail
            .as_deref()
            .map(|d| format!(" ({d})"))
            .unwrap_or_default();
        let back = match p.to {
            Some(to) => format!(", healthy again at {to:.1} s ({:.1} s)", to - p.from),
            None => ", still at the end".to_owned(),
        };
        let _ = writeln!(
            out,
            "    health   ✗ {} at {:.1} s{detail}{back}",
            p.status, p.from
        );
    }
}

/// A memory size in MB, or GB from 1 GB up.
fn bytes(value: f64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    if value >= 1024.0 * MB {
        format!("{:.2} GB", value / (1024.0 * MB))
    } else {
        format!("{:.0} MB", value / MB)
    }
}

/// The measured value in the threshold's own unit.
fn measured(t: &stormix_metrics::ThresholdResult) -> String {
    let metric = t
        .threshold
        .split([' ', '<', '>'])
        .next()
        .unwrap_or_default();
    match metric {
        "errors" | "checks" => format!("{:.2}%", t.actual),
        "cpu" => match &t.note {
            Some(note) => note.clone(),
            None => format!("{:.0}%", t.actual),
        },
        "rps" => format!("{:.1}/s", t.actual),
        _ => ms(t.actual),
    }
}

fn ms(value: f64) -> String {
    if value >= 1000.0 {
        format!("{:.2} s", value / 1000.0)
    } else if value >= 100.0 {
        format!("{value:.0} ms")
    } else {
        format!("{value:.1} ms")
    }
}

/// A count that may be small: one decimal under 10.
fn count(value: f64) -> String {
    if value < 10.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.0}")
    }
}

fn round(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_file(load: &str) -> TestFile {
        TestFile::from_yaml(&format!(
            "version: 1\nflow:\n  name: t\n  steps:\n    - name: a\n      request: {{ method: GET, url: \"http://x.test\" }}\n{load}"
        ))
        .unwrap()
    }

    fn overrides() -> Overrides {
        Overrides {
            vus: None,
            rate: None,
            duration: None,
            think_time: None,
            thresholds: Vec::new(),
            monitors: Vec::new(),
        }
    }

    #[test]
    fn monitor_flags_add_to_the_files_monitors() {
        let mut test = test_file("load: { constant_rate: { rate: 1, duration: 1s } }\n");
        Overrides {
            monitors: vec![
                "http://api/actuator/prometheus".into(),
                "http://api/actuator/".into(),
            ],
            ..overrides()
        }
        .apply(&mut test)
        .unwrap();
        assert_eq!(test.monitor[0].kind(), "prometheus");
        assert_eq!(test.monitor[1].kind(), "actuator");
        assert_eq!(test.monitor[1].target().url, "http://api/actuator/");
    }

    #[test]
    fn command_line_load_options_win() {
        let mut test = test_file("load:\n  constant_rate: { rate: 5, duration: 10s }\n");
        Overrides {
            duration: Some(Duration::from_secs(60)),
            thresholds: vec!["p95 < 1s".parse().unwrap()],
            ..overrides()
        }
        .apply(&mut test)
        .unwrap();
        assert_eq!(
            test.load,
            Some(LoadProfile::ConstantRate {
                rate: 5,
                duration: Duration::from_secs(60)
            })
        );
        assert_eq!(test.thresholds.len(), 1);

        let mut test = test_file("");
        Overrides {
            vus: Some(4),
            duration: Some(Duration::from_secs(5)),
            ..overrides()
        }
        .apply(&mut test)
        .unwrap();
        assert_eq!(
            describe(test.load.as_ref().unwrap()),
            "4 virtual users for 5s"
        );
    }

    #[test]
    fn says_what_is_missing() {
        assert!(overrides()
            .apply(&mut test_file(""))
            .unwrap_err()
            .contains("no `load` section"));
        let err = Overrides {
            rate: Some(5),
            ..overrides()
        }
        .apply(&mut test_file(""))
        .unwrap_err();
        assert!(err.contains("--rate needs --duration"), "{err}");
        let err = Overrides {
            duration: Some(Duration::from_secs(5)),
            ..overrides()
        }
        .apply(&mut test_file(
            "load:\n  ramping: { stages: [{ duration: 5s, target: 5 }] }\n",
        ))
        .unwrap_err();
        assert!(err.contains("ramping"), "{err}");
    }
}
