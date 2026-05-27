//! TEST-PINS-STUB-VOCAB-001 — assertion pins stub-state vocabulary.
//!
//! ## Rule statement
//!
//! Fires when a Go `*_test.go` source contains an assertion call
//! (`assert.Contains`, `assert.Equal`, `require.Contains`,
//! `require.Equal`, `strings.Contains`, `t.Fatal`, `t.Fatalf`,
//! `t.Skip`, `t.Skipf`, `t.Errorf`) whose string literal argument
//! matches the closed stub-vocab catalog (case-insensitive substring).
//!
//! ## Why
//!
//! Handler tests get scaffolded when the handler is a stub returning a
//! "not implemented" sentinel. The test asserts on that sentinel. When
//! the handler is later implemented for real, the test stays in place —
//! either passing because the new error string incidentally contains
//! the substring, or getting deleted instead of refactored. Either
//! failure mode produces a zombie test pinning the wrong state.
//!
//! ## Severity
//!
//! `info` (prototype) / `warning` (strict) / `error` (production /
//! tdd-iron-hand). The dispatcher maps the profile; this module
//! returns plain findings.
//!
//! ## Detection heuristic
//!
//! Byte-level line walker modelled after `HANDLER-NO-PANIC-001`:
//!
//! 1. Track block-comment depth (`/* ... */`) across lines.
//! 2. Strip `// line comments` so vocab-keywords inside trailing
//!    comments don't fire.
//! 3. Skip lines whose trimmed prefix is `//`.
//! 4. Find one of the catalog call-site needles
//!    (`assert.Contains(`, etc.); for each, extract the string
//!    literal argument and match (case-insensitive) against
//!    [`STUB_VOCAB`].
//!
//! ## Opt-out
//!
//! `# doctor:allow TEST-PINS-STUB-VOCAB-001 — reason "..."` anywhere
//! in the file silences all findings for that file. Reuses the shared
//! [`crate::allow_comment`] helper.
//!
//! ## What's NOT flagged
//!
//! - Test function names containing vocab (e.g.
//!   `TestNotImplementedReturns500`) — only call-site arguments are
//!   inspected.
//! - Non-assertion string literals (`var x = "TODO"`) — only the
//!   tracked assertion APIs trigger.
//! - Vocab inside `t.Log` / `t.Logf` calls — diagnostic-only APIs do
//!   not pin behaviour.
//! - Vocab inside block or line comments.
//! - Files carrying the `# doctor:allow TEST-PINS-STUB-VOCAB-001`
//!   opt-out comment.
//!
//! ## Reference
//!
//! - `docs/proposals/doctor-test-pins-stub-vocab.md` — full design.
//! - `crates/lazuli_doctor/src/error_handling/handler_no_panic_001.rs`
//!   — positive precedent for the byte-walker shape.

use std::path::{Path, PathBuf};

use crate::allow_comment::source_contains_doctor_allow;

// ── catalog ───────────────────────────────────────────────────────────────────

/// Closed stub-vocab catalog. New entries land via proposal amendment.
/// Match is case-insensitive substring against the extracted string
/// literal argument.
///
/// **Order matters**: when a literal matches multiple tokens, the
/// first match wins (surfaced in `Finding.matched_vocab`). Catalog is
/// sorted longest / most-specific first so milestone vocab
/// (`phase 1.1`) reports under that token rather than under a shorter
/// substring that happens to also hit.
pub const STUB_VOCAB: &[&str] = &[
    "not yet implemented",
    "not implemented",
    "unimplemented",
    "not yet",
    "phase 1.1",
    "phase 1",
    "phase 2",
    "coming soon",
    "placeholder",
    "fixme",
    "todo",
    "stub",
    "wip",
];

/// Assertion call-site needles whose first/second string literal
/// argument is inspected.
const ASSERTION_CALLS: &[&str] = &[
    "assert.Contains(",
    "assert.Equal(",
    "assert.EqualError(",
    "require.Contains(",
    "require.Equal(",
    "require.EqualError(",
    "strings.Contains(",
    "t.Fatal(",
    "t.Fatalf(",
    "t.Skip(",
    "t.Skipf(",
    "t.Errorf(",
    "t.Error(",
];

