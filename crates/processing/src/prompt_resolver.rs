//! Placeholder substitution for user-editable prompt templates.
//!
//! Replaces `{key}` tokens in a template with values from a placeholder map.
//! Unknown tokens pass through unchanged so typos remain visible to the user.

use std::collections::HashMap;

/// Substitute `{key}` tokens in `template` with values from `placeholders`.
///
/// Values must not contain `{...}` substrings matching other keys in the map —
/// iteration order is non-deterministic, so such collisions yield unstable output.
pub fn resolve_prompt(template: &str, placeholders: &HashMap<&str, String>) -> String {
    let mut out = template.to_string();
    for (key, value) in placeholders {
        let token = format!("{{{}}}", key);
        out = out.replace(&token, value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_placeholders_returns_template_unchanged() {
        let tmpl = "Hello {name}, you are {age}.";
        let result = resolve_prompt(tmpl, &HashMap::new());
        assert_eq!(result, "Hello {name}, you are {age}.");
    }

    #[test]
    fn substitutes_all_known_placeholders() {
        let tmpl = "Referral to {recipient_type} with {urgency} urgency.";
        let mut map = HashMap::new();
        map.insert("recipient_type", "Cardiologist".into());
        map.insert("urgency", "routine".into());
        let result = resolve_prompt(tmpl, &map);
        assert_eq!(result, "Referral to Cardiologist with routine urgency.");
    }

    #[test]
    fn unknown_placeholder_passes_through() {
        let tmpl = "Hello {name}, {missing_token} should stay.";
        let mut map = HashMap::new();
        map.insert("name", "Alice".into());
        let result = resolve_prompt(tmpl, &map);
        assert_eq!(result, "Hello Alice, {missing_token} should stay.");
    }

    #[test]
    fn literal_braces_without_known_keys_stay() {
        let tmpl = "Use { like this } is fine.";
        let mut map = HashMap::new();
        map.insert("name", "Alice".into());
        let result = resolve_prompt(tmpl, &map);
        assert_eq!(result, "Use { like this } is fine.");
    }

    #[test]
    fn same_placeholder_replaced_multiple_times() {
        let tmpl = "{name} went to see {name}.";
        let mut map = HashMap::new();
        map.insert("name", "Bob".into());
        let result = resolve_prompt(tmpl, &map);
        assert_eq!(result, "Bob went to see Bob.");
    }

    #[test]
    fn empty_value_substituted_correctly() {
        let tmpl = "Start\n{optional_line}\nEnd";
        let mut map = HashMap::new();
        map.insert("optional_line", "".into());
        let result = resolve_prompt(tmpl, &map);
        assert_eq!(result, "Start\n\nEnd");
    }
}
