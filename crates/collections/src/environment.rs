//! Environment files on their own, outside a collection: a flat YAML or
//! JSON map of names to values, a Postman environment export
//! (`{"name", "values": [{"key", "value", "enabled"}]}`) or a Bruno
//! environment (`.bru`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{bruno, read, ImportError};

#[derive(Debug, Clone, PartialEq)]
pub struct Environment {
    /// The export's own name for a Postman environment, otherwise the
    /// file name without its extension.
    pub name: String,
    pub variables: BTreeMap<String, String>,
    /// Names of secrets the file declares without a value (Bruno's
    /// `vars:secret`), to be supplied at run time.
    pub secrets: Vec<String>,
}

/// Reads the environment file at `path`.
pub fn read_file(path: &Path) -> Result<Environment, ImportError> {
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .trim_end_matches(".postman_environment")
        .to_owned();
    if path.extension().is_some_and(|e| e == "bru") {
        let (variables, secrets) = bruno::environment(path)?;
        return Ok(Environment {
            name: stem,
            variables,
            secrets,
        });
    }
    let mut env = parse(&read(path)?).map_err(|message| ImportError::Environment {
        file: path.to_owned(),
        message,
    })?;
    if env.name.is_empty() {
        env.name = stem;
    }
    Ok(env)
}

/// The folder a test file keeps its environments in: `environments/` next
/// to it, the same layout as a Bruno collection.
pub fn dir_for(test_file: &Path) -> PathBuf {
    test_file
        .parent()
        .unwrap_or(Path::new("."))
        .join("environments")
}

/// Every environment in `dir` (`.yaml`, `.yml`, `.json` or `.bru` files),
/// by name. A missing folder has none.
pub fn read_dir(dir: &Path) -> Result<Vec<Environment>, ImportError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(ImportError::Io {
                path: dir.to_owned(),
                source,
            })
        }
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .is_some_and(|e| matches!(e.to_str(), Some("yaml" | "yml" | "json" | "bru")))
        })
        .collect();
    files.sort();
    files.iter().map(|f| read_file(f)).collect()
}

/// The key a flat YAML or JSON environment lists its secrets under.
pub const SECRETS_KEY: &str = "$secrets";

/// A flat YAML file for `env`, which [`read_file`] reads back. Secrets
/// have no value; their names are listed under `$secrets`.
pub fn to_yaml(env: &Environment) -> String {
    let mut out = String::new();
    if !env.variables.is_empty() {
        out.push_str(
            &serde_yaml_ng::to_string(&env.variables).expect("a string map always serializes"),
        );
    }
    if !env.secrets.is_empty() {
        out.push_str("# Secrets have no value here; pass them with --var NAME=VALUE.\n");
        let list = serde_json::to_string(&env.secrets).expect("names always serialize");
        out.push_str(&format!("{SECRETS_KEY}: {list}\n"));
    }
    if out.is_empty() {
        out.push_str("{}\n");
    }
    out
}

/// A YAML or JSON environment. Its name is empty unless it carries one
/// (a Postman export's `name`).
pub fn parse(text: &str) -> Result<Environment, String> {
    // JSON is YAML, so one parser covers both.
    let value: Value = serde_yaml_ng::from_str(text).map_err(|e| e.to_string())?;
    let Value::Object(map) = value else {
        return Err("expected a map of variable names to values".into());
    };
    let mut env = Environment {
        name: String::new(),
        variables: BTreeMap::new(),
        secrets: Vec::new(),
    };
    if let Some(Value::Array(entries)) = map.get("values") {
        for entry in entries {
            let enabled = entry
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            if let (true, Some(key)) = (enabled, entry.get("key").and_then(Value::as_str)) {
                env.variables.insert(
                    key.to_owned(),
                    scalar(entry.get("value").unwrap_or(&Value::Null))?,
                );
            }
        }
        if let Some(name) = map.get("name").and_then(Value::as_str) {
            name.trim().clone_into(&mut env.name);
        }
        return Ok(env);
    }
    for (name, value) in &map {
        if name == SECRETS_KEY {
            let names = value.as_array().filter(|l| l.iter().all(Value::is_string));
            let Some(names) = names else {
                return Err(format!("{SECRETS_KEY}: expected a list of variable names"));
            };
            env.secrets
                .extend(names.iter().filter_map(Value::as_str).map(str::to_owned));
            continue;
        }
        env.variables.insert(
            name.clone(),
            scalar(value).map_err(|e| format!("{name}: {e}"))?,
        );
    }
    Ok(env)
}

