//! What both importers do once a request is read: apply auth, merge
//! inherited headers and stitch scripts together.

use base64::Engine as _;
use stormix_flow::{graphql, Body, GraphqlBody, Header, Method, Request};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Auth {
    #[default]
    None,
    Bearer(String),
    Basic {
        username: String,
        password: String,
    },
    ApiKey {
        key: String,
        value: String,
        in_query: bool,
    },
    /// OAuth 2, digest, AWS and the rest: named so the warning can say which.
    Unsupported(String),
}

pub fn method(name: &str) -> Option<Method> {
    Some(match name.to_ascii_uppercase().as_str() {
        "GET" => Method::Get,
        "POST" => Method::Post,
        "PUT" => Method::Put,
        "PATCH" => Method::Patch,
        "DELETE" => Method::Delete,
        "HEAD" => Method::Head,
        "OPTIONS" => Method::Options,
        _ => return None,
    })
}

pub fn has_header(request: &Request, name: &str) -> bool {
    request
        .headers
        .iter()
        .any(|h| h.name.eq_ignore_ascii_case(name))
}

/// Adds the header unless the request already sets it.
pub fn default_header(request: &mut Request, name: &str, value: &str) {
    if !has_header(request, name) {
        request.headers.push(Header {
            name: name.into(),
            value: value.into(),
        });
    }
}

/// Inherited headers go first; a request's own header of the same name wins.
pub fn inherit_headers(request: &mut Request, inherited: &[Header]) {
    let own = std::mem::take(&mut request.headers);
    for header in inherited {
        if !own
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case(&header.name))
        {
            request.headers.push(header.clone());
        }
    }
    request.headers.extend(own);
}

/// Applies `auth` to the request. Postman and Bruno apply auth after the
/// pre-request script, so a script can set the token first: Basic auth
/// built from variables therefore becomes a line of script, appended to
/// `pre_request`. A request's own `Authorization` header wins.
pub fn apply_auth(
    auth: &Auth,
    request: &mut Request,
    pre_request: &mut Vec<String>,
    step: &str,
    warnings: &mut Vec<String>,
) {
    match auth {
        Auth::None => {}
        Auth::Bearer(token) => default_header(request, "Authorization", &format!("Bearer {token}")),
        Auth::Basic { .. } if has_header(request, "Authorization") => {}
        Auth::Basic { username, password } => {
            if username.contains("{{") || password.contains("{{") {
                pre_request.push(format!(
                    "pm.request.headers.upsert({{ key: \"Authorization\", value: \"Basic \" + btoa(pm.variables.replaceIn({}) + \":\" + pm.variables.replaceIn({})) }});",
                    js_string(username),
                    js_string(password)
                ));
            } else {
                let encoded = base64::engine::general_purpose::STANDARD
                    .encode(format!("{username}:{password}"));
                default_header(request, "Authorization", &format!("Basic {encoded}"));
            }
        }
        Auth::ApiKey {
            key,
            value,
            in_query: false,
        } => default_header(request, key, value),
        Auth::ApiKey {
            key,
            value,
            in_query: true,
        } => {
            let sep = if request.url.contains('?') { '&' } else { '?' };
            request.url = format!("{}{sep}{key}={value}", request.url);
        }
        Auth::Unsupported(kind) => warnings.push(format!(
            "`{step}`: {kind} auth is not supported yet, so no credentials are sent; set the header yourself"
        )),
    }
}

/// Joins scripts that run one after the other (collection, folder,
/// request). Each keeps its own scope, so two that declare the same
/// `const` do not clash.
pub fn join_scripts(parts: Vec<String>) -> Option<String> {
    let parts: Vec<String> = parts.into_iter().filter(|p| !p.trim().is_empty()).collect();
    match parts.len() {
        0 => None,
        1 => parts.into_iter().next(),
        _ => Some(
            parts
                .iter()
                .map(|p| format!("await (async function () {{\n{}\n}})();", p.trim_end()))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    }
}

/// A JavaScript string literal (JSON's syntax is valid JavaScript).
pub fn js_string(s: &str) -> String {
    serde_json::Value::from(s).to_string()
}

/// Replaces `:name` path segments, the way Postman and Bruno path
/// variables work. The query string is left alone.
pub fn path_variables(url: &str, vars: &[(String, String)]) -> String {
    if vars.is_empty() {
        return url.to_owned();
    }
    let split = url.find(['?', '#']).unwrap_or(url.len());
    let (path, rest) = url.split_at(split);
    let path = path
        .split('/')
        .map(|segment| {
            segment
                .strip_prefix(':')
                .and_then(|name| vars.iter().find(|(k, _)| k == name))
                .map_or(segment, |(_, v)| v.as_str())
        })
        .collect::<Vec<_>>()
        .join("/");
    format!("{path}{rest}")
}

/// The `Content-Type` Postman and Bruno set for a raw body of this language.
/// A GraphQL body from a collection. Variables are JSON text there, often
/// with bare `{{variables}}` in it, as in `{ "id": {{id}} }`: that is kept as
/// text and filled in before parsing when sent, rather than lost.
pub fn graphql_body(
    query: String,
    variables: &str,
    step: &str,
    warnings: &mut Vec<String>,
) -> Body {
    if !variables.trim().is_empty() {
        if let Err(why) = graphql::check_variables_text(variables) {
            warnings.push(format!(
                "`{step}`: {why}; the request will fail until they are fixed"
            ));
        }
    }
    Body::Graphql(GraphqlBody {
        query,
        variables: graphql::variables_from_text(variables),
        operation_name: None,
        allow_errors: false,
    })
}

pub fn raw_content_type(language: &str) -> Option<&'static str> {
    Some(match language {
        "json" => "application/json",
        "xml" => "application/xml",
        "html" => "text/html",
        "javascript" => "application/javascript",
        "text" => "text/plain",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_variables_replace_whole_segments_only() {
        let vars = [("id".to_owned(), "7".to_owned())];
        assert_eq!(
            path_variables("{{base}}/users/:id/posts?x=:id", &vars),
            "{{base}}/users/7/posts?x=:id"
        );
        assert_eq!(
            path_variables("http://h:80/a/:idx", &vars),
            "http://h:80/a/:idx"
        );
    }

    #[test]
    fn scripts_keep_their_own_scope() {
        assert_eq!(
            join_scripts(vec![" ".into(), "a()".into()]).as_deref(),
            Some("a()")
        );
        assert_eq!(
            join_scripts(vec!["const x = 1;".into(), "const x = 2;".into()]).as_deref(),
            Some("await (async function () {\nconst x = 1;\n})();\nawait (async function () {\nconst x = 2;\n})();")
        );
    }
}
