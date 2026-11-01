//! Turns Postman v2.1 collections and Bruno collections into Stormix test
//! files, scripts included, so the engine can run them as they are.
//!
//! Folders are flattened in run order and their names kept in each step's
//! name ("Users / Create user"). Collection and folder auth, headers and
//! scripts are applied to every request under them. Anything that does not
//! carry over is reported in [`Imported::warnings`] rather than dropped
//! silently.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use stormix_flow::TestFile;

mod bru;
pub mod bruno;
mod common;
pub mod environment;
pub mod postman;

#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    pub test: TestFile,
    /// Environments shipped inside the collection, by name. Bruno keeps
    /// them in `environments/`; Postman exports them as separate files.
    pub environments: BTreeMap<String, BTreeMap<String, String>>,
    /// The names of each environment's secrets, which have no value in
    /// the collection and must be supplied at run time.
    pub secrets: BTreeMap<String, Vec<String>>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Postman,
    Bruno,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("cannot read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("not a Postman v2 collection: {0}")]
    Postman(String),
    #[error("{}:{line}: {message}", file.display())]
    Bru {
        file: PathBuf,
        line: usize,
        message: String,
    },
    #[error("{}: {message}", file.display())]
    Environment { file: PathBuf, message: String },
    #[error("{} is not a Bruno collection: it has no bruno.json and no .bru files", .0.display())]
    NotBruno(PathBuf),
}

/// Guesses the format from the path alone: a `.json` file is a Postman
/// export, a folder (or a `.bru` file inside one) is a Bruno collection.
pub fn detect(path: &Path) -> Option<Format> {
    if path.is_dir() {
        return Some(Format::Bruno);
    }
    match path.extension()?.to_str()? {
        "json" => Some(Format::Postman),
        "bru" => Some(Format::Bruno),
        _ => None,
    }
}

/// Imports the collection at `path`, a Postman `.json` export or a Bruno
/// collection folder. A `.bru` file stands for the folder it is in.
pub fn import(path: &Path) -> Result<Imported, ImportError> {
    match detect(path) {
        Some(Format::Postman) => postman::from_json(&read(path)?),
        Some(Format::Bruno) if path.is_dir() => bruno::from_dir(path),
        Some(Format::Bruno) => bruno::from_dir(path.parent().unwrap_or(Path::new("."))),
        None => Err(ImportError::Postman(format!(
            "{} is neither a .json export nor a Bruno folder",
            path.display()
        ))),
    }
}

fn read(path: &Path) -> Result<String, ImportError> {
    std::fs::read_to_string(path).map_err(|source| ImportError::Io {
        path: path.to_owned(),
        source,
    })
}
