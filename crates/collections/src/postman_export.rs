//! Writing a test file as a Postman collection (v2.1), with its
//! environments as Postman environment files.
//!
//! Steps named "Folder / Request" go into folders. Checks become `pm.test`
//! lines in each request's Tests script and extractions set collection
//! variables after the response, so later requests read them as
//! `{{name}}`, the same as in Stormix. What Postman cannot hold (steps that
//! are off, response caching, load settings, thresholds, tracing) is left
//! out and reported in the warnings.

use serde_json::{json, Map, Value};
use stormix_flow::{Assertion, Body, ExtractSource, GraphqlBody, Header, Step, TestFile};

use crate::common::js_string;
use crate::environment::Environment;
use crate::opencollection::export::{json_path_expr, method};

const SCHEMA: &str = "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";

/// The collection's JSON, and what could not be carried over.
pub fn to_json(test: &TestFile) -> (String, Vec<String>) {
    let flow = &test.flow;
    let mut warnings = Vec::new();
    let mut top: Vec<Value> = Vec::new();
    let mut placed: Vec<Vec<String>> = Vec::new();
    for step in &flow.steps {
        if !step.enabled {
            warnings.push(format!(
                "`{}` is turned off; Postman has no such switch, so it is left out",
                step.name
            ));
            continue;
        }
        let mut parts: Vec<String> = step
            .name
            .split(" / ")
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect();
        let name = parts.pop().unwrap_or_else(|| step.name.clone());
        folder_at(&mut top, &parts).push(item(step, &name, &mut warnings));
        parts.push(name);
        placed.push(parts);
    }
    let mut order = Vec::new();
    run_order(&top, &mut Vec::new(), &mut order);
    if order != placed {
        warnings.push(
            "steps of one folder are grouped together in Postman, so they run in a different order there".into(),
        );
    }
    if !flow.set_aside.is_empty() {
        warnings.push(format!(
            "{} set-aside step(s) are left out: Postman has nowhere to keep them",
            flow.set_aside.len()
        ));
    }
    if test.load.is_some() || !test.thresholds.is_empty() {
        warnings.push(
            "load settings and thresholds are Stormix's own; Postman has no place for them".into(),
        );
    }
    if flow.trace.is_some() {
        warnings.push("`trace` is Stormix's own; Postman sends no traceparent header".into());
    }

    let mut info = Map::new();
    info.insert("name".into(), json!(flow.name));
    if let Some(d) = flow.description.as_deref().filter(|d| !d.trim().is_empty()) {
        info.insert("description".into(), json!(d));
    }
    info.insert("schema".into(), json!(SCHEMA));
    let mut doc = Map::new();
    doc.insert("info".into(), Value::Object(info));
    doc.insert("item".into(), Value::Array(top));
    if !flow.variables.is_empty() {
        doc.insert(
            "variable".into(),
            flow.variables
                .iter()
                .map(|(k, v)| json!({ "key": k, "value": v }))
                .collect(),
        );
    }
    let text =
        serde_json::to_string_pretty(&Value::Object(doc)).expect("JSON values always serialize");
    (text, warnings)
}

/// An environment as a Postman environment file. Secrets are named
/// without a value.
pub fn environment_json(env: &Environment) -> String {
    let mut values: Vec<Value> = env
        .variables
        .iter()
        .filter(|(k, _)| !env.secrets.contains(k))
        .map(|(k, v)| json!({ "key": k, "value": v, "type": "default", "enabled": true }))
        .collect();
    values.extend(
        env.secrets
            .iter()
            .map(|s| json!({ "key": s, "value": "", "type": "secret", "enabled": true })),
    );
    let doc = json!({
        "name": env.name,
        "values": values,
        "_postman_variable_scope": "environment",
    });
    serde_json::to_string_pretty(&doc).expect("JSON values always serialize")
}

/// The file name Postman gives an exported environment.
pub fn environment_file_name(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("{}.postman_environment.json", safe.trim())
}

/// The items of the folder at `path`, creating folders as needed.
fn folder_at<'a>(mut items: &'a mut Vec<Value>, path: &[String]) -> &'a mut Vec<Value> {
    for name in path {
        let at = items
            .iter()
            .position(|i| i.get("item").is_some() && i["name"].as_str() == Some(name.as_str()));
        let at = at.unwrap_or_else(|| {
            items.push(json!({ "name": name, "item": [] }));
            items.len() - 1
        });
        items = items[at]["item"]
            .as_array_mut()
            .expect("a folder has items");
    }
    items
}

