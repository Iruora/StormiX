//! Variables from `--var name=value` and `--env file`.

use std::path::Path;

use stormix_engine::Variables;

/// Parses `name=value`. The value may itself contain `=`.
pub fn parse_var(arg: &str) -> Result<(String, String), String> {
    match arg.split_once('=') {
        Some((name, value)) if !name.trim().is_empty() => {
            Ok((name.trim().to_owned(), value.to_owned()))
        }
        _ => Err(format!("expected name=value, got `{arg}`")),
    }
}

/// Reads an environment file: a flat YAML or JSON map of names to values,
/// a Postman environment export or a Bruno environment (`.bru`).
pub fn read_env(path: &Path) -> Result<Variables, String> {
    let env = stormix_collections::environment::read_file(path).map_err(|e| e.to_string())?;
    let mut vars = Variables::new();
    vars.extend(env.variables);
    Ok(vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vars() {
        assert_eq!(parse_var("a=b=c"), Ok(("a".into(), "b=c".into())));
        assert_eq!(parse_var("empty="), Ok(("empty".into(), String::new())));
        assert!(parse_var("novalue").is_err());
        assert!(parse_var("=x").is_err());
    }
}
