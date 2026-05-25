//! Feature-body prelude parsers — the two cross-cutting line shapes
//! that live above the per-construct parsers in the feature walker:
//!
//! - `public contract <Symbol> as v<N>` — versioned export marker that
//!   prefaces the very next command/resource/query/record/enum.
//! - `uses <feature>[, <feature>]* [version v<N>]` — cross-feature
//!   contracts §5.4 (consumer-side imports).
//!
//! The `take_matching_public_contract` + `attach_public_contract_to_query`
//! pair are the lookahead helpers the walker uses to pin the pending
//! contract to the next declaration; they live here so all
//! `public contract` machinery stays in one place.

use super::super::common::{SourceLine, line_error, line_error_owned};
use super::super::error::ParseError;
use crate::ast::{PublicContractDeclAst, QueryDecl, Span, SqlQueryKind, UsesClauseAst};

/// Try to parse a feature-body line as `public contract <Symbol> as v<N>`.
/// Returns `Some((symbol, decl))` when the line matches; `None` otherwise.
/// Returns `Err` for malformed `public contract` lines.
pub(super) fn parse_public_contract_line(
    line: &SourceLine<'_>,
) -> Result<Option<(String, PublicContractDeclAst)>, ParseError> {
    let trimmed = line.text.trim_start();
    let Some(rest) = trimmed.strip_prefix("public contract ") else {
        return Ok(None);
    };

    let mut parts = rest.split_whitespace();
    let symbol = parts
        .next()
        .ok_or_else(|| line_error(line, "`public contract` requires a symbol name"))?
        .to_owned();
    if !is_public_contract_symbol(&symbol) {
        return Err(line_error(line, "`public contract` requires a symbol name"));
    }

    let as_kw = parts
        .next()
        .ok_or_else(|| line_error(line, "`public contract <X>` requires `as v<N>` suffix"))?;
    if as_kw != "as" {
        return Err(line_error(
            line,
            "`public contract <X>` requires `as v<N>` suffix",
        ));
    }

    let version_token = parts
        .next()
        .ok_or_else(|| line_error(line, "`public contract <X> as` requires a version `v<N>`"))?;
    let Some(version_digits) = version_token.strip_prefix('v') else {
        return Err(line_error(line, "version must start with `v`, e.g. `v1`"));
    };
    let version: u16 = version_digits
        .parse()
        .map_err(|_| line_error(line, "version must be a positive integer (u16)"))?;
    if version == 0 {
        return Err(line_error(line, "version must be a positive integer (u16)"));
    }
    if parts.next().is_some() {
        return Err(line_error(
            line,
            "`public contract <X> as v<N>` admits no trailing tokens",
        ));
    }

    Ok(Some((
        symbol,
        PublicContractDeclAst {
            version,
            span: Span::new(line.start, line.end),
        },
    )))
}

/// Parse the body of a `uses` line — comma-separated feature names with
/// an optional trailing `version v<N>` clause that applies to ALL entries
/// on the line. Returns one `UsesClauseAst` per imported feature.
///
/// Examples:
/// - `account` → `[{feature: "account", version: None}]`
/// - `org, user, billing` → 3 clauses, all `version: None`
/// - `account version v1` → `[{feature: "account", version: Some(1)}]`
/// - `account, billing version v2` → 2 clauses, BOTH at v2 (line-level pin)
pub(super) fn parse_uses_line(
    rest: &str,
    line: &SourceLine<'_>,
    line_span: Span,
) -> Result<Vec<UsesClauseAst>, ParseError> {
    let trimmed = rest.trim();
    if trimmed.is_empty() {
        return Err(line_error(
            line,
            "`uses` requires at least one feature name",
        ));
    }

    // Split into the feature-list portion and the optional `version v<N>`
    // suffix. Single-pass: find " version v" (whitespace-bounded keyword)
    // OR fall through with no pin.
    let (list_part, version) = match trimmed.find(" version ") {
        Some(idx) => {
            let list_part = &trimmed[..idx];
            let version_part = trimmed[idx + " version ".len()..].trim();
            let Some(digits) = version_part.strip_prefix('v') else {
                return Err(line_error(
                    line,
                    "`uses ... version v<N>` requires `v` prefix on version",
                ));
            };
            let version: u16 = digits
                .parse()
                .map_err(|_| line_error(line, "`uses ... version v<N>` requires a positive u16"))?;
            if version == 0 {
                return Err(line_error(
                    line,
                    "`uses ... version v<N>` requires a positive u16",
                ));
            }
            (list_part, Some(version))
        }
        None => (trimmed, None),
    };

    let mut clauses = Vec::new();
    for name in list_part.split(',') {
        let name = name.trim();
        if name.is_empty() {
            return Err(line_error(
                line,
                "`uses` list has an empty entry; check for trailing/duplicate commas",
            ));
        }
        // Feature names follow IDENT_LOWER convention; let the analyzer
        // enforce the lexical rule (it has the canonical regex). Here we
        // just confirm non-empty + no obvious whitespace inside.
        if name.chars().any(char::is_whitespace) {
            return Err(line_error(
                line,
                "feature names in `uses` list cannot contain whitespace; separate with commas",
            ));
        }
        clauses.push(UsesClauseAst {
            feature: name.to_owned(),
            version,
            span: line_span,
        });
    }
    Ok(clauses)
}

fn is_public_contract_symbol(symbol: &str) -> bool {
    !symbol.is_empty()
        && symbol.split('.').all(|part| {
            let mut chars = part.chars();
            matches!(chars.next(), Some(first) if first.is_ascii_alphabetic())
                && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        })
}

pub(super) fn take_matching_public_contract(
    line: &SourceLine<'_>,
    pending_contract: &mut Option<(String, PublicContractDeclAst)>,
    kind: &str,
    name: &str,
) -> Result<Option<PublicContractDeclAst>, ParseError> {
    let Some((symbol, contract)) = pending_contract.take() else {
        return Ok(None);
    };
    if symbol == name || symbol == format!("{kind}.{name}") {
        return Ok(Some(contract));
    }
    Err(line_error_owned(
        line,
        format!(
            "public contract `{symbol}` precedes a `{kind} {name}` declaration; the name must match the next symbol's name."
        ),
    ))
}

pub(super) fn attach_public_contract_to_query(
    line: &SourceLine<'_>,
    pending_contract: &mut Option<(String, PublicContractDeclAst)>,
    query: &mut QueryDecl,
) -> Result<(), ParseError> {
    match query {
        QueryDecl::List(q) => {
            q.public_contract =
                take_matching_public_contract(line, pending_contract, "query.list", &q.name)?;
        }
        QueryDecl::Lookup(q) => {
            q.public_contract =
                take_matching_public_contract(line, pending_contract, "query.lookup", &q.name)?;
        }
        QueryDecl::Sql(q) => {
            let kind = match q.kind {
                SqlQueryKind::Sql => "query.sql",
                SqlQueryKind::View => "query.view",
            };
            q.public_contract =
                take_matching_public_contract(line, pending_contract, kind, &q.name)?;
        }
    }
    Ok(())
}