// ── output ────────────────────────────────────────────────────────────────────

/// One TEST-PINS-STUB-VOCAB-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `_test.go` source path.
    pub path: PathBuf,
    /// 1-based source line where the assertion call starts.
    pub line: usize,
    /// 1-based column where the matched literal starts.
    pub column: usize,
    /// The pinned stub-vocab token from the catalog that matched.
    pub matched_vocab: String,
    /// The full extracted string literal (lower-cased trimming
    /// preserves length but enables debugging the match).
    pub literal: String,
    /// The assertion call-site needle that produced the match
    /// (e.g. `assert.Contains(`).
    pub call_site: String,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "TEST-PINS-STUB-VOCAB-001";

    /// Render the user-facing diagnostic body — surfaces the assertion
    /// site and the pinned vocab token so the author can rewrite the
    /// assertion against the handler's real contract.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::test_discipline::test_pins_stub_vocab_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("register_with_google_test.go"),
    ///     line: 25,
    ///     column: 5,
    ///     matched_vocab: "not implemented".into(),
    ///     literal: "not implemented".into(),
    ///     call_site: "assert.Contains(".into(),
    /// };
    /// assert!(f.message().contains("not implemented"));
    /// assert!(f.message().contains("assert.Contains"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{}:{} test asserts stub-state vocabulary — `{}` call pins literal \
             `\"{}\"` (matches catalog token `{}`). Rewrite the assertion against \
             the handler's real contract, or opt out with \
             `# doctor:allow TEST-PINS-STUB-VOCAB-001 — reason \"...\"`.",
            self.path.display(),
            self.line,
            self.call_site.trim_end_matches('('),
            self.literal,
            self.matched_vocab,
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run TEST-PINS-STUB-VOCAB-001 over a Go `_test.go` source. Returns
/// one finding per call-site-vocab match. Files carrying the
/// `# doctor:allow TEST-PINS-STUB-VOCAB-001` opt-out yield an empty
/// vector.
///
/// ## Examples
///
/// ```rust
/// use std::path::Path;
/// use lazuli_doctor::test_discipline::test_pins_stub_vocab_001::check;
///
/// let source = "\
/// package h
/// import \"testing\"
/// func TestX(t *testing.T) {
///     assert.Contains(t, err.Error(), \"not implemented\")
/// }
/// ";
/// let findings = check(source, Path::new("x_test.go"));
/// assert_eq!(findings.len(), 1);
/// ```
pub fn check(source: &str, path: &Path) -> Vec<Finding> {
    if source_contains_doctor_allow(source, Finding::CODE) {
        return Vec::new();
    }

    let mut findings = Vec::new();
    let mut block_comment_depth: usize = 0;

    for (idx, raw_line) in source.lines().enumerate() {
        let (stripped, depth_after) =
            strip_block_comments(raw_line, block_comment_depth);
        block_comment_depth = depth_after;

        let no_line_comment = strip_line_comment(&stripped);
        let trimmed = no_line_comment.trim_start();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        scan_line(&no_line_comment, idx + 1, path, &mut findings);
    }

    findings
}

/// Scan a single (comment-stripped) source line for assertion calls
/// pinning stub vocab.
fn scan_line(line: &str, line_no: usize, path: &Path, out: &mut Vec<Finding>) {
    for call in ASSERTION_CALLS {
        let mut search_from = 0usize;
        while let Some(rel) = line[search_from..].find(call) {
            let call_start = search_from + rel;
            if !has_call_boundary_before(line, call_start) {
                search_from = call_start + call.len();
                continue;
            }
            // Walk forward from the `(` collecting each string literal
            // argument until the matching `)` (depth-aware) or EOL.
            let arg_start = call_start + call.len();
            if let Some(literals) = extract_string_literals_in_call(line, arg_start) {
                for (lit, lit_col) in literals {
                    if let Some(vocab) = match_stub_vocab(&lit) {
                        out.push(Finding {
                            path: path.to_path_buf(),
                            line: line_no,
                            column: lit_col + 1,
                            matched_vocab: vocab.to_string(),
                            literal: lit,
                            call_site: (*call).to_string(),
                        });
                    }
                }
            }
            search_from = call_start + call.len();
        }
    }
}