fn scalar(value: &Value) -> Result<String, String> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Null => Ok(String::new()),
        _ => Err("values must be text, numbers or booleans".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_bruno_environments() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("local.bru");
        std::fs::write(
            &path,
            "vars {\n  base_url: http://x.test\n  ~off: 1\n}\nvars:secret [\n  token\n]\n",
        )
        .unwrap();
        let env = read_file(&path).unwrap();
        assert_eq!(env.name, "local");
        assert_eq!(
            env.variables.get("base_url").map(String::as_str),
            Some("http://x.test")
        );
        assert_eq!(env.variables.get("off"), None);
        assert_eq!(env.secrets, ["token"]);
    }

    #[test]
    fn writes_yaml_that_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let env = Environment {
            name: "local".into(),
            variables: [
                ("url".into(), "http://x.test:8080".into()),
                ("n".into(), "007".into()),
            ]
            .into(),
            secrets: vec!["token".into()],
        };
        let path = dir_for(&dir.path().join("api.stormix.yaml"));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("local.yaml"), to_yaml(&env)).unwrap();
        std::fs::write(path.join("notes.txt"), "not an environment").unwrap();
        let read = read_dir(&path).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].name, "local");
        assert_eq!(read[0].variables, env.variables);
        assert_eq!(read[0].secrets, env.secrets);
        assert!(read_dir(&dir.path().join("missing")).unwrap().is_empty());
    }

    #[test]
    fn reads_flat_yaml_and_json() {
        let env = parse("base_url: https://x.test\nport: 8080\ndebug: true\n").unwrap();
        assert_eq!(env.name, "");
        let vars = env.variables;
        assert_eq!(vars["base_url"], "https://x.test");
        assert_eq!(vars["port"], "8080");
        assert_eq!(vars["debug"], "true");
        let vars = parse(r#"{"token": "abc"}"#).unwrap().variables;
        assert_eq!(vars["token"], "abc");
        let env = parse("a: 1\n$secrets: [token, key]\n").unwrap();
        assert_eq!(env.secrets, ["token", "key"]);
        assert!(!env.variables.contains_key("$secrets"));
        assert!(parse("$secrets: token\n").is_err());
        assert!(parse("nested: { a: 1 }").is_err());
        assert!(parse("- a").is_err());
    }

    #[test]
    fn reads_postman_environments() {
        let env = parse(
            r#"{"name": "Staging", "values": [
                {"key": "base_url", "value": "https://staging.test", "enabled": true},
                {"key": "off", "value": "x", "enabled": false},
                {"key": "n", "value": 3}
            ]}"#,
        )
        .unwrap();
        assert_eq!(env.name, "Staging");
        let vars = env.variables;
        assert_eq!(vars["base_url"], "https://staging.test");
        assert_eq!(vars.get("off"), None);
        assert_eq!(vars["n"], "3");
    }

    #[test]
    fn names_a_file_after_its_stem() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dev.postman_environment.json");
        std::fs::write(&path, r#"{"values": [{"key": "a", "value": "1"}]}"#).unwrap();
        assert_eq!(read_file(&path).unwrap().name, "dev");
        let path = dir.path().join("qa.yaml");
        std::fs::write(&path, "a: 1\n").unwrap();
        assert_eq!(read_file(&path).unwrap().name, "qa");
    }
}