/// Postman runs items depth first, in the order they are listed.
fn run_order(items: &[Value], path: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    for i in items {
        let name = i["name"].as_str().unwrap_or_default().to_owned();
        path.push(name);
        match i.get("item").and_then(Value::as_array) {
            Some(inner) => run_order(inner, path, out),
            None => out.push(path.clone()),
        }
        path.pop();
    }
}

fn item(step: &Step, name: &str, warnings: &mut Vec<String>) -> Value {
    let r = &step.request;
    let mut request = Map::new();
    request.insert("method".into(), json!(method(r.method)));
    request.insert("header".into(), headers(&r.headers));
    if let Some(body) = &r.body {
        request.insert(
            "body".into(),
            body_json(body, &r.headers, &step.name, warnings),
        );
    }
    request.insert("url".into(), json!({ "raw": r.url }));

    let mut tests = Vec::new();
    for (var, source) in &step.extract {
        let value = match source {
            ExtractSource::Json(path) => match json_path_expr(path) {
                Some(expr) => format!("(function () {{ try {{ return {}; }} catch (e) {{ return undefined; }} }})()", body_expr(&expr)),
                None => {
                    warnings.push(format!(
                        "`{}`: Postman cannot follow the JSONPath {path}, so `{var}` is not set",
                        step.name
                    ));
                    continue;
                }
            },
            ExtractSource::Header(h) => format!("pm.response.headers.get({})", js_string(h)),
            ExtractSource::Regex(re) => format!(
                "(function () {{ const m = pm.response.text().match(new RegExp({})); return m ? (m[1] !== undefined ? m[1] : m[0]) : undefined; }})()",
                js_string(re)
            ),
        };
        tests.push(format!(
            "pm.collectionVariables.set({}, {value});",
            js_string(var)
        ));
    }
    for check in &step.assertions {
        let line = match check {
            Assertion::Status(code) => test(
                &format!("status is {code}"),
                &format!("pm.response.to.have.status({code});"),
            ),
            Assertion::MaxDurationMs(ms) => test(
                &format!("responds within {ms} ms"),
                &format!("pm.expect(pm.response.responseTime).to.be.at.most({ms});"),
            ),
            Assertion::Header(h) => {
                let get = format!("pm.response.headers.get({})", js_string(&h.name));
                let mut lines = Vec::new();
                if let Some(v) = &h.equals {
                    lines.push(format!("pm.expect({get}).to.eql({});", text_value(v)));
                }
                if let Some(v) = &h.contains {
                    lines.push(format!("pm.expect({get}).to.include({});", text_value(v)));
                }
                if lines.is_empty() {
                    lines.push(format!(
                        "pm.response.to.have.header({});",
                        js_string(&h.name)
                    ));
                }
                test(&format!("header {}", h.name), &lines.join(" "))
            }
            Assertion::Json(j) => match json_path_expr(&j.path) {
                Some(expr) => {
                    let actual = body_expr(&expr);
                    let body = match (&j.equals, j.exists) {
                        (Some(v), _) => format!("pm.expect({actual}).to.eql({});", json_value(v)),
                        (None, Some(false)) => format!("pm.expect({actual}).to.be.undefined;"),
                        (None, _) => format!("pm.expect({actual}).to.not.be.undefined;"),
                    };
                    test(&format!("json {}", j.path), &body)
                }
                None => {
                    warnings.push(format!(
                        "`{}`: the check on {} is left out; Postman cannot follow that JSONPath",
                        step.name, j.path
                    ));
                    continue;
                }
            },
            Assertion::BodyContains(text) => test(
                &format!("body contains {text}"),
                &format!(
                    "pm.expect(pm.response.text()).to.include({});",
                    text_value(text)
                ),
            ),
        };
        tests.push(line);
    }
    if let Some(code) = step.scripts.test.as_deref() {
        tests.push(code.to_owned());
    }

    let scripts = [&step.scripts.pre_request, &step.scripts.test];
    if scripts.iter().any(|s| {
        s.as_deref()
            .is_some_and(|s| s.contains("bru.") || s.contains("__bruAssert("))
    }) {
        warnings.push(format!(
            "`{}`: its scripts use Bruno's bru.* API, which Postman does not have",
            step.name
        ));
    }
    if step.cache {
        warnings.push(format!(
            "`{}`: response caching is Stormix's own; Postman sends the request every time",
            step.name
        ));
    }

    let mut events = Vec::new();
    if let Some(code) = step.scripts.pre_request.as_deref() {
        events.push(event("prerequest", code));
    }
    if !tests.is_empty() {
        events.push(event("test", &tests.join("\n")));
    }
    let mut doc = Map::new();
    doc.insert("name".into(), json!(name));
    if !events.is_empty() {
        doc.insert("event".into(), Value::Array(events));
    }
    doc.insert("request".into(), Value::Object(request));
    Value::Object(doc)
}

