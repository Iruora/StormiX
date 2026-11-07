//! Reading an OpenCollection folder or bundled file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use stormix_flow::{
    Body, Flow, Header, KeyValue, Request, Scripts, Step, TestFile, CURRENT_VERSION,
};

use super::{FOLDER_FILES, ROOT_FILES};
use crate::bruno::{assertion, Progress};
use crate::common::{self, js_string, Auth};
use crate::{read, ImportError, Imported};

/// Whether `path` is named like a collection's root file.
pub fn is_root_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| ROOT_FILES.contains(&n))
}

/// Whether the YAML file at `path` is an OpenCollection document: it has a
/// top-level `opencollection:` key. Stormix test files never do.
pub fn looks_bundled(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|text| {
        text.lines()
            .any(|l| l.starts_with("opencollection:") || l.starts_with("\"opencollection\":"))
    })
}

/// Imports the collection at `path`: its folder, its `opencollection.yml`
/// or a bundled file.
pub fn from_path(path: &Path) -> Result<Imported, ImportError> {
    from_path_with_progress(path, &mut |_| {})
}

/// Like [`from_path`], reporting each request file a folder import reads.
/// A bundled file is read at once, so it reports none.
pub fn from_path_with_progress(
    path: &Path,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<Imported, ImportError> {
    if path.is_dir() {
        return from_dir(path, on_progress);
    }
    let root = parse_file(path)?;
    let bundled = root.get("bundled").and_then(Value::as_bool);
    if is_root_file(path) && bundled != Some(true) && root.get("items").is_none() {
        return from_dir(path.parent().unwrap_or(Path::new(".")), on_progress);
    }
    from_bundle(path, &root)
}

/// One entry of a collection, in the order it runs.
enum Node {
    Folder {
        name: String,
        /// The folder's `request` defaults: headers, auth, variables, scripts.
        defaults: Value,
        items: Vec<Node>,
    },
    Item {
        /// The request as written, with `info`, `http` and `runtime`.
        value: Value,
        /// The name to use when the item has none: its file name.
        fallback: String,
    },
}

/// What the importer gathers from either shape before turning it into steps.
struct Collection {
    name: String,
    root: Value,
    items: Vec<Node>,
    environments: Vec<RawEnvironment>,
    warnings: Vec<String>,
}

fn from_dir(dir: &Path, on_progress: &mut dyn FnMut(Progress)) -> Result<Imported, ImportError> {
    let root_file = ROOT_FILES
        .iter()
        .map(|n| dir.join(n))
        .find(|p| p.is_file())
        .ok_or_else(|| ImportError::NotOpenCollection(dir.to_owned()))?;
    let root = parse_file(&root_file)?;
    let ignore: Vec<String> = root
        .pointer("/extensions/bruno/ignore")
        .and_then(Value::as_array)
        .map(|l| {
            l.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();

    let mut reader = DirReader {
        ignore,
        total: 0,
        done: 0,
        on_progress,
    };
    reader.total = reader.count(dir, true)?;
    let items = reader.read(dir, true)?;

    let mut environments = Vec::new();
    let env_dir = dir.join("environments");
    if env_dir.is_dir() {
        for file in yaml_files(&env_dir)? {
            let value = parse_file(&file)?;
            environments.push(raw_environment(&value, stem(&file)));
        }
    }
    Ok(build(Collection {
        name: collection_name(&root, dir),
        root,
        items,
        environments,
        warnings: Vec::new(),
    }))
}

fn from_bundle(file: &Path, root: &Value) -> Result<Imported, ImportError> {
    if root.get("opencollection").is_none() && root.get("items").is_none() {
        return Err(ImportError::NotOpenCollection(file.to_owned()));
    }
    let items = bundle_items(root.get("items"));
    let environments = root
        .pointer("/config/environments")
        .and_then(Value::as_array)
        .map(|envs| {
            envs.iter()
                .enumerate()
                .map(|(i, e)| raw_environment(e, format!("environment {}", i + 1)))
                .collect()
        })
        .unwrap_or_default();
    Ok(build(Collection {
        name: collection_name(root, file.parent().unwrap_or(Path::new("."))),
        root: root.clone(),
        items,
        environments,
        warnings: Vec::new(),
    }))
}

fn collection_name(root: &Value, dir: &Path) -> String {
    text(root.pointer("/info/name")).unwrap_or_else(|| {
        dir.canonicalize()
            .ok()
            .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "OpenCollection".into())
    })
}

/// A bundled file's `items`, folders holding their own `items`.
fn bundle_items(items: Option<&Value>) -> Vec<Node> {
    let mut nodes: Vec<(f64, Node)> = Vec::new();
    for (i, item) in items
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let fallback = format!("Request {}", i + 1);
        let node = if item_type(item) == Some("folder") || item.get("items").is_some() {
            Node::Folder {
                name: text(item.pointer("/info/name"))
                    .unwrap_or_else(|| format!("Folder {}", i + 1)),
                defaults: item.get("request").cloned().unwrap_or(Value::Null),
                items: bundle_items(item.get("items")),
            }
        } else {
            Node::Item {
                value: item.clone(),
                fallback,
            }
        };
        nodes.push((seq(item), node));
    }
    order(nodes)
}

/// Requests in `seq` order, then folders the same way: the order the `.bru`
/// importer runs a Bruno folder in.
fn order(mut nodes: Vec<(f64, Node)>) -> Vec<Node> {
    // A stable sort keeps the file's order among equal (or missing) seqs.
    nodes.sort_by(|a, b| {
        let folder = |n: &Node| matches!(n, Node::Folder { .. });
        folder(&a.1).cmp(&folder(&b.1)).then(a.0.total_cmp(&b.0))
    });
    nodes.into_iter().map(|(_, n)| n).collect()
}

struct DirReader<'a> {
    ignore: Vec<String>,
    total: usize,
    done: usize,
    on_progress: &'a mut dyn FnMut(Progress),
}

