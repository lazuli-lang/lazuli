//! Profile-specific line-level parsers for `parse_app_profiles`.

pub(super) fn profile_child(trimmed: &str) -> Option<&'static str> {
    match trimmed.split_whitespace().next()? {
        "urls" => Some("urls"),
        "bindings" => Some("bindings"),
        "integrations" => Some("integrations"),
        "deploy" => Some("deploy"),
        _ => None,
    }
}
