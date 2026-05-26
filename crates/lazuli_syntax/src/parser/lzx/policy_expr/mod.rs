//! Policy expression parsing — the cross-dialect bridge.
//!
//! ## What lives here
//!
//! - **`parse_policy_atom`**: a single `@<namespace>.<name>` token with
//!   the closed namespace catalog (`scope` | `role` | `actor` | `mfa` |
//!   `session` | `rate_budget` | `time`).
//! - **`try_parse_policy_expr`**: the public entry point used by both
//!   the `.lzx` audience `requires` clause and every `.lzi` `policy
//!   <expr>` payload. Returns `Ok(None)` on the back-compat raw-atom
//!   path so callers can keep the legacy single-string rendering.
//! - **`looks_like_policy_expr`**: cheap surface heuristic.
//! - **`PolicyExprParser`**: hand-rolled recursive-descent for the
//!   closed grammar (`or_expr := and_expr ("or" and_expr)*`, …).
//!
//! ## Why this is `pub(super)` and re-exported
//!
//! `.lzi` parsers in `crate::parser::lzi` resolve these helpers via
//! `super::lzx::try_parse_policy_expr`. The re-export in
//! `lzx/mod.rs` keeps that path stable across the R3-G split.

use crate::ast::{PolicyAtomAst, PolicyExprAst, Span};

use super::super::common::{SourceLine, is_kebab_or_snake_ident, line_error, line_error_owned};
use super::super::error::ParseError;

/// Parse a `@<namespace>.<name>` policy atom, with an optional raw
/// parenthesized argument suffix for step-up atoms such as
/// `@mfa.required(within:15m)`.
pub(in crate::parser) fn parse_policy_atom(
    line: &SourceLine<'_>,
    value: &str,
) -> Result<PolicyAtomAst, ParseError> {
    let atom = value.trim();
    let body = atom.strip_prefix('@').ok_or_else(|| {
        line_error(
            line,
            "policy atoms start with `@` (e.g. `@scope.workspace_admin`)",
        )
    })?;
    let (body, args) = if let Some((head, tail)) = body.split_once('(') {
        if !tail.ends_with(')') {
            return Err(line_error(
                line,
                "policy atom arguments must be closed with `)`",
            ));
        }
        let args = tail[..tail.len() - 1].trim();
        if args.is_empty() {
            return Err(line_error(line, "policy atom arguments cannot be empty"));
        }
        (head.trim(), Some(args.to_owned()))
    } else {
        (body.trim(), None)
    };
    let (namespace, name) = body.split_once('.').ok_or_else(|| {
        line_error(
            line,
            "policy atom must include a namespace and name (`@<ns>.<name>`)",
        )
    })?;
    if !matches!(
        namespace,
        "scope" | "role" | "actor" | "mfa" | "session" | "rate_budget" | "time"
    ) {
        return Err(line_error_owned(
            line,
            format!(
                "policy atom namespace `{}` is not in the closed catalog (`scope` | `role` | `actor` | `mfa` | `session` | `rate_budget` | `time`)",
                namespace
            ),
        ));
    }
    if !is_kebab_or_snake_ident(name) {
        return Err(line_error_owned(
            line,
            format!("policy atom name `{}` must be kebab/snake case", name),
        ));
    }
    Ok(PolicyAtomAst {
        namespace: namespace.to_owned(),
        name: name.to_owned(),
        args,
        span: Span::new(line.start, line.end),
    })
}

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

/// Hand-rolled recursive-descent parser for the closed policy
/// expression grammar:
///
/// ```text
/// or_expr   := and_expr ("or" and_expr)*
/// and_expr  := unary_expr ("and" unary_expr)*
/// unary_expr := "not" unary_expr | atom_expr
/// atom_expr := "(" or_expr ")"
///            | "authenticated"
///            | "has_role" <ident>
///            | "has_permission" <perm_ref>
///            | <policy_atom>     # @<ns>.<name>
/// ```
struct PolicyExprParser<'a, 'src> {
    input: &'a str,
    pos: usize,
    line: &'a SourceLine<'src>,
}

impl<'a, 'src> PolicyExprParser<'a, 'src> {
    fn new(input: &'a str, line: &'a SourceLine<'src>) -> Self {
        Self {
            input,
            pos: 0,
            line,
        }
    }

    fn is_at_end(&self) -> bool {
        self.skip_ws_peek();
        self.pos >= self.input.len()
    }

    fn remaining(&self) -> &str {
        &self.input[self.pos..]
    }

    fn skip_ws_peek(&self) -> usize {
        let bytes = self.input.as_bytes();
        let mut p = self.pos;
        while p < bytes.len() && bytes[p].is_ascii_whitespace() {
            p += 1;
        }
        p
    }

    fn skip_ws(&mut self) {
        self.pos = self.skip_ws_peek();
    }

    /// Consume the literal `kw` if it appears next (followed by
    /// whitespace, `(`, or end). Returns true on success.
    fn consume_keyword(&mut self, kw: &str) -> bool {
        self.skip_ws();
        let rest = &self.input[self.pos..];
        if !rest.starts_with(kw) {
            return false;
        }
        let after = &rest[kw.len()..];
        if !after.is_empty() {
            let c = after.as_bytes()[0];
            if !(c.is_ascii_whitespace() || c == b'(' || c == b')') {
                return false;
            }
        }
        self.pos += kw.len();
        true
    }

