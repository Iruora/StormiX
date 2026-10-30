//! IPC commands. Types are exported to `src/bindings.ts` by tauri-specta.

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

/// Runs the file's flow once, as a functional test. Refuses a file that
/// does not parse or has errors; the editor already shows why.
#[tauri::command]
#[specta::specta]
pub async fn run_test_file(text: String, runner: State<'_, Runner>) -> Result<FlowResult, String> {
    let test = TestFile::from_yaml(&text).map_err(|e| e.to_string())?;
    if let Some(issue) = test
        .validate()
        .into_iter()
        .find(|i| i.severity == Severity::Error)
    {
        return Err(format!("{}: {}", issue.path, issue.message));
    }
    Ok(runner.run(&test.flow, &Variables::new()).await)
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
    fn reports_parse_errors() {
        let v = validate_test_file("version: 1\nflow: [".into());
        assert!(v.test.is_none());
        assert!(v.parse_error.is_some());
    }
}
