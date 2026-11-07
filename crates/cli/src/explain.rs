//! `stormix explain`: asks the AI command line already on this machine
//! (Claude Code or GitHub Copilot CLI) what a run or load test result means.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;
use stormix_ai::{AskOptions, Provider, Redactor};
use stormix_engine::{FlowResult, LoadReport};

#[derive(Args)]
pub struct ExplainArgs {
    /// A result saved with `stormix run --format json` or `stormix load --format json`.
    result: PathBuf,
    /// The test file the result came from, so the AI sees how the steps are written.
    #[arg(long, short)]
    file: Option<PathBuf>,
    /// For a run: the step to explain. Defaults to the first that failed.
    #[arg(long, short)]
    step: Option<String>,
    /// claude or copilot. Defaults to the first one found.
    #[arg(long)]
    provider: Option<Provider>,
    /// The program to run, when it is not on PATH.
    #[arg(long)]
    provider_path: Option<PathBuf>,
    /// The model, as the provider names it. Defaults to the provider's own.
    #[arg(long)]
    model: Option<String>,
    /// A value to hide before anything is sent; repeat for several.
    /// Credentials in headers and token-like strings are always hidden.
    #[arg(long = "secret", value_name = "VALUE")]
    secrets: Vec<String>,
    /// Print what would be sent and stop, without asking.
    #[arg(long)]
    show_prompt: bool,
    /// Seconds to wait for an answer.
    #[arg(long, default_value_t = 120)]
    timeout: u32,
}

/// Exit codes: 0 answered, 2 nothing to explain or the AI could not answer.
pub fn explain(args: ExplainArgs) -> Result<ExitCode, String> {
    let text = std::fs::read_to_string(&args.result)
        .map_err(|e| format!("cannot read {}: {e}", args.result.display()))?;
    let file = match &args.file {
        Some(path) => Some(
            std::fs::read_to_string(path)
                .map_err(|e| format!("cannot read {}: {e}", path.display()))?,
        ),
        None => None,
    };
    let redactor = Redactor::new(args.secrets.clone());
    let prompt = if let Ok(report) = serde_json::from_str::<LoadReport>(&text) {
        stormix_ai::prompt::explain_load(&report, file.as_deref(), &redactor)
    } else if let Ok(run) = serde_json::from_str::<FlowResult>(&text) {
        let step = match &args.step {
            Some(name) => run
                .steps
                .iter()
                .find(|s| &s.name == name)
                .ok_or_else(|| format!("the run has no step `{name}`"))?,
            None => run
                .steps
                .iter()
                .find(|s| !s.skipped && !s.passed())
                .ok_or("every step passed; pass --step to explain one anyway")?,
        };
        stormix_ai::prompt::explain_step(step, file.as_deref(), None, &redactor)
    } else {
        return Err(format!(
            "{} is not a result from `stormix run --format json` or `stormix load --format json`",
            args.result.display()
        ));
    };
    if args.show_prompt {
        print!("{prompt}");
        return Ok(ExitCode::SUCCESS);
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("cannot start the runtime: {e}"))?;
    runtime.block_on(async {
        let found = match args.provider {
            Some(provider) => stormix_ai::detect(provider, args.provider_path.as_deref())
                .await
                .ok_or_else(|| stormix_ai::AiError::NotFound(provider).to_string())?,
            None => stormix_ai::detect_all(&[])
                .await
                .into_iter()
                .next()
                .ok_or("neither Claude Code (`claude`) nor GitHub Copilot CLI (`copilot`) was found; install one and sign in")?,
        };
        eprintln!("Asking {}…", found.provider.label());
        let options = AskOptions {
            model: args.model.clone(),
            timeout_s: Some(args.timeout),
        };
        let answer = stormix_ai::ask(&found, &prompt, &options, std::future::pending())
            .await
            .map_err(|e| e.to_string())?;
        println!("{}", answer.text);
        Ok(ExitCode::SUCCESS)
    })
}
