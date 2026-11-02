//! OpenCollection, the YAML collection format Bruno saves since v3
//! (<https://www.opencollection.com>). A collection is either a folder with
//! `opencollection.yml` at its root, a `folder.yml` per sub-folder, one
//! `.yml` file per request and environments under `environments/`, or a
//! single "bundled" file holding all of it.
//!
//! Import reads both shapes into a Stormix test file the same way the
//! `.bru` importer does, so a collection behaves the same whichever format
//! Bruno saved it in. Export writes a test file back out as an
//! OpenCollection folder or bundled file that Bruno opens.

pub(crate) mod export;
mod import;

pub use export::{to_bundle, to_files, write_dir, Exported};
pub use import::{environment, from_path, from_path_with_progress, is_root_file, looks_bundled};

/// The file at the root of a collection folder.
pub const ROOT_FILES: [&str; 2] = ["opencollection.yml", "opencollection.yaml"];
/// The file holding a sub-folder's own settings.
const FOLDER_FILES: [&str; 2] = ["folder.yml", "folder.yaml"];
/// The OpenCollection version export writes.
pub const VERSION: &str = "1.0.0";
