//! `stormix run`: one functional pass over a flow.

use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args;
use stormix_engine::{junit_report, FlowResult, RunOptions, Runner, Variables};
use stormix_flow::{Duration, Severity, TestFile};

use crate::vars::{parse_var, read_env};
use crate::{print_warnings, source, Format};

/// What `run` and `load` both take: the flow and how to send its requests.
#[derive(Args)]
pub struct Target {
    /// A test file, a Postman collection (.json) or a Bruno collection folder.
    pub file: PathBuf,
    /// Set a variable, overriding the file and the environment. Repeatable.
    #[arg(long = "var", value_name = "NAME=VALUE", value_parser = parse_var)]
    vars: Vec<(String, String)>,
    /// Variables from a YAML or JSON file, a Postman environment export or
    /// a Bruno environment (.bru). For a Bruno collection, the name of one
    /// of its environments works too.
    #[arg(long, value_name = "FILE|NAME")]
    env: Option<PathBuf>,
    /// Time allowed for each request.
    #[arg(long, default_value = "30s")]
    timeout: Duration,
    /// Accept invalid or self-signed TLS certificates.
    #[arg(long)]
    insecure: bool,
    /// Report redirects instead of following them.
    #[arg(long)]
    no_follow_redirects: bool,
    /// Time allowed for each pre-request or test script.
    #[arg(long, default_value = "5s")]
    script_timeout: Duration,
}

#[derive(Args)]
pub struct RunArgs {
    #[command(flatten)]
    target: Target,
    /// Also write a JUnit XML report here, for CI test tabs.
    #[arg(long, value_name = "FILE")]
    junit: Option<PathBuf>,
    /// Also write a self-contained HTML report here.
    #[arg(long, value_name = "FILE")]
    html: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
    /// Show passed checks and extracted values, not only failures.
    #[arg(long, short)]
    verbose: bool,
    /// Stop at the first failed step.
    #[arg(long)]
    bail: bool,
}

/// A loaded, valid test with its variables and a runner set up.
pub struct Prepared {
    pub source: source::Source,
    pub vars: Variables,
    pub runner: Runner,
}

