//! Saved workspaces: what the app had open, so it comes back after a restart.
//!
//! They live in one JSON file in the app's data folder, written whole on
//! every change (to a temporary file first, then renamed over it). Secret
//! values are never part of a workspace, only the names of secrets.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;

/// An environment as the app keeps it: its variables and the names of its
/// secrets, which have no value here.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkspaceEnv {
    pub variables: BTreeMap<String, String>,
    pub secrets: Vec<String>,
    /// The file it was read from, where Variables saves it; `None` for a
    /// collection's environment not saved yet.
    pub path: Option<String>,
}

/// Everything needed to pick the work back up.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Workspace {
    pub id: String,
    /// Shown in the list. Follows the open file until the user renames it.
    pub name: String,
    /// The user gave the name, so it no longer follows the file.
    pub renamed: bool,
    /// Milliseconds since 1970, set when saved.
    pub updated_ms: f64,
    /// The test file it is saved to, if any.
    pub path: Option<String>,
    /// The editor's text. `None` is the untouched example.
    pub text: Option<String>,
    /// The text has changes not saved to `path`.
    pub dirty: bool,
    /// The text came from an imported collection and has no file yet.
    pub imported: bool,
    /// Every environment in the list, by name.
    pub environments: BTreeMap<String, WorkspaceEnv>,
    /// The chosen environment, or empty for none.
    pub environment: String,
    /// The editor tab: steps, flow or yaml.
    pub tab: String,
    /// Steps left out of the last load test, by name.
    pub excluded: Vec<String>,
    /// The page: file or runs.
    pub view: String,
}

/// The saved workspaces, most recently used first, and which one to reopen.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Workspaces {
    pub last: Option<String>,
    pub items: Vec<Workspace>,
}

pub struct WorkspaceFile {
    path: PathBuf,
    data: Workspaces,
}

impl WorkspaceFile {
    /// Reads the file, or starts empty when there is none. A file that does
    /// not parse is kept aside as `.bad` rather than lost.
    pub fn open(path: &Path) -> Self {
        let data = match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
                tracing::warn!("{} does not parse, starting afresh: {err}", path.display());
                let _ = std::fs::rename(path, path.with_extension("json.bad"));
                Workspaces::default()
            }),
            Err(_) => Workspaces::default(),
        };
        Self {
            path: path.to_owned(),
            data,
        }
    }

    pub fn list(&self) -> Workspaces {
        self.data.clone()
    }

    /// Adds or replaces a workspace, moves it to the top and makes it the
    /// one to reopen.
    pub fn save(&mut self, mut ws: Workspace) -> Result<Workspace, String> {
        if ws.id.is_empty() {
            return Err("a workspace needs an id".into());
        }
        ws.updated_ms = now_ms();
        self.data.items.retain(|w| w.id != ws.id);
        self.data.items.insert(0, ws.clone());
        self.data.last = Some(ws.id.clone());
        self.write()?;
        Ok(ws)
    }

    /// Makes a workspace the one to reopen, without changing it.
    pub fn select(&mut self, id: &str) -> Result<(), String> {
        if !self.data.items.iter().any(|w| w.id == id) {
            return Err(format!("no workspace {id}"));
        }
        self.data.last = Some(id.to_owned());
        self.write()
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        self.data.items.retain(|w| w.id != id);
        if self.data.last.as_deref() == Some(id) {
            self.data.last = None;
        }
        self.write()
    }

    fn write(&self) -> Result<(), String> {
        let text = serde_json::to_string_pretty(&self.data).expect("workspaces always serialize");
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text)
            .and_then(|()| std::fs::rename(&tmp, &self.path))
            .map_err(|e| format!("cannot write {}: {e}", self.path.display()))
    }
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(id: &str) -> Workspace {
        Workspace {
            id: id.into(),
            name: id.into(),
            ..Default::default()
        }
    }

    #[test]
    fn saves_and_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        let mut file = WorkspaceFile::open(&path);
        assert_eq!(file.list(), Workspaces::default());

        file.save(ws("a")).unwrap();
        let mut b = ws("b");
        b.path = Some("/tmp/x.stormix.yaml".into());
        b.environments.insert(
            "local".into(),
            WorkspaceEnv {
                variables: [("base".to_owned(), "http://x".to_owned())].into(),
                secrets: vec!["token".into()],
                path: Some("/tmp/environments/local.yaml".into()),
            },
        );
        file.save(b.clone()).unwrap();

        let again = WorkspaceFile::open(&path).list();
        assert_eq!(again.last.as_deref(), Some("b"));
        assert_eq!(
            again
                .items
                .iter()
                .map(|w| w.id.as_str())
                .collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert_eq!(again.items[0].environments, b.environments);
        assert!(again.items[0].updated_ms > 0.0);

        // Saving again moves it back to the top.
        file.save(ws("a")).unwrap();
        assert_eq!(file.list().items[0].id, "a");
        file.select("b").unwrap();
        assert_eq!(WorkspaceFile::open(&path).list().last.as_deref(), Some("b"));
        assert!(file.select("missing").is_err());

        file.delete("b").unwrap();
        let after = WorkspaceFile::open(&path).list();
        assert_eq!(after.last, None);
        assert_eq!(after.items.len(), 1);
    }

    #[test]
    fn keeps_a_broken_file_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        std::fs::write(&path, "{ not json").unwrap();
        let file = WorkspaceFile::open(&path);
        assert_eq!(file.list(), Workspaces::default());
        assert!(dir.path().join("workspaces.json.bad").exists());
    }

    #[test]
    fn reads_older_files_with_missing_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        std::fs::write(&path, r#"{"items":[{"id":"a","name":"A"}]}"#).unwrap();
        let list = WorkspaceFile::open(&path).list();
        assert_eq!(list.items[0].name, "A");
        assert_eq!(list.items[0].text, None);
    }

    #[test]
    fn refuses_a_workspace_without_an_id() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = WorkspaceFile::open(&dir.path().join("w.json"));
        assert!(file.save(Workspace::default()).is_err());
    }
}
