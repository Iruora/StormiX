//! `stormix load`: the flow under its load profile, with live progress and
//! thresholds deciding the exit code.

use std::fmt::Write as _;
use std::io::{IsTerminal, Write as _};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;
use stormix_engine::{junit_load_report, run_load, LoadOptions, LoadReport, Progress};
use stormix_flow::{Duration, LoadProfile, TestFile, Threshold};

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

/// Exit codes: 0 every threshold held, 1 one did not, 2 the test could not start.
pub fn load(args: LoadArgs) -> Result<ExitCode, String> {
    let overrides = Overrides {
        vus: args.vus,
        rate: args.rate,
        duration: args.duration,
        think_time: args.think_time,
        thresholds: args.thresholds,
    };
    let Prepared {
        source,
        vars,
        runner,
    } = args.target.prepare(false, |test| overrides.apply(test))?;
    let test = &source.test;
    let profile = test.load.clone().expect("apply ensures a load profile");

    let options = LoadOptions {
        max_in_flight: args.max_in_flight,
        grace: args.grace.into(),
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
}

impl Overrides {
    /// Command-line load options win over the file's `load` section.
    fn apply(self, test: &mut TestFile) -> Result<(), String> {
        test.thresholds.extend(self.thresholds);
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
        let line = format!(
            "  {:>4}s/{}s  {:>6.1} req/s  p95 {:>7}  failed {:<4} in flight {:<4} target {}",
            p.elapsed.as_secs(),
            p.total.as_secs(),
            f64::from(p.second.requests),
            ms(p.second.p95_ms),
            p.second.failed,
            p.second.active,
            round(p.target),
        );
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

/// The measured value in the threshold's own unit.
fn measured(t: &stormix_metrics::ThresholdResult) -> String {
    let metric = t
        .threshold
        .split([' ', '<', '>'])
        .next()
        .unwrap_or_default();
    match metric {
        "errors" | "checks" => format!("{:.2}%", t.actual),
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
        }
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