/// `res.body…` from the JSONPath translation, read from Postman's response.
/// Text for a script. Stormix fills `{{name}}` in expected values; Postman
/// does not in scripts, so text that holds one is filled in when it runs.
fn text_value(text: &str) -> String {
    if text.contains("{{") {
        format!("pm.variables.replaceIn({})", js_string(text))
    } else {
        js_string(text)
    }
}

/// A JSON value for a script, with `{{name}}` filled in as for text.
fn json_value(value: &Value) -> String {
    let text = value.to_string();
    match value {
        Value::String(s) => text_value(s),
        _ if text.contains("{{") => {
            format!("JSON.parse(pm.variables.replaceIn({}))", js_string(&text))
        }
        _ => text,
    }
}

fn body_expr(expr: &str) -> String {
    format!(
        "pm.response.json(){}",
        expr.strip_prefix("res.body").unwrap_or(expr)
    )
}

fn test(name: &str, body: &str) -> String {
    format!("pm.test({}, function () {{ {body} }});", js_string(name))
}

fn event(listen: &str, code: &str) -> Value {
    json!({
        "listen": listen,
        "script": { "type": "text/javascript", "exec": code.lines().collect::<Vec<_>>() },
    })
}

fn headers(headers: &[Header]) -> Value {
    headers
        .iter()
        .map(|h| json!({ "key": h.name, "value": h.value }))
        .collect()
}

fn body_json(body: &Body, headers: &[Header], step: &str, warnings: &mut Vec<String>) -> Value {
    match body {
        Body::Json(value) => json!({
            "mode": "raw",
            "raw": serde_json::to_string_pretty(value).unwrap_or_default(),
            "options": { "raw": { "language": "json" } },
        }),
        Body::Raw(text) => {
            let content_type = headers
                .iter()
                .find(|h| h.name.eq_ignore_ascii_case("content-type"))
                .map(|h| h.value.to_ascii_lowercase())
                .unwrap_or_default();
            let language = if content_type.contains("json") {
                "json"
            } else if content_type.contains("xml") {
                "xml"
            } else if content_type.contains("html") {
                "html"
            } else {
                "text"
            };
            json!({ "mode": "raw", "raw": text, "options": { "raw": { "language": language } } })
        }
        Body::Form(fields) => json!({
            "mode": "urlencoded",
            "urlencoded": fields.iter().map(|f| json!({ "key": f.key, "value": f.value })).collect::<Vec<_>>(),
        }),
        Body::Multipart(fields) => json!({
            "mode": "formdata",
            "formdata": fields
                .iter()
                .map(|f| json!({ "key": f.key, "value": f.value, "type": "text" }))
                .collect::<Vec<_>>(),
        }),
        Body::Graphql(g) => graphql(g, step, warnings),
    }
}

