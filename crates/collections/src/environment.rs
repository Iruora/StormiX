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
    files_in(dir)?.iter().map(|f| read_file(f)).collect()
}

/// The environment files in `dir`, sorted. A missing folder has none.
pub fn files_in(dir: &Path) -> Result<Vec<PathBuf>, ImportError> {
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
    Ok(files)
}

/// Writes `env` to `path` in the file's own format, judged by its
/// extension and, for JSON, by what is already there: a Bruno `.bru`
/// environment, a Postman environment export, flat JSON or flat YAML.
/// What the model does not hold is kept where the format allows it: a
/// Postman export's other fields and disabled values, a `.bru` file's
/// disabled lines and other blocks. Secrets are written by name only,
/// never with a value. YAML comments are not kept.
pub fn write_file(path: &Path, env: &Environment) -> Result<(), String> {
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let text = match ext.as_str() {
        "bru" => to_bru(env, existing.as_deref())?,
        "json" => to_json(env, existing.as_deref())?,
        _ => to_yaml(env),
    };
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// A `.bru` environment: `vars { .. }` and `vars:secret [ .. ]`, keeping
/// the disabled lines and any other blocks of `existing`.
fn to_bru(env: &Environment, existing: Option<&str>) -> Result<String, String> {
    use crate::bru::{Block, Content};
    if let Some((k, _)) = env.variables.iter().find(|(_, v)| v.contains('\n')) {
        return Err(format!(
            "{k}: a .bru environment cannot hold a value on several lines"
        ));
    }
    let mut blocks = match existing {
        Some(text) => crate::bru::parse(text).map_err(|(line, m)| format!("line {line}: {m}"))?,
        None => Vec::new(),
    };
    let disabled_pairs: Vec<(String, String, bool)> = match blocks.iter().find(|b| b.name == "vars")
    {
        Some(Block {
            content: Content::Pairs(pairs),
            ..
        }) => pairs
            .iter()
            .filter(|(k, _, on)| !on && !env.variables.contains_key(k))
            .cloned()
            .collect(),
        _ => Vec::new(),
    };
    let disabled_secrets: Vec<String> = match blocks.iter().find(|b| b.name == "vars:secret") {
        Some(Block {
            content: Content::List(names),
            ..
        }) => names
            .iter()
            .filter(|n| {
                n.strip_prefix('~')
                    .is_some_and(|n| !env.secrets.iter().any(|s| s == n))
            })
            .cloned()
            .collect(),
        _ => Vec::new(),
    };
    let mut pairs: Vec<(String, String, bool)> = env
        .variables
        .iter()
        .map(|(k, v)| (k.clone(), v.clone(), true))
        .collect();
    pairs.extend(disabled_pairs);
    let mut secrets = env.secrets.clone();
    secrets.extend(disabled_secrets);
    let vars = Block {
        name: "vars".into(),
        line: 0,
        content: Content::Pairs(pairs),
    };
    let secret = Block {
        name: "vars:secret".into(),
        line: 0,
        content: Content::List(secrets),
    };
    let keep_secret =
        !env.secrets.is_empty() || matches!(&secret.content, Content::List(l) if !l.is_empty());
    let mut place =
        |block: Block, keep: bool| match blocks.iter().position(|b| b.name == block.name) {
            Some(i) if keep => blocks[i] = block,
            Some(i) => {
                blocks.remove(i);
            }
            None if keep => blocks.push(block),
            None => {}
        };
    place(vars, true);
    place(secret, keep_secret);
    Ok(crate::bru::write(&blocks))
}

/// A Postman export keeps its shape: values are updated in place, new ones
/// added, removed ones dropped (disabled ones stay). Any other JSON is
/// written as a flat map, with secret names under `$secrets`.
fn to_json(env: &Environment, existing: Option<&str>) -> Result<String, String> {
    let current: Option<Value> = existing.and_then(|t| serde_json::from_str(t).ok());
    let out = match current {
        Some(Value::Object(mut map)) if matches!(map.get("values"), Some(Value::Array(_))) => {
            let Some(Value::Array(entries)) = map.remove("values") else {
                unreachable!()
            };
            let mut seen = std::collections::BTreeSet::new();
            let mut values = Vec::new();
            for mut entry in entries {
                let key = entry.get("key").and_then(Value::as_str).map(str::to_owned);
                let enabled = entry
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                match key {
                    Some(key) if env.variables.contains_key(&key) => {
                        if seen.insert(key.clone()) {
                            entry["value"] = Value::String(env.variables[&key].clone());
                            entry["enabled"] = Value::Bool(true);
                            values.push(entry);
                        }
                    }
                    Some(_) if enabled => {}
                    _ => values.push(entry),
                }
            }
            for (key, value) in &env.variables {
                if !seen.contains(key) {
                    values.push(serde_json::json!({
                        "key": key, "value": value, "type": "default", "enabled": true
                    }));
                }
            }
            map.insert("values".into(), Value::Array(values));
            Value::Object(map)
        }
        _ => {
            let mut map: serde_json::Map<String, Value> = env
                .variables
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect();
            if !env.secrets.is_empty() {
                map.insert(SECRETS_KEY.into(), serde_json::json!(env.secrets));
            }
            Value::Object(map)
        }
    };
    let mut text = serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?;
    text.push('\n');
    Ok(text)
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

    fn env(vars: &[(&str, &str)], secrets: &[&str]) -> Environment {
        Environment {
            name: String::new(),
            variables: vars
                .iter()
                .map(|(k, v)| ((*k).into(), (*v).into()))
                .collect(),
            secrets: secrets.iter().map(|s| (*s).into()).collect(),
        }
    }

    #[test]
    fn writes_bru_back_keeping_disabled_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("local.bru");
        std::fs::write(
            &path,
            "vars {\n  base_url: http://x.test\n  ~off: 1\n  gone: 2\n}\nvars:secret [\n  token,\n  ~old\n]\n",
        )
        .unwrap();
        write_file(
            &path,
            &env(
                &[("base_url", "http://y.test"), ("added", "a b: c")],
                &["token"],
            ),
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("~off: 1"), "{text}");
        assert!(text.contains("~old"), "{text}");
        assert!(!text.contains("gone"), "{text}");
        let back = read_file(&path).unwrap();
        assert_eq!(back.variables["base_url"], "http://y.test");
        assert_eq!(back.variables["added"], "a b: c");
        assert_eq!(back.variables.get("off"), None);
        assert_eq!(back.secrets, ["token"]);
        assert!(write_file(&path, &env(&[("x", "a\nb")], &[])).is_err());
    }

    #[test]
    fn writes_postman_exports_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("staging.postman_environment.json");
        std::fs::write(
            &path,
            r#"{"id": "e1", "name": "Staging", "values": [
                {"key": "base_url", "value": "https://s.test", "type": "default", "enabled": true},
                {"key": "off", "value": "x", "enabled": false},
                {"key": "gone", "value": "y", "enabled": true}
            ], "_postman_exported_using": "Postman"}"#,
        )
        .unwrap();
        write_file(
            &path,
            &env(&[("base_url", "https://t.test"), ("new", "1")], &[]),
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let json: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["id"], "e1");
        assert_eq!(json["_postman_exported_using"], "Postman");
        let back = read_file(&path).unwrap();
        assert_eq!(back.name, "Staging");
        assert_eq!(back.variables.len(), 2);
        assert_eq!(back.variables["base_url"], "https://t.test");
        assert_eq!(back.variables["new"], "1");
        assert!(text.contains("\"off\""));
    }

    #[test]
    fn writes_flat_json_and_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let e = env(&[("a", "1"), ("url", "http://x.test")], &["token"]);
        for name in ["dev.json", "nested/dev.yaml"] {
            let path = dir.path().join(name);
            write_file(&path, &e).unwrap();
            let back = read_file(&path).unwrap();
            assert_eq!(back.variables, e.variables, "{name}");
            assert_eq!(back.secrets, e.secrets, "{name}");
        }
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
