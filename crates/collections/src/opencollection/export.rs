//! Writing a test file as an OpenCollection that Bruno opens.
//!
//! Steps named "Folder / Request" go into folders. Auth is already in the
//! steps' headers, so requests carry none of their own. Checks and
//! extractions become Bruno assertions and post-response variables where
//! Bruno has them, and lines of test script where it does not. What Bruno
//! cannot hold (steps that are off, response caching, load settings,
//! thresholds, tracing) is left out and reported in the warnings.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};
use stormix_flow::{Assertion, Body, ExtractSource, GraphqlBody, Header, Method, Step, TestFile};

use super::VERSION;
use crate::common::js_string;
use crate::environment::Environment;

/// A collection ready to write: each file's path inside the collection
/// folder and its YAML, and what could not be carried over.
#[derive(Debug, Clone, PartialEq)]
pub struct Exported {
    pub files: Vec<(PathBuf, String)>,
    pub warnings: Vec<String>,
}

/// The collection as a folder: `opencollection.yml`, one file per step,
/// `folder.yml` per folder and `environments/<name>.yml`.
pub fn to_files(test: &TestFile, environments: &[Environment]) -> Exported {
    let mut warnings = Vec::new();
    let tree = tree(test, &mut warnings);
    let mut files = vec![(
        PathBuf::from("opencollection.yml"),
        yaml(&root(test, false)),
    )];
    folder_files(&tree, Path::new(""), &mut files);
    for env in environments {
        files.push((
            PathBuf::from("environments").join(format!("{}.yml", file_name(&env.name))),
            yaml(&environment(env)),
        ));
    }
    Exported { files, warnings }
}

/// The collection as one bundled file, environments included.
pub fn to_bundle(test: &TestFile, environments: &[Environment]) -> (String, Vec<String>) {
    let mut warnings = Vec::new();
    let tree = tree(test, &mut warnings);
    let mut doc = root(test, true);
    let map = doc.as_object_mut().expect("root is an object");
    if !environments.is_empty() {
        map.insert(
            "config".into(),
            json!({ "environments": environments.iter().map(environment).collect::<Vec<_>>() }),
        );
    }
    // `items` reads best after the collection's own settings.
    let bundled = map.remove("bundled");
    map.insert("items".into(), Value::Array(bundle_items(&tree)));
    if let Some(b) = bundled {
        map.insert("bundled".into(), b);
    }
    (yaml(&doc), warnings)
}

/// Writes `exported` under `dir`, creating folders as needed. Files
/// already there with other names are left alone.
pub fn write_dir(dir: &Path, exported: &Exported) -> std::io::Result<()> {
    for (path, text) in &exported.files {
        let path = dir.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, text)?;
    }
    Ok(())
}

fn root(test: &TestFile, bundled: bool) -> Value {
    let flow = &test.flow;
    let mut doc = Map::new();
    doc.insert("opencollection".into(), json!(VERSION));
    doc.insert("info".into(), json!({ "name": flow.name }));
    if !flow.variables.is_empty() {
        let vars: Vec<Value> = flow
            .variables
            .iter()
            .map(|(k, v)| json!({ "name": k, "value": v }))
            .collect();
        doc.insert("request".into(), json!({ "variables": vars }));
    }
    if let Some(description) = flow.description.as_deref().filter(|d| !d.trim().is_empty()) {
        doc.insert(
            "docs".into(),
            json!({ "content": description, "type": "text/markdown" }),
        );
    }
    doc.insert("bundled".into(), json!(bundled));
    Value::Object(doc)
}

/// A folder of the collection: its requests and sub-folders, in run order.
#[derive(Default)]
struct Folder {
    name: String,
    entries: Vec<Entry>,
}

enum Entry {
    Request(Value),
    Folder(Folder),
}