    fn consume_char(&mut self, c: char) -> bool {
        self.skip_ws();
        let rest = &self.input[self.pos..];
        if rest.starts_with(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    /// Read a bare ident token (lowercase + digits + `_`). Used for
    /// `has_role <ident>`.
    fn read_ident(&mut self) -> Option<String> {
        self.skip_ws();
        let bytes = self.input.as_bytes();
        let start = self.pos;
        while self.pos < bytes.len() {
            let c = bytes[self.pos];
            if c.is_ascii_lowercase() || c == b'_' || (self.pos > start && c.is_ascii_digit()) {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            None
        } else {
            Some(self.input[start..self.pos].to_owned())
        }
    }

    /// Read a permission ref: 2-4 colon-separated lowercase segments.
    /// Mirrors `parse_permission_decl` validation; centralised here so
    /// `has_permission` malformed args raise a parse error
    /// (RBAC-POLICY-PREDICATE-FORM-001 spec).
    fn read_permission_ref(&mut self) -> Option<String> {
        self.skip_ws();
        let bytes = self.input.as_bytes();
        let start = self.pos;
        while self.pos < bytes.len() {
            let c = bytes[self.pos];
            if c.is_ascii_lowercase()
                || c == b'_'
                || c == b':'
                || (self.pos > start && c.is_ascii_digit())
            {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            None
        } else {
            Some(self.input[start..self.pos].to_owned())
        }
    }

    /// Read a `@<ns>.<name>` atom token, including one optional
    /// parenthesized argument suffix.
    fn read_atom_token(&mut self) -> Option<&str> {
        self.skip_ws();
        let bytes = self.input.as_bytes();
        if self.pos >= bytes.len() || bytes[self.pos] != b'@' {
            return None;
        }
        let start = self.pos;
        self.pos += 1;
        while self.pos < bytes.len() {
            let c = bytes[self.pos];
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-' || c == b'.' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start + 1 {
            // Just `@` with nothing after.
            self.pos = start;
            return None;
        }
        if self.pos < bytes.len() && bytes[self.pos] == b'(' {
            self.pos += 1;
            while self.pos < bytes.len() && bytes[self.pos] != b')' {
                self.pos += 1;
            }
            if self.pos < bytes.len() && bytes[self.pos] == b')' {
                self.pos += 1;
            }
        }
        Some(&self.input[start..self.pos])
    }

    fn parse_or(&mut self) -> Result<PolicyExprAst, ParseError> {
        let mut terms = vec![self.parse_and()?];
        while self.consume_keyword("or") {
            terms.push(self.parse_and()?);
        }
        Ok(if terms.len() == 1 {
            terms.into_iter().next().unwrap()
        } else {
            PolicyExprAst::Or(terms)
        })
    }

    fn parse_and(&mut self) -> Result<PolicyExprAst, ParseError> {
        let mut terms = vec![self.parse_unary()?];
        while self.consume_keyword("and") {
            terms.push(self.parse_unary()?);
        }
        Ok(if terms.len() == 1 {
            terms.into_iter().next().unwrap()
        } else {
            PolicyExprAst::And(terms)
        })
    }

    fn parse_unary(&mut self) -> Result<PolicyExprAst, ParseError> {
        if self.consume_keyword("not") {
            let inner = self.parse_unary()?;
            return Ok(PolicyExprAst::Not(Box::new(inner)));
        }
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> Result<PolicyExprAst, ParseError> {
        self.skip_ws();
        if self.consume_char('(') {
            let inner = self.parse_or()?;
            if !self.consume_char(')') {
                return Err(line_error(
                    self.line,
                    "unbalanced parens in policy expression (expected `)`)",
                ));
            }
            return Ok(inner);
        }
        if self.consume_keyword("authenticated") {
            return Ok(PolicyExprAst::Authenticated);
        }
        if self.consume_keyword("has_role") {
            let name = self.read_ident().ok_or_else(|| {
                line_error(
                    self.line,
                    "`has_role` requires an identifier (e.g. `has_role manager`)",
                )
            })?;
            return Ok(PolicyExprAst::HasRole(name));
        }
        if self.consume_keyword("has_permission") {
            let perm = self.read_permission_ref().ok_or_else(|| {
                line_error(
                    self.line,
                    "`has_permission` requires a permission ref (e.g. `has_permission users:read`)",
                )
            })?;
            // Validate shape: 2-4 colon-separated lowercase segments,
            // each non-empty. Mirrors the RBAC catalog grammar.
            if !is_valid_permission_ref(&perm) {
                return Err(line_error_owned(
                    self.line,
                    format!(
                        "`has_permission` argument `{}` must be 2-4 colon-separated lowercase segments",
                        perm
                    ),
                ));
            }
            return Ok(PolicyExprAst::HasPermission(perm));
        }
        if let Some(tok) = self.read_atom_token() {
            // Re-parse via parse_policy_atom to enforce the closed
            // namespace catalog. `tok` includes the leading `@`.
            let owned = tok.to_owned();
            let atom = parse_policy_atom(self.line, &owned)?;
            return Ok(PolicyExprAst::Atom(atom));
        }
        Err(line_error_owned(
            self.line,
            format!(
                "expected `authenticated`, `has_role`, `has_permission`, `not`, `(`, or `@<ns>.<name>` in policy expression; found `{}`",
                self.remaining()
            ),
        ))
    }
}

/// Permission ref shape: 2-4 colon-separated lowercase segments, each
/// non-empty, alphanumeric + `_`, first char lowercase. Mirrors the
/// `permission <ref>` catalog grammar (`parse_permission_decl`).
fn is_valid_permission_ref(s: &str) -> bool {
    let segments: Vec<&str> = s.split(':').collect();
    if segments.len() < 2 || segments.len() > 4 {
        return false;
    }
    for seg in segments {
        if seg.is_empty() {
            return false;
        }
        let mut chars = seg.chars();
        let first = chars.next().unwrap();
        if !first.is_ascii_lowercase() {
            return false;
        }
        for c in chars {
            if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
                return false;
            }
        }
    }
    true
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
