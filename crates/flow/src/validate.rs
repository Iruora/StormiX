//! Checks that go beyond parsing: empty flows, undefined variables, load
//! settings that would send nothing.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::graphql;
use crate::template::variable_refs;
use crate::{Assertion, Body, ExtractSource, LoadProfile, TestFile};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// The file can run, but probably not as intended.
    Warning,
    /// The file cannot run.
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub severity: Severity,
    /// Where in the file, like `flow.steps[1].request.url`.
    pub path: String,
    pub message: String,
}

impl TestFile {
    /// Every problem found, errors and warnings, in file order.
    pub fn validate(&self) -> Vec<Issue> {
        let mut issues = Issues::default();
        let flow = &self.flow;

        if flow.steps.is_empty() {
            issues.error("flow.steps", "the flow has no steps");
        } else if flow.enabled_steps().next().is_none() {
            issues.error(
                "flow.steps",
                "every step is disabled, so a run would send nothing",
            );
        }

        // Variables a step may use: the flow's own, plus whatever earlier steps
        // extract. Anything else must come from an environment, so it warns.
        let mut known: BTreeSet<&str> = flow.variables.keys().map(String::as_str).collect();
        let mut names = BTreeSet::new();
        // Scripts that have run by the time a step is sent; a variable they
        // name may well be one they set.
        let mut scripts: Vec<&str> = Vec::new();
        // Disabled steps are not sent, so they are not checked either, and
        // what they would extract is not available to the steps after them.
        for (i, step) in flow.steps.iter().enumerate().filter(|(_, s)| s.enabled) {
            let at = format!("flow.steps[{i}]");
            if step.name.trim().is_empty() {
                issues.error(&format!("{at}.name"), "the step has no name");
            } else if !names.insert(step.name.as_str()) {
                issues.warning(
                    &format!("{at}.name"),
                    &format!("another step is also named `{}`", step.name),
                );
            }

            let request = &step.request;
            if request.url.trim().is_empty() {
                issues.error(&format!("{at}.request.url"), "the URL is empty");
            } else if !(request.url.starts_with("http://")
                || request.url.starts_with("https://")
                || request.url.starts_with("{{"))
            {
                issues.warning(
                    &format!("{at}.request.url"),
                    "the URL does not start with http:// or https://",
                );
            }

            let mut templated = vec![(format!("{at}.request.url"), request.url.as_str())];
            for (h, header) in request.headers.iter().enumerate() {
                templated.push((format!("{at}.request.headers[{h}]"), header.value.as_str()));
            }
            let body_text;
            match &request.body {
                Some(Body::Raw(text)) => templated.push((format!("{at}.request.body"), text)),
                Some(Body::Json(value)) => {
                    body_text = value.to_string();
                    templated.push((format!("{at}.request.body"), &body_text));
                }
                Some(Body::Form(fields) | Body::Multipart(fields)) => {
                    for field in fields {
                        templated.push((format!("{at}.request.body"), field.value.as_str()));
                    }
                }
                Some(Body::Graphql(graphql)) => {
                    let at = format!("{at}.request.body.graphql");
                    if graphql.query.trim().is_empty() {
                        issues.error(&format!("{at}.query"), "the GraphQL query is empty");
                    }
                    templated.push((format!("{at}.query"), &graphql.query));
                    if let Some(name) = &graphql.operation_name {
                        templated.push((format!("{at}.operation_name"), name));
                    }
                    match &graphql.variables {
                        None | Some(serde_json::Value::Null) => {}
                        Some(serde_json::Value::String(text)) => {
                            if let Err(message) = graphql::check_variables_text(text) {
                                issues.error(&format!("{at}.variables"), &message);
                            }
                            templated.push((format!("{at}.variables"), text));
                        }
                        Some(value @ serde_json::Value::Object(_)) => {
                            body_text = value.to_string();
                            templated.push((format!("{at}.variables"), &body_text));
                        }
                        Some(_) => issues.error(
                            &format!("{at}.variables"),
                            "GraphQL variables are an object, like `{ id: 42 }`",
                        ),
                    }
                }
                None => {}
            }
            if let Some(source) = &step.scripts.pre_request {
                scripts.push(source);
            }
            for (path, text) in templated {
                for name in variable_refs(text) {
                    if !known.contains(name) && !mentioned(&scripts, name) {
                        issues.warning(
                            &path,
                            &format!("`{{{{{name}}}}}` is not defined in the flow or extracted by an earlier step; it must come from an environment"),
                        );
                    }
                }
            }

            for (name, source) in &step.extract {
                if let ExtractSource::Json(path) = source {
                    if !path.starts_with('$') {
                        issues.error(
                            &format!("{at}.extract.{name}"),
                            "a JSONPath starts with `$`, like `$.data.id`",
                        );
                    }
                }
            }
            for (a, assertion) in step.assertions.iter().enumerate() {
                let path = format!("{at}.assert[{a}]");
                match assertion {
                    Assertion::Status(code) if !(100..=599).contains(code) => {
                        issues.error(&path, &format!("{code} is not an HTTP status code"));
                    }
                    Assertion::Json(json) if !json.path.starts_with('$') => {
                        issues.error(&path, "a JSONPath starts with `$`, like `$.data.id`");
                    }
                    _ => {}
                }
            }
            // Extracted values only exist for the steps after this one.
            known.extend(step.extract.keys().map(String::as_str));
            if let Some(source) = &step.scripts.test {
                scripts.push(source);
            }
        }

        match &self.load {
            Some(LoadProfile::ConstantRate { rate, duration }) => {
                if *rate == 0 {
                    issues.error("load.constant_rate.rate", "the rate must be at least 1");
                }
                if duration.is_zero() {
                    issues.error(
                        "load.constant_rate.duration",
                        "the duration must be longer than 0s",
                    );
                }
            }
            Some(LoadProfile::Ramping { start_rate, stages }) => {
                if stages.is_empty() {
                    issues.error("load.ramping.stages", "ramping needs at least one stage");
                }
                for (s, stage) in stages.iter().enumerate() {
                    if stage.duration.is_zero() {
                        issues.error(
                            &format!("load.ramping.stages[{s}].duration"),
                            "the duration must be longer than 0s",
                        );
                    }
                }
                if *start_rate == 0 && stages.iter().all(|s| s.target == 0) {
                    issues.error(
                        "load.ramping.stages",
                        "every stage targets 0, so nothing is sent",
                    );
                }
            }
            Some(LoadProfile::VirtualUsers {
                count, duration, ..
            }) => {
                if *count == 0 {
                    issues.error("load.virtual_users.count", "there must be at least 1 user");
                }
                if duration.is_zero() {
                    issues.error(
                        "load.virtual_users.duration",
                        "the duration must be longer than 0s",
                    );
                }
            }
            None => {
                for (t, threshold) in self.thresholds.iter().enumerate() {
                    if threshold.metric.needs_load() {
                        issues.warning(
                            &format!("thresholds[{t}]"),
                            &format!(
                                "`{threshold}` is only measured under load; add a `load` section"
                            ),
                        );
                    }
                }
            }
        }

        issues.0
    }
}

