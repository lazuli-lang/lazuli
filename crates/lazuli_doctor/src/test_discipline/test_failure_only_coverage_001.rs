//! TEST-FAILURE-ONLY-COVERAGE-001 — a `_test.go` file declares one
//! or more `func Test*` entrypoints, but no single test exercises a
//! success path.
//!
//! Catches the failure-only-test theater shape: every Test* body
//! contains only error-side assertions (`require.Error`,
//! `assert.Error`, `assert.Contains(t, err.Error(), ...)`) or
//! `t.Skip`s outright, and zero bodies invoke a positive assertion
//! (`require.NoError`, `assert.Equal(t, expected, result)`,
//! `assert.NotEmpty(...)`, etc.). The handler's success contract is
//! unverified — a regression that changes the success shape or
//! returns a slightly-different error will still pass the file
//! green.
//!
//! Sibling to [`crate::test_discipline::test_handler_missing_001`]
//! (file exists?) and the planned `TEST-PINS-STUB-VOCAB-001`
//! (file pins stub vocab?). Three rules, overlapping but distinct:
//! missing → exists-but-pins-stub → exists-but-only-covers-failure.
//!
//! Default severity: `warning` (per spec — some genuinely-negative
//! suites are legitimate, e.g. validation matrices). Opt-in
//! `error` under tdd-iron-hand. File-level opt-out via
//! `# doctor:allow TEST-FAILURE-ONLY-COVERAGE-001 — reason "..."`.
//!
//! Reference: docs/proposals/doctor-test-failure-only-coverage.md.

use std::path::PathBuf;

use crate::allow_comment::source_contains_doctor_allow;
use crate::error_handling::walker::GoHandlerSourceFile;

// ── output ────────────────────────────────────────────────────────────────────

/// Categorised shape of a single `func Test*` body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestBodyKind {
    /// At least one success-side assertion (`require.NoError`,
    /// `assert.Equal(t, expected, result)`, etc.).
    HasSuccessAssertion,
    /// Only error-side assertions present (`require.Error`,
    /// `assert.Contains(t, err.Error(), ...)`).
    ErrorOnly,
    /// Body starts with `t.Skip` / `t.Skipf` / `t.SkipNow` after
    /// optional setup preamble. Counts as zero coverage.
    Skip,
    /// No assertions of any kind. Counts as zero coverage.
    Empty,
}

impl TestBodyKind {
    /// Stable kebab-case name used in the diagnostic body so the
    /// author can read which functions tripped the rule.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HasSuccessAssertion => "HasSuccessAssertion",
            Self::ErrorOnly => "ErrorOnly",
            Self::Skip => "Skip",
            Self::Empty => "Empty",
        }
    }
}

/// One `_test.go` file that contains no success-path coverage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Path relative to the workspace root.
    pub relative_path: PathBuf,
    /// Absolute path on disk — surfaced in the diagnostic.
    pub absolute_path: PathBuf,
    /// Per-function categorisation. Surfaced in the diagnostic body.
    pub body_kinds: Vec<(String, TestBodyKind)>,
}

impl Finding {
    /// Stable diagnostic code emitted with every finding.
    pub const CODE: &'static str = "TEST-FAILURE-ONLY-COVERAGE-001";