/// Extract every double-quoted string literal between `start` and the
/// call's matching `)` on the same line. Returns `Some(Vec<(literal,
/// byte_column)>)` where `byte_column` is the 0-based byte index of
/// the opening `"`. Returns `None` if we can't even start scanning
/// (defensive — should not happen in practice).
fn extract_string_literals_in_call(
    line: &str,
    start: usize,
) -> Option<Vec<(String, usize)>> {
    let bytes = line.as_bytes();
    if start > bytes.len() {
        return None;
    }
    let mut out = Vec::new();
    let mut i = start;
    let mut paren_depth: usize = 1;
    while i < bytes.len() && paren_depth > 0 {
        match bytes[i] {
            b'(' => {
                paren_depth += 1;
                i += 1;
            }
            b')' => {
                paren_depth -= 1;
                i += 1;
            }
            b'"' => {
                let lit_start = i;
                i += 1;
                let mut content = String::new();
                while i < bytes.len() {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        // Preserve escape semantics for matching — we
                        // only need substring matches, so include the
                        // raw escaped char.
                        content.push(bytes[i + 1] as char);
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'"' {
                        i += 1;
                        break;
                    }
                    content.push(bytes[i] as char);
                    i += 1;
                }
                out.push((content, lit_start));
            }
            b'`' => {
                // Raw string — single-line scan only (multi-line raw
                // strings are out of scope per the spec's risk section).
                let lit_start = i;
                i += 1;
                let mut content = String::new();
                while i < bytes.len() && bytes[i] != b'`' {
                    content.push(bytes[i] as char);
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1; // skip closing backtick
                }
                out.push((content, lit_start));
            }
            _ => {
                i += 1;
            }
        }
    }
    Some(out)
}

/// Return the first catalog vocab token contained (case-insensitive)
/// in `literal`, or `None` if no token matches.
fn match_stub_vocab(literal: &str) -> Option<&'static str> {
    let lower = literal.to_ascii_lowercase();
    for token in STUB_VOCAB {
        if lower.contains(token) {
            return Some(token);
        }
    }
    None
}

/// `assert.Contains(` (etc.) must be preceded by start-of-line or one
/// of: whitespace, `;`, `{`, `}`, `(`, `,`, `=`, `&`, `|`, `!`, `:`.
/// An identifier-char prefix means we're matching the tail of a longer
/// identifier (e.g. `myassert.Contains(`) — still flag, since the
/// receiver is irrelevant for stub-vocab pinning. Returns true also
/// for `.` so qualified prefixes like `pkg.assert.Contains(` flag.
fn has_call_boundary_before(line: &str, idx: usize) -> bool {
    if idx == 0 {
        return true;
    }
    let prev = line.as_bytes()[idx - 1];
    // For the dot-prefixed needles (`assert.`, `require.`, `t.`,
    // `strings.`) we need the char BEFORE the package qualifier to be
    // a non-identifier OR start-of-line. The needle itself includes
    // the qualifier, so the byte immediately before the needle is the
    // boundary check target.
    !(prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.')
}

/// Remove the slice of a line that's inside a `/* ... */` block
/// comment, tracking depth across lines. Mirrors
/// `handler_no_panic_001::strip_block_comments`.
fn strip_block_comments(line: &str, mut depth: usize) -> (String, usize) {
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if depth == 0 {
            if i + 1 < bytes.len()
                && bytes[i] == b'/'
                && bytes[i + 1] == b'*'
                && !in_string_at(line, i)
            {
                depth += 1;
                out.push(' ');
                out.push(' ');
                i += 2;
                continue;
            }
            out.push(bytes[i] as char);
            i += 1;
        } else {
            if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                depth -= 1;
                out.push(' ');
                out.push(' ');
                i += 2;
                continue;
            }
            out.push(' ');
            i += 1;
        }
    }
    (out, depth)
}

