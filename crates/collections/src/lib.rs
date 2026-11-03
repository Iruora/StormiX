//! Turns Postman v2.1 collections and Bruno collections (`.bru` or
//! OpenCollection YAML) into Stormix test files, scripts included, so the
//! engine can run them as they are. Test files can also be exported as
//! OpenCollection, which Bruno opens.
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
pub mod opencollection;
pub mod postman;
pub mod postman_export;

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
    /// Bruno's YAML format: a folder with `opencollection.yml`, that file,
    /// or a bundled `.yml` file holding the whole collection.
    OpenCollection,
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
    #[error("{}: {message}", file.display())]
    Yaml { file: PathBuf, message: String },
    #[error("{} is not an OpenCollection: it has no opencollection.yml", .0.display())]
    NotOpenCollection(PathBuf),
}

/// Guesses the format from the path: a `.json` file is a Postman export, a
/// folder (or a `.bru` file inside one) is a Bruno collection, and one
/// with `opencollection.yml` in it an OpenCollection. A `.yml` or `.yaml`
/// file is an OpenCollection only when it starts one (an `opencollection:`
/// key); otherwise it is taken for a Stormix test file and this says `None`.
pub fn detect(path: &Path) -> Option<Format> {
    if path.is_dir() {
        let open = opencollection::ROOT_FILES
            .iter()
            .any(|n| path.join(n).is_file());
        return Some(if open {
            Format::OpenCollection
        } else {
            Format::Bruno
        });
    }
    match path.extension()?.to_str()? {
        "json" => Some(Format::Postman),
        "bru" => Some(Format::Bruno),
        "yml" | "yaml"
            if opencollection::is_root_file(path) || opencollection::looks_bundled(path) =>
        {
            Some(Format::OpenCollection)
        }
        _ => None,
    }
}

/// Imports the collection at `path`: a Postman `.json` export, a Bruno
/// collection folder or an OpenCollection folder or bundled file. A `.bru`
/// file stands for the folder it is in, as does `opencollection.yml`.
pub fn import(path: &Path) -> Result<Imported, ImportError> {
    import_with_progress(path, &mut |_| {})
}

/// Like [`import`], reporting each request file a Bruno folder import
/// reads. A Postman export is one file, read at once, so it reports none.
pub fn import_with_progress(
    path: &Path,
    on_progress: &mut dyn FnMut(bruno::Progress),
) -> Result<Imported, ImportError> {
    match detect(path) {
        Some(Format::Postman) => postman::from_json(&read(path)?),
        Some(Format::Bruno) if path.is_dir() => bruno::from_dir_with_progress(path, on_progress),
        Some(Format::Bruno) => {
            bruno::from_dir_with_progress(path.parent().unwrap_or(Path::new(".")), on_progress)
        }
        Some(Format::OpenCollection) => opencollection::from_path_with_progress(path, on_progress),
        None => Err(ImportError::Postman(format!(
            "{} is neither a .json export, a Bruno folder nor an OpenCollection",
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