    /// Render the user-facing diagnostic body.
    pub fn message(&self) -> String {
        let mut body = format!(
            "{}: test file declares {} `func Test*` function(s), none of which exercise the handler's success path. Per-function shape:",
            self.relative_path.display(),
            self.body_kinds.len(),
        );
        for (name, kind) in &self.body_kinds {
            body.push_str("\n  - ");
            body.push_str(name);
            body.push_str(": ");
            body.push_str(kind.as_str());
        }
        body.push_str(
            "\n  --> Add a success-path test (`require.NoError(t, err)` + a positive \
             assertion on the result) or opt out with \
             `# doctor:allow TEST-FAILURE-ONLY-COVERAGE-001 — reason \"...\"`.",
        );
        body
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Walk every Go handler source file, emit findings for `_test.go`
/// files whose every `Test*` body is `ErrorOnly` / `Skip` / `Empty`.
///
/// The rule is per-file: a file with at least one `HasSuccessAssertion`
/// body silences the rule for the whole file (mixed
/// negative/positive coverage is the healthy shape).
///
/// ## Examples
///
/// ```no_run
/// use lazuli_doctor::error_handling::walker::walk_workspace_go_handlers;
/// use lazuli_doctor::test_discipline::test_failure_only_coverage_001::check;
/// use std::path::Path;
///
/// let files = walk_workspace_go_handlers(Path::new("/path/to/app"));
/// for f in check(&files) {
///     eprintln!("failure-only: {}", f.relative_path.display());
/// }
/// ```
pub fn check(files: &[GoHandlerSourceFile]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for file in files {
        if !file.is_test {
            continue;
        }
        if source_contains_doctor_allow(&file.source, Finding::CODE) {
            continue;
        }
        if filename_is_negative_carve_out(&file.relative_path) {
            continue;
        }
        if path_under_silent_dir(&file.relative_path) {
            continue;
        }
        let bodies = extract_test_bodies(&file.source);
        if bodies.is_empty() {
            // No `Test*` entrypoints at all (helpers-only file). The
            // pairing-rule cousin (TEST-HANDLER-MISSING-001) handles
            // that surface; this rule is silent.
            continue;
        }
        let any_success = bodies
            .iter()
            .any(|b| matches!(b.kind, TestBodyKind::HasSuccessAssertion));
        if any_success {
            continue;
        }
        findings.push(Finding {
            relative_path: file.relative_path.clone(),
            absolute_path: file.absolute_path.clone(),
            body_kinds: bodies.into_iter().map(|b| (b.func_name, b.kind)).collect(),
        });
    }
    findings
}

// ── internals ─────────────────────────────────────────────────────────────────

/// Carve-out for filename suffixes that signal negative-only intent
/// by convention. Matches the closed catalog from the spec.
fn filename_is_negative_carve_out(path: &std::path::Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    const NEGATIVE_SUFFIXES: &[&str] = &[
        "_reject_test.go",
        "_invalid_test.go",
        "_denies_test.go",
        "_negative_test.go",
    ];
    NEGATIVE_SUFFIXES.iter().any(|suf| name.ends_with(suf))
}

/// Carve-out for path segments that mark scaffolding directories.
/// Anything under `staging/` or `wip/` is exempt.
fn path_under_silent_dir(path: &std::path::Path) -> bool {
    path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .map(|s| matches!(s, "staging" | "wip"))
            .unwrap_or(false)
    })
}

/// One `func Test*` body, with its name and shape category.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TestBody {
    func_name: String,
    kind: TestBodyKind,
}

/// Closed catalog of success-side assertion markers (per spec §"Closed
/// catalog of success markers"). Substring match; first-argument is
/// not the literal token `err` (cheap parity scan).
const SUCCESS_ASSERTIONS: &[&str] = &[
    "require.NoError(",
    "assert.NoError(",
    "assert.NotEmpty(",
    "assert.NotNil(",
    "assert.True(",
    "assert.Equal(",
    "assert.EqualValues(",
    "assert.ElementsMatch(",
    "assert.JSONEq(",
    "require.True(",
    "require.NotNil(",
    "require.NotEmpty(",
    "require.Equal(",
    "require.EqualValues(",
];

/// Closed catalog of error-only assertion markers.
const ERROR_ASSERTIONS: &[&str] = &[
    "require.Error(",
    "assert.Error(",
    "assert.ErrorIs(",
    "assert.ErrorAs(",
    "require.ErrorIs(",
    "require.ErrorAs(",
];

/// Substrings that mean "this assertion targets `err`, not a real
/// result". Used to defuse `assert.NotEmpty(t, err.Error())` false
/// positives.
const ERR_TARGET_HINTS: &[&str] = &["err)", "err.Error()", "err,"];

/// Skip-marker prefixes.
const SKIP_PREFIXES: &[&str] = &["t.Skip(", "t.Skipf(", "t.SkipNow("];

