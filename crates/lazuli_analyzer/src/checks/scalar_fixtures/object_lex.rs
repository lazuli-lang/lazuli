//! Fixture-file JS lexer + key extractor, extracted from
//! `checks/scalar_fixtures.rs` (Rails-style R9).
//!
//! The semantic-fixtures check needs to inspect a plugin's `fixtures.ts`
//! file and recover the top-level keys of its `export const fixtures =
//! { ... }` declaration. We deliberately avoid pulling in a real JS
//! parser dependency: the analyzer stays self-contained. This module
//! owns the minimal lexer (string / comment / brace / bracket / paren
//! balanced walker) and the object-key extractor that drives it.
//!
//! Public surface is `pub(super)` — only the parent `scalar_fixtures`
//! module consumes these helpers.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const PACKAGE_JSON: &str = "package.json";

pub(super) fn load_package_json(plugin_root: &Path) -> Option<Value> {
    let raw = fs::read_to_string(plugin_root.join(PACKAGE_JSON)).ok()?;
    serde_json::from_str(&raw).ok()
}

pub(super) fn fixture_keys_for_plugin(
    plugin_root: &Path,
    fixtures_export: Option<&Value>,
) -> BTreeSet<String> {
    for candidate in fixture_source_candidates(plugin_root, fixtures_export) {
        let Ok(source) = fs::read_to_string(candidate) else {
            continue;
        };
        let keys = parse_fixtures_const_keys(&source);
        if !keys.is_empty() {
            return keys;
        }
    }
    BTreeSet::new()
}

fn fixture_source_candidates(plugin_root: &Path, fixtures_export: Option<&Value>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(value) = fixtures_export {
        let mut export_paths = Vec::new();
        collect_export_paths(value, &mut export_paths);
        for export_path in export_paths {
            if export_path.ends_with(".ts") && export_path.contains("fixtures") {
                candidates.push(plugin_root.join(export_path.trim_start_matches("./")));
            }
        }
    }

    candidates.push(plugin_root.join("fixtures.ts"));
    candidates.push(plugin_root.join("src").join("fixtures.ts"));
    dedupe_paths(candidates)
}

fn collect_export_paths(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(path) => out.push(path.clone()),
        Value::Array(values) => {
            for value in values {
                collect_export_paths(value, out);
            }
        }
        Value::Object(map) => {
            for value in map.values() {
                collect_export_paths(value, out);
            }
        }
        _ => {}
    }
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

fn parse_fixtures_const_keys(source: &str) -> BTreeSet<String> {
    let Some((body_start, body_end)) = find_fixtures_object_body(source) else {
        return BTreeSet::new();
    };
    parse_object_keys(&source[body_start..body_end])
}

fn find_fixtures_object_body(source: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    while let Some(relative) = source[offset..].find("fixtures") {
        let name_start = offset + relative;
        let name_end = name_start + "fixtures".len();
        if !identifier_boundary(source, name_start, name_end) {
            offset = name_end;
            continue;
        }
        if !line_before(source, name_start).contains("const") {
            offset = name_end;
            continue;
        }

        let after_name = &source[name_end..];
        let eq_relative = after_name.find('=')?;
        if after_name[..eq_relative].contains(';') {
            offset = name_end;
            continue;
        }
        let eq = name_end + eq_relative;
        let open_relative = source[eq + 1..].find('{')?;
        let open = eq + 1 + open_relative;
        let close = matching_brace(source, open)?;
        return Some((open + 1, close));
    }
    None
}

fn identifier_boundary(source: &str, start: usize, end: usize) -> bool {
    let before = start
        .checked_sub(1)
        .and_then(|idx| source.as_bytes().get(idx))
        .copied();
    let after = source.as_bytes().get(end).copied();
    !is_ident_byte(before) && !is_ident_byte(after)
}

