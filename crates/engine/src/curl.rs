//! A step's request as a `curl` command, to send it again outside Stormix.

use serde::{Deserialize, Serialize};
use stormix_flow::Step;

use crate::runner::{OutBody, Outgoing};
use crate::Variables;

/// The shell a command is quoted for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    /// bash, zsh and other POSIX shells, Git Bash and WSL on Windows.
    #[default]
    Posix,
    /// The Windows Command Prompt.
    Cmd,
}

/// A request ready to paste into a terminal, with what it could not fill in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CurlCommand {
    pub command: String,
    /// Variables with no value, left in the command as `{{name}}`.
    pub missing: Vec<String>,
    /// Ways the command may differ from what a run sends.
    pub notes: Vec<String>,
}

/// The step's request with `vars` filled in, as a run would send it: the
/// same body encodings and default `Content-Type`. Following redirects is
/// on, as it is in runs. A pre-request script is not run, since it may
/// send requests of its own; a note says so.
pub fn curl_command(step: &Step, vars: &Variables, shell: Shell) -> CurlCommand {
    let outgoing = Outgoing::from(&step.request);
    let mut missing = Vec::new();
    let mut notes = Vec::new();
    let render = |text: &str, missing: &mut Vec<String>| vars.render_collecting(text, missing);

    let url = render(&outgoing.url, &mut missing);
    let method = outgoing.method.to_ascii_uppercase();
    let mut headers: Vec<(String, String)> = outgoing
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), render(v, &mut missing)))
        .collect();
    let has_content_type = headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("content-type"));
    let mut json_default = || {
        if !has_content_type {
            headers.push(("Content-Type".into(), "application/json".into()));
        }
    };

    // `--data-raw` sends the text as is; `--form-string` never reads a file
    // for a value starting with `@` or `<`, unlike `-F`.
    let mut data: Vec<(&str, String)> = Vec::new();
    let mut drop_content_type = false;
    match &outgoing.body {
        OutBody::None => {}
        OutBody::Raw(text) => {
            data.push(("--data-raw", render(text, &mut missing)));
            // A run sends raw text with no `Content-Type` unless the step sets
            // one; `--data-raw` alone would add a form one.
            drop_content_type = !has_content_type;
        }
        OutBody::Json(value) => {
            json_default();
            data.push((
                "--data-raw",
                vars.render_json(value, &mut missing).to_string(),
            ));
        }
        OutBody::Graphql(graphql) => match crate::graphql::payload(graphql, vars, &mut missing) {
            Ok(payload) => {
                json_default();
                data.push(("--data-raw", payload.to_string()));
            }
            Err(message) => {
                notes.push(format!("The body was left out: {message}."));
            }
        },
        OutBody::Form(fields) => {
            let encoded = fields
                .iter()
                .map(|(k, v)| {
                    let (k, v) = (render(k, &mut missing), render(v, &mut missing));
                    format!("{}={}", form_encode(&k), form_encode(&v))
                })
                .collect::<Vec<_>>()
                .join("&");
            if !has_content_type {
                headers.push((
                    "Content-Type".into(),
                    "application/x-www-form-urlencoded".into(),
                ));
            }
            data.push(("--data-raw", encoded));
        }
        OutBody::Multipart(fields) => {
            for (k, v) in fields {
                let (k, v) = (render(k, &mut missing), render(v, &mut missing));
                data.push(("--form-string", format!("{k}={v}")));
            }
        }
    }

    if step
        .scripts
        .pre_request
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
    {
        notes.push(
            "The pre-request script was not run, so what it would change is not in the command."
                .into(),
        );
    }

    let q = |text: &str| quote(text, shell);
    // A URL that starts with `-` would be read as a curl option.
    let target = if url.starts_with('-') {
        format!("--url {}", q(&url))
    } else {
        q(&url)
    };
    let mut parts = vec![match method.as_str() {
        "GET" if data.is_empty() => format!("curl {target}"),
        // With a body curl sends POST by itself and, like a run, switches to
        // GET on a 301, 302 or 303; `-X POST` would keep POST.
        "POST" if !data.is_empty() => format!("curl {target}"),
        // `-X HEAD` waits for a body that never comes.
        "HEAD" => format!("curl --head {target}"),
        _ => format!("curl -X {method} {target}"),
    }];
    // Brackets and braces in a URL, as in `?filter[status]=on`, are sent as
    // written rather than read as curl ranges.
    parts.push("--globoff".into());
    for (name, value) in &headers {
        parts.push(format!("-H {}", q(&format!("{name}: {value}"))));
    }
    if drop_content_type {
        parts.push(format!("-H {}", q("Content-Type:")));
    }
    for (flag, value) in &data {
        parts.push(format!("{flag} {}", q(value)));
    }
    parts.push("--location".into());

    let joint = match shell {
        Shell::Posix => " \\\n  ",
        Shell::Cmd => " ^\n  ",
    };
    CurlCommand {
        command: parts.join(joint),
        missing,
        notes,
    }
}

