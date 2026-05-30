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
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::test_discipline::test_failure_only_coverage_001::TestBodyKind;
    ///
    /// assert_eq!(TestBodyKind::HasSuccessAssertion.as_str(), "HasSuccessAssertion");
    /// assert_eq!(TestBodyKind::ErrorOnly.as_str(), "ErrorOnly");
    /// assert_eq!(TestBodyKind::Skip.as_str(), "Skip");
    /// assert_eq!(TestBodyKind::Empty.as_str(), "Empty");
    /// ```
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
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::test_discipline::test_failure_only_coverage_001::{Finding, TestBodyKind};
    ///
    /// let f = Finding {
    ///     relative_path: PathBuf::from("features/account/handlers/foo_test.go"),
    ///     absolute_path: PathBuf::from("/abs/features/account/handlers/foo_test.go"),
    ///     body_kinds: vec![("TestFoo".into(), TestBodyKind::ErrorOnly)],
    /// };
    /// assert!(f.message().contains("TestFoo"));
    /// assert!(f.message().contains("ErrorOnly"));
    /// ```
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
        let line_start = i == 0 || bytes[i.saturating_sub(1)] == b'\n';
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
            if !(next.is_ascii_uppercase() || next.is_ascii_digit() || next == b'_' || next == b'(')
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
