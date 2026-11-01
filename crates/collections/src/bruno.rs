//! Bruno collections: a folder of `.bru` request files, with `bruno.json`,
//! an optional `collection.bru`, a `folder.bru` per sub-folder and
//! environments under `environments/`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use stormix_flow::{
    Body, Flow, Header, KeyValue, Request, Scripts, Step, TestFile, CURRENT_VERSION,
};

use crate::bru::{self, Block, Content};
use crate::common::{self, js_string, Auth};
use crate::{read, ImportError, Imported};

const METHODS: [&str; 7] = ["get", "post", "put", "patch", "delete", "head", "options"];

/// Operators that take no argument, like `res.body.id: isNumber`.
const UNARY: [&str; 12] = [
    "isEmpty",
    "isNotEmpty",
    "isNull",
    "isUndefined",
    "isDefined",
    "isTruthy",
    "isFalsy",
    "isJson",
    "isNumber",
    "isString",
    "isBoolean",
    "isArray",
];
const BINARY: [&str; 15] = [
    "eq",
    "neq",
    "gt",
    "gte",
    "lt",
    "lte",
    "in",
    "notIn",
    "contains",
    "notContains",
    "length",
    "matches",
    "notMatches",
    "startsWith",
    "endsWith",
];

/// Imports the collection in `dir`.
pub fn from_dir(dir: &Path) -> Result<Imported, ImportError> {
    let config = dir.join("bruno.json");
    let name = if config.is_file() {
        let text = read(&config)?;
        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| ImportError::Bru {
                file: config.clone(),
                line: e.line(),
                message: e.to_string(),
            })?;
        json.get("name").and_then(|n| n.as_str()).map(str::to_owned)
    } else if bru_files(dir)?.is_empty() {
        return Err(ImportError::NotBruno(dir.to_owned()));
    } else {
        None
    };
    let name = name.unwrap_or_else(|| {
        dir.canonicalize()
            .ok()
            .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "Bruno collection".into())
    });

    let mut importer = Importer::default();
    let mut scope = Scope::default();
    let mut variables = BTreeMap::new();
    let collection = dir.join("collection.bru");
    if collection.is_file() {
        let blocks = parse_file(&collection)?;
        // Collection variables are the flow's; environments and --var override them.
        if let Some(vars) = bru::find(&blocks, "vars:pre-request") {
            for (k, v) in vars.pairs() {
                variables.insert(k.to_owned(), v.to_owned());
            }
        }
        scope.enter(&blocks, false);
    }
    importer.walk(dir, &scope)?;
    if importer.steps.is_empty() {
        importer
            .warnings
            .push("the collection has no requests".into());
    }

    let mut environments = BTreeMap::new();
    let mut env_secrets = BTreeMap::new();
    let env_dir = dir.join("environments");
    if env_dir.is_dir() {
        for file in bru_files(&env_dir)? {
            let env_name = stem(&file);
            let (vars, secrets) = parse_environment(&read(&file)?, &file)?;
            for secret in &secrets {
                importer.warnings.push(format!(
                    "environment `{env_name}`: the secret `{secret}` has no value in the file; pass it with --var {secret}=..."
                ));
            }
            if !secrets.is_empty() {
                env_secrets.insert(env_name.clone(), secrets);
            }
            environments.insert(env_name, vars);
        }
    }

    Ok(Imported {
        test: TestFile {
            version: CURRENT_VERSION,
            flow: Flow {
                name,
                description: None,
                variables,
                steps: importer.steps,
                set_aside: Vec::new(),
            },
            load: None,
            thresholds: Vec::new(),
        },
        environments,
        secrets: env_secrets,
        warnings: importer.warnings,
    })
}

/// Reads one environment file, such as `environments/local.bru`: its
/// variables and the names of its secrets, which have no value in it.
#[allow(clippy::type_complexity)]
pub fn environment(path: &Path) -> Result<(BTreeMap<String, String>, Vec<String>), ImportError> {
    parse_environment(&read(path)?, path)
}

/// The variables and the names of secrets, which are kept out of the file.
fn parse_environment(
    text: &str,
    file: &Path,
) -> Result<(BTreeMap<String, String>, Vec<String>), ImportError> {
    let blocks = parse(text, file)?;
    let vars = bru::find(&blocks, "vars")
        .map(|b| {
            b.pairs()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect()
        })
        .unwrap_or_default();
    let secrets = match bru::find(&blocks, "vars:secret").map(|b| &b.content) {
        Some(Content::List(names)) => names
            .iter()
            .filter(|n| !n.starts_with('~'))
            .cloned()
            .collect(),
        _ => Vec::new(),
    };
    Ok((vars, secrets))
}