impl DirReader<'_> {
    fn skipped(&self, name: &str, top: bool) -> bool {
        name.starts_with('.')
            || name == "node_modules"
            || (top && name == "environments")
            || self.ignore.iter().any(|i| i == name)
    }

    fn request_files(&self, dir: &Path) -> Result<Vec<PathBuf>, ImportError> {
        Ok(yaml_files(dir)?
            .into_iter()
            .filter(|f| {
                let name = f.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                !ROOT_FILES.contains(&name) && !FOLDER_FILES.contains(&name)
            })
            .collect())
    }

    fn sub_dirs(&self, dir: &Path, top: bool) -> Result<Vec<PathBuf>, ImportError> {
        Ok(entries(dir)?
            .into_iter()
            .filter(|p| {
                p.is_dir()
                    && !self.skipped(&p.file_name().unwrap_or_default().to_string_lossy(), top)
            })
            .collect())
    }

    fn count(&self, dir: &Path, top: bool) -> Result<usize, ImportError> {
        let mut n = self.request_files(dir)?.len();
        for sub in self.sub_dirs(dir, top)? {
            n += self.count(&sub, false)?;
        }
        Ok(n)
    }

    fn read(&mut self, dir: &Path, top: bool) -> Result<Vec<Node>, ImportError> {
        let mut nodes = Vec::new();
        for file in self.request_files(dir)? {
            let value = parse_file(&file)?;
            self.done += 1;
            (self.on_progress)(Progress {
                done: self.done,
                total: self.total.max(self.done),
                file: file.clone(),
            });
            nodes.push((
                seq(&value),
                Node::Item {
                    value,
                    fallback: stem(&file),
                },
            ));
        }
        for sub in self.sub_dirs(dir, top)? {
            let settings = FOLDER_FILES
                .iter()
                .map(|n| sub.join(n))
                .find(|p| p.is_file())
                .map(|p| parse_file(&p))
                .transpose()?
                .unwrap_or(Value::Null);
            let name = text(settings.pointer("/info/name")).unwrap_or_else(|| stem(&sub));
            let items = self.read(&sub, false)?;
            nodes.push((
                seq(&settings),
                Node::Folder {
                    name,
                    defaults: settings.get("request").cloned().unwrap_or(Value::Null),
                    items,
                },
            ));
        }
        Ok(order(nodes))
    }
}

/// What a collection or folder passes down to the requests in it.
#[derive(Clone, Default)]
struct Scope {
    path: Vec<String>,
    headers: Vec<Header>,
    auth: Auth,
    vars: Vec<String>,
    pre_request: Vec<String>,
    post_response: Vec<String>,
    tests: Vec<String>,
}

