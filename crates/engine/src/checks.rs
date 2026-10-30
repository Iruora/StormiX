//! Assertions and extractions against one response.

use std::cell::OnceCell;

use reqwest::header::HeaderMap;
use serde_json::Value;
use serde_json_path::JsonPath;
use stormix_flow::{Assertion, ExtractSource};

use crate::{Check, Variables};

/// The parts of a response checks look at. The body is parsed as JSON at
/// most once, and only if a check needs it.
pub(crate) struct Response<'a> {
    pub status: u16,
    pub headers: &'a HeaderMap,
    pub body: &'a [u8],
    pub duration_ms: f64,
    json: OnceCell<Result<Value, String>>,
}

impl<'a> Response<'a> {
    pub fn new(status: u16, headers: &'a HeaderMap, body: &'a [u8], duration_ms: f64) -> Self {
        Self {
            status,
            headers,
            body,
            duration_ms,
            json: OnceCell::new(),
        }
    }

    fn json(&self) -> Result<&Value, &str> {
        self.json
            .get_or_init(|| {
                serde_json::from_slice(self.body).map_err(|e| format!("the body is not JSON ({e})"))
            })
            .as_ref()
            .map_err(String::as_str)
    }

    fn header(&self, name: &str) -> Option<String> {
        let values: Vec<_> = self
            .headers
            .get_all(name)
            .iter()
            .map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned())
            .collect();
        (!values.is_empty()).then(|| values.join(", "))
    }

    fn body_text(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(self.body)
    }

    /// The first value at `path`, or why there is none.
    fn json_path(&self, path: &str) -> Result<Option<&Value>, String> {
        let path = JsonPath::parse(path).map_err(|e| format!("invalid JSONPath ({e})"))?;
        let json = self.json().map_err(str::to_owned)?;
        Ok(path.query(json).first())
    }
}

pub(crate) fn assert(assertion: &Assertion, res: &Response<'_>, vars: &Variables) -> Check {
    match assertion {
        Assertion::Status(expected) => outcome(
            format!("status is {expected}"),
            res.status == *expected,
            || format!("got {}", res.status),
        ),
        Assertion::MaxDurationMs(max) => outcome(
            format!("responds within {max} ms"),
            res.duration_ms <= f64::from(*max),
            || format!("took {:.0} ms", res.duration_ms),
        ),
        Assertion::Header(h) => {
            let actual = res.header(&h.name);
            if let Some(expected) = &h.equals {
                let expected = vars.render(expected).text;
                outcome(
                    format!("header {} equals {expected:?}", h.name),
                    actual.as_deref() == Some(expected.as_str()),
                    || describe_header(actual.as_deref()),
                )
            } else if let Some(part) = &h.contains {
                let part = vars.render(part).text;
                outcome(
                    format!("header {} contains {part:?}", h.name),
                    actual.as_deref().is_some_and(|a| a.contains(&part)),
                    || describe_header(actual.as_deref()),
                )
            } else {
                outcome(
                    format!("header {} is present", h.name),
                    actual.is_some(),
                    || "missing".into(),
                )
            }
        }
        Assertion::Json(j) => {
            let found = res.json_path(&j.path);
            if let Some(expected) = &j.equals {
                let expected = vars.render_json(expected, &mut Vec::new());
                let name = format!("{} equals {}", j.path, expected);
                match found {
                    Ok(Some(actual)) => outcome(name, json_equals(actual, &expected), || {
                        format!("got {actual}")
                    }),
                    Ok(None) => failed(name, "no match".into()),
                    Err(why) => failed(name, why),
                }
            } else if j.exists == Some(false) {
                let name = format!("{} does not exist", j.path);
                match found {
                    Ok(Some(actual)) => failed(name, format!("got {actual}")),
                    Ok(None) => passed(name),
                    Err(why) => failed(name, why),
                }
            } else {
                let name = format!("{} exists", j.path);
                match found {
                    Ok(Some(_)) => passed(name),
                    Ok(None) => failed(name, "no match".into()),
                    Err(why) => failed(name, why),
                }
            }
        }
        Assertion::BodyContains(text) => {
            let text = vars.render(text).text;
            outcome(
                format!("body contains {text:?}"),
                res.body_text().contains(&text),
                || "not found".into(),
            )
        }
    }
}

/// The extracted value, and a check that records where it came from.
pub(crate) fn extract(
    name: &str,
    source: &ExtractSource,
    res: &Response<'_>,
) -> (Check, Option<String>) {
    let (label, value) = match source {
        ExtractSource::Json(path) => (
            format!("set {name} from {path}"),
            res.json_path(path)
                .and_then(|v| v.map(text_of).ok_or_else(|| "no match".to_owned())),
        ),
        ExtractSource::Header(header) => (
            format!("set {name} from header {header}"),
            res.header(header).ok_or_else(|| "missing".to_owned()),
        ),
        ExtractSource::Regex(pattern) => (
            format!("set {name} from /{pattern}/"),
            regex::Regex::new(pattern)
                .map_err(|e| format!("invalid regex ({e})"))
                .and_then(|re| {
                    let body = res.body_text();
                    re.captures(&body)
                        .map(|c| {
                            c.get(1)
                                .or_else(|| c.get(0))
                                .map_or("", |m| m.as_str())
                                .to_owned()
                        })
                        .ok_or_else(|| "no match".to_owned())
                }),
        ),
    };
    match value {
        Ok(v) => (passed(label), Some(v)),
        Err(why) => (failed(label, why), None),
    }
}