impl Target {
    /// Loads the file, refuses it if it has errors, and reads the environment.
    /// `adjust` may change the test (a load override) before it is checked.
    pub fn prepare(
        self,
        bail: bool,
        adjust: impl FnOnce(&mut TestFile) -> Result<(), String>,
    ) -> Result<Prepared, String> {
        let mut source = source::load(&self.file)?;
        print_warnings(&source.warnings);
        adjust(&mut source.test)?;
        let errors: Vec<_> = source
            .test
            .validate()
            .into_iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        if !errors.is_empty() {
            let lines: Vec<_> = errors
                .iter()
                .map(|i| format!("  {}: {}", i.path, i.message))
                .collect();
            return Err(format!(
                "{} is not valid:\n{}",
                self.file.display(),
                lines.join("\n")
            ));
        }

        let mut vars = match &self.env {
            Some(path) if path.exists() => read_env(path)?,
            Some(name) => {
                let name = name.to_string_lossy();
                match source.environments.get(name.as_ref()) {
                    Some(env) => {
                        let mut vars = Variables::new();
                        vars.extend(env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
                        vars
                    }
                    None if source.environments.is_empty() => {
                        return Err(format!("cannot read {name}: no such file"));
                    }
                    None => {
                        let names: Vec<_> =
                            source.environments.keys().map(String::as_str).collect();
                        return Err(format!(
                            "{name} is neither a file nor one of the collection's environments ({})",
                            names.join(", ")
                        ));
                    }
                }
            }
            None => Variables::new(),
        };
        vars.extend(self.vars);

        let runner = Runner::new(RunOptions {
            timeout: self.timeout.into(),
            insecure: self.insecure,
            follow_redirects: !self.no_follow_redirects,
            bail,
            script_timeout: self.script_timeout.into(),
        })
        .map_err(|e| e.to_string())?;
        Ok(Prepared {
            source,
            vars,
            runner,
        })
    }
}

/// Creates the parent folder of a report file if needed, then writes it.
pub fn write_report(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    std::fs::write(path, contents).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// Exit codes: 0 every check passed, 1 something failed, 2 the run could not start.
pub fn run(args: RunArgs) -> Result<ExitCode, String> {
    let Prepared {
        source,
        vars,
        runner,
    } = args.target.prepare(args.bail, |_| Ok(()))?;
    let test = &source.test;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("cannot start the runtime: {e}"))?;
    let result = runtime.block_on(runner.run(&test.flow, &vars));

    if let Some(path) = &args.junit {
        write_report(path, &junit_report(&result))?;
    }
    if let Some(path) = &args.html {
        write_report(path, &stormix_report::flow_report(&result))?;
    }
    match args.format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&result).expect("a result always serializes")
        ),
        Format::Text => {
            if test.load.is_some() {
                eprintln!("note: running the flow once; `stormix load` applies the load profile.");
            }
            print!("{}", render_text(&result, args.verbose));
        }
    }
    Ok(if result.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn render_text(result: &FlowResult, verbose: bool) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}\n", result.name);
    let width = result
        .steps
        .iter()
        .map(|s| s.name.chars().count())
        .max()
        .unwrap_or(0)
        .min(40);

    for step in &result.steps {
        let mark = if step.skipped {
            "-"
        } else if step.passed() {
            "✓"
        } else {
            "✗"
        };
        let outcome = if step.skipped {
            "skipped".to_owned()
        } else {
            let status = step.status.map_or("---".to_owned(), |s| s.to_string());
            format!("{:<7} {status} {:>6.0} ms", step.method, step.duration_ms)
        };
        let _ = writeln!(out, "  {mark} {:<width$}  {outcome}", step.name);

        if let Some(error) = &step.error {
            let _ = writeln!(out, "      ✗ {} {}: {error}", step.method, step.url);
        }
        for check in &step.checks {
            match (&check.message, check.passed) {
                (Some(why), false) => {
                    let _ = writeln!(out, "      ✗ {}: {why}", check.name);
                }
                (None, false) => {
                    let _ = writeln!(out, "      ✗ {}", check.name);
                }
                (_, true) if verbose => {
                    let _ = writeln!(out, "      ✓ {}", check.name);
                }
                _ => {}
            }
        }
        if verbose {
            for (name, value) in &step.extracted {
                let _ = writeln!(out, "      = {name} = {}", truncate(value, 60));
            }
        }
        for warning in &step.warnings {
            let _ = writeln!(out, "      ! {warning}");
        }
        // Script output helps most when something failed.
        if verbose || !step.passed() {
            for line in &step.logs {
                let _ = writeln!(out, "      > {}", truncate(line, 200));
            }
        }
    }

    let ran = result.steps.iter().filter(|s| !s.skipped).count();
    let failed = result.failed_steps();
    let skipped = result.steps.len() - ran;
    let (checks_passed, checks) = result.check_counts();
    let mut summary = format!(
        "{} steps: {} passed, {failed} failed",
        result.steps.len(),
        ran - failed
    );
    if skipped > 0 {
        let _ = write!(summary, ", {skipped} skipped");
    }
    let _ = writeln!(
        out,
        "\n{summary} · {checks_passed}/{checks} checks passed · {:.2} s",
        result.duration_ms / 1000.0
    );
    out
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let cut: String = text.chars().take(max - 1).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use stormix_engine::{Check, StepResult};

    fn check(name: &str, message: Option<&str>) -> Check {
        Check {
            name: name.into(),
            passed: message.is_none(),
            message: message.map(Into::into),
        }
    }

    fn step(name: &str) -> StepResult {
        StepResult {
            name: name.into(),
            method: "GET".into(),
            url: "https://x.test/me".into(),
            status: Some(200),
            duration_ms: 42.4,
            time_to_headers_ms: Some(40.0),
            response_bytes: 10,
            checks: vec![check("status is 200", None)],
            extracted: BTreeMap::new(),
            warnings: Vec::new(),
            logs: Vec::new(),
            error: None,
            skipped: false,
        }
    }

    fn sample() -> FlowResult {
        let mut login = step("Log in");
        login.extracted.insert("token".into(), "abc".into());
        let mut profile = step("Profile");
        profile.status = Some(401);
        profile.checks = vec![check("status is 200", Some("got 401"))];
        profile.warnings = vec!["`{{x}}` has no value".into()];
        let mut down = step("Down");
        down.status = None;
        down.checks.clear();
        down.error = Some("connection refused".into());
        let mut later = step("Later");
        later.skipped = true;
        later.checks.clear();
        FlowResult {
            name: "Login flow".into(),
            steps: vec![login, profile, down, later],
            duration_ms: 1234.0,
        }
    }

    #[test]
    fn text_shows_failures_and_a_summary() {
        let text = render_text(&sample(), false);
        let expected = "\
Login flow

  ✓ Log in   GET     200     42 ms
  ✗ Profile  GET     401     42 ms
      ✗ status is 200: got 401
      ! `{{x}}` has no value
  ✗ Down     GET     ---     42 ms
      ✗ GET https://x.test/me: connection refused
  - Later    skipped

4 steps: 1 passed, 2 failed, 1 skipped · 1/2 checks passed · 1.23 s
";
        assert_eq!(text, expected);
    }

    #[test]
    fn verbose_adds_passed_checks_and_values() {
        let text = render_text(&sample(), true);
        assert!(
            text.contains("      ✓ status is 200\n      = token = abc\n"),
            "{text}"
        );
    }

    #[test]
    fn truncates_long_values() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("abc", 4), "abc");
    }
}
