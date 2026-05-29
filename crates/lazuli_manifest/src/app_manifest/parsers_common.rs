//! Leaf-level recognizers and span helpers shared by every
//! `app_manifest` parser. Lives separately from concern-specific
//! parser groups so each can stay small.

use lazuli_ir::SpanRef;

pub(super) fn parse_quoted_prefix(value: &str) -> Option<(String, &str)> {
    let rest = value.strip_prefix('"')?;
    let end = rest.find('"')?;
    let quoted = &rest[..end];
    let tail = rest[end + 1..].trim();
    Some((quoted.to_owned(), tail))
}

pub(super) fn named_block_name<'a>(trimmed: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = trimmed.strip_prefix(keyword)?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let name = rest.split_whitespace().next()?;
    is_identifier(name).then_some(name)
}

pub(super) fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

pub(super) fn used_feature_name(trimmed: &str) -> Option<&str> {
    if trimmed.starts_with("feature ") {
        return trimmed.split_whitespace().nth(1);
    }
    trimmed
        .split(',')
        .next()
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

pub(super) fn split_items(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(super) fn parse_integration_header(trimmed: &str) -> Option<(String, String)> {
    let (name, kind) = trimmed.split_once(':')?;
    let name = name.trim();
    let kind = kind.trim();
    if is_identifier(name) && is_type_name(kind) {
        Some((name.to_owned(), kind.to_owned()))
    } else {
        None
    }
}

pub(super) fn parse_credential_binding(trimmed: &str) -> Option<(String, String)> {
    let mut parts = trimmed.split_whitespace();
    let name = parts.next()?;
    let source = parts.collect::<Vec<_>>().join(" ");
    if is_identifier(name) && !source.is_empty() {
        Some((name.to_owned(), source))
    } else {
        None
    }
}

pub(super) fn parse_route_guard_redirect(value: &str) -> Option<String> {
    let target = value.strip_prefix("redirect ")?.trim();
    Some(unquote(target).to_owned())
}

pub(super) fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

pub(super) fn line_start_offsets(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

pub(super) fn line_span_ref(line_starts: &[usize], line_index: usize, line: &str) -> SpanRef {
    let start = line_starts.get(line_index).copied().unwrap_or_default();
    SpanRef {
        start,
        end: start + line.len(),
    }
}

pub(super) fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

pub(super) fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub(super) fn is_type_name(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_uppercase())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}
