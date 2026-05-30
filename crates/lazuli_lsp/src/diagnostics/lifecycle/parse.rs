//! Generic text helpers shared across the lifecycle cluster.
//!
//! Identifier validation, header recognition, `uses` enumeration,
//! and the two slug/snake-case conversions used by the route-path
//! gate inference live here so every sub-module imports them
//! identically. Nothing here touches lifecycle semantics directly —
//! these are pure string operations.

use std::collections::HashSet;

use crate::leading_spaces;

pub(crate) fn lifecycle_top_level_named_header(trimmed: &str) -> Option<(&str, &str)> {
    for keyword in ["feature", "experience", "surface"] {
        if let Some(rest) = trimmed.strip_prefix(&format!("{keyword} ")) {
            let name = rest.split_whitespace().next().unwrap_or("");
            if !name.is_empty() {
                return Some((keyword, name));
            }
        }
    }
    None
}

pub(crate) fn lifecycle_uses_in_block(lines: &[&str], start: usize, end: usize) -> Vec<String> {
    let mut uses = Vec::new();
    let mut seen = HashSet::new();
    for idx in start..end {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed == "uses" {
            let indent = leading_spaces(line);
            for child in lines.iter().take(end).skip(idx + 1) {
                if child.trim_start().is_empty() || child.trim_start().starts_with('#') {
                    continue;
                }
                if leading_spaces(child) <= indent {
                    break;
                }
                let name = child.split_whitespace().next().unwrap_or("");
                if lifecycle_ident(name) && seen.insert(name.to_owned()) {
                    uses.push(name.to_owned());
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("uses ") {
            for name in lifecycle_parse_uses_rest(rest) {
                if seen.insert(name.clone()) {
                    uses.push(name);
                }
            }
        }
    }
    uses
}

pub(crate) fn lifecycle_parse_uses_rest(rest: &str) -> Vec<String> {
    let mut names = Vec::new();
    for token in rest.replace(',', " ").split_whitespace() {
        if token == "version" {
            break;
        }
        if matches!(token, "feature" | "experience") {
            continue;
        }
        if lifecycle_ident(token) {
            names.push(token.to_owned());
        }
    }
    names
}

pub(crate) fn lifecycle_ident(token: &str) -> bool {
    !token.is_empty()
        && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && token
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
}

pub(crate) fn slug_for_lifecycle_token(token: &str) -> String {
    let mut slug = String::new();
    for (idx, ch) in token.chars().enumerate() {
        if ch == '_' || ch == ' ' {
            slug.push('-');
        } else if ch.is_ascii_uppercase() {
            if idx > 0 {
                slug.push('-');
            }
            slug.push(ch.to_ascii_lowercase());
        } else {
            slug.push(ch.to_ascii_lowercase());
        }
    }
    slug
}

pub(crate) fn snake_case(token: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in token.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if idx > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == ' ' {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    out
}
