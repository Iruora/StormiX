//! `{{variable}}` references, the templating Postman and Bruno share.

/// The variable names referenced as `{{name}}` in `text`, in order, with
/// repeats. Dynamic variables such as `{{$guid}}` are left out: the runner
/// provides them, so they never need a definition.
pub fn variable_refs(text: &str) -> Vec<&str> {
    let mut refs = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let name = after[..end].trim();
        if !name.is_empty() && !name.starts_with('$') {
            refs.push(name);
        }
        rest = &after[end + 2..];
    }
    refs
}

/// `text` with each `{{name}}` replaced by `lookup(name)`. Names are passed
/// trimmed, dynamic ones with their `$`. A name `lookup` does not know stays
/// in the text as written and is listed in `missing`, so a request still goes
/// out and the report says why it looked wrong.
pub fn render(text: &str, mut lookup: impl FnMut(&str) -> Option<String>) -> Rendered {
    let mut out = String::with_capacity(text.len());
    let mut missing = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        out.push_str(&rest[..start]);
        let name = after[..end].trim();
        match lookup(name) {
            Some(value) if !name.is_empty() => out.push_str(&value),
            _ => {
                out.push_str(&rest[start..start + 2 + end + 2]);
                if !name.is_empty() && !missing.iter().any(|m| m == name) {
                    missing.push(name.to_owned());
                }
            }
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Rendered { text: out, missing }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub text: String,
    /// Names with no value, each once, in order of first use.
    pub missing: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(name: &str) -> Option<String> {
        match name {
            "host" => Some("api.test".into()),
            "id" => Some("42".into()),
            _ => None,
        }
    }

    #[test]
    fn renders_known_and_keeps_unknown() {
        let r = render("https://{{host}}/u/{{ id }}?t={{token}}&t2={{token}}", vars);
        assert_eq!(r.text, "https://api.test/u/42?t={{token}}&t2={{token}}");
        assert_eq!(r.missing, ["token"]);
    }

    #[test]
    fn leaves_text_without_templates_alone() {
        let r = render("plain {{ and unclosed", vars);
        assert_eq!(r.text, "plain {{ and unclosed");
        assert!(r.missing.is_empty());
    }

    #[test]
    fn finds_names_in_order() {
        assert_eq!(
            variable_refs("{{base_url}}/users/{{ id }}?q={{base_url}}"),
            ["base_url", "id", "base_url"]
        );
    }

    #[test]
    fn skips_dynamic_and_unclosed() {
        assert_eq!(variable_refs("{{$guid}} {{}} {{open"), Vec::<&str>::new());
        assert_eq!(variable_refs("no templates"), Vec::<&str>::new());
    }
}