/// What a folder passes down to the requests in it.
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
    /// Takes in what a `collection.bru` or `folder.bru` adds.
    fn enter(&mut self, blocks: &[Block], folder: bool) {
        if let Some(headers) = bru::find(blocks, "headers") {
            common_headers(&mut self.headers, headers);
        }
        if let Some(mode) = bru::find(blocks, "auth").and_then(|b| b.get("mode")) {
            if let Some(auth) = auth(mode, blocks) {
                self.auth = auth;
            }
        }
        if folder {
            self.vars.extend(var_lines(blocks));
        }
        push_text(&mut self.pre_request, blocks, "script:pre-request");
        push_text(&mut self.post_response, blocks, "script:post-response");
        push_text(&mut self.tests, blocks, "tests");
    }
}

#[derive(Default)]
struct Importer {
    steps: Vec<Step>,
    warnings: Vec<String>,
}

impl Importer {
    /// Requests in `dir` in `seq` order, then each sub-folder the same way.
    fn walk(&mut self, dir: &Path, scope: &Scope) -> Result<(), ImportError> {
        let mut requests = Vec::new();
        for file in bru_files(dir)? {
            let file_name = file.file_name().unwrap_or_default();
            if file_name == "collection.bru" || file_name == "folder.bru" {
                continue;
            }
            let blocks = parse_file(&file)?;
            let seq = seq(&blocks);
            requests.push((seq, file, blocks));
        }
        requests.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        for (_, file, blocks) in requests {
            self.request(&file, &blocks, scope);
        }

        let mut folders = Vec::new();
        for entry in entries(dir)? {
            let name = entry
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if !entry.is_dir()
                || name.starts_with('.')
                || name == "node_modules"
                || (name == "environments" && scope.path.is_empty())
            {
                continue;
            }
            let folder_file = entry.join("folder.bru");
            let blocks = if folder_file.is_file() {
                parse_file(&folder_file)?
            } else {
                Vec::new()
            };
            folders.push((seq(&blocks), name, entry, blocks));
        }
        folders.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        for (_, dir_name, path, blocks) in folders {
            let mut inner = scope.clone();
            let label = bru::find(&blocks, "meta")
                .and_then(|m| m.get("name"))
                .map_or(dir_name, str::to_owned);
            inner.enter(&blocks, true);
            inner.path.push(label);
            self.walk(&path, &inner)?;
        }
        Ok(())
    }