/// Walk `source` byte-by-byte, finding every `func Test*(...) {`
/// header and extracting its body via brace-depth tracking. Returns
/// one [`TestBody`] per discovered function.
fn extract_test_bodies(source: &str) -> Vec<TestBody> {
    let mut bodies = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Cheap line-anchored search: only consider `func ` at the
        // start of a line (after optional whitespace). Skips `func`
        // appearing inside string literals / comments / nested
        // closures.
        let line_start = i == 0
            || bytes[i.saturating_sub(1)] == b'\n';
        if !line_start {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
            j += 1;
        }
        if !bytes_starts_with(bytes, j, b"func ") {
            i += 1;
            continue;
        }
        let after_func = j + b"func ".len();
        if !bytes_starts_with(bytes, after_func, b"Test") {
            i = after_func;
            continue;
        }
        // Parse the function name: `Test<rest>` up to `(`.
        let name_start = after_func;
        let mut name_end = name_start;
        while name_end < bytes.len() && bytes[name_end] != b'(' && bytes[name_end] != b' ' {
            name_end += 1;
        }
        if name_end >= bytes.len() {
            break;
        }
        let func_name = std::str::from_utf8(&bytes[name_start..name_end])
            .unwrap_or("")
            .to_owned();
        // Validate next char after `Test` — reject `TestifyHelper`.
        let post_test = name_start + b"Test".len();
        if let Some(next) = bytes.get(post_test).copied() {
            if !(next.is_ascii_uppercase()
                || next.is_ascii_digit()
                || next == b'_'
                || next == b'(')
            {
                i = name_end;
                continue;
            }
        } else {
            break;
        }
        // Walk forward to the matching opening brace.
        let mut k = name_end;
        while k < bytes.len() && bytes[k] != b'{' {
            k += 1;
        }
        if k >= bytes.len() {
            break;
        }
        // Brace-depth body extraction.
        let body_start = k + 1;
        let mut depth: i32 = 1;
        let mut m = body_start;
        let mut in_string: Option<u8> = None;
        let mut in_line_comment = false;
        let mut in_block_comment = false;
        while m < bytes.len() && depth > 0 {
            let c = bytes[m];
            if in_line_comment {
                if c == b'\n' {
                    in_line_comment = false;
                }
                m += 1;
                continue;
            }
            if in_block_comment {
                if c == b'*' && bytes.get(m + 1) == Some(&b'/') {
                    in_block_comment = false;
                    m += 2;
                    continue;
                }
                m += 1;
                continue;
            }
            if let Some(quote) = in_string {
                if c == b'\\' {
                    m += 2;
                    continue;
                }
                if c == quote {
                    in_string = None;
                }
                m += 1;
                continue;
            }
            match c {
                b'/' if bytes.get(m + 1) == Some(&b'/') => {
                    in_line_comment = true;
                    m += 2;
                }
                b'/' if bytes.get(m + 1) == Some(&b'*') => {
                    in_block_comment = true;
                    m += 2;
                }
                b'"' | b'`' | b'\'' => {
                    in_string = Some(c);
                    m += 1;
                }
                b'{' => {
                    depth += 1;
                    m += 1;
                }
                b'}' => {
                    depth -= 1;
                    m += 1;
                }
                _ => m += 1,
            }
        }
        let body_end = m.saturating_sub(1);
        let body = std::str::from_utf8(&bytes[body_start..body_end]).unwrap_or("");
        let kind = categorise_body(body);
        bodies.push(TestBody { func_name, kind });
        i = m;
    }
    bodies
}

fn bytes_starts_with(haystack: &[u8], offset: usize, needle: &[u8]) -> bool {
    if offset + needle.len() > haystack.len() {
        return false;
    }
    &haystack[offset..offset + needle.len()] == needle
}

/// Categorise one function body. Order matters: success-assertion
/// wins over error-only; skip wins over empty.
fn categorise_body(body: &str) -> TestBodyKind {
    if body_has_success_assertion(body) {
        return TestBodyKind::HasSuccessAssertion;
    }
    if body_starts_with_skip(body) {
        return TestBodyKind::Skip;
    }
    if body_has_error_assertion(body) {
        return TestBodyKind::ErrorOnly;
    }
    if body_has_any_assertion_marker(body) {
        // Some assertion-looking call we didn't classify — treat as
        // success so we don't fire on benign branches.
        return TestBodyKind::HasSuccessAssertion;
    }
    TestBodyKind::Empty
}

fn body_has_success_assertion(body: &str) -> bool {
    SUCCESS_ASSERTIONS.iter().any(|marker| {
        let mut search_from = 0;
        while let Some(rel) = body[search_from..].find(marker) {
            let abs = search_from + rel;
            let after = &body[abs + marker.len()..];
            // Defuse `assert.NotEmpty(t, err.Error())` and similar —
            // if the call's tail leads with an `err` token, it's an
            // error-side assertion masquerading as positive.
            let trimmed = after.trim_start();
            // Skip the leading `t, ` if present (testify convention).
            let after_t = if let Some(rest) = trimmed.strip_prefix("t,") {
                rest.trim_start()
            } else if let Some(rest) = trimmed.strip_prefix("t ,") {
                rest.trim_start()
            } else {
                trimmed
            };
            let is_err_target = ERR_TARGET_HINTS
                .iter()
                .any(|hint| after_t.starts_with(hint.trim_end_matches([',', ')'])));
            if !is_err_target {
                return true;
            }
            search_from = abs + marker.len();
        }
        false
    })
}

