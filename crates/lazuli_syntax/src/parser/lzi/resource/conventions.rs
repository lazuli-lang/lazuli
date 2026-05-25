//! Resource-level `conventions [<name>, ...]` slot parser.
//!
//! `conventions` is a closed catalog — today the catalog contains
//! exactly `crud` and `me`. Unknown identifiers raise the
//! `conventions_unknown` diagnostic with a nearest-match suggestion
//! (single-character Levenshtein, per `crud` / `me` proposals §4.3).
//!
//! ```text
//! resource Customer
//!   conventions [crud, me]
//! ```
//!
//! Closed-grammar rules:
//!
//! - The RHS must be a bracketed list (`[...]`). Bare identifiers are
//!   a parse error.
//! - Empty lists (`conventions []`) are rejected — authors omit the
//!   slot entirely if they have no convention to declare.
//! - Each identifier must be in the closed catalog. Duplicates are
//!   accepted at parse time; the analyzer deduplicates downstream.
//!
//! Visibility: `parse_resource_conventions_list` is `pub(super)` and
//! is the only entry point consumed by `resource/mod.rs`.

use super::super::super::common::{SourceLine, line_error, line_error_owned};
use super::super::super::error::ParseError;

use crate::ast::ResourceConventionAst;

pub(super) fn parse_resource_conventions_list(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<Vec<ResourceConventionAst>, ParseError> {
    let rest = rest.trim();
    let inner = rest
        .strip_prefix('[')
        .ok_or_else(|| {
            line_error(
                line,
                "`conventions` requires a bracketed identifier list: `conventions [crud]`",
            )
        })?
        .strip_suffix(']')
        .ok_or_else(|| line_error(line, "`conventions [<name>, ...]` must close with `]`"))?;
    let inner_trimmed = inner.trim();
    if inner_trimmed.is_empty() {
        return Err(line_error(
            line,
            "`conventions []` is not allowed — list at least one convention or omit the slot entirely",
        ));
    }
    let mut entries: Vec<ResourceConventionAst> = Vec::new();
    for raw in inner_trimmed.split(',') {
        let ident = raw.trim();
        if ident.is_empty() {
            return Err(line_error(
                line,
                "`conventions [...]` entries must be non-empty identifiers separated by commas",
            ));
        }
        match resource_convention_ident(ident) {
            Some(c) => entries.push(c),
            None => {
                let suggestion = nearest_resource_convention(ident);
                let msg = match suggestion {
                    Some(s) => format!(
                        "conventions_unknown: `{}` is not in the closed catalog. did you mean `{}`?",
                        ident, s,
                    ),
                    None => format!(
                        "conventions_unknown: `{}` is not in the closed catalog (known: `crud`)",
                        ident,
                    ),
                };
                return Err(line_error_owned(line, msg));
            }
        }
    }
    Ok(entries)
}

/// Map a parsed identifier to the closed catalog of resource-level
/// conventions. Returns `None` for any unknown identifier — the caller
/// raises `conventions_unknown` with a nearest-match suggestion.
fn resource_convention_ident(ident: &str) -> Option<ResourceConventionAst> {
    match ident {
        "crud" => Some(ResourceConventionAst::Crud),
        "me" => Some(ResourceConventionAst::Me),
        _ => None,
    }
}

/// Suggest the nearest closed-catalog convention identifier for an
/// unknown token. Single-character Levenshtein per crud §4.3 / me
/// §4.3 — returns the closest match within edit-distance 1.
fn nearest_resource_convention(ident: &str) -> Option<&'static str> {
    const CATALOG: &[&str] = &["crud", "me"];
    let mut best: Option<(&'static str, usize)> = None;
    for candidate in CATALOG {
        let d = levenshtein_distance(ident, candidate);
        if d <= 1 && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((candidate, d));
        }
    }
    best.map(|(s, _)| s)
}

/// Minimal Levenshtein distance used by `nearest_resource_convention`.
/// Lives next to its single caller and avoids a new dependency. Inputs
/// are short identifiers, so the dynamic-programming table is trivially
/// sized.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let (n, m) = (a_chars.len(), b_chars.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr: Vec<usize> = vec![0; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}