fn tree(test: &TestFile, warnings: &mut Vec<String>) -> Folder {
    let flow = &test.flow;
    let mut top = Folder::default();
    let mut placed: Vec<Vec<String>> = Vec::new();
    for step in &flow.steps {
        if !step.enabled {
            warnings.push(format!(
                "`{}` is turned off; Bruno has no such switch, so it is left out",
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
        let folder = folder_at(&mut top, &parts);
        let seq = folder.entries.len() + 1;
        folder
            .entries
            .push(Entry::Request(request(step, &name, seq, warnings)));
        parts.push(name);
        placed.push(parts);
    }
    if !flow.set_aside.is_empty() {
        warnings.push(format!(
            "{} set-aside step(s) are left out: Bruno has nowhere to keep them",
            flow.set_aside.len()
        ));
    }
    if test.load.is_some() || !test.thresholds.is_empty() {
        warnings.push(
            "load settings and thresholds are Stormix's own; Bruno has no place for them".into(),
        );
    }
    if flow.trace.is_some() {
        warnings.push("`trace` is Stormix's own; Bruno sends no traceparent header".into());
    }

    // Bruno runs a folder's own requests before its sub-folders.
    let mut order = Vec::new();
    run_order(&top, &mut Vec::new(), &mut order);
    if order != placed {
        warnings.push(
            "Bruno runs a folder's own requests before its sub-folders, so the steps run in a different order there".into(),
        );
    }
    top
}

fn folder_at<'a>(mut folder: &'a mut Folder, path: &[String]) -> &'a mut Folder {
    for name in path {
        let at = folder
            .entries
            .iter()
            .position(|e| matches!(e, Entry::Folder(f) if &f.name == name));
        let at = at.unwrap_or_else(|| {
            folder.entries.push(Entry::Folder(Folder {
                name: name.clone(),
                entries: Vec::new(),
            }));
            folder.entries.len() - 1
        });
        let Entry::Folder(inner) = &mut folder.entries[at] else {
            unreachable!()
        };
        folder = inner;
    }
    folder
}

fn run_order(folder: &Folder, path: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    for entry in &folder.entries {
        if let Entry::Request(r) = entry {
            let mut p = path.clone();
            p.push(r["info"]["name"].as_str().unwrap_or_default().to_owned());
            out.push(p);
        }
    }
    for entry in &folder.entries {
        if let Entry::Folder(f) = entry {
            path.push(f.name.clone());
            run_order(f, path, out);
            path.pop();
        }
    }
}

fn folder_files(folder: &Folder, dir: &Path, files: &mut Vec<(PathBuf, String)>) {
    let mut used: Vec<String> = Vec::new();
    let mut unique = |name: &str| {
        let base = file_name(name);
        let mut candidate = base.clone();
        let mut n = 2;
        while used.contains(&candidate.to_lowercase()) {
            candidate = format!("{base}-{n}");
            n += 1;
        }
        used.push(candidate.to_lowercase());
        candidate
    };
    let mut folder_seq = 0;
    for entry in &folder.entries {
        match entry {
            Entry::Request(r) => {
                let name = unique(r["info"]["name"].as_str().unwrap_or("request"));
                files.push((dir.join(format!("{name}.yml")), yaml(r)));
            }
            Entry::Folder(f) => {
                folder_seq += 1;
                let sub = dir.join(unique(&f.name));
                files.push((
                    sub.join("folder.yml"),
                    yaml(
                        &json!({ "info": { "name": f.name, "type": "folder", "seq": folder_seq } }),
                    ),
                ));
                folder_files(f, &sub, files);
            }
        }
    }
}

fn bundle_items(folder: &Folder) -> Vec<Value> {
    let mut folder_seq = 0;
    folder
        .entries
        .iter()
        .map(|entry| match entry {
            Entry::Request(r) => r.clone(),
            Entry::Folder(f) => {
                folder_seq += 1;
                json!({
                    "info": { "name": f.name, "type": "folder", "seq": folder_seq },
                    "items": bundle_items(f),
                })
            }
        })
        .collect()
}

fn request(step: &Step, name: &str, seq: usize, warnings: &mut Vec<String>) -> Value {
    let r = &step.request;
    let graphql = matches!(r.body, Some(Body::Graphql(_)));
    let mut details = Map::new();
    details.insert("method".into(), json!(method(r.method)));
    details.insert("url".into(), json!(r.url));
    if !r.headers.is_empty() {
        details.insert("headers".into(), headers(&r.headers));
    }
    match &r.body {
        None => {}
        Some(Body::Graphql(g)) => {
            details.insert("body".into(), graphql_body(g, &step.name, warnings));
        }
        Some(body) => {
            details.insert("body".into(), http_body(body, &r.headers));
        }
    }

    let mut runtime = Map::new();
    let mut actions = Vec::new();
    let mut after = Vec::new();
    for (var, source) in &step.extract {
        match source {
            // A JSONPath Bruno cannot follow is reported below.
            ExtractSource::Json(path) => {
                if let Some(expr) = json_path_expr(path) {
                    actions.push(set_variable(var, &expr));
                }
            }
            ExtractSource::Header(header) => {
                actions.push(set_variable(var, &header_expr(header)));
            }
            ExtractSource::Regex(re) => after.push(format!(
                "bru.setVar({}, (function () {{\n  const body = res.getBody();\n  const m = (typeof body === \"string\" ? body : JSON.stringify(body)).match(new RegExp({}));\n  return m ? (m[1] !== undefined ? m[1] : m[0]) : undefined;\n}})());",
                js_string(var),
                js_string(re)
            )),
        }
    }
    let unmapped: Vec<String> = step
        .extract
        .iter()
        .filter_map(|(var, s)| match s {
            ExtractSource::Json(p) if json_path_expr(p).is_none() => Some(format!("`{var}` ({p})")),
            _ => None,
        })
        .collect();
    if !unmapped.is_empty() {
        warnings.push(format!(
            "`{}`: Bruno cannot read these JSONPaths, so they are not set: {}",
            step.name,
            unmapped.join(", ")
        ));
    }

    let mut assertions = Vec::new();
    let mut tests = Vec::new();
    for check in &step.assertions {
        match check {
            Assertion::Status(code) => assertions.push(assert("res.status", "eq", Some(code.to_string()))),
            Assertion::MaxDurationMs(ms) => {
                assertions.push(assert("res.responseTime", "lte", Some(ms.to_string())))
            }
            Assertion::Header(h) => {
                let expr = header_expr(&h.name);
                match (&h.equals, &h.contains) {
                    (Some(v), _) => assertions.push(assert(&expr, "eq", Some(literal(v)))),
                    (None, Some(v)) => assertions.push(assert(&expr, "contains", Some(literal(v)))),
                    (None, None) => assertions.push(assert(&expr, "isDefined", None)),
                }
                if let (Some(_), Some(v)) = (&h.equals, &h.contains) {
                    assertions.push(assert(&expr, "contains", Some(literal(v))));
                }
            }
            Assertion::Json(j) => match json_path_expr(&j.path) {
                Some(expr) => match (&j.equals, j.exists) {
                    (Some(Value::String(s)), _) => assertions.push(assert(&expr, "eq", Some(literal(s)))),
                    (Some(v), _) => assertions.push(assert(&expr, "eq", Some(v.to_string()))),
                    (None, Some(false)) => assertions.push(assert(&expr, "isUndefined", None)),
                    (None, _) => assertions.push(assert(&expr, "isDefined", None)),
                },
                None => warnings.push(format!(
                    "`{}`: the check on {} is left out; Bruno cannot read that JSONPath",
                    step.name, j.path
                )),
            },
            Assertion::BodyContains(text) => tests.push(format!(
                "test({}, function () {{\n  const body = res.getBody();\n  expect(typeof body === \"string\" ? body : JSON.stringify(body)).to.include({});\n}});",
                js_string(&format!("body contains {text}")),
                js_string(text)
            )),
        }
    }

    let mut scripts = Vec::new();
    if let Some(code) = step.scripts.pre_request.as_deref() {
        scripts.push(json!({ "type": "before-request", "code": with_helpers(code) }));
    }
    if !after.is_empty() {
        scripts.push(json!({ "type": "after-response", "code": after.join("\n") }));
    }
    let own_tests = step.scripts.test.as_deref().map(with_helpers);
    let tests: Vec<String> = tests.into_iter().chain(own_tests).collect();
    if !tests.is_empty() {
        scripts.push(json!({ "type": "tests", "code": tests.join("\n") }));
    }
    let uses_pm = [&step.scripts.pre_request, &step.scripts.test]
        .iter()
        .any(|s| s.as_deref().is_some_and(|s| s.contains("pm.")));
    if uses_pm {
        warnings.push(format!(
            "`{}`: its scripts use Postman's pm.* API, which Bruno only partly runs",
            step.name
        ));
    }
    if step.cache {
        warnings.push(format!(
            "`{}`: response caching is Stormix's own; Bruno sends the request every time",
            step.name
        ));
    }

    if !scripts.is_empty() {
        runtime.insert("scripts".into(), Value::Array(scripts));
    }
    if !assertions.is_empty() {
        runtime.insert("assertions".into(), Value::Array(assertions));
    }
    if !actions.is_empty() {
        runtime.insert("actions".into(), Value::Array(actions));
    }

    let kind = if graphql { "graphql" } else { "http" };
    let mut doc = Map::new();
    doc.insert(
        "info".into(),
        json!({ "name": name, "type": kind, "seq": seq }),
    );
    doc.insert(kind.into(), Value::Object(details));
    if !runtime.is_empty() {
        doc.insert("runtime".into(), Value::Object(runtime));
    }
    Value::Object(doc)
}

/// Imported Bruno assertions call Stormix's `__bruAssert`, which Bruno does
/// not have: a script that uses it gets its own copy first. In Stormix the
/// built-in one is used.
const BRU_ASSERT: &str = r#"// Stormix's helper for the assertions imported into this script.
const __bruAssert = globalThis.__bruAssert || (function () {
  const value = (raw) => {
    const t = String(raw).trim();
    if (t === "") return undefined;
    if (/^(true|false|null)$/.test(t)) return JSON.parse(t);
    if (/^-?\d+(\.\d+)?$/.test(t)) return Number(t);
    if (/^(['"`]).*\1$/.test(t)) return t.slice(1, -1);
    if (/^[\[{]/.test(t)) {
      try { return JSON.parse(t); } catch (e) {}
    }
    return t;
  };
  const list = (arg) => (Array.isArray(arg) ? arg : String(arg).split(",").map(value));
  return function (actual, op, raw) {
    const arg = value(raw);
    const e = expect(actual);
    switch (op) {
      case "eq": return e.to.eql(arg);
      case "neq": return e.not.to.eql(arg);
      case "gt": return e.to.be.above(arg);
      case "gte": return e.to.be.at.least(arg);
      case "lt": return e.to.be.below(arg);
      case "lte": return e.to.be.at.most(arg);
      case "in": return e.to.be.oneOf(list(arg));
      case "notIn": return e.not.to.be.oneOf(list(arg));
      case "contains": return e.to.include(arg);
      case "notContains": return e.not.to.include(arg);
      case "length": return e.to.have.lengthOf(arg);
      case "matches": return e.to.match(new RegExp(arg));
      case "notMatches": return e.not.to.match(new RegExp(arg));
      case "startsWith": return e.to.satisfy((v) => String(v).startsWith(arg));
      case "endsWith": return e.to.satisfy((v) => String(v).endsWith(arg));
      case "between": { const [lo, hi] = list(arg); return e.to.be.within(lo, hi); }
      case "isEmpty": return e.to.be.empty;
      case "isNotEmpty": return e.not.to.be.empty;
      case "isNull": return e.to.be.null;
      case "isUndefined": return e.to.be.undefined;
      case "isDefined": return e.not.to.be.undefined;
      case "isTruthy": return e.to.be.ok;
      case "isFalsy": return e.not.to.be.ok;
      case "isJson": return e.to.satisfy((v) => v !== null && typeof v === "object");
      case "isNumber": return e.to.be.a("number");
      case "isString": return e.to.be.a("string");
      case "isBoolean": return e.to.be.a("boolean");
      case "isArray": return e.to.be.an("array");
      default: throw new Error("unknown assert operator " + op);
    }
  };
})();
"#;

fn with_helpers(code: &str) -> String {
    if code.contains("__bruAssert(") {
        format!("{BRU_ASSERT}{code}")
    } else {
        code.to_owned()
    }
}

fn headers(headers: &[Header]) -> Value {
    headers
        .iter()
        .map(|h| json!({ "name": h.name, "value": h.value }))
        .collect()
}

fn http_body(body: &Body, headers: &[Header]) -> Value {
    match body {
        Body::Json(value) => json!({
            "type": "json",
            "data": serde_json::to_string_pretty(value).unwrap_or_default(),
        }),
        Body::Raw(text) => {
            let content_type = headers
                .iter()
                .find(|h| h.name.eq_ignore_ascii_case("content-type"))
                .map(|h| h.value.to_ascii_lowercase())
                .unwrap_or_default();
            let kind = if content_type.contains("json") {
                "json"
            } else if content_type.contains("xml") {
                "xml"
            } else if content_type.contains("sparql") {
                "sparql"
            } else {
                "text"
            };
            json!({ "type": kind, "data": text })
        }
        Body::Form(fields) => json!({
            "type": "form-urlencoded",
            "data": fields.iter().map(|f| json!({ "name": f.key, "value": f.value })).collect::<Vec<_>>(),
        }),
        Body::Multipart(fields) => json!({
            "type": "multipart-form",
            "data": fields
                .iter()
                .map(|f| json!({ "name": f.key, "type": "text", "value": f.value }))
                .collect::<Vec<_>>(),
        }),
        Body::Graphql(_) => unreachable!("GraphQL steps are written as graphql requests"),
    }
}

fn graphql_body(g: &GraphqlBody, step: &str, warnings: &mut Vec<String>) -> Value {
    if g.operation_name.is_some() {
        warnings.push(format!(
            "`{step}`: Bruno has no operation name; it runs the query's only (or first) operation"
        ));
    }
    if g.allow_errors {
        warnings.push(format!(
            "`{step}`: `allow_errors` is Stormix's own; Bruno never fails a step on GraphQL errors"
        ));
    }
    let mut body = Map::new();
    body.insert("query".into(), json!(g.query));
    match &g.variables {
        None => {}
        Some(Value::String(text)) => {
            body.insert("variables".into(), json!(text));
        }
        Some(v) => {
            body.insert(
                "variables".into(),
                json!(serde_json::to_string_pretty(v).unwrap_or_default()),
            );
        }
    }
    Value::Object(body)
}

/// An environment as an OpenCollection document. Secrets are named
/// without a value.
pub(crate) fn environment(env: &Environment) -> Value {
    let mut vars: Vec<Value> = env
        .variables
        .iter()
        .filter(|(k, _)| !env.secrets.contains(k))
        .map(|(k, v)| json!({ "name": k, "value": v }))
        .collect();
    vars.extend(
        env.secrets
            .iter()
            .map(|s| json!({ "name": s, "secret": true })),
    );
    json!({ "name": env.name, "variables": vars })
}

fn set_variable(name: &str, expr: &str) -> Value {
    json!({
        "type": "set-variable",
        "phase": "after-response",
        "selector": { "expression": expr, "method": "jsonq" },
        "variable": { "name": name, "scope": "runtime" },
    })
}

fn assert(expr: &str, op: &str, value: Option<String>) -> Value {
    let mut a = Map::new();
    a.insert("expression".into(), json!(expr));
    a.insert("operator".into(), json!(op));
    if let Some(v) = value {
        a.insert("value".into(), json!(v));
    }
    Value::Object(a)
}

/// Bruno reads an assertion value as a number, boolean or JSON when it
/// looks like one, so text that does is quoted to stay text.
fn literal(text: &str) -> String {
    let t = text.trim();
    let ambiguous = t.is_empty()
        || t != text
        || matches!(t, "true" | "false" | "null")
        || t.parse::<f64>().is_ok()
        || t.starts_with(['"', '\'', '`', '[', '{']);
    if ambiguous {
        js_string(text)
    } else {
        text.to_owned()
    }
}

/// `res.headers["name"]`: Bruno's header names are lower case.
fn header_expr(name: &str) -> String {
    format!("res.headers[{}]", js_string(&name.to_ascii_lowercase()))
}

/// A JSONPath as a JavaScript expression over `res.body`, for the plain
/// paths Bruno can follow: `$.a.b[0]`, `$['a b']`. Wildcards, filters and
/// recursive descent have no equivalent.
fn json_path_expr(path: &str) -> Option<String> {
    let rest = path.trim().strip_prefix('$')?;
    let mut out = String::from("res.body");
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '.' => {
                let mut key = String::new();
                while let Some(&n) = chars.peek() {
                    if n == '.' || n == '[' {
                        break;
                    }
                    key.push(n);
                    chars.next();
                }
                if key.is_empty() || key == "*" {
                    return None;
                }
                if key
                    .chars()
                    .all(|k| k.is_ascii_alphanumeric() || k == '_' || k == '$')
                    && !key.starts_with(|k: char| k.is_ascii_digit())
                {
                    out.push('.');
                    out.push_str(&key);
                } else {
                    out.push_str(&format!("[{}]", js_string(&key)));
                }
            }
            '[' => {
                let mut inner = String::new();
                for n in chars.by_ref() {
                    if n == ']' {
                        break;
                    }
                    inner.push(n);
                }
                let inner = inner.trim();
                if let Ok(i) = inner.parse::<i64>() {
                    if i < 0 {
                        return None;
                    }
                    out.push_str(&format!("[{i}]"));
                } else {
                    let key = inner
                        .strip_prefix('\'')
                        .and_then(|k| k.strip_suffix('\''))
                        .or_else(|| inner.strip_prefix('"').and_then(|k| k.strip_suffix('"')))?;
                    out.push_str(&format!("[{}]", js_string(key)));
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

fn method(m: Method) -> &'static str {
    match m {
        Method::Get => "GET",
        Method::Post => "POST",
        Method::Put => "PUT",
        Method::Patch => "PATCH",
        Method::Delete => "DELETE",
        Method::Head => "HEAD",
        Method::Options => "OPTIONS",
    }
}

/// A file or folder name for `name`: lower case, words joined by `-`.
fn file_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_owned();
    if out.is_empty() {
        "request".into()
    } else {
        out
    }
}

fn yaml(value: &Value) -> String {
    serde_yaml_ng::to_string(value).expect("JSON values always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_paths_become_expressions() {
        assert_eq!(json_path_expr("$").as_deref(), Some("res.body"));
        assert_eq!(
            json_path_expr("$.data.token").as_deref(),
            Some("res.body.data.token")
        );
        assert_eq!(
            json_path_expr("$.items[0].id").as_deref(),
            Some("res.body.items[0].id")
        );
        assert_eq!(
            json_path_expr("$['a b'].c-d").as_deref(),
            Some(r#"res.body["a b"]["c-d"]"#)
        );
        assert_eq!(json_path_expr("$..id"), None);
        assert_eq!(json_path_expr("$.items[*].id"), None);
        assert_eq!(json_path_expr("$.items[?(@.id)]"), None);
    }

    #[test]
    fn literals_that_look_like_values_are_quoted() {
        assert_eq!(literal("ok"), "ok");
        assert_eq!(literal("200"), "\"200\"");
        assert_eq!(literal("true"), "\"true\"");
        assert_eq!(literal("{x}"), "\"{x}\"");
    }

    #[test]
    fn the_assert_helper_knows_every_operator() {
        for op in crate::bruno::OPERATORS {
            assert!(BRU_ASSERT.contains(&format!("case \"{op}\"")), "{op}");
        }
        assert!(with_helpers("x()").eq("x()"));
        assert!(with_helpers("__bruAssert(1, \"eq\", \"1\")").starts_with("// Stormix's helper"));
    }

    #[test]
    fn names_make_tidy_files() {
        assert_eq!(file_name("Log in"), "log-in");
        assert_eq!(file_name("GET /users/:id"), "get-users-id");
        assert_eq!(file_name("***"), "request");
    }
}
