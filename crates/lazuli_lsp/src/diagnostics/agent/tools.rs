//! Tool-reference shape check for `agent.tools` entries.
//!
//! Cross-feature reachability is doctor's job — this layer only catches
//! malformed shorthand (e.g. `query.list` with no name; `customer..by_id`).

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{is_lower_ident, leading_spaces, simple_canonical_diagnostic};

use super::iter_agent_blocks;

/// Reject tool entries whose *shape* is invalid. Cross-feature
/// reachability is doctor's job — this layer only catches malformed
/// shorthand (e.g. `query.list` with no name; `customer..by_id`).
pub(crate) fn agent_tools_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (_, body) in iter_agent_blocks(source) {
        let mut in_tools = false;
        for &line_index in &body {
            let raw = lines[line_index];
            let trimmed = raw.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let leading = leading_spaces(raw);
            if leading == 4 {
                in_tools = trimmed == "tools";
                continue;
            }
            if !in_tools {
                continue;
            }
            if leading != 6 {
                continue;
            }
            if let Some(message) = validate_tool_reference_shape(trimmed) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    raw,
                    DiagnosticSeverity::ERROR,
                    "agent_tools_diagnostics",
                    &message,
                ));
            }
        }
    }

    diagnostics
}

/// Validate one tool-entry source token. The closed shapes:
///   - `@tool.<seg>(.<seg>)*` — adapter tool
///   - `query.list.<name>` / `query.lookup.<name>` / `query.sql.<name>` /
///     `query.view.<name>`
///   - `query.<name>` (unspecified subkind — doctor narrows)
///   - `command.<name>` / `api.<name>`
///   - `<feature>.<above>` cross-feature prefix
pub(crate) fn validate_tool_reference_shape(text: &str) -> Option<String> {
    if text.split_whitespace().count() != 1 {
        return Some("each tool entry is a single qualified reference (one per line)".to_owned());
    }
    let token = text.trim();
    if token.contains("..") {
        return Some(format!("tool reference `{token}` has an empty segment"));
    }

    if let Some(rest) = token.strip_prefix("@tool.") {
        if rest.is_empty() {
            return Some("`@tool.` requires a name (e.g. `@tool.web_search`)".to_owned());
        }
        if rest.split('.').any(|seg| !is_lower_ident(seg)) {
            return Some(format!(
                "`@tool.<...>` segments must be lower_snake idents; got `{token}`"
            ));
        }
        return None;
    }

    let segments: Vec<&str> = token.split('.').collect();
    let valid_local = matches!(
        segments.as_slice(),
        ["query", "list", _name]
            | ["query", "lookup", _name]
            | ["query", "sql", _name]
            | ["query", "view", _name]
            | ["query", _name]
            | ["command", _name]
            | ["api", _name]
    );
    if valid_local {
        if segments.iter().any(|seg| !is_lower_ident(seg)) {
            return Some(format!(
                "tool reference `{token}` segments must be lower_snake idents"
            ));
        }
        return None;
    }

    let valid_cross = matches!(
        segments.as_slice(),
        [_feature, "query", "list", _name]
            | [_feature, "query", "lookup", _name]
            | [_feature, "query", "sql", _name]
            | [_feature, "query", "view", _name]
            | [_feature, "query", _name]
            | [_feature, "command", _name]
            | [_feature, "api", _name]
    );
    if valid_cross {
        if segments.iter().any(|seg| !is_lower_ident(seg)) {
            return Some(format!(
                "tool reference `{token}` segments must be lower_snake idents"
            ));
        }
        return None;
    }

    Some(format!(
        "tool reference `{token}` is not a recognised shape; expected `<feature>.<kind>.<name>`, `<kind>.<name>`, or `@tool.<dotted>` where kind is `query[.list|.lookup|.sql|.view]`, `command`, or `api`"
    ))
}