/// A string as itself; anything else as JSON text, like Postman does.
fn text_of(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Exact JSON equality, except that a string matches a number or boolean
/// with the same text: values filled in from variables are always strings.
fn json_equals(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Number(_) | Value::Bool(_), Value::String(s)) => {
            serde_json::from_str::<Value>(s).is_ok_and(|parsed| parsed == *actual)
        }
        _ => actual == expected,
    }
}

fn describe_header(actual: Option<&str>) -> String {
    actual.map_or_else(|| "missing".into(), |a| format!("got {a:?}"))
}

fn outcome(name: String, ok: bool, why: impl FnOnce() -> String) -> Check {
    if ok {
        passed(name)
    } else {
        failed(name, why())
    }
}

fn passed(name: String) -> Check {
    Check {
        name,
        passed: true,
        message: None,
    }
}

fn failed(name: String, message: String) -> Check {
    Check {
        name,
        passed: false,
        message: Some(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;
    use serde_json::json;
    use stormix_flow::{HeaderAssertion, JsonAssertion};

    fn headers() -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("content-type", HeaderValue::from_static("application/json"));
        h.append("x-tag", HeaderValue::from_static("a"));
        h.append("x-tag", HeaderValue::from_static("b"));
        h
    }

    const BODY: &[u8] = br#"{"user":"demo","id":7,"items":[{"sku":"A1"}],"ok":true}"#;

    fn run(assertion: Assertion) -> Check {
        let h = headers();
        let res = Response::new(200, &h, BODY, 120.0);
        let mut vars = Variables::new();
        vars.set("who", "demo");
        assert(&assertion, &res, &vars)
    }

    fn json(path: &str, equals: Option<Value>, exists: Option<bool>) -> Assertion {
        Assertion::Json(JsonAssertion {
            path: path.into(),
            equals,
            exists,
        })
    }

    #[test]
    fn status_and_duration() {
        assert!(run(Assertion::Status(200)).passed);
        assert_eq!(
            run(Assertion::Status(201)).message.as_deref(),
            Some("got 200")
        );
        assert!(run(Assertion::MaxDurationMs(120)).passed);
        assert_eq!(
            run(Assertion::MaxDurationMs(100)).message.as_deref(),
            Some("took 120 ms")
        );
    }

    #[test]
    fn headers_match_case_insensitively_and_join_repeats() {
        let h = |equals: Option<&str>, contains: Option<&str>| {
            run(Assertion::Header(HeaderAssertion {
                name: "X-Tag".into(),
                equals: equals.map(Into::into),
                contains: contains.map(Into::into),
            }))
        };
        assert!(h(None, None).passed);
        assert!(h(Some("a, b"), None).passed);
        assert!(h(None, Some("b")).passed);
        assert!(!h(Some("a"), None).passed);
    }

    #[test]
    fn json_assertions() {
        assert!(run(json("$.user", Some(json!("{{who}}")), None)).passed);
        assert!(run(json("$.id", Some(json!(7)), None)).passed);
        assert!(run(json("$.id", Some(json!("7")), None)).passed);
        assert!(run(json("$.ok", Some(json!("true")), None)).passed);
        assert!(run(json("$.items[0].sku", None, None)).passed);
        assert!(run(json("$.missing", None, Some(false))).passed);
        assert_eq!(
            run(json("$.missing", None, None)).message.as_deref(),
            Some("no match")
        );
        assert_eq!(
            run(json("$.id", Some(json!(8)), None)).message.as_deref(),
            Some("got 7")
        );
        assert!(run(json("$[", None, None))
            .message
            .unwrap()
            .starts_with("invalid JSONPath"));
    }

    #[test]
    fn body_contains() {
        assert!(run(Assertion::BodyContains("\"user\":\"{{who}}\"".into())).passed);
        assert!(!run(Assertion::BodyContains("nope".into())).passed);
    }

    #[test]
    fn extractions() {
        let h = headers();
        let res = Response::new(200, &h, BODY, 1.0);
        let get = |s: ExtractSource| extract("v", &s, &res);
        assert_eq!(
            get(ExtractSource::Json("$.user".into())).1.as_deref(),
            Some("demo")
        );
        assert_eq!(
            get(ExtractSource::Json("$.id".into())).1.as_deref(),
            Some("7")
        );
        assert_eq!(
            get(ExtractSource::Json("$.items".into())).1.as_deref(),
            Some(r#"[{"sku":"A1"}]"#)
        );
        assert_eq!(
            get(ExtractSource::Header("Content-Type".into()))
                .1
                .as_deref(),
            Some("application/json")
        );
        assert_eq!(
            get(ExtractSource::Regex(r#""sku":"(\w+)""#.into()))
                .1
                .as_deref(),
            Some("A1")
        );
        let (check, value) = get(ExtractSource::Json("$.nope".into()));
        assert_eq!((check.passed, value), (false, None));
    }

    #[test]
    fn non_json_bodies_fail_json_checks_clearly() {
        let h = HeaderMap::new();
        let res = Response::new(200, &h, b"<html>", 1.0);
        let check = assert(&json("$.a", None, None), &res, &Variables::new());
        assert!(check.message.unwrap().starts_with("the body is not JSON"));
    }
}