fn is_ident_byte(byte: Option<u8>) -> bool {
    byte.map(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        .unwrap_or(false)
}

fn line_before(source: &str, offset: usize) -> &str {
    let start = source[..offset].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
    &source[start..offset]
}

fn matching_brace(source: &str, open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' | b'"' | b'`' => i = skip_string(bytes, i),
            b'/' if bytes.get(i + 1) == Some(&b'/') => i = skip_line_comment(bytes, i),
            b'/' if bytes.get(i + 1) == Some(&b'*') => i = skip_block_comment(bytes, i),
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i);
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

fn parse_object_keys(body: &str) -> BTreeSet<String> {
    let bytes = body.as_bytes();
    let mut keys = BTreeSet::new();
    let mut i = 0;

    while i < bytes.len() {
        i = skip_ws_and_comments(bytes, i);
        if i >= bytes.len() {
            break;
        }
        if bytes[i] == b',' {
            i += 1;
            continue;
        }
        if bytes.get(i..i + 3) == Some(b"...") {
            i = skip_value(bytes, i + 3);
            continue;
        }

        let Some((key, after_key)) = parse_key(bytes, i) else {
            i += 1;
            continue;
        };
        let after_key = skip_ws_and_comments(bytes, after_key);
        if bytes.get(after_key) == Some(&b':') {
            keys.insert(key);
            i = skip_value(bytes, after_key + 1);
        } else if after_key >= bytes.len() || bytes.get(after_key) == Some(&b',') {
            keys.insert(key);
            i = after_key.saturating_add(1);
        } else {
            i = after_key.saturating_add(1);
        }
    }

    keys
}

fn parse_key(bytes: &[u8], i: usize) -> Option<(String, usize)> {
    match bytes.get(i).copied()? {
        b'\'' | b'"' => parse_string_key(bytes, i),
        byte if byte == b'_' || byte.is_ascii_alphabetic() => {
            let mut end = i + 1;
            while bytes
                .get(end)
                .map(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                .unwrap_or(false)
            {
                end += 1;
            }
            Some((String::from_utf8_lossy(&bytes[i..end]).into_owned(), end))
        }
        _ => None,
    }
}

fn parse_string_key(bytes: &[u8], i: usize) -> Option<(String, usize)> {
    let quote = *bytes.get(i)?;
    let mut out = String::new();
    let mut j = i + 1;
    while j < bytes.len() {
        let byte = bytes[j];
        if byte == b'\\' {
            if let Some(next) = bytes.get(j + 1).copied() {
                out.push(next as char);
                j += 2;
                continue;
            }
            return None;
        }
        if byte == quote {
            return Some((out, j + 1));
        }
        out.push(byte as char);
        j += 1;
    }
    None
}

fn skip_ws_and_comments(bytes: &[u8], mut i: usize) -> usize {
    loop {
        while bytes
            .get(i)
            .map(|b| b.is_ascii_whitespace())
            .unwrap_or(false)
        {
            i += 1;
        }
        if bytes.get(i) == Some(&b'/') && bytes.get(i + 1) == Some(&b'/') {
            i = skip_line_comment(bytes, i);
            continue;
        }
        if bytes.get(i) == Some(&b'/') && bytes.get(i + 1) == Some(&b'*') {
            i = skip_block_comment(bytes, i);
            continue;
        }
        return i;
    }
}

fn skip_value(bytes: &[u8], mut i: usize) -> usize {
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' | b'"' | b'`' => i = skip_string(bytes, i),
            b'/' if bytes.get(i + 1) == Some(&b'/') => i = skip_line_comment(bytes, i),
            b'/' if bytes.get(i + 1) == Some(&b'*') => i = skip_block_comment(bytes, i),
            b'(' => {
                paren += 1;
                i += 1;
            }
            b')' => {
                paren = paren.saturating_sub(1);
                i += 1;
            }
            b'[' => {
                bracket += 1;
                i += 1;
            }
            b']' => {
                bracket = bracket.saturating_sub(1);
                i += 1;
            }
            b'{' => {
                brace += 1;
                i += 1;
            }
            b'}' => {
                if paren == 0 && bracket == 0 && brace == 0 {
                    return i;
                }
                brace = brace.saturating_sub(1);
                i += 1;
            }
            b',' if paren == 0 && bracket == 0 && brace == 0 => return i + 1,
            _ => i += 1,
        }
    }
    i
}

fn skip_string(bytes: &[u8], i: usize) -> usize {
    let quote = bytes[i];
    let mut j = i + 1;
    while j < bytes.len() {
        if bytes[j] == b'\\' {
            j = (j + 2).min(bytes.len());
            continue;
        }
        if bytes[j] == quote {
            return j + 1;
        }
        j += 1;
    }
    bytes.len()
}

fn skip_line_comment(bytes: &[u8], i: usize) -> usize {
    let mut j = i + 2;
    while j < bytes.len() && bytes[j] != b'\n' {
        j += 1;
    }
    j
}

fn skip_block_comment(bytes: &[u8], i: usize) -> usize {
    let mut j = i + 2;
    while j + 1 < bytes.len() {
        if bytes[j] == b'*' && bytes[j + 1] == b'/' {
            return j + 2;
        }
        j += 1;
    }
    bytes.len()
}