#[derive(Default)]
struct Issues(Vec<Issue>);

impl Issues {
    fn error(&mut self, path: &str, message: &str) {
        self.push(Severity::Error, path, message);
    }

    fn warning(&mut self, path: &str, message: &str) {
        self.push(Severity::Warning, path, message);
    }

    fn push(&mut self, severity: Severity, path: &str, message: &str) {
        self.0.push(Issue {
            severity,
            path: path.to_owned(),
            message: message.to_owned(),
        });
    }
}

/// Whether any script names `name` as a string literal, as in
/// `pm.environment.set("token", ...)`.
fn mentioned(scripts: &[&str], name: &str) -> bool {
    scripts.iter().any(|source| {
        ['"', '\'', '`']
            .iter()
            .any(|q| source.contains(&format!("{q}{name}{q}")))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issues(yaml: &str) -> Vec<(Severity, String)> {
        TestFile::from_yaml(yaml)
            .unwrap()
            .validate()
            .into_iter()
            .map(|i| (i.severity, i.path))
            .collect()
    }

    #[test]
    fn a_clean_file_has_no_issues() {
        let yaml = r#"
version: 1
flow:
  name: ok
  variables: { base: "https://example.com" }
  steps:
    - name: token
      request: { method: POST, url: "{{base}}/token" }
      extract: { token: { json: $.token } }
    - name: me
      request:
        method: GET
        url: "{{base}}/me"
        headers: [{ name: Authorization, value: "Bearer {{token}}" }]
"#;
        assert_eq!(issues(yaml), []);
    }

    #[test]
    fn variables_must_be_defined_before_use() {
        let yaml = r#"
version: 1
flow:
  name: order
  steps:
    - name: uses token too early
      request: { method: GET, url: "https://example.com/{{token}}" }
    - name: sets token
      request: { method: GET, url: "https://example.com" }
      extract: { token: { header: X-Token } }
"#;
        assert_eq!(
            issues(yaml),
            [(Severity::Warning, "flow.steps[0].request.url".into())]
        );
    }

    #[test]
    fn variables_a_script_names_count_as_set() {
        let yaml = r#"
version: 1
flow:
  name: scripts
  steps:
    - name: log in
      request: { method: POST, url: "https://example.com/login" }
      scripts: { test: 'pm.environment.set("token", pm.response.json().token)' }
    - name: me
      request:
        method: GET
        url: "https://example.com/me/{{id}}"
        headers: [{ name: Authorization, value: "Bearer {{token}}" }]
      scripts: { pre_request: "bru.setVar('id', 7)" }
    - name: other
      request: { method: GET, url: "https://example.com/{{other}}" }
"#;
        assert_eq!(
            issues(yaml),
            [(Severity::Warning, "flow.steps[2].request.url".into())]
        );
    }

    #[test]
    fn disabled_and_set_aside_steps_are_not_checked() {
        let yaml = r#"
version: 1
flow:
  name: partial
  steps:
    - name: sets token, but is off
      enabled: false
      request: { method: GET, url: "" }
      extract: { token: { header: X-Token } }
    - name: me
      request: { method: GET, url: "https://example.com/{{token}}" }
  set_aside:
    - name: parked
      request: { method: GET, url: "" }
"#;
        // The disabled step's empty URL is not reported, and its extract
        // does not define `token` for the step after it.
        assert_eq!(
            issues(yaml),
            [(Severity::Warning, "flow.steps[1].request.url".into())]
        );
    }

    #[test]
    fn a_flow_with_every_step_disabled_cannot_run() {
        let yaml = r#"
version: 1
flow:
  name: off
  steps:
    - name: ping
      enabled: false
      request: { method: GET, url: "https://example.com" }
"#;
        assert_eq!(issues(yaml), [(Severity::Error, "flow.steps".into())]);
    }

    #[test]
    fn reports_structural_errors() {
        let yaml = r#"
version: 1
flow:
  name: broken
  steps:
    - name: a
      request: { method: GET, url: "" }
      extract: { id: { json: data.id } }
      assert: [{ status: 42 }]
    - name: a
      request: { method: GET, url: "ftp://example.com" }
load:
  constant_rate: { rate: 0, duration: 0s }
"#;
        assert_eq!(
            issues(yaml),
            [
                (Severity::Error, "flow.steps[0].request.url".into()),
                (Severity::Error, "flow.steps[0].extract.id".into()),
                (Severity::Error, "flow.steps[0].assert[0]".into()),
                (Severity::Warning, "flow.steps[1].name".into()),
                (Severity::Warning, "flow.steps[1].request.url".into()),
                (Severity::Error, "load.constant_rate.rate".into()),
                (Severity::Error, "load.constant_rate.duration".into()),
            ]
        );
    }

    #[test]
    fn checks_graphql_bodies() {
        let yaml = r#"
version: 1
flow:
  name: graphql
  variables: { id: "7" }
  steps:
    - name: ok
      request:
        method: POST
        url: "https://example.com/graphql"
        body: { graphql: { query: "query ($id: Int!) { user(id: $id) { name } }", variables: { id: "{{id}}" } } }
    - name: undefined
      request:
        method: POST
        url: "https://example.com/graphql"
        body: { graphql: { query: "{ x }", variables: '{ "id": {{other}} }' } }
    - name: broken
      request:
        method: POST
        url: "https://example.com/graphql"
        body: { graphql: { query: " ", variables: '{ "id": ' } }
    - name: not an object
      request:
        method: POST
        url: "https://example.com/graphql"
        body: { graphql: { query: "{ x }", variables: [1] } }
"#;
        let at = |i: usize, field: &str| format!("flow.steps[{i}].request.body.graphql.{field}");
        assert_eq!(
            issues(yaml),
            [
                (Severity::Warning, at(1, "variables")),
                (Severity::Error, at(2, "query")),
                (Severity::Error, at(2, "variables")),
                (Severity::Error, at(3, "variables")),
            ]
        );
    }

    #[test]
    fn throughput_thresholds_need_load() {
        let yaml = r#"
version: 1
flow:
  name: functional
  steps:
    - name: ping
      request: { method: GET, url: "https://example.com" }
thresholds: [rps > 10, p95 < 100ms]
"#;
        assert_eq!(issues(yaml), [(Severity::Warning, "thresholds[0]".into())]);
    }
}