    fn request(&mut self, file: &Path, blocks: &[Block], scope: &Scope) {
        let meta = bru::find(blocks, "meta");
        let name = meta
            .and_then(|m| m.get("name"))
            .map_or_else(|| stem(file), str::to_owned);
        let step = scope
            .path
            .iter()
            .chain([&name])
            .cloned()
            .collect::<Vec<_>>()
            .join(" / ");
        let kind = meta.and_then(|m| m.get("type")).unwrap_or("http");
        if !matches!(kind, "http" | "graphql") {
            self.warnings.push(format!(
                "`{step}`: {kind} requests are not supported; skipped"
            ));
            return;
        }
        let Some(http) = blocks.iter().find(|b| METHODS.contains(&b.name.as_str())) else {
            self.warnings.push(format!(
                "`{step}` has no method block, like `get {{ ... }}`; skipped"
            ));
            return;
        };
        let method = common::method(&http.name).expect("METHODS are all supported");

        let mut url = http.get("url").unwrap_or_default().to_owned();
        if let Some(params) = bru::find(blocks, "params:path") {
            let vars: Vec<(String, String)> = params
                .pairs()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect();
            url = common::path_variables(&url, &vars);
        }
        let mut request = Request {
            method,
            url,
            headers: Vec::new(),
            body: None,
        };
        if let Some(headers) = bru::find(blocks, "headers") {
            common_headers(&mut request.headers, headers);
        }
        common::inherit_headers(&mut request, &scope.headers);
        self.body(
            http.get("body").unwrap_or("none"),
            blocks,
            &mut request,
            &step,
        );

        let mut pre_request = scope.vars.clone();
        pre_request.extend(var_lines(blocks));
        pre_request.extend(scope.pre_request.iter().cloned());
        push_text(&mut pre_request, blocks, "script:pre-request");
        let auth = match http.get("auth").unwrap_or("none") {
            "inherit" => Some(scope.auth.clone()),
            mode => auth(mode, blocks),
        };
        if let Some(auth) = auth {
            common::apply_auth(
                &auth,
                &mut request,
                &mut pre_request,
                &step,
                &mut self.warnings,
            );
        }

        // Bruno's order after the response: variables, scripts, assertions, tests.
        let mut test = Vec::new();
        if let Some(vars) = bru::find(blocks, "vars:post-response") {
            let lines: Vec<String> = vars
                .pairs()
                .map(|(k, expr)| format!("bru.setVar({}, ({expr}));", js_string(k)))
                .collect();
            test.push(lines.join("\n"));
        }
        test.extend(scope.post_response.iter().cloned());
        push_text(&mut test, blocks, "script:post-response");
        if let Some(asserts) = bru::find(blocks, "assert") {
            test.push(
                asserts
                    .pairs()
                    .map(|(k, v)| assertion(k, v))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        test.extend(scope.tests.iter().cloned());
        push_text(&mut test, blocks, "tests");

        self.steps.push(Step {
            name: step,
            enabled: true,
            request,
            extract: BTreeMap::new(),
            assertions: Vec::new(),
            scripts: Scripts {
                pre_request: common::join_scripts(pre_request),
                test: common::join_scripts(test),
            },
        });
    }

    fn body(&mut self, mode: &str, blocks: &[Block], request: &mut Request, step: &str) {
        let text = |name: &str| bru::find(blocks, name).map(|b| b.text().to_owned());
        match mode {
            "json" | "text" | "xml" => {
                if let Some(body) = text(&format!("body:{mode}")) {
                    let language = if mode == "text" { "text" } else { mode };
                    if let Some(content_type) = common::raw_content_type(language) {
                        common::default_header(request, "Content-Type", content_type);
                    }
                    request.body = Some(Body::Raw(body));
                }
            }
            "formUrlEncoded" => {
                if let Some(form) = bru::find(blocks, "body:form-urlencoded") {
                    request.body = Some(Body::Form(key_values(form)));
                }
            }
            "multipartForm" => {
                if let Some(form) = bru::find(blocks, "body:multipart-form") {
                    let (files, fields): (Vec<_>, Vec<_>) = key_values(form)
                        .into_iter()
                        .partition(|kv| kv.value.starts_with("@file("));
                    if !files.is_empty() {
                        self.warnings.push(format!(
                            "`{step}`: file fields in multipart forms are not supported yet; they are left out"
                        ));
                    }
                    request.body = Some(Body::Multipart(fields));
                    request
                        .headers
                        .retain(|h| !h.name.eq_ignore_ascii_case("content-type"));
                }
            }
            "graphql" => {
                let query = text("body:graphql").unwrap_or_default();
                let variables = text("body:graphql:vars")
                    .and_then(|v| serde_json::from_str(&v).ok())
                    .unwrap_or(serde_json::Value::Null);
                let payload = serde_json::json!({ "query": query, "variables": variables });
                common::default_header(request, "Content-Type", "application/json");
                request.body = Some(Body::Raw(payload.to_string()));
            }
            "none" => {}
            other => self.warnings.push(format!(
                "`{step}`: {other} bodies are not supported yet; the request is sent without a body"
            )),
        }
    }
}

/// `res.status: eq 200` becomes a test of the same name. The left side is a
/// JavaScript expression; the right side an operator and its argument, or
/// just a value, which means `eq`.
fn assertion(expr: &str, rule: &str) -> String {
    let (op, arg) = match rule.split_once(char::is_whitespace) {
        Some((op, arg)) if BINARY.contains(&op) => (op, arg.trim()),
        _ if UNARY.contains(&rule) => (rule, ""),
        _ => ("eq", rule),
    };
    format!(
        "test({}, function () {{ __bruAssert(({expr}), {}, bru.interpolate({})); }});",
        js_string(&format!("{expr}: {rule}")),
        js_string(op),
        js_string(arg)
    )
}

fn auth(mode: &str, blocks: &[Block]) -> Option<Auth> {
    let block = bru::find(blocks, &format!("auth:{mode}"));
    let get = |key: &str| {
        block
            .and_then(|b| b.get(key))
            .unwrap_or_default()
            .to_owned()
    };
    Some(match mode {
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
            in_query: get("placement") == "queryparams",
        },
        other => Auth::Unsupported(other.to_owned()),
    })
}

/// Request and folder variables, set before the scripts run.
fn var_lines(blocks: &[Block]) -> Option<String> {
    let vars = bru::find(blocks, "vars:pre-request")?;
    let lines: Vec<String> = vars
        .pairs()
        .map(|(k, v)| {
            format!(
                "bru.setVar({}, bru.interpolate({}));",
                js_string(k),
                js_string(v)
            )
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn common_headers(headers: &mut Vec<Header>, block: &Block) {
    for (name, value) in block.pairs() {
        headers.retain(|h| !h.name.eq_ignore_ascii_case(name));
        headers.push(Header {
            name: name.to_owned(),
            value: value.to_owned(),
        });
    }
}

fn key_values(block: &Block) -> Vec<KeyValue> {
    block
        .pairs()
        .map(|(k, v)| KeyValue {
            key: k.to_owned(),
            value: v.to_owned(),
        })
        .collect()
}

fn push_text(parts: &mut Vec<String>, blocks: &[Block], name: &str) {
    if let Some(block) = bru::find(blocks, name) {
        parts.push(block.text().to_owned());
    }
}

fn seq(blocks: &[Block]) -> u32 {
    bru::find(blocks, "meta")
        .and_then(|m| m.get("seq"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(u32::MAX)
}

fn parse_file(file: &Path) -> Result<Vec<Block>, ImportError> {
    parse(&read(file)?, file)
}

fn parse(text: &str, file: &Path) -> Result<Vec<Block>, ImportError> {
    bru::parse(text).map_err(|(line, message)| ImportError::Bru {
        file: file.to_owned(),
        line,
        message,
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

fn bru_files(dir: &Path) -> Result<Vec<PathBuf>, ImportError> {
    Ok(entries(dir)?
        .into_iter()
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "bru"))
        .collect())
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
