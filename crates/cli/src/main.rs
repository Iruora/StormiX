//! `stormix`, the headless front end to the same engine as the desktop app.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use stormix_flow::{Severity, TestFile};

const EXAMPLE: &str = include_str!("../../../examples/login.stormix.yaml");

#[derive(Parser)]
#[command(
    name = "stormix",
    version,
    about = "Stress test APIs and run collection flows"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check a test file without sending any request.
    Validate {
        file: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Exit with an error on warnings too.
        #[arg(long)]
        strict: bool,
    },
    /// Write an example test file to start from.
    Init {
        #[arg(default_value = "stormix.yaml")]
        file: PathBuf,
        /// Replace the file if it exists.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Validate {
            file,
            format,
            strict,
        } => validate(&file, format, strict),
        Command::Init { file, force } => init(&file, force),
    };
    match result {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

/// Exit codes: 0 clean, 1 issues that fail the check, 2 the file could not be read.
fn validate(file: &Path, format: Format, strict: bool) -> Result<ExitCode, String> {
    let text = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    let test = TestFile::from_yaml(&text).map_err(|e| format!("{}: {e}", file.display()))?;
    let issues = test.validate();

    match format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&issues).expect("issues always serialize")
        ),
        Format::Text => {
            for issue in &issues {
                let label = match issue.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                };
                println!("{label}: {}: {}", issue.path, issue.message);
            }
            if issues.is_empty() {
                println!(
                    "{}: `{}` is valid ({} steps)",
                    file.display(),
                    test.flow.name,
                    test.flow.steps.len()
                );
            }
        }
    }

    let fails = issues
        .iter()
        .any(|i| i.severity == Severity::Error || strict);
    Ok(if fails {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn init(file: &Path, force: bool) -> Result<ExitCode, String> {
    if file.exists() && !force {
        return Err(format!(
            "{} already exists; pass --force to replace it",
            file.display()
        ));
    }
    std::fs::write(file, EXAMPLE).map_err(|e| format!("cannot write {}: {e}", file.display()))?;
    println!(
        "Wrote {}. Check it with `stormix validate {0}`.",
        file.display()
    );
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_example_is_valid() {
        let test = TestFile::from_yaml(EXAMPLE).unwrap();
        assert_eq!(test.validate(), []);
    }

    #[test]
    fn init_refuses_to_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stormix.yaml");
        assert!(init(&path, false).is_ok());
        assert!(init(&path, false).is_err());
        assert!(init(&path, true).is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), EXAMPLE);
    }

    #[test]
    fn verify_cli() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