fn quote(text: &str, shell: Shell) -> String {
    match shell {
        Shell::Posix => format!("'{}'", text.replace('\'', r"'\''")),
        Shell::Cmd => quote_cmd(text),
    }
}

/// Quoting for cmd.exe, as browsers do for "Copy as cURL (cmd)": the text
/// sits in `^"…^"`, curl's own `\` escapes protect quotes and backslashes,
/// and `^` makes cmd take every other special character literally, `%`
/// included, so nothing is read as a variable.
fn quote_cmd(text: &str) -> String {
    let mut out = String::from("^\"");
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // Backslashes are literal to the argument parser unless a quote
            // follows them, including the closing one.
            '\\' => {
                let mut run = 1;
                while chars.peek() == Some(&'\\') {
                    chars.next();
                    run += 1;
                }
                let before_quote = matches!(chars.peek(), Some('"') | None);
                let n = if before_quote { run * 2 } else { run };
                out.push_str(&"\\".repeat(n));
            }
            '"' => out.push_str("\\^\""),
            '%' => {
                out.push('%');
                if chars
                    .peek()
                    .is_some_and(|n| n.is_ascii_alphanumeric() || *n == '_')
                {
                    out.push('^');
                }
            }
            '\r' => {}
            // A caret ends the line; the empty line after it keeps the newline.
            '\n' => out.push_str("^\n\n"),
            c if c.is_ascii_alphanumeric()
                || c.is_whitespace()
                || "_-:=+~'/.,?;()*`".contains(c) =>
            {
                out.push(c)
            }
            c if !c.is_ascii() => out.push(c),
            c => {
                out.push('^');
                out.push(c);
            }
        }
    }
    out.push_str("^\"");
    out
}

