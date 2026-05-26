//! Recursive-descent parser for the closed policy-expression grammar.
//!
//! Grammar:
//!
//! ```text
//! or_expr   := and_expr ("or" and_expr)*
//! and_expr  := unary_expr ("and" unary_expr)*
//! unary_expr := "not" unary_expr | atom_expr
//! atom_expr := "(" or_expr ")"
//!            | "authenticated"
//!            | "has_role" <ident>
//!            | "has_permission" <perm_ref>
//!            | <policy_atom>     # @<ns>.<name>
//! ```
//!
//! Hand-rolled because the surface is small and closed; pulling in a
//! parser-combinator crate would gold-plate a leaf module the rest of
//! the language never reaches.

use crate::ast::PolicyExprAst;

use super::super::super::common::{SourceLine, line_error, line_error_owned};
use super::super::super::error::ParseError;
use super::atom::parse_policy_atom;

pub(super) struct PolicyExprParser<'a, 'src> {
    input: &'a str,
    pos: usize,
    line: &'a SourceLine<'src>,
}

impl<'a, 'src> PolicyExprParser<'a, 'src> {
    pub(super) fn new(input: &'a str, line: &'a SourceLine<'src>) -> Self {
        Self {
            input,
            pos: 0,
            line,
        }
    }

    pub(super) fn is_at_end(&self) -> bool {
        let bytes = self.input.as_bytes();
        let mut p = self.pos;
        while p < bytes.len() && bytes[p].is_ascii_whitespace() {
            p += 1;
        }
        p >= self.input.len()
    }

    pub(super) fn remaining(&self) -> &str {
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

    pub(super) fn parse_or(&mut self) -> Result<PolicyExprAst, ParseError> {
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
