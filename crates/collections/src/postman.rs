//! Postman collection v2.0 and v2.1 exports.

use std::collections::BTreeMap;

use serde_json::Value;
use stormix_flow::{
    Body, Flow, Header, KeyValue, Request, Scripts, Step, TestFile, CURRENT_VERSION,
};

use crate::common::{self, Auth};
use crate::{ImportError, Imported};

/// Imports a collection from its exported JSON.
pub fn from_json(text: &str) -> Result<Imported, ImportError> {
    let root: Value =
        serde_json::from_str(text).map_err(|e| ImportError::Postman(e.to_string()))?;
    let info = root
        .get("info")
        .ok_or_else(|| ImportError::Postman("there is no `info` section".into()))?;
    if let Some(schema) = info.get("schema").and_then(Value::as_str) {
        if !schema.contains("/v2.") {
            return Err(ImportError::Postman(format!(
                "the schema is {schema}; export it again as Collection v2.1"
            )));
        }
    }
    let name = info
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Postman collection")
        .to_owned();

    let mut importer = Importer::default();
    let scope = Scope {
        path: Vec::new(),
        auth: auth(root.get("auth")).unwrap_or(Auth::None),
        pre_request: events(&root, "prerequest").into_iter().collect(),
        test: events(&root, "test").into_iter().collect(),
    };
    for item in array(root.get("item")) {
        importer.item(item, &scope);
    }
    if importer.steps.is_empty() {
        importer
            .warnings
            .push("the collection has no requests".into());
    }

    let variables = array(root.get("variable"))
        .iter()
        .filter(|v| !disabled(v))
        .filter_map(|v| Some((v.get("key")?.as_str()?.to_owned(), text_of(v.get("value")))))
        .collect();
    Ok(Imported {
        test: TestFile {
            version: CURRENT_VERSION,
            flow: Flow {
                name,
                description: description(info.get("description")),
                variables,
                steps: importer.steps,
                set_aside: Vec::new(),
                trace: None,
            },
            load: None,
            thresholds: Vec::new(),
        },
        environments: BTreeMap::new(),
        secrets: BTreeMap::new(),
        warnings: importer.warnings,
    })
}

/// What a folder passes down to the items in it.
#[derive(Clone)]
struct Scope {
    path: Vec<String>,
    auth: Auth,
    pre_request: Vec<String>,
    test: Vec<String>,
}

#[derive(Default)]
struct Importer {
    steps: Vec<Step>,
    warnings: Vec<String>,
}

impl Importer {
    fn item(&mut self, item: &Value, scope: &Scope) {
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Untitled")
            .to_owned();
        if let Some(children) = item.get("item") {
            let mut inner = scope.clone();
            inner.path.push(name);
            if let Some(auth) = auth(item.get("auth")) {
                inner.auth = auth;
            }
            inner.pre_request.extend(events(item, "prerequest"));
            inner.test.extend(events(item, "test"));
            for child in array(Some(children)) {
                self.item(child, &inner);
            }
            return;
        }
        let Some(request) = item.get("request") else {
            self.warnings
                .push(format!("`{name}` has no request; skipped"));
            return;
        };

        let step_name = scope
            .path
            .iter()
            .chain([&name])
            .cloned()
            .collect::<Vec<_>>()
            .join(" / ");
        if let Some(mut built) = self.request(request, &step_name) {
            let mut pre_request = scope.pre_request.clone();
            pre_request.extend(events(item, "prerequest"));
            let mut test = scope.test.clone();
            test.extend(events(item, "test"));
            let auth = auth(request.get("auth")).unwrap_or_else(|| scope.auth.clone());
            common::apply_auth(
                &auth,
                &mut built,
                &mut pre_request,
                &step_name,
                &mut self.warnings,
            );
            self.steps.push(Step {
                name: step_name,
                enabled: true,
                cache: false,
                request: built,
                extract: BTreeMap::new(),
                assertions: Vec::new(),
                scripts: Scripts {
                    pre_request: common::join_scripts(pre_request),
                    test: common::join_scripts(test),
                },
            });
        }
    }

