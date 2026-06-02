//! `@doctor.allow(CODE, reason: "...")` — first-class waiver line parser (spec 0028).
//!
//! A waiver used to live ONLY in a `# doctor:allow <CODE>` comment that the
//! parser discarded as trivia. Spec 0028 adds the first-class line annotation
//! `@doctor.allow(CODE, reason: "...")`, recognized here and captured (as a
//! [`DoctorAllowDecl`]) by the line walker; `lazuli_cli`'s module loader lifts
//! each decl into the FROZEN `lazuli_ir::DoctorAllow` carried on
//! `Module.doctor_allows`.
//!
//! `lazuli_syntax` does NOT depend on `lazuli_ir` (the parser yields AST, the
//! analyzer lowers to IR), so the captured shape lives here as an AST-local
//! struct and the IR conversion happens one layer up.
//!
//! This module owns the SINGLE recognizer for the node line
//! ([`recognize_node_line`]) — reused by the `lazuli_doctor` string scanner
//! (the back-compat bridge) AND the migration codemod, so the three never
//! drift. It also lifts the legacy comment form ([`scan_legacy_comment_allows`])
//! into the same decl shape (`legacy: true`).
//!
//! ## Grammar
//!
//! - `@doctor.allow(<CODE>)` — bare waiver, no reason.
//! - `@doctor.allow(<CODE>, reason: "<text>")` — with a single-line reason.
//!
//! `<CODE>` is an unquoted token. Whitespace around `(` / `,` / `:` is
//! insignificant. The recognizer is per-line, so a reason is single-line by
//! construction. A malformed annotation (`@doctor.allow` with no parens,
//! unterminated quote) is a parse ERROR.

use crate::ast::Span;

use super::super::common::{SourceLine, line_error};
use super::super::error::ParseError;
use super::helpers::{parse_named_args, split_call_signature};

/// The line head that identifies a node-form waiver. The keyword catalog
/// registers this exact literal (`@doctor.allow`) so parser↔registry parity
/// recognizes it.
pub const NODE_HEAD: &str = "@doctor.allow";

/// Scope of a captured waiver. Mirrors `lazuli_ir::DoctorAllowScope`; kept
/// AST-local so `lazuli_syntax` stays IR-free. The loader maps it 1:1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoctorAllowScopeDecl {
    /// Applies to the whole file (col-0, before any feature).
    File,
    /// 1-based source line of the construct the waiver waives.
    Construct { line: usize },
}

/// A captured `@doctor.allow(...)` (or lifted legacy comment) waiver, in
/// AST-local terms. The module loader converts this to `lazuli_ir::DoctorAllow`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorAllowDecl {
    /// The rule code being waived, verbatim (case preserved).
    pub code: String,
    /// The `reason: "..."` value; `None` when omitted.
    pub reason: Option<String>,
    /// File-level or construct-level (1-based line).
    pub scope: DoctorAllowScopeDecl,
    /// `true` when recovered from a `# doctor:allow` comment.
    pub legacy: bool,
    /// Node-form source span; `None` for legacy comment-scan recoveries.
    pub span: Option<Span>,
}

/// True when `trimmed` (a leading-whitespace-stripped line) begins a node-form
/// `@doctor.allow(...)` waiver.
///
/// ## Examples
///
/// ```rust
/// use lazuli_syntax::doctor_allow::is_node_line;
/// assert!(is_node_line("@doctor.allow(X-1)"));
/// assert!(is_node_line("@doctor.allow(X-1, reason: \"y\")"));
/// assert!(!is_node_line("# doctor:allow X-1"));
/// ```
pub fn is_node_line(trimmed: &str) -> bool {
    let after = match trimmed.strip_prefix(NODE_HEAD) {
        Some(rest) => rest,
        None => return false,
    };
    after.trim_start().starts_with('(')
}

