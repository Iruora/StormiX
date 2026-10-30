//! The values `{{name}}` resolves to while a flow runs.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use stormix_flow::{render, Rendered};

/// Later sources win: flow variables, then an environment, then values set on
/// the command line, then whatever earlier steps extracted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Variables {
    values: BTreeMap<String, String>,
}

impl Variables {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets every entry, replacing values already there.
    pub fn extend<K: Into<String>, V: Into<String>>(
        &mut self,
        entries: impl IntoIterator<Item = (K, V)>,
    ) {
        for (k, v) in entries {
            self.values.insert(k.into(), v.into());
        }
    }

    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.values.insert(name.into(), value.into());
    }

    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    /// `text` with its `{{name}}` references replaced. Also knows the dynamic
    /// variables Postman collections commonly use.
    pub fn render(&self, text: &str) -> Rendered {
        render(text, |name| match name.strip_prefix('$') {
            Some(dynamic) => dynamic_value(dynamic),
            None => self.get(name).map(str::to_owned),
        })
    }

    /// Renders every string inside a JSON value, keys included.
    pub fn render_json(
        &self,
        value: &serde_json::Value,
        missing: &mut Vec<String>,
    ) -> serde_json::Value {
        use serde_json::Value;
        match value {
            Value::String(s) => Value::String(self.render_collecting(s, missing)),
            Value::Array(items) => {
                Value::Array(items.iter().map(|v| self.render_json(v, missing)).collect())
            }
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(k, v)| {
                        (
                            self.render_collecting(k, missing),
                            self.render_json(v, missing),
                        )
                    })
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    /// Like [`render`](Self::render), adding names with no value to `missing` once each.
    pub fn render_collecting(&self, text: &str, missing: &mut Vec<String>) -> String {
        let rendered = self.render(text);
        for name in rendered.missing {
            if !missing.contains(&name) {
                missing.push(name);
            }
        }
        rendered.text
    }
}

fn dynamic_value(name: &str) -> Option<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    Some(match name {
        "guid" | "randomUUID" => uuid::Uuid::new_v4().to_string(),
        "timestamp" => now.as_secs().to_string(),
        "timestampMs" => now.as_millis().to_string(),
        "randomInt" => fastrand::u32(0..=1000).to_string(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn later_values_win() {
        let mut vars = Variables::new();
        vars.extend([("host", "a"), ("id", "1")]);
        vars.extend([("host", "b")]);
        assert_eq!(vars.render("{{host}}/{{id}}").text, "b/1");
    }

    #[test]
    fn dynamic_variables() {
        let vars = Variables::new();
        let guid = vars.render("{{$guid}}").text;
        assert_eq!(guid.len(), 36);
        assert!(vars.render("{{$timestamp}}").text.parse::<u64>().is_ok());
        assert_eq!(vars.render("{{$unknown}}").missing, ["$unknown"]);
    }

    #[test]
    fn renders_inside_json() {
        let mut vars = Variables::new();
        vars.set("user", "demo");
        let mut missing = Vec::new();
        let out = vars.render_json(
            &json!({ "name": "{{user}}", "tags": ["{{user}}", 3], "{{key}}": true }),
            &mut missing,
        );
        assert_eq!(
            out,
            json!({ "name": "demo", "tags": ["demo", 3], "{{key}}": true })
        );
        assert_eq!(missing, ["key"]);
    }
}