fn body_has_error_assertion(body: &str) -> bool {
    if ERROR_ASSERTIONS.iter().any(|m| body.contains(m)) {
        return true;
    }
    // `assert.Contains(t, err.Error(), ...)` / `require.Contains(...,
    // err.Error(), ...)` — detect by the `err.Error()` substring
    // inside any `Contains(` call.
    for marker in &["assert.Contains(", "require.Contains("] {
        let mut search_from = 0;
        while let Some(rel) = body[search_from..].find(marker) {
            let abs = search_from + rel;
            let tail = &body[abs..];
            // Cheap: look at the next 200 chars for `err.Error()`.
            let window_end = tail.len().min(200);
            if tail[..window_end].contains("err.Error()") {
                return true;
            }
            search_from = abs + marker.len();
        }
    }
    false
}

fn body_starts_with_skip(body: &str) -> bool {
    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        // Skip past common preamble lines: `ctx, cleanup := ...`,
        // `defer cleanup()`, `t.Helper()`, `t.Parallel()`.
        if line.starts_with("ctx")
            || line.starts_with("defer ")
            || line.starts_with("t.Helper(")
            || line.starts_with("t.Parallel(")
            || line.starts_with("//")
            || line.starts_with("var ")
        {
            continue;
        }
        return SKIP_PREFIXES.iter().any(|p| line.starts_with(p));
    }
    false
}

