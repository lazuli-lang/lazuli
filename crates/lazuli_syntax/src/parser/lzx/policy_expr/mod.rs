//! Policy expression parsing — the cross-dialect bridge.
//!
//! ## What lives here
//!
//! - [`atom`] — [`parse_policy_atom`]: a single `@<namespace>.<name>` token
//!   with the closed namespace catalog (`scope` | `role` | `actor` | `mfa` |
//!   `session` | `rate_budget` | `time`).
//! - [`parser`] — the hand-rolled recursive-descent grammar
//!   (`or_expr := and_expr ("or" and_expr)*`, …).
//! - This module — [`try_parse_policy_expr`] and [`looks_like_policy_expr`]:
//!   the public entry points used by both the `.lzx` audience `requires`
//!   clause and every `.lzi` `policy <expr>` payload. `try_parse_policy_expr`
//!   returns `Ok(None)` on the back-compat raw-atom path so callers can
//!   keep the legacy single-string rendering.
//!
//! ## Why this is `pub(super)` and re-exported
//!
//! `.lzi` parsers in `crate::parser::lzi` resolve these helpers via
//! `super::lzx::try_parse_policy_expr`. The re-export in
//! `lzx/mod.rs` keeps that path stable across the R3-G split.

use crate::ast::PolicyExprAst;

use super::super::common::{SourceLine, line_error_owned};
use super::super::error::ParseError;

pub mod atom;
pub mod parser;

pub(in crate::parser) use atom::parse_policy_atom;

use parser::PolicyExprParser;

/// RB.S6 — recognize the new `has_role` / `has_permission` /
/// `authenticated` predicates within a `policy <expr>` payload. The
/// caller passes the raw payload (`rest.trim()` from `policy <rest>`);
/// the helper returns:
///
/// - `Ok(Some(expr))` when the payload is a structured expression
///   (contains `has_role` / `has_permission` / `authenticated` /
///   `and` / `or` / `not` / parens).
/// - `Ok(None)` when the payload is a bare legacy atom
///   (`@policy.<name>` / `@role.<name>` / etc.) — back-compat path,
///   caller keeps the raw string and skips the expression form.
/// - `Err(_)` when the payload looks expression-shaped but is
///   malformed (unknown predicate, bad permission ref, etc.).
pub(in crate::parser) fn try_parse_policy_expr(
    line: &SourceLine<'_>,
    payload: &str,
) -> Result<Option<PolicyExprAst>, ParseError> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    // Back-compat fast path: bare atom (no spaces, no parens, no keyword
    // boundaries). Examples: `@policy.create`, `@role.admin`,
    // `@scope.same_company`. The caller keeps the raw string for the
    // legacy single-atom rendering.
    if !looks_like_policy_expr(trimmed) {
        return Ok(None);
    }
    let mut parser = PolicyExprParser::new(trimmed, line);
    let expr = parser.parse_or()?;
    if !parser.is_at_end() {
        return Err(line_error_owned(
            line,
            format!(
                "unexpected trailing input in policy expression: `{}`",
                parser.remaining()
            ),
        ));
    }
    Ok(Some(expr))
}

