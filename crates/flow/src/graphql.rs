//! GraphQL helpers shared by the runner, the validator and the importers.

use std::collections::BTreeMap;

use serde_json::Value;

/// How a query declares a variable: `$ids: [ID!]!` is named `ID`, a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableType {
    /// The innermost named type, like `Int` or `UserInput`.
    pub name: String,
    pub list: bool,
}

/// The variables a GraphQL document declares, by name without the `$`.
/// When several operations declare the same name, the first one wins.
/// A document that does not parse gives whatever was found before the
/// problem; the server reports the problem itself.
pub fn variable_types(query: &str) -> BTreeMap<String, VariableType> {
    let tokens = tokens(query);
    let mut types = BTreeMap::new();
    let mut i = 0;
    // `$name :` only occurs in variable definitions: where a variable is
    // used, as in `user(id: $id)`, it is followed by `,`, `)` or `}`.
    while i + 2 < tokens.len() {
        if let (Token::Punct('$'), Token::Name(name), Token::Punct(':')) =
            (&tokens[i], &tokens[i + 1], &tokens[i + 2])
        {
            let mut j = i + 3;
            let mut list = false;
            while let Some(Token::Punct('[')) = tokens.get(j) {
                list = true;
                j += 1;
            }
            if let Some(Token::Name(ty)) = tokens.get(j) {
                types.entry(name.clone()).or_insert(VariableType {
                    name: ty.clone(),
                    list,
                });
            }
            i = j;
        } else {
            i += 1;
        }
    }
    types
}

/// A templated variable value, once filled in, as the JSON its declared type
/// needs: `"42"` becomes `42` for an `Int`, `"true"` becomes `true` for a
/// `Boolean`, and JSON text becomes an object or list for an input type or a
/// list. Text that does not fit the type is sent as text, so the server's
/// own error says what was wrong.
pub fn coerce(text: String, ty: &VariableType) -> Value {
    if ty.list {
        if let Ok(value @ Value::Array(_)) = serde_json::from_str(&text) {
            return value;
        }
    }
    let trimmed = text.trim();
    match ty.name.as_str() {
        "String" | "ID" => Value::String(text),
        "Int" => match trimmed.parse::<i64>() {
            Ok(n) => n.into(),
            Err(_) => Value::String(text),
        },
        "Float" => match trimmed
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
        {
            Some(n) => Value::Number(n),
            None => Value::String(text),
        },
        "Boolean" => match trimmed {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => Value::String(text),
        },
        // An input object, an enum or a custom scalar. Only an object or
        // list is unambiguous; anything else stays text, which is also what
        // an enum value is sent as.
        _ => match serde_json::from_str(trimmed) {
            Ok(value @ (Value::Object(_) | Value::Array(_))) => value,
            _ => Value::String(text),
        },
    }
}

/// Whether JSON text with `{{variables}}` in it, as Postman and Bruno keep
/// GraphQL variables, is JSON once they are filled in. Each placeholder is
/// tried as `0`, which fits both `"{{name}}"` and a bare `{{name}}`.
pub fn check_variables_text(text: &str) -> Result<(), String> {
    let filled = crate::render(text, |_| Some("0".to_owned())).text;
    match serde_json::from_str::<Value>(&filled) {
        Ok(Value::Object(_) | Value::Null) => Ok(()),
        Ok(_) => Err("GraphQL variables must be a JSON object".to_owned()),
        Err(err) => Err(format!("GraphQL variables are not valid JSON: {err}")),
    }
}

/// GraphQL variables as Postman and Bruno export them: JSON text, possibly
/// with bare `{{variables}}` in it. Text that is JSON already becomes an
/// object; templated text stays text, filled in and parsed when sent.
/// Blank text means no variables.
pub fn variables_from_text(text: &str) -> Option<Value> {
    if text.trim().is_empty() {
        return None;
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Null) => None,
        Ok(value @ Value::Object(_)) => Some(value),
        _ => Some(Value::String(text.to_owned())),
    }
}

#[derive(Debug, PartialEq)]
enum Token {
    Punct(char),
    Name(String),
    /// Numbers, strings and anything else that only needs skipping.
    Other,
}