    fn request(&mut self, request: &Value, step: &str) -> Option<Request> {
        // A request can be saved as just its URL.
        if let Some(url) = request.as_str() {
            return Some(Request {
                method: stormix_flow::Method::Get,
                url: url.to_owned(),
                headers: Vec::new(),
                body: None,
            });
        }
        let method_name = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("GET");
        let Some(method) = common::method(method_name) else {
            self.warnings.push(format!(
                "`{step}`: the {method_name} method is not supported; skipped"
            ));
            return None;
        };
        let mut built = Request {
            method,
            url: url(request.get("url")),
            headers: array(request.get("header"))
                .iter()
                .filter(|h| !disabled(h))
                .filter_map(|h| {
                    Some(Header {
                        name: h.get("key")?.as_str()?.to_owned(),
                        value: text_of(h.get("value")),
                    })
                })
                .collect(),
            body: None,
        };
        if let Some(body) = request.get("body") {
            self.body(body, &mut built, step);
        }
        Some(built)
    }

    fn body(&mut self, body: &Value, request: &mut Request, step: &str) {
        if disabled(body) {
            return;
        }
        match body.get("mode").and_then(Value::as_str).unwrap_or("none") {
            "raw" => {
                let text = text_of(body.get("raw"));
                if text.is_empty() {
                    return;
                }
                let language = body
                    .pointer("/options/raw/language")
                    .and_then(Value::as_str)
                    .unwrap_or("text");
                if let Some(content_type) = common::raw_content_type(language) {
                    common::default_header(request, "Content-Type", content_type);
                }
                request.body = Some(Body::Raw(text));
            }
            "urlencoded" => request.body = Some(Body::Form(fields(body.get("urlencoded")))),
            "formdata" => {
                let all = array(body.get("formdata"));
                if all
                    .iter()
                    .any(|f| !disabled(f) && f.get("type").and_then(Value::as_str) == Some("file"))
                {
                    self.warnings.push(format!(
                        "`{step}`: file fields in form-data are not supported yet; they are left out"
                    ));
                }
                let text_fields = all
                    .iter()
                    .filter(|f| f.get("type").and_then(Value::as_str) != Some("file"))
                    .cloned()
                    .collect::<Vec<_>>();
                request.body = Some(Body::Multipart(fields(Some(&Value::Array(text_fields)))));
                // reqwest sets the boundary; a fixed Content-Type would lose it.
                request
                    .headers
                    .retain(|h| !h.name.eq_ignore_ascii_case("content-type"));
            }
            "graphql" => {
                let graphql = body.get("graphql").cloned().unwrap_or(Value::Null);
                let variables = match graphql.get("variables") {
                    Some(Value::String(text)) => text.clone(),
                    None | Some(Value::Null) => String::new(),
                    Some(other) => other.to_string(),
                };
                request.body = Some(common::graphql_body(
                    text_of(graphql.get("query")),
                    &variables,
                    step,
                    &mut self.warnings,
                ));
            }
            "file" => self.warnings.push(format!(
                "`{step}`: file bodies are not supported yet; the request is sent without a body"
            )),
            _ => {}
        }
    }
}

fn url(url: Option<&Value>) -> String {
    match url {
        Some(Value::String(s)) => s.clone(),
        Some(url @ Value::Object(_)) => {
            let raw = match url.get("raw").and_then(Value::as_str) {
                Some(raw) => raw.to_owned(),
                None => built_url(url),
            };
            let vars: Vec<(String, String)> = array(url.get("variable"))
                .iter()
                .filter_map(|v| Some((v.get("key")?.as_str()?.to_owned(), text_of(v.get("value")))))
                .collect();
            common::path_variables(&raw, &vars)
        }
        _ => String::new(),
    }
}