impl Scope {
    /// Takes in a collection's or folder's `request` defaults. Collection
    /// variables become the flow's, so only a folder's are set by script.
    fn enter(&mut self, defaults: &Value, folder: bool) {
        for header in headers(defaults.get("headers")) {
            self.headers
                .retain(|h| !h.name.eq_ignore_ascii_case(&header.name));
            self.headers.push(header);
        }
        if let Some(Some(auth)) = defaults.get("auth").map(auth) {
            self.auth = auth;
        }
        if folder {
            self.vars.extend(var_lines(defaults.get("variables")));
        }
        self.post_response
            .extend(action_lines(defaults.get("actions")));
        let scripts = scripts(defaults.get("scripts"));
        self.pre_request.extend(scripts.before);
        self.post_response.extend(scripts.after);
        self.tests.extend(scripts.tests);
    }
}

fn build(collection: Collection) -> Imported {
    let Collection {
        name,
        root,
        items,
        environments,
        mut warnings,
    } = collection;
    let defaults = root.get("request").cloned().unwrap_or(Value::Null);
    let variables: BTreeMap<String, String> = enabled(defaults.get("variables"))
        .filter_map(|v| Some((text(v.get("name"))?, variable_value(v.get("value")))))
        .collect();
    let mut scope = Scope::default();
    scope.enter(&defaults, false);

    let mut steps = Vec::new();
    walk(&items, &scope, &mut steps, &mut warnings);
    if steps.is_empty() {
        warnings.push("the collection has no requests".into());
    }

    let (environments, secrets) = resolve_environments(environments, &mut warnings);
    Imported {
        test: TestFile {
            version: CURRENT_VERSION,
            flow: Flow {
                name,
                description: docs(root.get("docs")),
                variables,
                steps,
                set_aside: Vec::new(),
                trace: None,
            },
            load: None,
            thresholds: Vec::new(),
            monitor: Vec::new(),
        },
        environments,
        secrets,
        warnings,
    }
}

fn walk(nodes: &[Node], scope: &Scope, steps: &mut Vec<Step>, warnings: &mut Vec<String>) {
    for node in nodes {
        match node {
            Node::Folder {
                name,
                defaults,
                items,
            } => {
                let mut inner = scope.clone();
                inner.enter(defaults, true);
                inner.path.push(name.clone());
                walk(items, &inner, steps, warnings);
            }
            Node::Item { value, fallback } => {
                if let Some(step) = request(value, fallback, scope, warnings) {
                    steps.push(step);
                }
            }
        }
    }
}