fn tokens(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '#' => {
                while i < chars.len() && chars[i] != '\n' && chars[i] != '\r' {
                    i += 1;
                }
            }
            '"' if chars.get(i + 1) == Some(&'"') && chars.get(i + 2) == Some(&'"') => {
                i += 3;
                while i < chars.len() {
                    if chars[i] == '\\' && chars[i + 1..].starts_with(&['"', '"', '"']) {
                        i += 4;
                    } else if chars[i..].starts_with(&['"', '"', '"']) {
                        i += 3;
                        break;
                    } else {
                        i += 1;
                    }
                }
                tokens.push(Token::Other);
            }
            '"' => {
                i += 1;
                while i < chars.len() && chars[i] != '"' && chars[i] != '\n' {
                    i += if chars[i] == '\\' { 2 } else { 1 };
                }
                i += 1;
                tokens.push(Token::Other);
            }
            c if c == '_' || c.is_ascii_alphabetic() => {
                let start = i;
                while i < chars.len() && (chars[i] == '_' || chars[i].is_ascii_alphanumeric()) {
                    i += 1;
                }
                tokens.push(Token::Name(chars[start..i].iter().collect()));
            }
            c if c == '-' || c.is_ascii_digit() => {
                i += 1;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '.' | '+' | '-'))
                {
                    i += 1;
                }
                tokens.push(Token::Other);
            }
            c if c.is_whitespace() || c == ',' || c == '\u{feff}' => i += 1,
            c => {
                tokens.push(Token::Punct(c));
                i += 1;
            }
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ty(name: &str, list: bool) -> VariableType {
        VariableType {
            name: name.to_owned(),
            list,
        }
    }

    #[test]
    fn finds_declared_variable_types() {
        let query = r#"
            # $ignored: Int in a comment
            query Users($id: Int!, $ids: [ID!]!, $filter: UserFilter = { name: "$x: Int" }) {
              user(id: $id) { name(format: "$y: Float") }
              users(ids: $ids, filter: $filter) { id }
            }
            mutation Rename($id: String, $on: Boolean) { rename(id: $id, on: $on) }
        "#;
        let types = variable_types(query);
        assert_eq!(
            types.into_iter().collect::<Vec<_>>(),
            [
                ("filter".to_owned(), ty("UserFilter", false)),
                ("id".to_owned(), ty("Int", false)),
                ("ids".to_owned(), ty("ID", true)),
                ("on".to_owned(), ty("Boolean", false)),
            ]
        );
    }

    #[test]
    fn coerces_filled_in_values_to_their_declared_type() {
        assert_eq!(coerce("42".into(), &ty("Int", false)), Value::from(42));
        assert_eq!(coerce("4x".into(), &ty("Int", false)), Value::from("4x"));
        assert_eq!(coerce("1.5".into(), &ty("Float", false)), Value::from(1.5));
        assert_eq!(
            coerce("true".into(), &ty("Boolean", false)),
            Value::from(true)
        );
        assert_eq!(coerce("42".into(), &ty("String", false)), Value::from("42"));
        assert_eq!(coerce("42".into(), &ty("ID", false)), Value::from("42"));
        assert_eq!(
            coerce("[1, 2]".into(), &ty("Int", true)),
            serde_json::json!([1, 2])
        );
        assert_eq!(coerce("7".into(), &ty("Int", true)), Value::from(7));
        assert_eq!(
            coerce(r#"{"name":"a"}"#.into(), &ty("UserInput", false)),
            serde_json::json!({ "name": "a" })
        );
        assert_eq!(
            coerce("ACTIVE".into(), &ty("Status", false)),
            Value::from("ACTIVE")
        );
        assert_eq!(coerce("42".into(), &ty("Long", false)), Value::from("42"));
    }

    #[test]
    fn variables_text_is_kept_as_text_only_when_templated() {
        assert_eq!(variables_from_text("  "), None);
        assert_eq!(
            variables_from_text(r#"{ "id": "{{id}}" }"#),
            Some(serde_json::json!({ "id": "{{id}}" }))
        );
        assert_eq!(
            variables_from_text(r#"{ "id": {{id}} }"#),
            Some(Value::from(r#"{ "id": {{id}} }"#))
        );
        assert!(check_variables_text(r#"{ "id": {{id}}, "name": "{{name}}" }"#).is_ok());
        assert!(check_variables_text(r#"{ "id": {{id}} "#).is_err());
        assert!(check_variables_text("[1]").is_err());
    }
}