fn body_has_any_assertion_marker(body: &str) -> bool {
    // Any `assert.` / `require.` / `t.Error` call counts as "some
    // assertion exists, just not in our catalog". Used to defuse
    // false positives on custom assertion helpers — we only fire
    // when the body is GENUINELY empty.
    body.contains("assert.") || body.contains("require.") || body.contains("t.Error")
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn mk_file(path: &str, source: &str) -> GoHandlerSourceFile {
        GoHandlerSourceFile {
            feature_name: "account".to_owned(),
            bucket: "handlers".to_owned(),
            relative_path: PathBuf::from(path),
            absolute_path: PathBuf::from(format!("/abs/{}", path)),
            source: source.to_owned(),
            loc_count: source.lines().count(),
            is_test: path.ends_with("_test.go"),
        }
    }

    #[test]
    fn positive_only_require_error_fires() {
        let src = "package h\n\nfunc TestFoo(t *testing.T) {\n  require.Error(t, err)\n}\n";
        let f = mk_file("features/account/handlers/foo_test.go", src);
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].body_kinds[0].0, "TestFoo");
        assert_eq!(findings[0].body_kinds[0].1, TestBodyKind::ErrorOnly);
        assert_eq!(Finding::CODE, "TEST-FAILURE-ONLY-COVERAGE-001");
    }

    #[test]
    fn positive_skip_fires() {
        let src = "package h\n\nfunc TestFoo(t *testing.T) {\n  t.Skip(\"needs creds\")\n}\n";
        let f = mk_file("features/account/handlers/foo_test.go", src);
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].body_kinds[0].1, TestBodyKind::Skip);
    }

    #[test]
    fn positive_pilot_incident_replay_fires() {
        // Verbatim shape of the 2026-05-27 hostpoint incident: one
        // ErrorOnly body + one Skip body.
        let src = r#"package h

func TestRegisterWithGoogle_ReturnsNotImplementedStub(t *testing.T) {
    ctx, cleanup := testsupport.Setup(t)
    defer cleanup()
    _, err := RegisterWithGoogle(ctx, gen.Input{IDToken: "any"})
    require.Error(t, err)
    assert.Contains(t, err.Error(), "not implemented")
}

func TestRegisterWithGoogle_RealGoogleOIDCFlow(t *testing.T) {
    t.Skip("requires Google OIDC credentials")
}
"#;
        let f = mk_file(
            "features/account/handlers/register_with_google_test.go",
            src,
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].body_kinds.len(), 2);
        let kinds: Vec<_> = findings[0].body_kinds.iter().map(|(_, k)| *k).collect();
        assert!(kinds.contains(&TestBodyKind::ErrorOnly));
        assert!(kinds.contains(&TestBodyKind::Skip));
        let msg = findings[0].message();
        assert!(msg.contains("TestRegisterWithGoogle_ReturnsNotImplementedStub"));
        assert!(msg.contains("TestRegisterWithGoogle_RealGoogleOIDCFlow"));
        assert!(msg.contains("ErrorOnly"));
        assert!(msg.contains("Skip"));
    }

    #[test]
    fn positive_empty_body_fires() {
        let src = "package h\n\nfunc TestFoo(t *testing.T) {}\n";
        let f = mk_file("features/account/handlers/foo_test.go", src);
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].body_kinds[0].1, TestBodyKind::Empty);
    }

    #[test]
    fn negative_success_assertion_silent() {
        let src = r#"package h

func TestFoo(t *testing.T) {
    result, err := DoIt(ctx)
    require.NoError(t, err)
    assert.Equal(t, "ok", result)
}
"#;
        let f = mk_file("features/account/handlers/foo_test.go", src);
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn negative_mixed_success_and_failure_silent() {
        // File has a Success body AND an ErrorOnly body — mixed
        // coverage is the healthy shape; rule does not fire.
        let src = r#"package h

func TestFoo_Success(t *testing.T) {
    result, err := DoIt(ctx)
    require.NoError(t, err)
    assert.NotEmpty(t, result)
}

func TestFoo_RejectsBad(t *testing.T) {
    _, err := DoIt(ctx)
    require.Error(t, err)
}
"#;
        let f = mk_file("features/account/handlers/foo_test.go", src);
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn negative_reject_suffix_silent() {
        let src = "package h\n\nfunc TestFoo(t *testing.T) {\n  require.Error(t, err)\n}\n";
        let f = mk_file(
            "features/account/handlers/validate_reject_test.go",
            src,
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn negative_invalid_suffix_silent() {
        let src = "package h\n\nfunc TestFoo(t *testing.T) {\n  require.Error(t, err)\n}\n";
        let f = mk_file("features/account/handlers/parse_invalid_test.go", src);
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn negative_staging_path_silent() {
        let src = "package h\n\nfunc TestFoo(t *testing.T) {\n  require.Error(t, err)\n}\n";
        let f = mk_file("features/account/handlers/staging/foo_test.go", src);
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn negative_file_allow_comment_silences() {
        let src = r#"# doctor:allow TEST-FAILURE-ONLY-COVERAGE-001 — reason "covered in e2e"
package h

func TestFoo(t *testing.T) {
    require.Error(t, err)
}
"#;
        let f = mk_file("features/account/handlers/foo_test.go", src);
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn negative_no_test_functions_silent() {
        // Helper-only file (no `func Test*`). Out of scope.
        let src = "package h\n\nfunc setUp(t *testing.T) {}\n";
        let f = mk_file("features/account/handlers/helpers_test.go", src);
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn negative_assert_notempty_on_err_does_not_count_as_success() {
        // `assert.NotEmpty(t, err.Error())` is an error-side
        // assertion in disguise — must NOT promote to success.
        let src = r#"package h

func TestFoo(t *testing.T) {
    _, err := DoIt(ctx)
    require.Error(t, err)
    assert.NotEmpty(t, err.Error())
}
"#;
        let f = mk_file("features/account/handlers/foo_test.go", src);
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].body_kinds[0].1, TestBodyKind::ErrorOnly);
    }

    #[test]
    fn negative_non_test_go_file_silent() {
        // The walker yields handler `.go` files too; the rule must
        // only consider `*_test.go`.
        let src = "package h\n\nfunc Foo() {}\n";
        let f = mk_file("features/account/handlers/foo.go", src);
        // Manually clear is_test (the helper sets it via filename).
        let mut f = f;
        f.is_test = false;
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn message_lists_kinds_per_function() {
        let src = r#"package h

func TestA(t *testing.T) {
    require.Error(t, err)
}

func TestB(t *testing.T) {
    t.Skip("nope")
}
"#;
        let f = mk_file("features/account/handlers/foo_test.go", src);
        let findings = check(&[f]);
        let msg = findings[0].message();
        assert!(msg.contains("TestA: ErrorOnly"));
        assert!(msg.contains("TestB: Skip"));
        assert!(msg.contains("doctor:allow"));
    }
}