/// Lightweight, infallible recognizer used by the doctor string-scanner and the
/// codemod: extract `(code, reason)` from a node-form waiver line. Returns
/// `None` when the line is not a well-formed `@doctor.allow(<CODE>, ...)`.
///
/// This is the STRING-LEVEL bridge primitive (spec 0028 grader R1a): it lets
/// `source_contains_doctor_allow` honor the node form without a `&Module`.
///
/// ## Examples
///
/// ```rust
/// use lazuli_syntax::doctor_allow::recognize_node_line;
///
/// let (code, reason) =
///     recognize_node_line("  @doctor.allow(LZI-FILE-SIZE-001, reason: \"x\")").unwrap();
/// assert_eq!(code, "LZI-FILE-SIZE-001");
/// assert_eq!(reason.as_deref(), Some("x"));
///
/// let (code, reason) = recognize_node_line("@doctor.allow(X-1)").unwrap();
/// assert_eq!(code, "X-1");
/// assert_eq!(reason, None);
///
/// assert!(recognize_node_line("# doctor:allow X-1").is_none());
/// ```
pub fn recognize_node_line(line: &str) -> Option<(String, Option<String>)> {
    let trimmed = line.trim_start();
    if !is_node_line(trimmed) {
        return None;
    }
    let rest = trimmed.strip_prefix(NODE_HEAD)?.trim_start();
    let inner = rest.strip_prefix('(')?;
    let close = inner.rfind(')')?;
    let args = &inner[..close];
    parse_args_loose(args)
}

/// Parse the parenthesized arg content `<CODE>[, reason: "<text>"]` loosely
/// (no span, no error — used by the string bridge / codemod).
fn parse_args_loose(args: &str) -> Option<(String, Option<String>)> {
    let args = args.trim();
    if args.is_empty() {
        return None;
    }
    let (code_part, rest) = match args.split_once(',') {
        Some((c, r)) => (c.trim(), Some(r.trim())),
        None => (args.trim(), None),
    };
    if code_part.is_empty() || code_part.contains(':') {
        return None;
    }
    let reason = rest.and_then(extract_reason_value);
    Some((code_part.to_string(), reason))
}

/// Pull the `"..."` value out of a `reason: "..."` clause. Tolerant of extra
/// whitespace; returns `None` when there is no quoted reason.
fn extract_reason_value(clause: &str) -> Option<String> {
    let clause = clause.trim();
    let after_key = clause.strip_prefix("reason")?.trim_start();
    let after_colon = after_key.strip_prefix(':')?.trim_start();
    let inner = after_colon.strip_prefix('"')?;
    let close = inner.find('"')?;
    Some(inner[..close].to_string())
}

/// Parse a node-form waiver line into a [`DoctorAllowDecl`] with a span and the
/// given `scope`. STRICT: a malformed annotation is a [`ParseError`] (reuses
/// `split_call_signature` + `parse_named_args`, the same machinery
/// `@cap.Hashed(algorithm: argon2id)` / declarative calls use).
pub(in crate::parser) fn parse(
    line: &SourceLine<'_>,
    scope: DoctorAllowScopeDecl,
) -> Result<DoctorAllowDecl, ParseError> {
    let trimmed = line.text.trim_start();
    let rest = trimmed
        .strip_prefix(NODE_HEAD)
        .ok_or_else(|| line_error(line, "expected `@doctor.allow(...)` waiver annotation"))?;
    // Reuse the shared call-signature splitter: `@doctor.allow(args)` →
    // ("", "args"). It enforces the trailing `)`.
    let (head, args) = split_call_signature(line, rest)?;
    if !head.is_empty() {
        return Err(line_error(
            line,
            "`@doctor.allow` takes its code in parentheses, e.g. `@doctor.allow(CODE, reason: \"why\")`",
        ));
    }
    if args.trim().is_empty() {
        return Err(line_error(
            line,
            "`@doctor.allow(...)` requires a rule code, e.g. `@doctor.allow(LZI-FILE-SIZE-001, reason: \"why\")`",
        ));
    }
    let (code_part, named_part) = match args.split_once(',') {
        Some((c, r)) => (c.trim(), r.trim()),
        None => (args.trim(), ""),
    };
    if code_part.is_empty() {
        return Err(line_error(line, "`@doctor.allow(...)` requires a rule code"));
    }
    if code_part.contains(':') {
        return Err(line_error(
            line,
            "the first `@doctor.allow(...)` argument is the bare rule code (no `:`)",
        ));
    }

    let mut reason = None;
    if !named_part.is_empty() {
        let named = parse_named_args(line, named_part)?;
        for arg in named {
            if arg.name == "reason" {
                reason = Some(unquote_reason(line, &arg.value)?);
            } else {
                return Err(line_error(
                    line,
                    "`@doctor.allow(...)` only accepts a `reason: \"...\"` named argument",
                ));
            }
        }
    }

    Ok(DoctorAllowDecl {
        code: code_part.to_string(),
        reason,
        scope,
        legacy: false,
        span: Some(Span::new(line.start, line.end.max(line.start + 1))),
    })
}

