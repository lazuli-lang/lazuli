//! Tiny shared lexers for `.lzx` surfaces.
//!
//! Heavily consumed by other catalog modules (api, cache, profile,
//! workspace, external, app, …) — these ride the standard
//! `pub(crate) use diagnostics::lzx::*;` re-export so every existing
//! `crate::*` import keeps resolving.

use crate::is_identifier;

pub(crate) fn lzx_declared_path_params(path: &str) -> Vec<String> {
    let mut params = Vec::new();
    let bytes = path.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b':' {
            let start = index + 1;
            let mut end = start;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_'))
            {
                end += 1;
            }
            if end > start {
                params.push(path[start..end].to_owned());
            }
            index = end;
            continue;
        }

        if bytes[index] == b'[' {
            let start = index + 1;
            if let Some(close_offset) = path[start..].find(']') {
                let raw = &path[start..start + close_offset];
                let name = raw.trim_start_matches("...");
                if is_identifier(name) {
                    params.push(name.to_owned());
                }
                index = start + close_offset + 1;
                continue;
            }
        }

        index += 1;
    }

    params
}

pub(crate) fn unquote_lzx_literal(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
}

pub(crate) fn is_quoted_lzx_literal(value: &str) -> bool {
    value.starts_with('"') && value.ends_with('"') && value.len() >= 2
}

pub(crate) fn split_items(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(crate) fn route_slot_name(route: &str) -> Option<&str> {
    route
        .split_once(':')
        .map(|(name, _)| name.trim())
        .or_else(|| route.split_whitespace().next())
        .filter(|name| is_identifier(name))
}

pub(crate) fn lzx_route_references(source: &str) -> Vec<&str> {
    path_references(source, "route.")
}

pub(crate) fn path_references<'a>(source: &'a str, prefix: &str) -> Vec<&'a str> {
    let mut references = Vec::new();
    let mut rest = source;

    while let Some(start) = rest.find(prefix) {
        let after = &rest[start + prefix.len()..];
        let len = after
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            .count();
        if len > 0 {
            references.push(&after[..len]);
        }
        rest = &after[len..];
    }

    references
}
