//! `stormix diagnose`: where exactly one step's request fails.

use std::process::ExitCode;

use clap::Args;
use stormix_engine::StageOutcome;

use crate::run::{Prepared, Target};
use crate::Format;

#[derive(Args)]
pub struct DiagnoseArgs {
    #[command(flatten)]
    target: Target,
    /// The step to diagnose. The enabled steps before it run first, for
    /// the variables they set.
    #[arg(long, short)]
    step: String,
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
}

/// Exit codes: 0 the request went through, 1 it failed somewhere, 2 it could not start.
pub fn diagnose(args: DiagnoseArgs) -> Result<ExitCode, String> {
    let Prepared {
        source,
        vars,
        runner,
    } = args.target.prepare(false, |_| Ok(()))?;
    let flow = &source.test.flow;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("cannot start the runtime: {e}"))?;
    let diagnosis = runtime
        .block_on(runner.diagnose(flow, &vars, &args.step))
        .map_err(|message| {
            let names: Vec<_> = flow
                .enabled_steps()
                .map(|s| format!("`{}`", s.name))
                .collect();
            format!("{message}; its steps are {}", names.join(", "))
        })?;

    match args.format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&diagnosis).expect("a diagnosis always serializes")
        ),
        Format::Text => {
            println!(
                "Diagnosing `{}`: {} {}\n",
                diagnosis.step.name, diagnosis.step.method, diagnosis.step.url
            );
            let width = diagnosis
                .stages
                .iter()
                .map(|s| s.name.len())
                .max()
                .unwrap_or(0);
            for stage in &diagnosis.stages {
                let mark = match stage.outcome {
                    StageOutcome::Passed => "ok  ",
                    StageOutcome::Failed => "FAIL",
                    StageOutcome::Warning => "warn",
                    StageOutcome::Info => "    ",
                    StageOutcome::NotReached => " -  ",
                };
                let time = stage
                    .duration_ms
                    .map(|ms| format!(" ({} ms)", ms.round()))
                    .unwrap_or_default();
                println!("  {mark} {:width$}  {}{time}", stage.name, stage.summary);
            }
            println!("\n{}", diagnosis.verdict);
        }
    }
    Ok(if diagnosis.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