/// Strip the surrounding quotes from a `reason:` value. Errors on a missing /
/// unterminated quote (mirrors the strictness of the report-modifier lexer).
fn unquote_reason(line: &SourceLine<'_>, value: &str) -> Result<String, ParseError> {
    let value = value.trim();
    let inner = value
        .strip_prefix('"')
        .ok_or_else(|| line_error(line, "`reason` value must be a `\"...\"` string literal"))?;
    let close = inner
        .find('"')
        .ok_or_else(|| line_error(line, "`reason` string literal is missing its closing quote"))?;
    Ok(inner[..close].to_string())
}

/// Walk `source` and capture EVERY waiver: strict node-form
/// `@doctor.allow(...)` lines (with File/Construct scope) PLUS lifted legacy
/// `# doctor:allow` comments. A malformed node annotation is a [`ParseError`]
/// (never silently dropped) — this is the parser-capture entry point the module
/// loader calls.
///
/// Scope rule: a node line whose NEXT non-trivia, non-waiver line is a
/// construct gets `Construct { line }` = that construct's 1-based line. A node
/// line with no following construct (trailing / file-only) is `File`. Column-0
/// node lines before any feature are also `File` (the same outcome — no
/// following construct at deeper scope is required; we key purely on whether a
/// following construct exists).
///
/// ## Examples
///
/// ```rust
/// use lazuli_syntax::doctor_allow::{capture_doctor_allows, DoctorAllowScopeDecl};
///
/// let src = "@doctor.allow(LZI-FILE-SIZE-001, reason: \"gen\")\nfeature billing\n";
/// let got = capture_doctor_allows(src).unwrap();
/// assert_eq!(got.len(), 1);
/// assert_eq!(got[0].code, "LZI-FILE-SIZE-001");
/// assert_eq!(got[0].scope, DoctorAllowScopeDecl::Construct { line: 2 });
/// ```
pub fn capture_doctor_allows(source: &str) -> Result<Vec<DoctorAllowDecl>, ParseError> {
    let lines = super::super::common::source_lines(source);
    let mut out = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.text.trim_start();
        if !is_node_line(trimmed) {
            continue;
        }
        // Scope: find the next line that is neither trivia (blank/`#`) nor
        // itself a node-form waiver. If one exists, this waiver attaches to it
        // (Construct{line}); otherwise it is File-scoped.
        let scope = next_construct_line(&lines, idx + 1)
            .map(|line_no| DoctorAllowScopeDecl::Construct { line: line_no })
            .unwrap_or(DoctorAllowScopeDecl::File);
        out.push(parse(line, scope)?);
    }

    // Legacy comment-form waivers (whole-file scope), captured in source order
    // after the node forms.
    out.extend(scan_legacy_comment_allows(source));
    Ok(out)
}

