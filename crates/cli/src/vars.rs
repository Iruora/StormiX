//! Variables from `--var name=value` and `--env file`.

use std::path::Path;

use serde_json::Value;
use stormix_engine::Variables;

/// Parses `name=value`. The value may itself contain `=`.
pub fn parse_var(arg: &str) -> Result<(String, String), String> {
    match arg.split_once('=') {
        Some((name, value)) if !name.trim().is_empty() => {
            Ok((name.trim().to_owned(), value.to_owned()))
        }
        _ => Err(format!("expected name=value, got `{arg}`")),
    }
}

/// Reads an environment file: a flat YAML or JSON map of names to values,
/// a Postman environment export (`{"values": [{"key", "value", "enabled"}]}`)
/// or a Bruno environment (`.bru`).
pub fn read_env(path: &Path) -> Result<Variables, String> {
    if path.extension().is_some_and(|e| e == "bru") {
        let env = stormix_collections::bruno::environment(path).map_err(|e| e.to_string())?;
        let mut vars = Variables::new();
        vars.extend(env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        return Ok(vars);
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    parse_env(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn parse_env(text: &str) -> Result<Variables, String> {
    // JSON is YAML, so one parser covers both.
    let value: Value = serde_yaml_ng::from_str(text).map_err(|e| e.to_string())?;
    let Value::Object(map) = value else {
        return Err("expected a map of variable names to values".into());
    };
    let mut vars = Variables::new();
    if let Some(Value::Array(entries)) = map.get("values") {
        for entry in entries {
            let enabled = entry
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            if let (true, Some(key)) = (enabled, entry.get("key").and_then(Value::as_str)) {
                vars.set(key, scalar(entry.get("value").unwrap_or(&Value::Null))?);
            }
        }
        return Ok(vars);
    }
    for (name, value) in &map {
        vars.set(
            name.clone(),
            scalar(value).map_err(|e| format!("{name}: {e}"))?,
        );
    }
    Ok(vars)
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
    fn parses_vars() {
        assert_eq!(parse_var("a=b=c"), Ok(("a".into(), "b=c".into())));
        assert_eq!(parse_var("empty="), Ok(("empty".into(), String::new())));
        assert!(parse_var("novalue").is_err());
        assert!(parse_var("=x").is_err());
    }

    #[test]
    fn reads_bruno_environments() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("local.bru");
        std::fs::write(
            &path,
            "vars {\n  base_url: http://x.test\n  ~off: 1\n}\nvars:secret [\n  token\n]\n",
        )
        .unwrap();
        let vars = read_env(&path).unwrap();
        assert_eq!(vars.get("base_url"), Some("http://x.test"));
        assert_eq!(vars.get("off"), None);
    }

    #[test]
    fn reads_flat_yaml_and_json() {
        let vars = parse_env("base_url: https://x.test\nport: 8080\ndebug: true\n").unwrap();
        assert_eq!(vars.get("base_url"), Some("https://x.test"));
        assert_eq!(vars.get("port"), Some("8080"));
        assert_eq!(vars.get("debug"), Some("true"));
        let vars = parse_env(r#"{"token": "abc"}"#).unwrap();
        assert_eq!(vars.get("token"), Some("abc"));
        assert!(parse_env("nested: { a: 1 }").is_err());
        assert!(parse_env("- a").is_err());
    }

    #[test]
    fn reads_postman_environments() {
        let vars = parse_env(
            r#"{"name": "Staging", "values": [
                {"key": "base_url", "value": "https://staging.test", "enabled": true},
                {"key": "off", "value": "x", "enabled": false},
                {"key": "n", "value": 3}
            ]}"#,
        )
        .unwrap();
        assert_eq!(vars.get("base_url"), Some("https://staging.test"));
        assert_eq!(vars.get("off"), None);
        assert_eq!(vars.get("n"), Some("3"));
    }
}