/// For exports that only have the parts of the URL, not `raw`.
fn built_url(url: &Value) -> String {
    let join = |v: Option<&Value>, sep: &str| match v {
        Some(Value::Array(parts)) => parts
            .iter()
            .map(|p| text_of(Some(p)))
            .collect::<Vec<_>>()
            .join(sep),
        other => text_of(other),
    };
    let mut out = String::new();
    if let Some(protocol) = url.get("protocol").and_then(Value::as_str) {
        out.push_str(protocol);
        out.push_str("://");
    }
    out.push_str(&join(url.get("host"), "."));
    if let Some(port) = url.get("port").and_then(Value::as_str) {
        out.push(':');
        out.push_str(port);
    }
    let path = join(url.get("path"), "/");
    if !path.is_empty() {
        out.push('/');
        out.push_str(&path);
    }
    let query: Vec<String> = array(url.get("query"))
        .iter()
        .filter(|q| !disabled(q))
        .filter_map(|q| {
            Some(format!(
                "{}={}",
                q.get("key")?.as_str()?,
                text_of(q.get("value"))
            ))
        })
        .collect();
    if !query.is_empty() {
        out.push('?');
        out.push_str(&query.join("&"));
    }
    out
}

/// `None` means inherit from the parent.
fn auth(auth: Option<&Value>) -> Option<Auth> {
    let auth = auth?;
    let kind = auth.get("type").and_then(Value::as_str)?;
    // v2.1 stores parameters as [{key, value}]; v2.0 as a plain object.
    let param = |name: &str| -> String {
        match auth.get(kind) {
            Some(Value::Array(params)) => params
                .iter()
                .find(|p| p.get("key").and_then(Value::as_str) == Some(name))
                .map(|p| text_of(p.get("value")))
                .unwrap_or_default(),
            Some(Value::Object(map)) => text_of(map.get(name)),
            _ => String::new(),
        }
    };
    Some(match kind {
        "inherit" => return None,
        "noauth" => Auth::None,
        "bearer" => Auth::Bearer(param("token")),
        "basic" => Auth::Basic {
            username: param("username"),
            password: param("password"),
        },
        "apikey" => Auth::ApiKey {
            key: match param("key") {
                k if k.is_empty() => "X-API-Key".into(),
                k => k,
            },
            value: param("value"),
            in_query: param("in") == "query",
        },
        other => Auth::Unsupported(other.to_owned()),
    })
}

/// The script for one `listen` event (`prerequest` or `test`), if any.
fn events(item: &Value, listen: &str) -> Option<String> {
    let scripts: Vec<String> = array(item.get("event"))
        .iter()
        .filter(|e| !disabled(e) && e.get("listen").and_then(Value::as_str) == Some(listen))
        .map(|e| match e.pointer("/script/exec") {
            Some(Value::Array(lines)) => lines
                .iter()
                .map(|l| text_of(Some(l)))
                .collect::<Vec<_>>()
                .join("\n"),
            other => text_of(other),
        })
        .filter(|s| !s.trim().is_empty())
        .collect();
    (!scripts.is_empty()).then(|| scripts.join("\n"))
}

fn fields(list: Option<&Value>) -> Vec<KeyValue> {
    array(list)
        .iter()
        .filter(|f| !disabled(f))
        .filter_map(|f| {
            Some(KeyValue {
                key: f.get("key")?.as_str()?.to_owned(),
                value: text_of(f.get("value")),
            })
        })
        .collect()
}

fn description(value: Option<&Value>) -> Option<String> {
    let text = match value? {
        Value::Object(map) => text_of(map.get("content")),
        other => text_of(Some(other)),
    };
    (!text.trim().is_empty()).then_some(text)
}

fn array(value: Option<&Value>) -> &[Value] {
    value.and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn disabled(value: &Value) -> bool {
    value.get("disabled").and_then(Value::as_bool) == Some(true)
}

/// Postman values are usually strings but may be numbers or booleans.
fn text_of(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}