/// Return the 1-based line number of the next non-trivia, non-node-waiver line
/// at or after `from` (0-based index into `lines`). `None` when none follows.
fn next_construct_line(lines: &[SourceLine<'_>], from: usize) -> Option<usize> {
    for (offset, line) in lines.iter().enumerate().skip(from) {
        let trimmed = line.text.trim_start();
        if super::super::common::is_trivia(trimmed) || is_node_line(trimmed) {
            continue;
        }
        return Some(offset + 1);
    }
    None
}

/// Scan raw `source` for legacy `# doctor:allow <CODE> [— reason "..."]`
/// comment-form waivers and lift each into a [`DoctorAllowDecl`] with
/// `legacy: true`. Scope is always [`DoctorAllowScopeDecl::File`] (the comment
/// scan is whole-file, matching the historical `source_contains_doctor_allow`
/// semantics).
///
/// ## Examples
///
/// ```rust
/// use lazuli_syntax::doctor_allow::scan_legacy_comment_allows;
///
/// let got = scan_legacy_comment_allows("# doctor:allow X-1 — reason \"y\"\nfeature a\n");
/// assert_eq!(got.len(), 1);
/// assert_eq!(got[0].code, "X-1");
/// assert_eq!(got[0].reason.as_deref(), Some("y"));
/// assert!(got[0].legacy);
/// ```
pub fn scan_legacy_comment_allows(source: &str) -> Vec<DoctorAllowDecl> {
    let mut out = Vec::new();
    for line in source.lines() {
        if let Some((code, reason)) = recognize_legacy_comment_line(line) {
            out.push(DoctorAllowDecl {
                code,
                reason,
                scope: DoctorAllowScopeDecl::File,
                legacy: true,
                span: None,
            });
        }
    }
    out
}

/// Recognize a legacy `# doctor:allow <CODE> [— reason "..."]` comment line and
/// extract `(code, reason)`. Returns `None` when the line is not a legacy
/// waiver comment. Case-insensitive on the `doctor:allow` keyword; CODE case is
/// preserved.
///
/// ## Examples
///
/// ```rust
/// use lazuli_syntax::doctor_allow::recognize_legacy_comment_line;
///
/// let (code, reason) =
///     recognize_legacy_comment_line("# doctor:allow X-1 — reason \"y\"").unwrap();
/// assert_eq!(code, "X-1");
/// assert_eq!(reason.as_deref(), Some("y"));
/// assert!(recognize_legacy_comment_line("feature a").is_none());
/// ```
pub fn recognize_legacy_comment_line(line: &str) -> Option<(String, Option<String>)> {
    let lower = line.to_ascii_lowercase();
    let pos = lower.find("doctor:allow")?;
    if !line[..pos].contains('#') {
        return None;
    }
    let after = line[pos + "doctor:allow".len()..].trim_start();
    let code = after.split_whitespace().next()?;
    if code.is_empty() {
        return None;
    }
    let reason = after
        .to_ascii_lowercase()
        .find("reason")
        .and_then(|rpos| extract_reason_after(&after[rpos + "reason".len()..]));
    Some((code.to_string(), reason))
}

/// Pull a `"..."` value out of the text following a bare `reason` keyword
/// (the legacy tail uses `— reason "..."` / `reason "..."`, no colon).
fn extract_reason_after(after_keyword: &str) -> Option<String> {
    let open = after_keyword.find('"')?;
    let rest = &after_keyword[open + 1..];
    let close = rest.find('"')?;
    Some(rest[..close].to_string())
}

#[cfg(test)]
mod tests {
    use super::super::super::common::source_lines;
    use super::*;

    fn first_line(src: &str) -> SourceLine<'_> {
        source_lines(src).into_iter().next().unwrap()
    }

    #[test]
    fn parses_code_and_reason() {
        let src = "@doctor.allow(LZI-FILE-SIZE-001, reason: \"generated table\")";
        let line = first_line(src);
        let got = parse(&line, DoctorAllowScopeDecl::File).unwrap();
        assert_eq!(got.code, "LZI-FILE-SIZE-001");
        assert_eq!(got.reason.as_deref(), Some("generated table"));
        assert!(!got.legacy);
        assert!(got.span.is_some());
    }

    #[test]
    fn parses_reason_with_commas_and_at_sign() {
        // Spec 0028 regression: a natural-language reason containing commas
        // (and `, @role.X`) must round-trip through the STRICT codegen-path
        // parser as a single string literal, not split into bogus named args.
        let src = "@doctor.allow(LZI-PREDICATE-GATE-001, reason: \"predicate-gated, admin-only, see @role.Admin\")";
        let line = first_line(src);
        let got = parse(&line, DoctorAllowScopeDecl::File).unwrap();
        assert_eq!(got.code, "LZI-PREDICATE-GATE-001");
        assert_eq!(
            got.reason.as_deref(),
            Some("predicate-gated, admin-only, see @role.Admin")
        );
    }

    #[test]
    fn capture_reason_with_commas_round_trips() {
        // Same payload through the module-loader capture entry point that
        // `lazuli generate go` actually calls.
        let src = "@doctor.allow(X-1, reason: \"a, b, @role.Y\")\nfeature billing\n";
        let got = capture_doctor_allows(src).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].code, "X-1");
        assert_eq!(got[0].reason.as_deref(), Some("a, b, @role.Y"));
    }

    #[test]
    fn parses_code_only() {
        let line = first_line("@doctor.allow(X-1)");
        let got = parse(&line, DoctorAllowScopeDecl::Construct { line: 4 }).unwrap();
        assert_eq!(got.code, "X-1");
        assert_eq!(got.reason, None);
        assert_eq!(got.scope, DoctorAllowScopeDecl::Construct { line: 4 });
    }

    #[test]
    fn malformed_no_parens_is_error() {
        let line = first_line("@doctor.allow X-1");
        assert!(parse(&line, DoctorAllowScopeDecl::File).is_err());
    }

    #[test]
    fn malformed_unterminated_quote_is_error() {
        let line = first_line("@doctor.allow(X-1, reason: \"oops)");
        assert!(parse(&line, DoctorAllowScopeDecl::File).is_err());
    }

    #[test]
    fn empty_parens_is_error() {
        let line = first_line("@doctor.allow()");
        assert!(parse(&line, DoctorAllowScopeDecl::File).is_err());
    }

    #[test]
    fn code_with_colon_is_error() {
        let line = first_line("@doctor.allow(reason: \"x\")");
        assert!(parse(&line, DoctorAllowScopeDecl::File).is_err());
    }

    #[test]
    fn recognize_node_line_matches_both_forms() {
        assert_eq!(
            recognize_node_line("@doctor.allow(X-1)"),
            Some(("X-1".to_string(), None))
        );
        assert_eq!(
            recognize_node_line("  @doctor.allow(X-1, reason: \"y\")"),
            Some(("X-1".to_string(), Some("y".to_string())))
        );
        assert_eq!(recognize_node_line("# doctor:allow X-1"), None);
        assert_eq!(recognize_node_line("feature a"), None);
    }

    #[test]
    fn legacy_comment_captured_with_reason() {
        let got = scan_legacy_comment_allows("# doctor:allow X-1 — reason \"y\"\n");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].code, "X-1");
        assert_eq!(got[0].reason.as_deref(), Some("y"));
        assert!(got[0].legacy);
        assert_eq!(got[0].scope, DoctorAllowScopeDecl::File);
    }

    #[test]
    fn legacy_comment_bare_no_reason() {
        let got = scan_legacy_comment_allows("  # doctor:allow Y-2\n");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].code, "Y-2");
        assert_eq!(got[0].reason, None);
    }

    #[test]
    fn legacy_ignores_non_comment_mentions() {
        let got = scan_legacy_comment_allows("purpose \"doctor:allow X-1 is the opt-out\"\n");
        assert!(got.is_empty());
    }

    #[test]
    fn capture_node_attaches_to_following_construct() {
        let src = "@doctor.allow(X-1, reason: \"y\")\nfeature billing\n";
        let got = capture_doctor_allows(src).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].code, "X-1");
        assert_eq!(got[0].scope, DoctorAllowScopeDecl::Construct { line: 2 });
        assert!(!got[0].legacy);
    }

    #[test]
    fn capture_node_with_no_following_construct_is_file() {
        let src = "feature billing\n@doctor.allow(X-1)\n";
        let got = capture_doctor_allows(src).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].scope, DoctorAllowScopeDecl::File);
    }

    #[test]
    fn capture_stacked_node_waivers_share_following_construct() {
        let src = "@doctor.allow(A-1)\n@doctor.allow(B-2)\nfeature billing\n";
        let got = capture_doctor_allows(src).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].scope, DoctorAllowScopeDecl::Construct { line: 3 });
        assert_eq!(got[1].scope, DoctorAllowScopeDecl::Construct { line: 3 });
    }

    #[test]
    fn capture_node_above_multiline_construct_uses_head_line() {
        // The construct spans multiple lines; the waiver attaches to its HEAD.
        let src = "@doctor.allow(X-1)\nresource Account\n  email: Text required\n";
        let got = capture_doctor_allows(src).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].scope, DoctorAllowScopeDecl::Construct { line: 2 });
    }

    #[test]
    fn capture_includes_legacy_and_node_together() {
        let src = "# doctor:allow OLD-9 — reason \"old\"\n@doctor.allow(NEW-9)\nfeature x\n";
        let got = capture_doctor_allows(src).unwrap();
        // One node + one legacy.
        assert_eq!(got.len(), 2);
        let node = got.iter().find(|d| !d.legacy).unwrap();
        assert_eq!(node.code.as_str(), "NEW-9");
        let legacy = got.iter().find(|d| d.legacy).unwrap();
        assert_eq!(legacy.code.as_str(), "OLD-9");
    }

    #[test]
    fn capture_malformed_node_is_error() {
        let src = "@doctor.allow(\nfeature x\n";
        assert!(capture_doctor_allows(src).is_err());
    }

    #[test]
    fn capture_only_node_lines_not_discarded_as_trivia() {
        // A file that is ONLY waiver lines parses them (not trivia).
        let src = "@doctor.allow(A-1)\n@doctor.allow(B-2)\n";
        let got = capture_doctor_allows(src).unwrap();
        assert_eq!(got.len(), 2);
    }
}