fn request(
    item: &Value,
    fallback: &str,
    scope: &Scope,
    warnings: &mut Vec<String>,
) -> Option<Step> {
    let own = text(item.pointer("/info/name")).unwrap_or_else(|| fallback.to_owned());
    let step = scope
        .path
        .iter()
        .chain([&own])
        .cloned()
        .collect::<Vec<_>>()
        .join(" / ");
    let kind = item_type(item).unwrap_or(if item.get("graphql").is_some() {
        "graphql"
    } else {
        "http"
    });
    let details = match kind {
        "http" | "graphql" => item.get(kind).unwrap_or(&Value::Null),
        "script" => {
            warnings.push(format!(
                "`{step}`: shared script files are not supported; skipped"
            ));
            return None;
        }
        other => {
            warnings.push(format!(
                "`{step}`: {other} requests are not supported; skipped"
            ));
            return None;
        }
    };
    let method_name = text(details.get("method"))
        .unwrap_or_else(|| if kind == "graphql" { "POST" } else { "GET" }.into());
    let Some(method) = common::method(&method_name) else {
        warnings.push(format!(
            "`{step}`: the {method_name} method is not supported; skipped"
        ));
        return None;
    };

    let mut url = text(details.get("url")).unwrap_or_default();
    let params: Vec<&Value> = enabled(details.get("params")).collect();
    let path_vars: Vec<(String, String)> = params
        .iter()
        .filter(|p| p.get("type").and_then(Value::as_str) == Some("path"))
        .filter_map(|p| {
            Some((
                text(p.get("name"))?,
                text(p.get("value")).unwrap_or_default(),
            ))
        })
        .collect();
    url = common::path_variables(&url, &path_vars);
    // Bruno keeps query parameters in the URL too; add only those it lacks.
    for p in params
        .iter()
        .filter(|p| p.get("type").and_then(Value::as_str) == Some("query"))
    {
        let Some(name) = text(p.get("name")) else {
            continue;
        };
        let query = url.split_once('?').map(|(_, q)| q).unwrap_or_default();
        if query
            .split('&')
            .any(|pair| pair.split('=').next() == Some(name.as_str()))
        {
            continue;
        }
        let sep = if url.contains('?') { '&' } else { '?' };
        url = format!(
            "{url}{sep}{name}={}",
            text(p.get("value")).unwrap_or_default()
        );
    }

    let mut request = Request {
        method,
        url,
        headers: headers(details.get("headers")),
        body: None,
    };
    common::inherit_headers(&mut request, &scope.headers);
    if kind == "graphql" {
        let body = selected(details.get("body"), "body");
        let query = text(body.and_then(|b| b.get("query"))).unwrap_or_default();
        let variables = match body.and_then(|b| b.get("variables")) {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Null) | None => String::new(),
            Some(other) => serde_json::to_string_pretty(other).unwrap_or_default(),
        };
        request.body = Some(common::graphql_body(query, &variables, &step, warnings));
    } else if let Some(body) = selected(details.get("body"), "body") {
        http_body(body, &mut request, &step, warnings);
    }

    let runtime = item.get("runtime").unwrap_or(&Value::Null);
    let own_scripts = scripts(runtime.get("scripts"));

    let mut pre_request = scope.vars.clone();
    pre_request.extend(var_lines(runtime.get("variables")));
    pre_request.extend(scope.pre_request.iter().cloned());
    pre_request.extend(own_scripts.before);
    // Auth sits in the request's details; Bruno's first previews kept it
    // under `runtime`.
    let auth = match details.get("auth").or_else(|| runtime.get("auth")) {
        None => Some(Auth::None),
        Some(value) => self::auth(value),
    }
    .unwrap_or_else(|| scope.auth.clone());
    common::apply_auth(
        common::Dialect::Bruno,
        &auth,
        &mut request,
        &mut pre_request,
        &step,
        warnings,
    );

    // Bruno's order after the response: variables, scripts, assertions, tests.
    let mut test = Vec::new();
    test.extend(action_lines(runtime.get("actions")));
    test.extend(scope.post_response.iter().cloned());
    test.extend(own_scripts.after);
    let asserts: Vec<String> = enabled(runtime.get("assertions"))
        .filter_map(|a| {
            let expr = text(a.get("expression"))?;
            let op = text(a.get("operator")).unwrap_or_else(|| "eq".into());
            let rule = match a.get("value").filter(|v| !v.is_null()) {
                Some(v) => format!("{op} {}", variable_value(Some(v))),
                None => op,
            };
            Some(assertion(&expr, &rule))
        })
        .collect();
    if !asserts.is_empty() {
        test.push(asserts.join("\n"));
    }
    test.extend(scope.tests.iter().cloned());
    test.extend(own_scripts.tests);

    Some(Step {
        name: step,
        enabled: true,
        cache: false,
        request,
        extract: BTreeMap::new(),
        assertions: Vec::new(),
        scripts: Scripts {
            pre_request: common::join_scripts(pre_request),
            test: common::join_scripts(test),
        },
    })
}

fn http_body(body: &Value, request: &mut Request, step: &str, warnings: &mut Vec<String>) {
    let kind = text(body.get("type")).unwrap_or_default();
    let data = body.get("data");
    match kind.as_str() {
        "json" | "text" | "xml" | "sparql" => {
            if let Some(content_type) = common::raw_content_type(&kind) {
                common::default_header(request, "Content-Type", content_type);
            } else if kind == "sparql" {
                common::default_header(request, "Content-Type", "application/sparql-query");
            }
            request.body = Some(Body::Raw(variable_value(data)));
        }
        "form-urlencoded" => {
            request.body = Some(Body::Form(form_fields(data)));
        }
        "multipart-form" => {
            let (files, fields): (Vec<&Value>, Vec<&Value>) =
                enabled(data).partition(|f| f.get("type").and_then(Value::as_str) == Some("file"));
            if !files.is_empty() {
                warnings.push(format!(
                    "`{step}`: file fields in multipart forms are not supported yet; they are left out"
                ));
            }
            request.body = Some(Body::Multipart(
                fields
                    .into_iter()
                    .filter_map(|f| {
                        Some(KeyValue {
                            key: text(f.get("name"))?,
                            value: variable_value(f.get("value")),
                        })
                    })
                    .collect(),
            ));
            request
                .headers
                .retain(|h| !h.name.eq_ignore_ascii_case("content-type"));
        }
        "graphql" => {
            let query = text(
                body.get("query")
                    .or_else(|| data.and_then(|d| d.get("query"))),
            );
            let variables = text(
                body.get("variables")
                    .or_else(|| data.and_then(|d| d.get("variables"))),
            );
            request.body = Some(common::graphql_body(
                query.unwrap_or_default(),
                &variables.unwrap_or_default(),
                step,
                warnings,
            ));
        }
        "none" | "" => {}
        other => warnings.push(format!(
            "`{step}`: {other} bodies are not supported yet; the request is sent without a body"
        )),
    }
}

