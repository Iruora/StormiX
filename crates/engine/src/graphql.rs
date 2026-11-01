//! GraphQL requests: the JSON payload sent for a step's `graphql` body.

use serde_json::{Map, Value};
use stormix_flow::graphql::{coerce, variable_types};
use stormix_flow::GraphqlBody;

use crate::Variables;

/// `{ query, variables, operationName }` with variables filled in. A
/// `{{name}}` in a variable's value is sent as the type the query declares
/// for that variable; variables kept as JSON text are filled in, then parsed.
pub(crate) fn payload(
    body: &GraphqlBody,
    vars: &Variables,
    missing: &mut Vec<String>,
) -> Result<Value, String> {
    let query = vars.render_collecting(&body.query, missing);
    let variables = match &body.variables {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => {
            let text = vars.render_collecting(text, missing);
            match serde_json::from_str::<Value>(&text) {
                Ok(Value::Null) => None,
                Ok(value @ Value::Object(_)) => Some(value),
                Ok(_) => return Err("the GraphQL variables are not a JSON object".to_owned()),
                Err(err) => {
                    return Err(format!(
                        "the GraphQL variables are not valid JSON once variables are filled in ({err}): {text}"
                    ))
                }
            }
        }
        Some(Value::Object(map)) => {
            let types = variable_types(&query);
            let filled = map
                .iter()
                .map(|(key, value)| {
                    let key = vars.render_collecting(key, missing);
                    let value = match (value, types.get(&key)) {
                        (Value::String(text), Some(ty)) if text.contains("{{") => {
                            coerce(vars.render_collecting(text, missing), ty)
                        }
                        _ => vars.render_json(value, missing),
                    };
                    (key, value)
                })
                .collect();
            Some(Value::Object(filled))
        }
        Some(other) => Some(vars.render_json(other, missing)),
    };
    let mut payload = Map::new();
    payload.insert("query".to_owned(), Value::String(query));
    if let Some(variables) = variables {
        payload.insert("variables".to_owned(), variables);
    }
    if let Some(name) = &body.operation_name {
        let name = vars.render_collecting(name, missing);
        if !name.trim().is_empty() {
            payload.insert("operationName".to_owned(), Value::String(name));
        }
    }
    Ok(Value::Object(payload))
}

/// The payload as scripts see it before variables are filled in. Variables
/// kept as JSON text are spliced in as written, as Postman and Bruno do.
pub(crate) fn unfilled_text(body: &GraphqlBody) -> String {
    let mut text = format!("{{\"query\":{}", Value::String(body.query.clone()));
    match &body.variables {
        None | Some(Value::Null) => {}
        Some(Value::String(vars)) => {
            text.push_str(",\"variables\":");
            text.push_str(vars);
        }
        Some(vars) => {
            text.push_str(",\"variables\":");
            text.push_str(&vars.to_string());
        }
    }
    if let Some(name) = &body.operation_name {
        text.push_str(",\"operationName\":");
        text.push_str(&Value::String(name.clone()).to_string());
    }
    text.push('}');
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn body(query: &str, variables: Value) -> GraphqlBody {
        GraphqlBody {
            query: query.to_owned(),
            variables: Some(variables),
            operation_name: None,
            allow_errors: false,
        }
    }

    fn vars() -> Variables {
        let mut vars = Variables::new();
        vars.extend([
            ("id", "42"),
            ("name", "Ada"),
            ("on", "true"),
            ("tags", r#"["a","b"]"#),
        ]);
        vars
    }

    #[test]
    fn templated_values_follow_the_declared_types() {
        let query = "query ($id: Int!, $name: String, $on: Boolean, $tags: [String!]) { x }";
        let body = body(
            query,
            json!({ "id": "{{id}}", "name": "{{name}}", "on": "{{on}}", "tags": "{{tags}}",
                    "literal": "42", "nested": { "n": "{{id}}" }, "count": 3 }),
        );
        let mut missing = Vec::new();
        let payload = payload(&body, &vars(), &mut missing).unwrap();
        assert_eq!(
            payload["variables"],
            json!({ "id": 42, "name": "Ada", "on": true, "tags": ["a", "b"],
                    "literal": "42", "nested": { "n": "42" }, "count": 3 })
        );
        assert!(missing.is_empty());
    }

    #[test]
    fn variables_text_is_filled_in_then_parsed() {
        let body = body(
            "query { x }",
            json!(r#"{ "id": {{id}}, "name": "{{name}}" }"#),
        );
        let payload = payload(&body, &vars(), &mut Vec::new()).unwrap();
        assert_eq!(payload["variables"], json!({ "id": 42, "name": "Ada" }));

        let broken = super::payload(
            &self::body("query { x }", json!(r#"{ "id": {{nope}} }"#)),
            &vars(),
            &mut Vec::new(),
        );
        assert!(broken.unwrap_err().contains("not valid JSON"));
    }

    #[test]
    fn operation_name_is_sent_and_absent_variables_are_left_out() {
        let body = GraphqlBody {
            query: "query A { a } query B { b }".into(),
            variables: None,
            operation_name: Some("B".into()),
            allow_errors: false,
        };
        let payload = payload(&body, &vars(), &mut Vec::new()).unwrap();
        assert_eq!(
            payload,
            json!({ "query": "query A { a } query B { b }", "operationName": "B" })
        );
        assert_eq!(
            unfilled_text(&body),
            r#"{"query":"query A { a } query B { b }","operationName":"B"}"#
        );
    }
}
