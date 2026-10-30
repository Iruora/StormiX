//! IPC commands. Types are exported to `src/bindings.ts` by tauri-specta.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use specta::Type;
use stormix_engine::{FlowResult, Runner, Variables};
use stormix_flow::{Issue, Severity, TestFile};
use tauri::State;

const EXAMPLE: &str = include_str!("../../../../examples/login.stormix.yaml");

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
}

/// The parsed file (when it parses) and everything wrong with it.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Validation {
    pub test: Option<TestFile>,
    /// Set when the YAML does not parse; `issues` is then empty.
    pub parse_error: Option<String>,
    pub issues: Vec<Issue>,
}

/// A collection converted to a test file, ready for the editor.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportedCollection {
    pub yaml: String,
    /// Environments that came with it (Bruno), by name.
    pub environments: BTreeMap<String, BTreeMap<String, String>>,
    /// What did not carry over.
    pub warnings: Vec<String>,
}

#[tauri::command]
#[specta::specta]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}

#[tauri::command]
#[specta::specta]
pub fn example_test_file() -> String {
    EXAMPLE.to_owned()
}

#[tauri::command]
#[specta::specta]
pub fn validate_test_file(text: String) -> Validation {
    match TestFile::from_yaml(&text) {
        Ok(test) => Validation {
            issues: test.validate(),
            test: Some(test),
            parse_error: None,
        },
        Err(err) => Validation {
            test: None,
            parse_error: Some(err.to_string()),
            issues: Vec::new(),
        },
    }
}

/// Imports a Postman `.json` export or a Bruno collection folder.
#[tauri::command]
#[specta::specta]
pub fn import_collection(path: String) -> Result<ImportedCollection, String> {
    let path = Path::new(&path);
    let imported = stormix_collections::import(path).map_err(|e| e.to_string())?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    Ok(ImportedCollection {
        yaml: format!("# Imported from {name}.\n{}", imported.test.to_yaml()),
        environments: imported.environments,
        warnings: imported.warnings,
    })
}

/// Runs the file's flow once, as a functional test, with `variables` (an
/// environment) over the flow's own. Refuses a file that does not parse or
/// has errors; the editor already shows why.
#[tauri::command]
#[specta::specta]
pub async fn run_test_file(
    text: String,
    variables: BTreeMap<String, String>,
    runner: State<'_, Runner>,
) -> Result<FlowResult, String> {
    let test = TestFile::from_yaml(&text).map_err(|e| e.to_string())?;
    if let Some(issue) = test
        .validate()
        .into_iter()
        .find(|i| i.severity == Severity::Error)
    {
        return Err(format!("{}: {}", issue.path, issue.message));
    }
    let mut vars = Variables::new();
    vars.extend(variables);
    Ok(runner.run(&test.flow, &vars).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_the_example() {
        let v = validate_test_file(example_test_file());
        assert!(v.test.is_some());
        assert_eq!(v.parse_error, None);
        assert_eq!(v.issues, []);
    }

    #[test]
    fn imports_the_bruno_fixture() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../examples/collections/bruno"
        );
        let imported = import_collection(path.into()).unwrap();
        assert_eq!(imported.warnings, Vec::<String>::new());
        assert!(imported.environments.contains_key("local"));
        let v = validate_test_file(imported.yaml);
        assert_eq!(v.parse_error, None);
        assert_eq!(v.test.unwrap().flow.steps.len(), 6);
    }

    #[test]
    fn reports_parse_errors() {
        let v = validate_test_file("version: 1\nflow: [".into());
        assert!(v.test.is_none());
        assert!(v.parse_error.is_some());
    }
}
