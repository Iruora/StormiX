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

#[cfg(test)]
mod tests {
    use super::*;

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
