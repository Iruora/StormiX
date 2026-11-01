//! IPC commands. Types are exported to `src/bindings.ts` by tauri-specta.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;
use stormix_engine::{run_load, FlowResult, LoadOptions, LoadReport, Runner, Variables};
use stormix_flow::{Issue, Severity, TestFile};
use stormix_store::{RunData, RunDiff, RunSummary, Store, StoredRun};
use tauri::{AppHandle, State};
use tauri_specta::Event;

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
    /// Secret names of each environment, which have no value in it.
    pub secrets: BTreeMap<String, Vec<String>>,
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
        secrets: imported.secrets,
        warnings: imported.warnings,
    })
}

/// An environment read from a file of its own.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LoadedEnvironment {
    /// The Postman export's name, otherwise the file name.
    pub name: String,
    pub variables: BTreeMap<String, String>,
    /// Secrets the file names without a value.
    pub secrets: Vec<String>,
}

impl From<stormix_collections::environment::Environment> for LoadedEnvironment {
    fn from(env: stormix_collections::environment::Environment) -> Self {
        Self {
            name: env.name,
            variables: env.variables,
            secrets: env.secrets,
        }
    }
}

/// Reads an environment file: flat YAML or JSON, a Postman environment
/// export or a Bruno `.bru` environment.
#[tauri::command]
#[specta::specta]
pub fn read_environment(path: String) -> Result<LoadedEnvironment, String> {
    stormix_collections::environment::read_file(Path::new(&path))
        .map(Into::into)
        .map_err(|e| e.to_string())
}

/// The environments in the `environments/` folder next to a test file,
/// where `stormix import` writes a collection's.
#[tauri::command]
#[specta::specta]
pub fn environments_beside(path: String) -> Result<Vec<LoadedEnvironment>, String> {
    let dir = stormix_collections::environment::dir_for(Path::new(&path));
    stormix_collections::environment::read_dir(&dir)
        .map(|envs| envs.into_iter().map(Into::into).collect())
        .map_err(|e| e.to_string())
}

/// Sent once a second while a load test runs.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct LoadProgress {
    pub elapsed_s: u32,
    pub total_s: u32,
    pub requests: u32,
    pub failed: u32,
    pub p95_ms: f64,
    pub active: u32,
}

/// Runs the file's flow under its load profile and checks its thresholds,
/// sending [`LoadProgress`] events as it goes.
#[tauri::command]
#[specta::specta]
pub async fn run_load_test(
    text: String,
    variables: BTreeMap<String, String>,
    source: Option<String>,
    app: AppHandle,
    runner: State<'_, Runner>,
    store: State<'_, Mutex<Store>>,
) -> Result<LoadReport, String> {
    let test = checked(&text)?;
    let Some(profile) = &test.load else {
        return Err("the file has no `load` section".into());
    };
    let mut vars = Variables::new();
    vars.extend(variables);
    let report = run_load(
        &runner,
        &test.flow,
        &vars,
        profile,
        &test.thresholds,
        &LoadOptions::default(),
        |p| {
            let _ = LoadProgress {
                elapsed_s: p.elapsed.as_secs() as u32,
                total_s: p.total.as_secs() as u32,
                requests: p.second.requests,
                failed: p.second.failed,
                p95_ms: p.second.p95_ms,
                active: p.second.active,
            }
            .emit(&app);
        },
    )
    .await;
    save(&store, RunData::Load(report.clone()), source.as_deref());
    Ok(report)
}

/// Keeps a run in the history. A history that cannot be written must not
/// lose the result the user is waiting for, so failures are only logged.
fn save(store: &Mutex<Store>, data: RunData, source: Option<&str>) {
    let store = store.lock().unwrap_or_else(|e| e.into_inner());
    if let Err(err) = store.save(&data, source) {
        tracing::warn!("could not save the run: {err}");
    }
}

