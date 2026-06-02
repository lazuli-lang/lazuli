//! Small lexer-adjacent helpers shared by the `.lzi` parser
//! sub-modules.
//!
//! - `is_policy_identifier` — sniff predicate used by policy / index /
//!   field-constraint parsers.
//! - `split_call_signature` — split `foo.bar(arg: expr, ...)` into
//!   `("foo.bar", "arg: expr, ...")`.
//! - `parse_named_args` + `split_top_level_commas` — parse the
//!   parenthesized `name: expr, name: expr` arg lists used by
//!   `target` / `invalidates` / declarative calls.
//! - `split_first_token` / `take_identifier` / `take_quoted_string` —
//!   tiny lexers reused by the `report` column modifier parser.
//!
//! Every helper is `pub(super)` — visible across `lzi/*` parsers but
//! never re-exported above `parser::lzi`.

use super::super::common::{SourceLine, line_error, line_error_owned};
use super::super::error::ParseError;
use crate::ast::{Span, TargetArgDecl};

pub(in crate::parser) fn is_policy_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Split `foo.bar(arg: expr, ...)` into `("foo.bar", "arg: expr, ...")`.
/// Returns the query reference and the **content** between the parens
/// (or an empty string when no parens are present).
pub(in crate::parser) fn split_call_signature<'a>(
    line: &SourceLine<'_>,
    rest: &'a str,
) -> Result<(&'a str, &'a str), ParseError> {
    let rest = rest.trim_end();
    if let Some(open) = rest.find('(') {
        if !rest.ends_with(')') {
            return Err(line_error(
                line,
                "call expression must end with `)` (e.g. `query.by_id(id: route.id)`)",
            ));
        }
        let query = rest[..open].trim();
        let args = rest[open + 1..rest.len() - 1].trim();
        Ok((query, args))
    } else {
        Ok((rest.trim(), ""))
    }
}

/// Parse `name: expr, name: expr, ...` arg lists. Splits on the
/// top-level comma (no nested parens in v0 — `derived from` inside
/// queries is the only nesting today and doesn't appear in call args).
pub(in crate::parser) fn parse_named_args(
    line: &SourceLine<'_>,
    text: &str,
) -> Result<Vec<TargetArgDecl>, ParseError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let mut out: Vec<TargetArgDecl> = Vec::new();
    for piece in split_top_level_commas(text) {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        let (name, value) = piece.split_once(':').ok_or_else(|| {
            line_error(
                line,
                "call arguments use `<name>: <expr>` (e.g. `id: route.id`)",
            )
        })?;
        let name = name.trim();
        if name.is_empty() {
            return Err(line_error(line, "call argument requires a name before `:`"));
        }
        out.push(TargetArgDecl {
            name: name.to_owned(),
            value: value.trim().to_owned(),
            span: Span::new(line.start, line.end),
        });
    }
    Ok(out)
}