fn graphql(g: &GraphqlBody, step: &str, warnings: &mut Vec<String>) -> Value {
    if g.operation_name.is_some() {
        warnings.push(format!(
            "`{step}`: Postman has no operation name; it runs the query's only (or first) operation"
        ));
    }
    if g.allow_errors {
        warnings.push(format!(
            "`{step}`: `allow_errors` is Stormix's own; Postman never fails a request on GraphQL errors"
        ));
    }
    let variables = match &g.variables {
        None => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(v) => serde_json::to_string_pretty(v).unwrap_or_default(),
    };
    json!({ "mode": "graphql", "graphql": { "query": g.query, "variables": variables } })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"
version: 1
flow:
  name: Shop
  variables: { base_url: "https://api.example.com" }
  steps:
    - name: Auth / Log in
      request:
        method: POST
        url: "{{base_url}}/login"
        headers: [{ name: Accept, value: application/json }]
        body: { json: { user: demo } }
      extract: { token: { json: $.data.token } }
      assert:
        - status: 200
        - json: { path: $.data.ok, equals: true }
    - name: Read the profile
      request:
        method: GET
        url: "{{base_url}}/me"
        headers: [{ name: Authorization, value: "Bearer {{token}}" }]
      assert:
        - header: { name: Content-Type, contains: json }
    - name: Off
      enabled: false
      request: { method: GET, url: "{{base_url}}/off" }
"#;

    #[test]
    fn exports_requests_folders_and_checks() {
        let test = TestFile::from_yaml(FILE).unwrap();
        let (text, warnings) = to_json(&test);
        let doc: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(doc["info"]["schema"], SCHEMA);
        assert_eq!(doc["variable"][0]["key"], "base_url");
        let folder = &doc["item"][0];
        assert_eq!(folder["name"], "Auth");
        let login = &folder["item"][0];
        assert_eq!(login["name"], "Log in");
        assert_eq!(login["request"]["method"], "POST");
        assert_eq!(
            login["request"]["body"]["options"]["raw"]["language"],
            "json"
        );
        let tests = login["event"][0]["script"]["exec"].as_array().unwrap();
        let tests: Vec<&str> = tests.iter().filter_map(Value::as_str).collect();
        assert!(tests
            .iter()
            .any(|l| l.contains("pm.collectionVariables.set(\"token\"")
                && l.contains("pm.response.json().data.token")));
        assert!(tests
            .iter()
            .any(|l| l.contains("pm.response.to.have.status(200)")));
        assert!(tests
            .iter()
            .any(|l| l.contains("pm.expect(pm.response.json().data.ok).to.eql(true)")));
        assert_eq!(doc["item"][1]["name"], "Read the profile");
        assert_eq!(
            doc["item"].as_array().unwrap().len(),
            2,
            "the step that is off is left out"
        );
        assert!(warnings.iter().any(|w| w.contains("`Off` is turned off")));
    }

    #[test]
    fn imports_back_to_the_same_requests() {
        let test = TestFile::from_yaml(FILE).unwrap();
        let (text, _) = to_json(&test);
        let back = crate::postman::from_json(&text).unwrap();
        let sent: Vec<_> = test
            .flow
            .steps
            .iter()
            .filter(|s| s.enabled)
            .map(|s| &s.request)
            .collect();
        let read: Vec<_> = back.test.flow.steps.iter().map(|s| &s.request).collect();
        assert_eq!(read.len(), sent.len());
        for (a, b) in sent.iter().zip(read) {
            assert_eq!(a.method, b.method);
            assert_eq!(a.url, b.url);
            for h in &a.headers {
                assert!(b.headers.contains(h), "{h:?} in {:?}", b.headers);
            }
        }
        assert_eq!(back.test.flow.steps[0].name, "Auth / Log in");
        assert_eq!(back.test.flow.variables, test.flow.variables);
    }

    #[test]
    fn expected_values_fill_in_variables() {
        assert_eq!(text_value("ok"), "\"ok\"");
        assert_eq!(
            text_value("{{user}}"),
            "pm.variables.replaceIn(\"{{user}}\")"
        );
        assert_eq!(json_value(&json!(3)), "3");
        assert_eq!(
            json_value(&json!({ "id": "{{id}}" })),
            r#"JSON.parse(pm.variables.replaceIn("{\"id\":\"{{id}}\"}"))"#
        );
    }

    #[test]
    fn environments_name_their_secrets() {
        let env = Environment {
            name: "staging".into(),
            variables: [("base_url".to_owned(), "https://s".to_owned())].into(),
            secrets: vec!["api_key".into()],
        };
        let doc: Value = serde_json::from_str(&environment_json(&env)).unwrap();
        assert_eq!(doc["_postman_variable_scope"], "environment");
        assert_eq!(doc["values"][1]["key"], "api_key");
        assert_eq!(doc["values"][1]["type"], "secret");
        assert_eq!(
            environment_file_name("staging"),
            "staging.postman_environment.json"
        );
    }
}
