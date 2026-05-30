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
        QueryDecl::Compose(q) => {
            q.public_contract =
                take_matching_public_contract(line, pending_contract, "query.compose", &q.name)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod public_contract_tests {
    use super::super::parse_feature_skeletons;

    #[test]
    fn parse_public_contract_attaches_to_enum() {
        let source = r#"
feature account
  domain
    public contract Gender as v1
    enum Gender
      female = 1
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let contract = features[0].enums[0].public_contract.as_ref().unwrap();
        assert_eq!(contract.version, 1);
    }

    #[test]
    fn parse_public_contract_attaches_to_resource() {
        let source = r#"
feature account
  domain
    public contract User as v2
    resource User
      email: @semantic.Email required
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let contract = features[0].resources[0].public_contract.as_ref().unwrap();
        assert_eq!(contract.version, 2);
    }

    #[test]
    fn parse_public_contract_attaches_to_command() {
        let source = r#"
feature account
  public contract command.create_user as v3
  command create_user
    input
      email: Text required
    returns User
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let contract = features[0].commands[0].public_contract.as_ref().unwrap();
        assert_eq!(contract.version, 3);
    }

    #[test]
    fn parse_public_contract_attaches_to_record() {
        let source = r#"
feature account
  domain
    public contract Address as v4
    record Address
      line1: Text required
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let contract = features[0].records[0].public_contract.as_ref().unwrap();
        assert_eq!(contract.version, 4);
    }

    #[test]
    fn parse_public_contract_mismatched_name_errors() {
        let source = r#"
feature account
  domain
    public contract Gender as v1
    enum Status
      active = 1
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("public contract `Gender` precedes a `enum Status` declaration"),
            "got {message}"
        );
    }

    #[test]
    fn parse_public_contract_trailing_no_symbol_errors() {
        let source = r#"
feature account
  domain
    public contract Gender as v1
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("trailing `public contract` declaration"),
            "got {message}"
        );
    }

    #[test]
    fn parse_public_contract_identity_attaches_to_auth() {
        // Per docs/proposals/cross-feature-contracts.md §5.3 row 7 —
        // `public contract identity as v<N>` is a special singleton form
        // recognized inside the `auth` block (NOT at feature level).
        let source = r#"
feature account
  auth
    public contract identity as v1
    identity Customer.email
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let auth = features[0].auth.as_ref().expect("auth block");
        let contract = auth
            .identity
            .public_contract
            .as_ref()
            .expect("identity contract attached");
        assert_eq!(contract.version, 1);
        assert_eq!(auth.identity.field, "Customer.email");
    }

    #[test]
    fn parse_public_contract_identity_below_identity_errors() {
        // The contract MUST appear ABOVE the identity line; below is an
        // ordering error.
        let source = r#"
feature account
  auth
    identity Customer.email
    public contract identity as v1
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("ABOVE the `identity` line"),
            "got {message}"
        );
    }

    // -------------------------------------------------------------------------
    // Cross-feature contracts §5.4 — feature-level `uses` line parsing,
    // optionally with consumer-side `version v<N>` pin.
    // -------------------------------------------------------------------------

    #[test]
    fn parses_uses_single_feature_no_pin() {
        let source = r#"
feature billing
  uses account
"#;
        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].uses_clauses.len(), 1);
        assert_eq!(features[0].uses_clauses[0].feature, "account");
        assert_eq!(features[0].uses_clauses[0].version, None);
    }

    #[test]
    fn parses_uses_with_version_pin() {
        let source = r#"
feature billing
  uses account version v2
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let clauses = &features[0].uses_clauses;
        assert_eq!(clauses.len(), 1);
        assert_eq!(clauses[0].feature, "account");
        assert_eq!(clauses[0].version, Some(2));
    }

    #[test]
    fn parses_uses_comma_list_shares_line_level_pin() {
        // The trailing `version v<N>` applies to ALL entries on the line.
        let source = r#"
feature billing
  uses org, user, account version v1
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let clauses = &features[0].uses_clauses;
        assert_eq!(clauses.len(), 3);
        assert_eq!(clauses[0].feature, "org");
        assert_eq!(clauses[1].feature, "user");
        assert_eq!(clauses[2].feature, "account");
        for clause in clauses {
            assert_eq!(clause.version, Some(1));
        }
    }

    #[test]
    fn parses_multiple_uses_lines_independently() {
        // Each `uses` line carries its own pin (or none).
        let source = r#"
feature billing
  uses account version v1
  uses notifications
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let clauses = &features[0].uses_clauses;
        assert_eq!(clauses.len(), 2);
        assert_eq!(clauses[0].feature, "account");
        assert_eq!(clauses[0].version, Some(1));
        assert_eq!(clauses[1].feature, "notifications");
        assert_eq!(clauses[1].version, None);
    }

    #[test]
    fn parses_uses_empty_entry_errors() {
        let source = r#"
feature billing
  uses account, , billing
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(message.contains("empty entry"), "got {message}");
    }

    #[test]
    fn parses_uses_bad_version_errors() {
        let source = r#"
feature billing
  uses account version 1
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(message.contains("`v` prefix"), "got {message}");
    }

    #[test]
    fn parses_uses_zero_version_errors() {
        let source = r#"
feature billing
  uses account version v0
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(message.contains("positive u16"), "got {message}");
    }
}