fn form_fields(data: Option<&Value>) -> Vec<KeyValue> {
    enabled(data)
        .filter_map(|f| {
            Some(KeyValue {
                key: text(f.get("name"))?,
                value: variable_value(f.get("value")),
            })
        })
        .collect()
}

/// `inherit` is `None`; anything else is the auth to apply.
fn auth(value: &Value) -> Option<Auth> {
    let kind = match value {
        Value::String(s) => s.as_str(),
        Value::Object(_) => value.get("type").and_then(Value::as_str).unwrap_or("none"),
        _ => "none",
    };
    let get = |key: &str| text(value.get(key)).unwrap_or_default();
    Some(match kind {
        "inherit" => return None,
        "none" => Auth::None,
        "bearer" => Auth::Bearer(get("token")),
        "basic" => Auth::Basic {
            username: get("username"),
            password: get("password"),
        },
        "apikey" => Auth::ApiKey {
            key: get("key"),
            value: get("value"),
            in_query: matches!(get("placement").as_str(), "query" | "queryparams"),
        },
        other => Auth::Unsupported(other.to_owned()),
    })
}

fn headers(list: Option<&Value>) -> Vec<Header> {
    let mut out: Vec<Header> = Vec::new();
    for h in enabled(list) {
        let Some(name) = text(h.get("name")) else {
            continue;
        };
        out.retain(|o| !o.name.eq_ignore_ascii_case(&name));
        out.push(Header {
            name,
            value: variable_value(h.get("value")),
        });
    }
    out
}

