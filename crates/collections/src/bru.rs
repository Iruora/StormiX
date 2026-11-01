//! The `.bru` file format: top-level blocks such as `meta { ... }`,
//! `headers { ... }` or `script:pre-request { ... }`, each holding either
//! `key: value` lines or free text (JSON bodies, scripts, docs), indented by
//! two spaces. `vars:secret [ ... ]` holds a list of names.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub name: String,
    pub line: usize,
    pub content: Content,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// `(key, value, enabled)`; a line starting with `~` is disabled.
    Pairs(Vec<(String, String, bool)>),
    Text(String),
    List(Vec<String>),
}

/// Blocks whose body is text, not `key: value` lines.
fn is_text(name: &str) -> bool {
    name.starts_with("body:") && !matches!(name, "body:form-urlencoded" | "body:multipart-form")
        || name.starts_with("script:")
        || matches!(name, "tests" | "docs")
}

/// Parses a whole file. Errors carry a 1-based line number.
pub fn parse(text: &str) -> Result<Vec<Block>, (usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_end();
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        let (name, close) = if let Some(name) = line.strip_suffix('{') {
            (name.trim(), "}")
        } else if let Some(name) = line.strip_suffix('[') {
            (name.trim(), "]")
        } else {
            return Err((
                i + 1,
                format!("expected a block like `name {{`, found `{}`", line.trim()),
            ));
        };
        if name.is_empty() || name.contains(char::is_whitespace) {
            return Err((i + 1, format!("`{}` is not a block name", line.trim())));
        }
        let start = i;
        let mut body = Vec::new();
        i += 1;
        loop {
            let Some(line) = lines.get(i) else {
                return Err((start + 1, format!("the `{name}` block is never closed")));
            };
            i += 1;
            if line.trim_end() == close {
                break;
            }
            body.push(dedent(line));
        }
        let content = if close == "]" {
            Content::List(
                body.iter()
                    .map(|l| l.trim().trim_end_matches(',').to_owned())
                    .filter(|l| !l.is_empty())
                    .collect(),
            )
        } else if is_text(name) {
            Content::Text(body.join("\n").trim_matches('\n').to_owned())
        } else {
            let mut pairs = Vec::new();
            for (n, line) in body.iter().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let (line, enabled) = match line.strip_prefix('~') {
                    Some(rest) => (rest, false),
                    None => (line, true),
                };
                let split = line
                    .find(": ")
                    .or_else(|| line.strip_suffix(':').map(str::len))
                    .or_else(|| line.find(':'))
                    .ok_or_else(|| {
                        (
                            start + n + 2,
                            format!("expected `key: value`, found `{line}`"),
                        )
                    })?;
                let key = unquote(line[..split].trim());
                let value = line[split + 1..].trim().to_owned();
                pairs.push((key, value, enabled));
            }
            Content::Pairs(pairs)
        };
        blocks.push(Block {
            name: name.to_owned(),
            line: start + 1,
            content,
        });
    }
    Ok(blocks)
}

fn dedent(line: &str) -> &str {
    line.strip_prefix("  ")
        .unwrap_or_else(|| line.strip_prefix(' ').unwrap_or(line))
}

fn unquote(key: &str) -> String {
    key.strip_prefix('"')
        .and_then(|k| k.strip_suffix('"'))
        .unwrap_or(key)
        .to_owned()
}

impl Block {
    /// The enabled pairs of a `key: value` block.
    pub fn pairs(&self) -> impl Iterator<Item = (&str, &str)> {
        let pairs = match &self.content {
            Content::Pairs(pairs) => pairs.as_slice(),
            _ => &[],
        };
        pairs
            .iter()
            .filter(|(_, _, enabled)| *enabled)
            .map(|(k, v, _)| (k.as_str(), v.as_str()))
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.pairs().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    pub fn text(&self) -> &str {
        match &self.content {
            Content::Text(text) => text,
            _ => "",
        }
    }
}

/// Writes blocks back out in the format [`parse`] reads.
pub fn write(blocks: &[Block]) -> String {
    let mut out = String::new();
    for (i, block) in blocks.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        match &block.content {
            Content::Pairs(pairs) => {
                out.push_str(&format!("{} {{\n", block.name));
                for (key, value, enabled) in pairs {
                    let off = if *enabled { "" } else { "~" };
                    let key = if key.contains(':')
                        || key.contains(char::is_whitespace)
                        || key.starts_with('~')
                    {
                        format!("\"{key}\"")
                    } else {
                        key.clone()
                    };
                    let sep = if value.is_empty() { ":" } else { ": " };
                    out.push_str(&format!("  {off}{key}{sep}{value}\n"));
                }
                out.push_str("}\n");
            }
            Content::List(items) => {
                out.push_str(&format!("{} [\n", block.name));
                for (j, item) in items.iter().enumerate() {
                    let comma = if j + 1 < items.len() { "," } else { "" };
                    out.push_str(&format!("  {item}{comma}\n"));
                }
                out.push_str("]\n");
            }
            Content::Text(text) => {
                out.push_str(&format!("{} {{\n", block.name));
                for line in text.lines() {
                    if line.is_empty() {
                        out.push('\n');
                    } else {
                        out.push_str(&format!("  {line}\n"));
                    }
                }
                out.push_str("}\n");
            }
        }
    }
    out
}

pub fn find<'a>(blocks: &'a [Block], name: &str) -> Option<&'a Block> {
    blocks.iter().find(|b| b.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pairs_text_and_lists() {
        let blocks = parse(
            "meta {\n  name: Log in\n  seq: 2\n}\n\nheaders {\n  Content-Type: application/json\n  ~X-Off: 1\n  \"a b\": c\n}\n\nbody:json {\n  {\n    \"user\": \"{{user}}\"\n  }\n}\n\nvars:secret [\n  password,\n  token\n]\n",
        )
        .unwrap();
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0].get("name"), Some("Log in"));
        assert_eq!(
            blocks[1].pairs().collect::<Vec<_>>(),
            [("Content-Type", "application/json"), ("a b", "c")]
        );
        assert_eq!(blocks[2].text(), "{\n  \"user\": \"{{user}}\"\n}");
        assert_eq!(
            blocks[3].content,
            Content::List(vec!["password".into(), "token".into()])
        );
    }

    #[test]
    fn keeps_closing_braces_inside_text() {
        let blocks = parse("script:pre-request {\n  if (a) {\n    b();\n  }\n}\n").unwrap();
        assert_eq!(blocks[0].text(), "if (a) {\n  b();\n}");
    }

    #[test]
    fn reports_where_it_went_wrong() {
        assert_eq!(parse("meta {\n  name: x\n").unwrap_err().0, 1);
        assert_eq!(parse("\nnot a block\n").unwrap_err().0, 2);
        assert_eq!(parse("headers {\n  nocolon\n}\n").unwrap_err().0, 2);
    }
}
