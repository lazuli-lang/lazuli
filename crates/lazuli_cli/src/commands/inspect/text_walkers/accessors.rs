//! Direct-child accessors and small expression helpers.
//!
//! `direct_child_value`, `direct_child_values`, `block_scalar_value`,
//! `block_prefixed_value`, and `block_has_exact_line` reach into a
//! block returned by `blocks::*` and pull out scalar values or
//! presence flags from the immediate children.
//!
//! The expression helpers (`strip_quotes`, `typed_declaration`,
//! `trailing_scalar_value_after`, `parse_event_list`,
//! `qualify_event_ref`, `emits_derived_effect`) parse single-line
//! syntactic patterns that recur across commands, queries, jobs and
//! webhooks — particularly the `emits …` and `name: Type` shapes.

use crate::commands::inspect::expand::leading_spaces;

pub(in crate::commands::inspect) fn direct_child_value(
    lines: &[String],
    prefix: &str,
) -> Option<String> {
    let child_indent = lines.first().map(|line| leading_spaces(line) + 2)?;

    lines.iter().find_map(|line| {
        if leading_spaces(line) == child_indent {
            line.trim_start().strip_prefix(prefix).map(str::to_owned)
        } else {
            None
        }
    })
}

pub(in crate::commands::inspect) fn direct_child_values(
    lines: &[String],
    prefix: &str,
) -> Vec<String> {
    let Some(child_indent) = lines.first().map(|line| leading_spaces(line) + 2) else {
        return Vec::new();
    };

    lines
        .iter()
        .filter_map(|line| {
            if leading_spaces(line) == child_indent {
                line.trim_start()
                    .strip_prefix(prefix)
                    .map(str::trim)
                    .map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

pub(in crate::commands::inspect) fn block_scalar_value<'a>(
    lines: &'a [String],
    keyword: &str,
) -> Option<&'a str> {
    lines.iter().skip(1).find_map(|line| {
        (leading_spaces(line) == 4)
            .then(|| line.trim_start().strip_prefix(keyword))
            .flatten()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

pub(in crate::commands::inspect) fn block_prefixed_value<'a>(
    lines: &'a [String],
    prefix: &str,
) -> Option<&'a str> {
    lines.iter().skip(1).find_map(|line| {
        (leading_spaces(line) == 4)
            .then(|| line.trim_start().strip_prefix(prefix))
            .flatten()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

pub(in crate::commands::inspect) fn block_has_exact_line(lines: &[String], expected: &str) -> bool {
    lines
        .iter()
        .skip(1)
        .any(|line| leading_spaces(line) == 4 && line.trim_start() == expected)
}

pub(in crate::commands::inspect) fn strip_quotes(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
        .to_owned()
}

pub(in crate::commands::inspect) fn typed_declaration(trimmed_line: &str) -> Option<(&str, &str)> {
    let (name, rest) = trimmed_line.split_once(':')?;
    let name = name.trim();
    let ty = rest.split_whitespace().next()?;

    if name.is_empty() || ty.is_empty() {
        None
    } else {
        Some((name, ty))
    }
}

pub(in crate::commands::inspect) fn trailing_scalar_value_after<'a>(
    trimmed_line: &'a str,
    keyword: &str,
) -> Option<&'a str> {
    let mut tokens = trimmed_line.split_whitespace();
    while let Some(token) = tokens.next() {
        if token == keyword {
            return tokens.next();
        }
    }
    None
}

pub(in crate::commands::inspect) fn parse_event_list(source: &str) -> Vec<String> {
    let first = source.split_whitespace().next().unwrap_or(source);
    first
        .split(',')
        .map(str::trim)
        .filter(|event| {
            !event.is_empty()
                && event
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.')
        })
        .map(str::to_owned)
        .collect()
}

pub(in crate::commands::inspect) fn qualify_event_ref(feature: &str, event: &str) -> String {
    if event.contains('.') {
        event.to_owned()
    } else {
        format!("{feature}.{event}")
    }
}

pub(in crate::commands::inspect) fn emits_derived_effect(emits_rest: &str) -> Option<&'static str> {
    let mut tokens = emits_rest.split_whitespace();
    tokens.next()?;
    if tokens.next()? != "from" {
        return None;
    }
    match tokens.next()? {
        "creates" => Some("creates"),
        "updates" => Some("updates"),
        "deletes" => Some("deletes"),
        _ => None,
    }
}