/// Drop everything after the first unescaped `//` that's outside a
/// string literal.
fn strip_line_comment(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] == b'/' && bytes[i + 1] == b'/' && !in_string_at(line, i) {
            return line[..i].to_owned();
        }
        i += 1;
    }
    line.to_owned()
}

/// Return `true` if byte index `idx` falls inside a `"..."` string
/// literal on this line. Counts unescaped `"` to the left.
fn in_string_at(line: &str, idx: usize) -> bool {
    let bytes = line.as_bytes();
    let limit = idx.min(bytes.len());
    let mut in_str = false;
    let mut prev = b' ';
    for &b in bytes.iter().take(limit) {
        if b == b'"' && prev != b'\\' {
            in_str = !in_str;
        }
        prev = b;
    }
    in_str
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> &'static Path {
        Path::new("features/account/handlers/register_with_google_test.go")
    }

    #[test]
    fn hostpoint_reproducer_fires() {
        // Verbatim shape from
        // app/features/account/handlers/register_with_google_test.go:25.
        let source = "\
package accounthandlers

import \"testing\"

func TestRegisterWithGoogle_StubReturnsNotImplemented(t *testing.T) {
\t_, err := RegisterWithGoogle(ctx, input)
\tassert.Contains(t, err.Error(), \"not implemented\")
}
";
        let findings = check(source, p());
        assert_eq!(findings.len(), 1, "expected one finding, got {findings:?}");
        assert_eq!(findings[0].line, 7);
        assert_eq!(findings[0].matched_vocab, "not implemented");
        assert_eq!(findings[0].call_site, "assert.Contains(");
        assert_eq!(Finding::CODE, "TEST-PINS-STUB-VOCAB-001");
    }

    #[test]
    fn legitimate_assertion_silent() {
        // Real contract assertion against an enumerated error variant.
        let source = "\
package h
func TestLogin(t *testing.T) {
\t_, err := Login(ctx, in)
\tassert.Contains(t, err.Error(), \"invalid_credentials\")
}
";
        assert!(check(source, p()).is_empty());
    }

    #[test]
    fn comment_with_vocab_silent() {
        // Line-comment containing TODO must not fire.
        let source = "\
package h
func TestX(t *testing.T) {
\t// TODO: revisit when stub is replaced
\tassert.NoError(t, err)
}
";
        assert!(check(source, p()).is_empty());
    }

    #[test]
    fn block_comment_with_vocab_silent() {
        let source = "\
package h
/*
not implemented yet — see ticket
*/
func TestX(t *testing.T) {
\tassert.NoError(t, err)
}
";
        assert!(check(source, p()).is_empty());
    }

    #[test]
    fn test_function_name_with_vocab_silent() {
        // Function name itself contains "NotImplemented" but no
        // assertion call pins stub-vocab.
        let source = "\
package h
func TestNotImplementedReturns500(t *testing.T) {
\tresp := DoRequest()
\tassert.Equal(t, 500, resp.StatusCode)
}
";
        let findings = check(source, p());
        assert!(
            findings.is_empty(),
            "function name vocab must not fire; got {findings:?}"
        );
    }

    #[test]
    fn string_literal_outside_assertion_silent() {
        // `var x = "TODO"` — literal exists but not inside an
        // assertion call from the catalog.
        let source = "\
package h
func TestX(t *testing.T) {
\tvar x = \"TODO\"
\t_ = x
}
";
        assert!(check(source, p()).is_empty());
    }

    #[test]
    fn t_log_with_vocab_silent() {
        // t.Log is diagnostic-only and NOT in the catalog.
        let source = "\
package h
func TestX(t *testing.T) {
\tt.Log(\"not implemented yet\")
\tassert.NoError(t, err)
}
";
        assert!(check(source, p()).is_empty());
    }

    #[test]
    fn doctor_allow_opt_out_silences() {
        let source = "\
# doctor:allow TEST-PINS-STUB-VOCAB-001 — reason \"Phase 1.1 stub explicitly preserved\"
package h
func TestX(t *testing.T) {
\tassert.Contains(t, err.Error(), \"not implemented\")
}
";
        assert!(check(source, p()).is_empty());
    }

    #[test]
    fn require_equal_stub_fires() {
        let source = "\
package h
func TestX(t *testing.T) {
\trequire.Equal(t, \"stub\", got)
}
";
        let findings = check(source, p());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].matched_vocab, "stub");
        assert_eq!(findings[0].call_site, "require.Equal(");
    }

    #[test]
    fn t_skip_with_not_ready_fires() {
        let source = "\
package h
func TestX(t *testing.T) {
\tt.Skip(\"not yet implemented\")
}
";
        let findings = check(source, p());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].call_site, "t.Skip(");
        // Longest-match wins per STUB_VOCAB ordering.
        assert_eq!(findings[0].matched_vocab, "not yet implemented");
    }

    #[test]
    fn t_fatal_not_implemented_fires() {
        let source = "\
package h
func TestX(t *testing.T) {
\tt.Fatal(\"not implemented\")
}
";
        let findings = check(source, p());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].call_site, "t.Fatal(");
    }

    #[test]
    fn case_insensitive_match() {
        let source = "\
package h
func TestX(t *testing.T) {
\tassert.Contains(t, err.Error(), \"Not Implemented\")
}
";
        let findings = check(source, p());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].matched_vocab, "not implemented");
    }

    #[test]
    fn phase_one_dot_one_milestone_vocab_fires() {
        // Lazuli-specific milestone vocab from the spec.
        let source = "\
package h
func TestX(t *testing.T) {
\tassert.Contains(t, msg, \"Phase 1.1 stub\")
}
";
        let findings = check(source, p());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].matched_vocab, "phase 1.1");
    }

    #[test]
    fn coming_soon_marketing_vocab_fires() {
        let source = "\
package h
func TestX(t *testing.T) {
\tassert.Equal(t, \"coming soon\", banner)
}
";
        let findings = check(source, p());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].matched_vocab, "coming soon");
    }

    #[test]
    fn trailing_line_comment_with_vocab_silent() {
        // The actual assertion is fine; vocab is hidden in trailing //.
        let source = "\
package h
func TestX(t *testing.T) {
\tassert.NoError(t, err) // TODO: tighten when stub lands
}
";
        assert!(check(source, p()).is_empty());
    }

    #[test]
    fn multiple_assertions_each_fire() {
        let source = "\
package h
func TestA(t *testing.T) { assert.Contains(t, e.Error(), \"not implemented\") }
func TestB(t *testing.T) { require.Equal(t, \"stub\", got) }
";
        let findings = check(source, p());
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn strings_contains_pin_fires() {
        let source = "\
package h
func TestX(t *testing.T) {
\tif strings.Contains(err.Error(), \"not implemented\") {
\t\tt.Errorf(\"unexpected\")
\t}
}
";
        let findings = check(source, p());
        // strings.Contains AND t.Errorf are both catalog entries — but
        // t.Errorf's literal "unexpected" doesn't match. Exactly one.
        assert_eq!(findings.len(), 1, "got {findings:?}");
        assert_eq!(findings[0].call_site, "strings.Contains(");
    }

    #[test]
    fn message_renders_path_line_and_vocab() {
        let source = "\
package h
func TestX(t *testing.T) {
\tassert.Contains(t, err.Error(), \"not implemented\")
}
";
        let finding = check(source, p()).into_iter().next().unwrap();
        let msg = finding.message();
        assert!(msg.contains("register_with_google_test.go"));
        assert!(msg.contains(":3"));
        assert!(msg.contains("not implemented"));
        assert!(msg.contains("assert.Contains"));
    }

    #[test]
    fn code_constant_is_stable() {
        assert_eq!(Finding::CODE, "TEST-PINS-STUB-VOCAB-001");
    }
}