/// Request and folder variables, set before the scripts run.
fn var_lines(list: Option<&Value>) -> Option<String> {
    let lines: Vec<String> = enabled(list)
        .filter_map(|v| {
            Some(format!(
                "bru.setVar({}, bru.interpolate({}));",
                js_string(&text(v.get("name"))?),
                js_string(&variable_value(v.get("value")))
            ))
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// `set-variable` actions run after the response, Bruno's post-response
/// variables: a JavaScript expression over `res`.
fn action_lines(list: Option<&Value>) -> Option<String> {
    let lines: Vec<String> = enabled(list)
        .filter(|a| {
            text(a.get("type")).as_deref() == Some("set-variable")
                && text(a.get("phase")).as_deref().unwrap_or("after-response") == "after-response"
        })
        .filter_map(|a| {
            let name = text(a.pointer("/variable/name"))?;
            let expr = text(a.pointer("/selector/expression"))?;
            Some(format!("bru.setVar({}, ({expr}));", js_string(&name)))
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

#[derive(Default)]
struct ScriptParts {
    before: Vec<String>,
    after: Vec<String>,
    tests: Vec<String>,
}

fn scripts(list: Option<&Value>) -> ScriptParts {
    let mut parts = ScriptParts::default();
    for script in list.and_then(Value::as_array).into_iter().flatten() {
        let Some(code) = text(script.get("code")) else {
            continue;
        };
        match script.get("type").and_then(Value::as_str) {
            Some("before-request") => parts.before.push(code),
            Some("after-response") => parts.after.push(code),
            Some("tests") => parts.tests.push(code),
            _ => {}
        }
    }
    parts
}

/// An environment as written, before `extends` is followed.
struct RawEnvironment {
    name: String,
    variables: BTreeMap<String, String>,
    secrets: Vec<String>,
    extends: Option<String>,
}

fn raw_environment(value: &Value, fallback: String) -> RawEnvironment {
    let mut env = RawEnvironment {
        name: text(value.get("name")).unwrap_or(fallback),
        variables: BTreeMap::new(),
        secrets: Vec::new(),
        extends: text(value.get("extends")),
    };
    for v in enabled(value.get("variables")) {
        let Some(name) = text(v.get("name")) else {
            continue;
        };
        if v.get("secret").and_then(Value::as_bool) == Some(true) {
            env.secrets.push(name);
        } else {
            env.variables.insert(name, variable_value(v.get("value")));
        }
    }
    env
}

/// Reads an OpenCollection environment document: its name, variables and
/// the names of its secrets, which have no value in it. An environment
/// that `extends` another gets only its own variables here.
pub fn environment(value: &Value) -> Option<(String, BTreeMap<String, String>, Vec<String>)> {
    value.get("variables")?.as_array()?;
    let env = raw_environment(value, String::new());
    Some((env.name, env.variables, env.secrets))
}

#[allow(clippy::type_complexity)]
fn resolve_environments(
    raw: Vec<RawEnvironment>,
    warnings: &mut Vec<String>,
) -> (
    BTreeMap<String, BTreeMap<String, String>>,
    BTreeMap<String, Vec<String>>,
) {
    let by_name: BTreeMap<&str, &RawEnvironment> =
        raw.iter().map(|e| (e.name.as_str(), e)).collect();
    let mut environments = BTreeMap::new();
    let mut secrets = BTreeMap::new();
    for env in &raw {
        // Parents first, so the environment's own values win.
        let mut chain = vec![env];
        while let Some(parent) = chain
            .last()
            .and_then(|e| e.extends.as_deref())
            .and_then(|p| by_name.get(p))
        {
            if chain.iter().any(|e| e.name == parent.name) {
                break;
            }
            chain.push(parent);
        }
        if let Some(missing) = chain
            .last()
            .and_then(|e| e.extends.as_deref())
            .filter(|p| !by_name.contains_key(p))
        {
            warnings.push(format!(
                "environment `{}` extends `{missing}`, which the collection does not have",
                env.name
            ));
        }
        let mut vars = BTreeMap::new();
        let mut names: Vec<String> = Vec::new();
        for e in chain.iter().rev() {
            vars.extend(e.variables.clone());
            for s in &e.secrets {
                vars.remove(s);
                if !names.contains(s) {
                    names.push(s.clone());
                }
            }
        }
        names.retain(|s| !env.variables.contains_key(s));
        for secret in &names {
            warnings.push(format!(
                "environment `{}`: the secret `{secret}` has no value in the file; pass it with --var {secret}=...",
                env.name
            ));
        }
        if !names.is_empty() {
            secrets.insert(env.name.clone(), names);
        }
        environments.insert(env.name.clone(), vars);
    }
    (environments, secrets)
}

/// A variable's value: text, a typed value (`{ type, data }`) or a list of
/// variants, of which the selected one (else the first) is used.
fn variable_value(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Array(variants)) => {
            let chosen = variants
                .iter()
                .find(|v| v.get("selected").and_then(Value::as_bool) == Some(true))
                .or_else(|| variants.first());
            variable_value(chosen.and_then(|v| v.get("value")))
        }
        Some(obj @ Value::Object(_)) => match obj.get("data") {
            Some(data) => variable_value(Some(data)),
            None => obj.to_string(),
        },
    }
}

/// A body given as a list of variants: the selected one, else the first.
fn selected<'a>(value: Option<&'a Value>, key: &str) -> Option<&'a Value> {
    match value? {
        Value::Array(variants) => variants
            .iter()
            .find(|v| v.get("selected").and_then(Value::as_bool) == Some(true))
            .or_else(|| variants.first())
            .and_then(|v| v.get(key)),
        Value::Null => None,
        other => Some(other),
    }
}

/// The entries of a list that are not `disabled: true`.
fn enabled(list: Option<&Value>) -> impl Iterator<Item = &Value> {
    list.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|v| v.get("disabled").and_then(Value::as_bool) != Some(true))
}

fn text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn docs(value: Option<&Value>) -> Option<String> {
    let text = match value? {
        Value::String(s) => s.clone(),
        other => other.get("content")?.as_str()?.to_owned(),
    };
    (!text.trim().is_empty()).then_some(text)
}

fn item_type(item: &Value) -> Option<&str> {
    item.pointer("/info/type")
        .or_else(|| item.get("type"))
        .and_then(Value::as_str)
}

fn seq(value: &Value) -> f64 {
    value
        .pointer("/info/seq")
        .and_then(Value::as_f64)
        .unwrap_or(f64::MAX)
}

fn parse_file(file: &Path) -> Result<Value, ImportError> {
    let text = read(file)?;
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_yaml_ng::from_str(&text).map_err(|e| ImportError::Yaml {
        file: file.to_owned(),
        message: e.to_string(),
    })
}

fn entries(dir: &Path) -> Result<Vec<PathBuf>, ImportError> {
    let io = |source| ImportError::Io {
        path: dir.to_owned(),
        source,
    };
    let mut paths = std::fs::read_dir(dir)
        .map_err(io)?
        .map(|e| e.map(|e| e.path()).map_err(io))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    Ok(paths)
}

fn yaml_files(dir: &Path) -> Result<Vec<PathBuf>, ImportError> {
    Ok(entries(dir)?
        .into_iter()
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "yml" || e == "yaml"))
        .collect())
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
