//! Tiny line-oriented parser for Go handler signatures.
//!
//! Extracts the first return type from `func Name(... ) (... )` declarations
//! — accepts single-line and multi-line signatures, named returns, and
//! parenthesized return groups. Used by the parent `returns_list_001` rule
//! to decide whether a `returns list <T>` handler returned an opaque type.

use super::GoReturnType;
use super::type_emit::exported_func_name;

pub(super) fn find_handler_return(source: &str, handler_name: &str) -> Option<GoReturnType> {
    let fn_name = exported_func_name(handler_name);
    let lines: Vec<&str> = source.lines().collect();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("func ") else {
            continue;
        };
        if !starts_with_func_name(rest.trim_start(), &fn_name) {
            continue;
        }

        let signature = collect_signature(&lines, idx);
        return parse_return_type(&signature, idx + 1);
    }

    None
}

pub(super) fn starts_with_func_name(rest: &str, fn_name: &str) -> bool {
    rest.strip_prefix(fn_name)
        .is_some_and(|after| after.trim_start().starts_with('('))
}

pub(super) fn collect_signature(lines: &[&str], start: usize) -> String {
    let mut out = String::new();
    for line in lines.iter().skip(start).take(12) {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
        if line.trim_end().ends_with('{') {
            break;
        }
    }
    out
}

pub(super) fn parse_return_type(signature: &str, start_line: usize) -> Option<GoReturnType> {
    let params_start = signature.find('(')?;
    let params_end = matching_delimiter(signature, params_start, '(', ')')?;
    let after_params_start = params_end + 1;
    let after_params = &signature[after_params_start..];
    let skipped = after_params.len() - after_params.trim_start().len();
    let return_start = after_params_start + skipped;
    let after = &signature[return_start..];

    if after.starts_with('(') {
        let group_end = matching_delimiter(signature, return_start, '(', ')')?;
        let returns = &signature[(return_start + 1)..group_end];
        let first = first_top_level_comma_part(returns)?;
        let local_start = returns.find(first).unwrap_or(0);
        let absolute = return_start + 1 + local_start;
        let (line, column) = line_col_for_offset(signature, start_line, absolute);
        return Some(GoReturnType {
            raw: first.trim().to_owned(),
            line,
            column,
        });
    }

    let end = after
        .find(|ch: char| ch == '{' || ch.is_whitespace())
        .unwrap_or(after.len());
    let raw = after[..end].trim();
    if raw.is_empty() {
        return None;
    }
    let (line, column) = line_col_for_offset(signature, start_line, return_start);
    Some(GoReturnType {
        raw: raw.to_owned(),
        line,
        column,
    })
}

pub(super) fn matching_delimiter(
    source: &str,
    open_at: usize,
    open: char,
    close: char,
) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, ch) in source.char_indices().skip_while(|(idx, _)| *idx < open_at) {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

pub(super) fn first_top_level_comma_part(source: &str) -> Option<&str> {
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;

    for (idx, ch) in source.char_indices() {
        match ch {
            '(' => paren += 1,
            ')' => paren = paren.saturating_sub(1),
            '[' => bracket += 1,
            ']' => bracket = bracket.saturating_sub(1),
            '{' => brace += 1,
            '}' => brace = brace.saturating_sub(1),
            ',' if paren == 0 && bracket == 0 && brace == 0 => {
                let first = source[..idx].trim();
                return (!first.is_empty()).then_some(first);
            }
            _ => {}
        }
    }

    let trimmed = source.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

pub(super) fn line_col_for_offset(
    source: &str,
    start_line: usize,
    offset: usize,
) -> (usize, usize) {
    let mut line = start_line;
    let mut column = 1;
    for (idx, ch) in source.char_indices() {
        if idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

pub(super) fn is_opaque_json_return(raw: &str) -> bool {
    let compact: String = raw.chars().filter(|ch| !ch.is_whitespace()).collect();
    matches!(
        compact.as_str(),
        "interface{}" | "any" | "lazuli.JSON" | "[]byte"
    )
}

pub(super) fn strip_named_return(raw: &str) -> String {
    let trimmed = raw.trim();
    let mut parts = trimmed.split_whitespace();
    let Some(first) = parts.next() else {
        return String::new();
    };
    let rest: Vec<&str> = parts.collect();
    if rest.is_empty() {
        return trimmed.to_owned();
    }
    if is_go_identifier(first) && !is_known_go_type_start(first) {
        return rest.join(" ");
    }
    trimmed.to_owned()
}

pub(super) fn is_go_identifier(raw: &str) -> bool {
    let mut chars = raw.chars();
    matches!(chars.next(), Some(ch) if ch == '_' || ch.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

pub(super) fn is_known_go_type_start(raw: &str) -> bool {
    matches!(
        raw,
        "interface"
            | "any"
            | "map"
            | "chan"
            | "func"
            | "error"
            | "string"
            | "bool"
            | "int"
            | "int64"
            | "float64"
            | "struct"
    ) || raw.starts_with("[]")
        || raw.contains('.')
}

pub(super) fn returns_list_location(source: &str, header_line: usize) -> Option<(usize, usize)> {
    let lines: Vec<&str> = source.lines().collect();
    let start = header_line.saturating_sub(1);
    let mut end = start + 1;
    while end < lines.len() && super::super::leading_spaces(lines[end]) > 2 {
        end += 1;
    }
    for (idx, line) in lines.iter().enumerate().take(end).skip(start + 1) {
        let trimmed = line.trim_start();
        if trimmed.starts_with("returns list ") || trimmed.starts_with("returns list\t") {
            let column = line.find("returns").map(|col| col + 1).unwrap_or(1);
            return Some((idx + 1, column));
        }
    }
    None
}