/// Cheap surface heuristic: does the payload contain any of the closed
/// expression keywords or grouping punctuation?
pub(in crate::parser) fn looks_like_policy_expr(payload: &str) -> bool {
    if payload.contains('(') || payload.contains(')') {
        return true;
    }
    // Tokenize on whitespace; any token equal to a reserved keyword
    // qualifies as expression-shaped.
    for tok in payload.split_whitespace() {
        match tok {
            "authenticated" | "has_role" | "has_permission" | "and" | "or" | "not" => {
                return true;
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::super::super::common::SourceLine;
    use super::{looks_like_policy_expr, try_parse_policy_expr};
    use crate::ast::PolicyExprAst;

    fn line(text: &'static str) -> SourceLine<'static> {
        SourceLine {
            text,
            indent: 0,
            start: 0,
            end: text.len(),
        }
    }

    #[test]
    fn legacy_atom_falls_back_to_none() {
        let l = line("policy @policy.create");
        assert_eq!(
            try_parse_policy_expr(&l, "@policy.create").unwrap(),
            None,
            "bare @policy.* atom must remain raw-string back-compat"
        );
        assert!(!looks_like_policy_expr("@policy.create"));
        assert!(!looks_like_policy_expr("@role.admin"));
    }

    #[test]
    fn authenticated_alone_parses() {
        let l = line("policy authenticated");
        let expr = try_parse_policy_expr(&l, "authenticated").unwrap().unwrap();
        assert_eq!(expr, PolicyExprAst::Authenticated);
    }

    #[test]
    fn has_role_parses() {
        let l = line("policy has_role manager");
        let expr = try_parse_policy_expr(&l, "has_role manager")
            .unwrap()
            .unwrap();
        assert_eq!(expr, PolicyExprAst::HasRole("manager".into()));
    }

    #[test]
    fn has_permission_parses() {
        let l = line("policy has_permission queries:start");
        let expr = try_parse_policy_expr(&l, "has_permission queries:start")
            .unwrap()
            .unwrap();
        assert_eq!(expr, PolicyExprAst::HasPermission("queries:start".into()));
    }

    #[test]
    fn has_permission_three_segments_parses() {
        let l = line("policy has_permission report:repasse:mark");
        let expr = try_parse_policy_expr(&l, "has_permission report:repasse:mark")
            .unwrap()
            .unwrap();
        assert_eq!(
            expr,
            PolicyExprAst::HasPermission("report:repasse:mark".into())
        );
    }

    #[test]
    fn malformed_permission_ref_errors() {
        let l = line("policy has_permission users");
        let err = try_parse_policy_expr(&l, "has_permission users").unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("must be 2-4 colon-separated"),
            "expected segment-count error, got: {msg}"
        );
    }

    #[test]
    fn missing_has_role_arg_errors() {
        let l = line("policy has_role");
        let err = try_parse_policy_expr(&l, "has_role").unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("`has_role` requires an identifier"),
            "expected missing-ident error, got: {msg}"
        );
    }

    #[test]
    fn and_combinator_parses() {
        let l = line("policy authenticated and has_role manager");
        let expr = try_parse_policy_expr(&l, "authenticated and has_role manager")
            .unwrap()
            .unwrap();
        match expr {
            PolicyExprAst::And(terms) => {
                assert_eq!(terms.len(), 2);
                assert_eq!(terms[0], PolicyExprAst::Authenticated);
                assert_eq!(terms[1], PolicyExprAst::HasRole("manager".into()));
            }
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn or_combinator_parses() {
        let l = line("policy has_role manager or has_role admin");
        let expr = try_parse_policy_expr(&l, "has_role manager or has_role admin")
            .unwrap()
            .unwrap();
        match expr {
            PolicyExprAst::Or(terms) => {
                assert_eq!(terms.len(), 2);
                assert_eq!(terms[0], PolicyExprAst::HasRole("manager".into()));
                assert_eq!(terms[1], PolicyExprAst::HasRole("admin".into()));
            }
            other => panic!("expected Or, got {other:?}"),
        }
    }

    #[test]
    fn not_combinator_parses() {
        let l = line("policy not has_role viewer");
        let expr = try_parse_policy_expr(&l, "not has_role viewer")
            .unwrap()
            .unwrap();
        match expr {
            PolicyExprAst::Not(inner) => {
                assert_eq!(*inner, PolicyExprAst::HasRole("viewer".into()));
            }
            other => panic!("expected Not, got {other:?}"),
        }
    }

    #[test]
    fn parens_override_precedence() {
        // `and` binds tighter than `or`; parens force the alternative
        // grouping. We expect `Or([authenticated, And([X,Y])])` without
        // parens vs `And([Or([authenticated, X]), Y])` with parens.
        let l = line("policy");
        let raw = "(authenticated or has_role manager) and has_permission queries:start";
        let expr = try_parse_policy_expr(&l, raw).unwrap().unwrap();
        match expr {
            PolicyExprAst::And(terms) => {
                assert_eq!(terms.len(), 2);
                match &terms[0] {
                    PolicyExprAst::Or(_) => {}
                    other => panic!("expected Or under And, got {other:?}"),
                }
                assert_eq!(
                    terms[1],
                    PolicyExprAst::HasPermission("queries:start".into())
                );
            }
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn embedded_atom_parses() {
        // `has_role X or @actor.system` mixes a predicate with an atom.
        let l = line("policy");
        let expr = try_parse_policy_expr(&l, "has_role admin or @actor.system")
            .unwrap()
            .unwrap();
        match expr {
            PolicyExprAst::Or(terms) => {
                assert_eq!(terms.len(), 2);
                assert_eq!(terms[0], PolicyExprAst::HasRole("admin".into()));
                match &terms[1] {
                    PolicyExprAst::Atom(atom) => {
                        assert_eq!(atom.namespace, "actor");
                        assert_eq!(atom.name, "system");
                    }
                    other => panic!("expected Atom, got {other:?}"),
                }
            }
            other => panic!("expected Or, got {other:?}"),
        }
    }
}
