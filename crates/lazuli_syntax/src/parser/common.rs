//! Shared parser infrastructure.
//!
//! Every parser in this crate works on the same shape: a UTF-8 source
//! string is split into indent-aware `SourceLine`s and walked with a
//! cursor index. Helpers here own that contract — line indexing, trivia
//! skipping, span-anchored error construction, and depth-aware token
//! scanning that ignores brackets and quoted literals.
//!
//! These helpers are crate-private. Sub-modules (`lzi.rs`, `lzx.rs`,
//! the still-resident parsers in `mod.rs`) import them via
//! `use super::common::*;`. None of these names is part of the public
//! ABI — they are leaf-mechanics by design.

use crate::ast::Span;

use super::error::ParseError;

/// A single source line lifted out of the input. `indent` is the count
/// of leading ASCII spaces; `start` / `end` are byte offsets into the
/// original source so we can build authoring-aligned `Span`s.
#[derive(Debug)]
pub(crate) struct SourceLine<'a> {
    pub(crate) text: &'a str,
    pub(crate) indent: usize,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

/// Split `source` into one `SourceLine` per `\n`-delimited record.
/// Indent is measured in spaces; tabs are not normalized — the grammar
/// is space-only by convention.
pub(crate) fn source_lines(source: &str) -> Vec<SourceLine<'_>> {
    let mut offset = 0;
    let mut lines = Vec::new();

    for text in source.lines() {
        let start = offset;
        let end = start + text.len();
        lines.push(SourceLine {
            text,
            indent: text.bytes().take_while(|byte| *byte == b' ').count(),
            start,
            end,
        });
        offset = end + 1;
    }

    lines
}

/// A blank line or a `#`-prefixed comment. Trivia is skipped wholesale
/// by every loop in this crate.
pub(crate) fn is_trivia(trimmed: &str) -> bool {
    trimmed.is_empty() || trimmed.starts_with('#')
}

/// Strip a `# ...` inline comment from the tail of a source line,
/// honoring `"..."` and `'...'` quoted strings (a `#` inside a quoted
/// literal stays put). Returns the input unchanged when the line carries
/// no comment.
pub(crate) fn strip_inline_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match in_quote {
            Some(q) if b == q => in_quote = None,
            Some(_) if b == b'\\' && i + 1 < bytes.len() => i += 1,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => in_quote = Some(b),
            None if b == b'#' => return line[..i].trim_end(),
            None => {}
        }
        i += 1;
    }
    line
}

/// Span-anchored error with a `'static` message. Use when the message
/// is a compile-time literal (no user-token interpolation).
pub(crate) fn line_error(line: &SourceLine<'_>, message: &'static str) -> ParseError {
    ParseError::Pest {
        message: message.to_owned(),
        span: Span::new(line.start, line.end.max(line.start + 1)),
    }
}

/// Owned-message variant of `line_error`. Used by parsers that need to
/// interpolate user-supplied tokens into the diagnostic.
pub(crate) fn line_error_owned(line: &SourceLine<'_>, message: String) -> ParseError {
    ParseError::Pest {
        message,
        span: Span::new(line.start, line.end.max(line.start + 1)),
    }
}

/// Find `needle` at depth 0 (not inside parens / brackets). Quote
/// handling is naive — callers that need quote-awareness use
/// `find_top_level_token` instead.
pub(crate) fn find_token(text: &str, needle: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let needle_bytes = needle.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i + needle_bytes.len() <= bytes.len() {
        let ch = bytes[i] as char;
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            _ => {}
        }
        if depth == 0 && &bytes[i..i + needle_bytes.len()] == needle_bytes {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Find `needle` outside of quoted strings and outside parenthesized
/// regions. Mirrors the depth/quote-aware logic in `find_token` but also
/// suppresses matches inside `"..."` literals — design values often carry
/// `"#hex"` and `"cubic-bezier(...)"` strings that we never want to scan
/// into.
pub(crate) fn find_top_level_token(text: &str, needle: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let needle_bytes = needle.as_bytes();
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut i = 0;
    while i + needle_bytes.len() <= bytes.len() {
        let b = bytes[i];
        match b {
            b'"' => in_quote = !in_quote,
            b'(' | b'[' if !in_quote => depth += 1,
            b')' | b']' if !in_quote => depth -= 1,
            _ => {}
        }
        if !in_quote && depth == 0 && &bytes[i..i + needle_bytes.len()] == needle_bytes {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Parse the lowercase `true` / `false` literals used by `.lzx` flag
/// fields. Returns `None` for anything else so the caller can surface
/// a typed diagnostic.
pub(crate) fn parse_lzx_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Identifier check used across audience / view / cell / route names:
/// kebab-case (`workspace-admin`) and snake_case (`workspace_admin`)
/// both pass; anything else (PascalCase, spaces, leading digit, etc.)
/// rejects.
pub(crate) fn is_kebab_or_snake_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else { return false };
    if !first.is_ascii_lowercase() {
        return false;
    }
    for c in chars {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
            return false;
        }
    }
    true
}

/// `.lzx` bare identifier — starts with an ASCII letter and continues
/// with alphanumerics / `_`. Looser than `is_kebab_or_snake_ident`
/// because some LZX slots accept PascalCase as well as snake_case.
pub(crate) fn is_lzx_bare_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else { return false };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `.lzx` resume reference — single-segment `<view>` or two-segment
/// `<feature>.<view>` where each segment passes `is_lzx_bare_ident`.
pub(crate) fn is_lzx_resume_ref(s: &str) -> bool {
    let parts: Vec<_> = s.split('.').collect();
    matches!(parts.as_slice(), [name] if is_lzx_bare_ident(name))
        || matches!(parts.as_slice(), [feature, name] if is_lzx_bare_ident(feature) && is_lzx_bare_ident(name))
}

/// Split a comma-separated `.lzx` value list, trimming whitespace and
/// dropping empty entries. Used by audience / role / scope lists.
pub(crate) fn split_lzx_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Strip surrounding double quotes from an `.lzx` literal value.
/// Returns the input unchanged when no enclosing quotes are present.
pub(crate) fn unquote_lzx_value(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
}

/// Split an `.lzx` arrow expression (`lhs → rhs` or `lhs -> rhs`) into
/// its two sides. Accepts both the Unicode arrow and its ASCII fallback;
/// the earliest occurrence wins when both are present.
pub(crate) fn split_lzx_arrow(text: &str) -> Option<(&str, &str)> {
    let unicode = text.find('→').map(|idx| (idx, '→'.len_utf8()));
    let ascii = text.find("->").map(|idx| (idx, 2));
    let (idx, len) = match (unicode, ascii) {
        (Some(u), Some(a)) => {
            if u.0 <= a.0 {
                u
            } else {
                a
            }
        }
        (Some(u), None) => u,
        (None, Some(a)) => a,
        (None, None) => return None,
    };
    Some((&text[..idx], &text[idx + len..]))
}
