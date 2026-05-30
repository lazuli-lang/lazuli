//! `@<namespace>.<name>` policy atom — closed namespace catalog.

use crate::ast::{PolicyAtomAst, Span};

use super::super::super::common::{
    SourceLine, is_kebab_or_snake_ident, line_error, line_error_owned,
};
use super::super::super::error::ParseError;

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