/// `application/x-www-form-urlencoded`, as reqwest encodes a form.
fn form_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(yaml: &str) -> Step {
        let value: serde_json::Value = serde_yaml_ng::from_str(yaml).expect("YAML");
        serde_json::from_value(value).expect("a step")
    }

    fn vars() -> Variables {
        let mut vars = Variables::new();
        vars.extend([
            ("base", "https://api.test"),
            ("token", "s3cr'et"),
            ("id", "42"),
        ]);
        vars
    }

    #[test]
    fn get_with_headers_and_missing() {
        let out = curl_command(
            &step(
                r#"
name: user
request:
  method: GET
  url: "{{base}}/users/{{id}}?q={{query}}"
  headers:
    - { name: Authorization, value: "Bearer {{token}}" }
"#,
            ),
            &vars(),
            Shell::Posix,
        );
        assert_eq!(
            out.command,
            "curl 'https://api.test/users/42?q={{query}}' \\\n  --globoff \\\n  -H 'Authorization: Bearer s3cr'\\''et' \\\n  --location"
        );
        assert_eq!(out.missing, ["query"]);
        assert!(out.notes.is_empty());
    }

    #[test]
    fn json_body_gets_content_type() {
        let out = curl_command(
            &step(
                r#"
name: create
request:
  method: POST
  url: "{{base}}/items"
  body:
    json: { id: "{{id}}", n: 3 }
"#,
            ),
            &vars(),
            Shell::Posix,
        );
        assert_eq!(
            out.command,
            "curl 'https://api.test/items' \\\n  --globoff \\\n  -H 'Content-Type: application/json' \\\n  --data-raw '{\"id\":\"42\",\"n\":3}' \\\n  --location"
        );
    }

    #[test]
    fn graphql_payload_and_typed_variables() {
        let out = curl_command(
            &step(
                r#"
name: gql
request:
  method: POST
  url: "{{base}}/graphql"
  body:
    graphql:
      query: "query U($id: Int!) { user(id: $id) { name } }"
      variables: { id: "{{id}}" }
"#,
            ),
            &vars(),
            Shell::Posix,
        );
        assert!(out.command.contains(
            r#"--data-raw '{"query":"query U($id: Int!) { user(id: $id) { name } }","variables":{"id":42}}'"#
        ));
        assert!(out.command.contains("-H 'Content-Type: application/json'"));
    }

    #[test]
    fn form_and_multipart() {
        let form = curl_command(
            &step(
                r#"
name: f
request:
  method: POST
  url: "{{base}}/login"
  body:
    form: [ { key: user, value: "a b&c" }, { key: pass, value: "{{token}}" } ]
"#,
            ),
            &vars(),
            Shell::Posix,
        );
        assert!(form
            .command
            .contains("-H 'Content-Type: application/x-www-form-urlencoded'"));
        assert!(form
            .command
            .contains("--data-raw 'user=a+b%26c&pass=s3cr%27et'"));

        let multi = curl_command(
            &step(
                r#"
name: m
request:
  method: PUT
  url: "{{base}}/up"
  body:
    multipart: [ { key: note, value: "@not-a-file" } ]
"#,
            ),
            &vars(),
            Shell::Posix,
        );
        assert!(multi.command.contains("--form-string 'note=@not-a-file'"));
        assert!(!multi.command.contains("Content-Type"));
    }

    #[test]
    fn head_and_pre_request_note() {
        let out = curl_command(
            &step(
                r#"
name: h
request: { method: HEAD, url: "{{base}}/" }
scripts:
  pre_request: "req.setHeader('x', '1')"
"#,
            ),
            &vars(),
            Shell::Posix,
        );
        assert!(out.command.starts_with("curl --head 'https://api.test/'"));
        assert_eq!(out.notes.len(), 1);
    }

    #[test]
    fn raw_body_without_content_type_sends_none() {
        let out = curl_command(
            &step(
                r#"
name: r
request:
  method: PATCH
  url: "{{base}}/items?filter[status]=on"
  body: { raw: "id={{id}}" }
"#,
            ),
            &vars(),
            Shell::Posix,
        );
        assert_eq!(
            out.command,
            "curl -X PATCH 'https://api.test/items?filter[status]=on' \\\n  --globoff \\\n  -H 'Content-Type:' \\\n  --data-raw 'id=42' \\\n  --location"
        );
    }

    #[test]
    fn cmd_backslashes_reach_curl_unchanged() {
        // Literal unless a quote follows: then doubled, with the quote escaped.
        assert_eq!(quote_cmd(r"C:\dir\f"), r#"^"C:\dir\f^""#);
        assert_eq!(
            quote_cmd(r#"{"a":"x\"y\\"}"#),
            r#"^"^{\^"a\^":\^"x\\\^"y\\\\\^"^}^""#
        );
        assert_eq!(quote_cmd(r"ends\"), r#"^"ends\\^""#);
    }

    #[test]
    fn url_starting_with_a_dash_is_not_an_option() {
        let mut v = vars();
        v.set("target", "-K./payload.curl");
        let out = curl_command(
            &step("name: d\nrequest: { method: GET, url: \"{{target}}\" }\n"),
            &v,
            Shell::Posix,
        );
        assert!(out.command.starts_with("curl --url '-K./payload.curl'"));
    }

    #[test]
    fn cmd_quoting() {
        assert_eq!(quote_cmd(r#"{"a":"b"}"#), r#"^"^{\^"a\^":\^"b\^"^}^""#);
        assert_eq!(quote_cmd("100%path & %1"), "^\"100%^path ^& %^1^\"");
        let out = curl_command(
            &step(
                r#"
name: c
request: { method: DELETE, url: "{{base}}/x?a=1&b=2" }
"#,
            ),
            &vars(),
            Shell::Cmd,
        );
        assert_eq!(
            out.command,
            "curl -X DELETE ^\"https://api.test/x?a=1^&b=2^\" ^\n  --globoff ^\n  --location"
        );
    }
}
