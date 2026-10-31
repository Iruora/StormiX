//! `stormix`, the headless front end to the same engine as the desktop app.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use stormix_flow::Severity;
#[cfg(test)]
use stormix_flow::TestFile;

mod load;
mod run;
mod source;
mod vars;

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
    /// Check a test file or collection without sending any request.
    Validate {
        file: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Exit with an error on warnings too.
        #[arg(long)]
        strict: bool,
    },
    /// Run a test file or collection once and check every assertion and
    /// script test, as a functional test.
    Run(run::RunArgs),
    /// Run a test file or collection under load and check its thresholds.
    Load(load::LoadArgs),
    /// Convert a Postman collection (.json) or Bruno collection (folder)
    /// into a Stormix test file.
    Import {
        collection: PathBuf,
        /// Write the test file here instead of printing it.
        #[arg(long, short)]
        output: Option<PathBuf>,
        /// Replace the output file if it exists.
        #[arg(long)]
        force: bool,
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

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
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
        Command::Run(args) => run::run(args),
        Command::Load(args) => load::load(args),
        Command::Import {
            collection,
            output,
            force,
        } => import(&collection, output.as_deref(), force),
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

fn print_warnings(warnings: &[String]) {
    for warning in warnings {
        eprintln!("warning: {warning}");
    }
}

/// Exit codes: 0 clean, 1 issues that fail the check, 2 the file could not be read.
fn validate(file: &Path, format: Format, strict: bool) -> Result<ExitCode, String> {
    let source = source::load(file)?;
    print_warnings(&source.warnings);
    let test = source.test;
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

fn import(collection: &Path, output: Option<&Path>, force: bool) -> Result<ExitCode, String> {
    if !source::is_collection(collection) {
        return Err(format!(
            "{} is not a collection: expected a Postman .json export or a Bruno folder",
            collection.display()
        ));
    }
    let source = source::load(collection)?;
    print_warnings(&source.warnings);
    let header = format!(
        "# Imported from {} by `stormix import`.\n",
        collection.display()
    );
    let yaml = format!("{header}{}", source.test.to_yaml());
    let Some(output) = output else {
        print!("{yaml}");
        return Ok(ExitCode::SUCCESS);
    };
    if output.exists() && !force {
        return Err(format!(
            "{} already exists; pass --force to replace it",
            output.display()
        ));
    }
    std::fs::write(output, yaml).map_err(|e| format!("cannot write {}: {e}", output.display()))?;
    eprintln!(
        "Wrote {} ({} steps).",
        output.display(),
        source.test.flow.steps.len()
    );
    if !source.environments.is_empty() {
        let names: Vec<_> = source.environments.keys().map(String::as_str).collect();
        eprintln!(
            "The collection has environments ({}); pass one with --env {}/environments/<name>.bru.",
            names.join(", "),
            collection.display()
        );
    }
    Ok(ExitCode::SUCCESS)
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
    fn imports_collections_to_test_files_that_load_back() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("bruno.yaml");
        let bruno = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/collections/bruno");
        assert!(import(&bruno, Some(&out), false).is_ok());
        assert!(import(&bruno, Some(&out), false).is_err());

        let back = source::load(&out).unwrap();
        assert_eq!(back.test, source::load(&bruno).unwrap().test);
        assert!(import(&out, None, false)
            .unwrap_err()
            .contains("is not a collection"));
    }

    #[test]
    fn verify_cli() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