/// Parses the file and refuses it if it has errors; the editor already
/// shows why.
fn checked(text: &str) -> Result<TestFile, String> {
    let test = TestFile::from_yaml(text).map_err(|e| e.to_string())?;
    if let Some(issue) = test
        .validate()
        .into_iter()
        .find(|i| i.severity == Severity::Error)
    {
        return Err(format!("{}: {}", issue.path, issue.message));
    }
    Ok(test)
}

/// Runs the file's flow once, as a functional test, with `variables` (an
/// environment) over the flow's own. Refuses a file that does not parse or
/// has errors; the editor already shows why.
#[tauri::command]
#[specta::specta]
pub async fn run_test_file(
    text: String,
    variables: BTreeMap<String, String>,
    source: Option<String>,
    runner: State<'_, Runner>,
    store: State<'_, Mutex<Store>>,
) -> Result<FlowResult, String> {
    let test = checked(&text)?;
    let mut vars = Variables::new();
    vars.extend(variables);
    let result = runner.run(&test.flow, &vars).await;
    save(
        &store,
        RunData::Functional(result.clone()),
        source.as_deref(),
    );
    Ok(result)
}

/// The most recent runs first.
#[tauri::command]
#[specta::specta]
pub fn list_runs(limit: u32, store: State<'_, Mutex<Store>>) -> Result<Vec<RunSummary>, String> {
    lock(&store).list(limit).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn get_run(id: u32, store: State<'_, Mutex<Store>>) -> Result<StoredRun, String> {
    lock(&store).get(id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn delete_run(id: u32, store: State<'_, Mutex<Store>>) -> Result<(), String> {
    lock(&store).delete(id).map_err(|e| e.to_string())
}

/// Compares an older run with a newer one.
#[tauri::command]
#[specta::specta]
pub fn diff_runs(
    before: u32,
    after: u32,
    store: State<'_, Mutex<Store>>,
) -> Result<RunDiff, String> {
    let store = lock(&store);
    let (a, b) = (
        store.get(before).map_err(|e| e.to_string())?,
        store.get(after).map_err(|e| e.to_string())?,
    );
    Ok(stormix_store::diff(&a.data, &b.data))
}

/// Writes a stored run as a JSON, JUnit XML or HTML report.
#[tauri::command]
#[specta::specta]
pub fn export_run(
    id: u32,
    path: String,
    format: ReportFormat,
    store: State<'_, Mutex<Store>>,
) -> Result<(), String> {
    let run = lock(&store).get(id).map_err(|e| e.to_string())?;
    let text = match (format, &run.data) {
        (ReportFormat::Json, RunData::Functional(r)) => to_json(r),
        (ReportFormat::Json, RunData::Load(r)) => to_json(r),
        (ReportFormat::Junit, RunData::Functional(r)) => stormix_engine::junit_report(r),
        (ReportFormat::Junit, RunData::Load(r)) => stormix_engine::junit_load_report(r),
        (ReportFormat::Html, RunData::Functional(r)) => stormix_report::flow_report(r),
        (ReportFormat::Html, RunData::Load(r)) => stormix_report::load_report(r),
    };
    std::fs::write(&path, text).map_err(|e| format!("cannot write {path}: {e}"))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReportFormat {
    Json,
    Junit,
    Html,
}

fn to_json(value: &impl Serialize) -> String {
    serde_json::to_string_pretty(value).expect("results always serialize")
}

fn lock(store: &Mutex<Store>) -> std::sync::MutexGuard<'_, Store> {
    store.lock().unwrap_or_else(|e| e.into_inner())
}

/// Reads a test file the user picked.
#[tauri::command]
#[specta::specta]
pub fn read_text_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("cannot read {path}: {e}"))
}

/// Saves the editor's text where the user chose.
#[tauri::command]
#[specta::specta]
pub fn write_text_file(path: String, text: String) -> Result<(), String> {
    std::fs::write(&path, text).map_err(|e| format!("cannot write {path}: {e}"))
}

/// The visual editor works on the parsed file and writes YAML back.
#[tauri::command]
#[specta::specta]
pub fn test_file_to_yaml(test: TestFile) -> String {
    test.to_yaml()
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
