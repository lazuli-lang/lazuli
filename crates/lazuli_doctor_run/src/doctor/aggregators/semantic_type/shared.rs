//! Shared semantic-type catalog primitives — the closed catalog
//! constants, the `@semantic.<Name>` recognisers (`unknown_semantic_*`,
//! `is_known_semantic_type_name`), the diagnostic-pushers
//! (`push_unknown_semantic_type[_text]`), and the small line-anchor
//! helpers (`span_line_col`, `query_line_col`, `find_nested_type_site_line`).
//!
//! Each helper is read by both the syntax-level walker
//! (`syntax_feature.rs`) and the IR-level walker (`feature.rs`).
//!
//! Wave R7-3 extract.

use std::path::Path;

use crate::doctor::scanners::leading_spaces;
use crate::doctor::{DoctorDiagnostic, DoctorSeverity, line_col_for_offset};

pub(crate) const SEMANTIC_TYPE_UNKNOWN_CODE: &str = "semantic_type_unknown";

/// Render the closed semantic-type catalog for the diagnostic message,
/// DERIVED from the single source of truth `lazuli_keywords::SEMANTIC_TYPES`
/// (upper-cased to match the historical message style). Deriving — instead of
/// re-listing — is what stops the doctor's catalog from drifting away from the
/// parser/analyzer catalog (the DEFECT 2 bug: `PositiveDecimal` / `NonNegativeInt`
/// shipped in keywords + codegen + runtime but the doctor's hand-maintained
/// list never learned them, so `lazuli doctor` false-rejected valid programs).
fn semantic_type_catalog() -> String {
    lazuli_keywords::SEMANTIC_TYPES
        .iter()
        .map(|name| name.to_uppercase())
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn push_unknown_semantic_type(
    path: &Path,
    type_ref: &lazuli_ir::TypeRef,
    loc: (usize, usize),
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    if let Some(name) = unknown_semantic_type_name(type_ref) {
        diagnostics.push(DoctorDiagnostic {
            path: path.to_path_buf(),
            line: loc.0,
            column: loc.1,
            severity: DoctorSeverity::Error,
            code: SEMANTIC_TYPE_UNKNOWN_CODE.to_owned(),
            message: format!(
                "unknown @semantic type \"{name}\"; the closed catalog is {{{}}}.",
                semantic_type_catalog()
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
}

pub(crate) fn push_unknown_semantic_type_text(
    path: &Path,
    source: &str,
    type_text: &str,
    offset: usize,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    let loc = line_col_for_offset(source, offset);
    for name in unknown_semantic_type_names_in_text(type_text) {
        diagnostics.push(DoctorDiagnostic {
            path: path.to_path_buf(),
            line: loc.0,
            column: loc.1,
            severity: DoctorSeverity::Error,
            code: SEMANTIC_TYPE_UNKNOWN_CODE.to_owned(),
            message: format!(
                "unknown @semantic type \"{name}\"; the closed catalog is {{{}}}.",
                semantic_type_catalog()
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
}

pub(crate) fn unknown_semantic_type_name(type_ref: &lazuli_ir::TypeRef) -> Option<&str> {
    match type_ref {
        lazuli_ir::TypeRef::UserDefined(qname)
            if qname.name.starts_with("@semantic.")
                && !is_known_semantic_type_name(qname.name.as_str()) =>
        {
            Some(qname.name.as_str())
        }
        lazuli_ir::TypeRef::Many(inner) => unknown_semantic_type_name(inner),
        _ => None,
    }
}

pub(crate) fn unknown_semantic_type_names_in_text(type_text: &str) -> Vec<&str> {
    type_text
        .split(|ch: char| !(ch == '@' || ch == '.' || ch == '_' || ch.is_ascii_alphanumeric()))
        .filter(|token| token.starts_with("@semantic.") && !is_known_semantic_type_name(token))
        .collect()
}

pub(crate) fn is_known_semantic_type_name(name: &str) -> bool {
    let Some(short) = name.strip_prefix("@semantic.") else {
        return false;
    };
    // DERIVED from the single source of truth in `lazuli_keywords`. The doctor
    // no longer keeps a parallel hand-maintained list (the DEFECT 2 drift
    // source): adding a scalar to `SEMANTIC_TYPES` teaches the parser,
    // analyzer, codegen AND doctor at once. `is_semantic_type` also honours the
    // tolerated all-caps acronym aliases (`URL`/`UUID`) that codegen accepts.
    lazuli_keywords::is_semantic_type(short)
}

pub(crate) fn span_line_col(
    source: &str,
    span: Option<&lazuli_ir::SpanRef>,
) -> Option<(usize, usize)> {
    span.map(|span| line_col_for_offset(source, span.start))
}

pub(crate) fn query_line_col(source: &str, query: &lazuli_ir::Query) -> Option<(usize, usize)> {
    match query {
        lazuli_ir::Query::List(query) => span_line_col(source, query.span_ref.as_ref()),
        lazuli_ir::Query::Lookup(query) => span_line_col(source, query.span_ref.as_ref()),
        lazuli_ir::Query::Sql(query) => span_line_col(source, query.span_ref.as_ref()),
    }
}

pub(crate) fn find_nested_type_site_line(
    source: &str,
    parent_line: usize,
    site_name: &str,
) -> Option<(usize, usize)> {
    let lines: Vec<&str> = source.lines().collect();
    let parent_index = parent_line.checked_sub(1)?;
    let parent_indent = lines
        .get(parent_index)
        .map(|line| leading_spaces(line))
        .unwrap_or(0);
    let field_prefix = format!("{site_name}:");
    let route_prefix = format!("route {site_name}:");

    for (idx, line) in lines.iter().enumerate().skip(parent_index + 1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent <= parent_indent {
            break;
        }
        if trimmed.starts_with(&field_prefix) || trimmed.starts_with(&route_prefix) {
            let column = line
                .find(site_name)
                .map(|offset| offset + 1)
                .unwrap_or(indent + 1);
            return Some((idx + 1, column));
        }
    }

    None
}