pub(in crate::parser) fn split_top_level_commas(text: &str) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut start = 0;
    for (idx, ch) in text.char_indices() {
        // A `"..."` string literal is opaque: parens AND commas inside it are
        // payload, never structure. Without this, a natural-language
        // `reason: "predicate-gated, admin-only"` would split mid-quote and the
        // post-comma fragment (no `:`) would fail named-arg parsing — the spec
        // 0028 `@doctor.allow` codegen regression. We do not honor `\"` escapes:
        // the `.lzi` string grammar has none today, and the reason value is
        // re-unquoted (first `"` … next `"`) one layer up, so the boundaries
        // must agree.
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&text[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    out.push(&text[start..]);
    out
}

/// Split off the first whitespace-delimited token from `input`. Returns
/// `(token, rest_after_token)`. Returns `None` for an empty input.
pub(in crate::parser) fn split_first_token(input: &str) -> Option<(&str, &str)> {
    let input = input.trim_start();
    if input.is_empty() {
        return None;
    }
    let end = input
        .find(|c: char| c.is_ascii_whitespace())
        .unwrap_or(input.len());
    Some((&input[..end], &input[end..]))
}

/// Take an identifier prefix (`[A-Za-z_][A-Za-z0-9_]*`). Returns
/// `(ident, remainder)`.
pub(in crate::parser) fn take_identifier(input: &str) -> (&str, &str) {
    let mut end = 0;
    for (i, c) in input.char_indices() {
        let ok = if i == 0 {
            c.is_ascii_alphabetic() || c == '_'
        } else {
            c.is_ascii_alphanumeric() || c == '_'
        };
        if !ok {
            break;
        }
        end = i + c.len_utf8();
    }
    (&input[..end], &input[end..])
}

/// Take a `"..."` quoted string from the head of `input`. Returns
/// `(content_without_quotes, remainder_after_close_quote)`.
pub(in crate::parser) fn take_quoted_string<'a>(
    input: &'a str,
    line: &SourceLine<'_>,
) -> Result<(String, &'a str), ParseError> {
    let rest = input.strip_prefix('"').ok_or_else(|| {
        line_error(
            line,
            "report column modifier value must be a `\"...\"` literal",
        )
    })?;
    let close_idx = rest
        .find('"')
        .ok_or_else(|| line_error(line, "report column modifier missing closing quote"))?;
    let value = rest[..close_idx].to_owned();
    let tail = &rest[close_idx + 1..];
    Ok((value, tail))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InvariantForm {
    TerminalImmutable,
    SingleStatePerScope { state: String, scope_field: String },
    NoJumpMoreThanOne,
}

/// Parses the closed-catalog invariant form from the raw tail after `invariant `.
/// Whitespace is significant only as a token separator; leading/trailing whitespace
/// is trimmed before matching.
///
/// Called from the lifecycle block parser (`super::lifecycle::parse_lifecycle_block`)
/// to enforce closed-catalog discipline at parse time — unknown forms are
/// rejected here, never silently coerced downstream
/// (`docs/proposals/lifecycle-vocab.md` §3.4).
pub(crate) fn parse_invariant_form(
    line: &SourceLine<'_>,
    raw: &str,
) -> Result<InvariantForm, ParseError> {
    let raw = raw.trim();
    if raw == "terminal_immutable" {
        return Ok(InvariantForm::TerminalImmutable);
    }
    if raw == "no_jump_more_than_one" {
        return Ok(InvariantForm::NoJumpMoreThanOne);
    }

    let parts: Vec<&str> = raw.split_whitespace().collect();
    if parts.len() == 4 && parts[0] == "single" && parts[2] == "per" {
        let state = parts[1].to_owned();
        let scope_field = parts[3].to_owned();
        if state.is_empty() || scope_field.is_empty() {
            return Err(line_error(
                line,
                "`invariant single <state> per <scope_field>` requires both names",
            ));
        }
        return Ok(InvariantForm::SingleStatePerScope { state, scope_field });
    }

    Err(line_error_owned(
        line,
        format!(
            "unknown invariant form `{}` - closed catalog is `terminal_immutable`, `single <state> per <scope_field>`, `no_jump_more_than_one` (see docs/proposals/lifecycle-vocab.md §3.4)",
            raw
        ),
    ))
}

// =============================================================================
// `parse_invariant_form` helper tests.
// =============================================================================
#[cfg(test)]
mod invariant_form_helper_tests {
    use super::super::super::common::SourceLine;
    use super::{InvariantForm, parse_invariant_form};

    fn make_invariant_line() -> SourceLine<'static> {
        SourceLine {
            text: "  invariant test",
            indent: 2,
            start: 0,
            end: 16,
        }
    }

    #[test]
    fn parses_terminal_immutable() {
        let line = make_invariant_line();
        assert_eq!(
            parse_invariant_form(&line, "terminal_immutable").unwrap(),
            InvariantForm::TerminalImmutable
        );
    }

    #[test]
    fn parses_no_jump_more_than_one() {
        let line = make_invariant_line();
        assert_eq!(
            parse_invariant_form(&line, "no_jump_more_than_one").unwrap(),
            InvariantForm::NoJumpMoreThanOne
        );
    }

    #[test]
    fn parses_single_state_per_scope() {
        let line = make_invariant_line();
        let form = parse_invariant_form(&line, "single gold per item_id").unwrap();

        match form {
            InvariantForm::SingleStatePerScope { state, scope_field } => {
                assert_eq!(state, "gold");
                assert_eq!(scope_field, "item_id");
            }
            _ => panic!("expected SingleStatePerScope"),
        }
    }

    #[test]
    fn rejects_unknown_form() {
        let line = make_invariant_line();
        let err = parse_invariant_form(&line, "my_custom_thing").unwrap_err();
        let msg = format!("{:?}", err);
        assert!(msg.contains("closed catalog"));
    }

    #[test]
    fn rejects_single_with_missing_tokens() {
        let line = make_invariant_line();
        let err = parse_invariant_form(&line, "single gold per").unwrap_err();
        let msg = format!("{:?}", err);
        assert!(msg.contains("closed catalog") || msg.contains("requires"));
    }

    #[test]
    fn rejects_single_with_wrong_separator() {
        let line = make_invariant_line();
        let err = parse_invariant_form(&line, "single gold by item_id").unwrap_err();
        let msg = format!("{:?}", err);
        assert!(msg.contains("closed catalog"));
    }

    #[test]
    fn rejects_predicate_style() {
        let line = make_invariant_line();
        let err = parse_invariant_form(&line, "single gold where item_id = parent.id").unwrap_err();
        let msg = format!("{:?}", err);
        assert!(msg.contains("closed catalog"));
    }
}
