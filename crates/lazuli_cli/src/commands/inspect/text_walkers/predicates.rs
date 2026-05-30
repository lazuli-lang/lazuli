//! Line-classifier predicates for transitions, view anchors,
//! tests, and inspect subjects.
//!
//! Each predicate inspects one trimmed line and tags its kind without
//! parsing the surrounding block — they're cheap branch helpers
//! consumed by the higher-level projection walkers when classifying
//! what subject a given line declares.
//!
//! `is_transition_line` / `transition_name` / `transition_requires`
//! cover the `name: from -> to requires @policy.x` shape used by
//! workflow blocks. `inspect_subject` returns the prefixed identifier
//! (`command.X`, `rule.X`, `view.@anchor.X`, `transition.X`) that
//! the inspector emits as the "subject" of an authored row.
//! `test_group` classifies a test assertion into `authz`,
//! `transition`, `predicate`, `anchor`, or `other`.

use super::blocks::command_name;

pub(in crate::commands::inspect) fn transition_name(trimmed_line: &str) -> Option<&str> {
    trimmed_line.split_once(':')?.0.split_whitespace().next()
}

pub(in crate::commands::inspect) fn is_transition_line(trimmed_line: &str) -> bool {
    let Some((left, right)) = trimmed_line.split_once(':') else {
        return false;
    };

    !left.trim().is_empty() && right.contains("->")
}

pub(in crate::commands::inspect) fn transition_requires(trimmed_line: &str) -> Option<String> {
    let (_, rhs) = trimmed_line.split_once(':')?;
    let (_, after_arrow) = rhs.trim().split_once("->")?;
    let mut tokens = after_arrow.split_whitespace();
    tokens.next()?;

    while let Some(token) = tokens.next() {
        if token == "requires" {
            return tokens.next().map(str::to_owned);
        }
    }

    None
}

pub(in crate::commands::inspect) fn inspect_subject(trimmed_line: &str) -> Option<String> {
    if let Some(name) = command_name(trimmed_line) {
        Some(format!("command.{name}"))
    } else if trimmed_line.starts_with("rule ") {
        Some(format!(
            "rule.{}",
            trimmed_line
                .trim_start_matches("rule ")
                .trim_matches('"')
                .to_owned()
        ))
    } else if view_anchor_line(trimmed_line) {
        trimmed_line
            .split(" id ")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .map(|anchor| format!("view.{anchor}"))
            .or_else(|| Some("view.anchor".to_owned()))
    } else if is_transition_line(trimmed_line) {
        transition_name(trimmed_line).map(|name| format!("transition.{name}"))
    } else {
        None
    }
}

pub(in crate::commands::inspect) fn view_anchor_line(trimmed_line: &str) -> bool {
    trimmed_line.starts_with("view ") && trimmed_line.contains(" id @anchor.")
}

pub(in crate::commands::inspect) fn test_group(assertion: &str) -> &'static str {
    if assertion.starts_with("permits @")
        || assertion.starts_with("forbids @")
        || assertion.contains(" as @")
    {
        "authz"
    } else if assertion.contains(" from ") {
        "transition"
    } else if assertion.contains(" when ") {
        "predicate"
    } else if assertion.starts_with("allows extension ")
        || assertion.starts_with("denies extension ")
    {
        "anchor"
    } else {
        "other"
    }
}
