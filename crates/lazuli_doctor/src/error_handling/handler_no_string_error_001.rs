//! `HANDLER-NO-STRING-ERROR-001` — flag ad-hoc string errors
//! (`errors.New("...")` / `fmt.Errorf("..." no %w/%v)`) inside Go
//! handler function bodies.
//!
//! Scans every non-test `.go` file the
//! [`crate::error_handling::walker::walk_workspace_go_handlers`] walker
//! yields. Two patterns fire:
//!
//! - `errors.New("literal")` invoked **inside a function body** (i.e.
//!   `brace_depth >= 1`). Package-level declarations like
//!   `var ErrFoo = errors.New("foo")` at file scope are the canonical
//!   sentinel-error pattern and are silent.
//! - `fmt.Errorf("literal")` with NO `%w` and NO `%v` formatting verb
//!   AND no argument after the format string — that's pure string
//!   formatting wrapping nothing, equivalent to `errors.New`.
//!
//! ## Why
//!
//! Lazuli's contract is: every handler error has a declared variant
//! with `messageKey`, `status`, and `retriable` metadata (see
//! `ERROR-DECLARED-EXHAUSTIVE-001` on the `.lzi` side). Ad-hoc
//! `errors.New("could not foo")` bypasses that contract — the caller
//! cannot route on it, the frontend has no `messageKey` to localize,
//! and the audit emitter has no variant name to log.
//!
//! Package-level `var ErrFoo = errors.New("foo")` is the **escape
//! hatch**: a named sentinel the handler returns and the caller routes
//! on via `errors.Is`. The rule recognizes that shape and stays silent.
//!
//! Default severity: `Warning`. Under `tdd-iron-hand` preset: `Error`.
//!
//! ## Heuristic
//!
//! Line-based, no `go/ast` dep. Per file:
//!
//! 1. Track block-comment depth (`/* ... */`) and strip the in-comment
//!    slice.
//! 2. Strip `// line comments`.
//! 3. Track brace depth via `{` / `}` count on each surviving line.
//!    `func ` declarations themselves don't open a brace until the
//!    `{` token (`func F() { ...` opens on the same line).
//! 4. For each line:
//!    - Allow `var Err` … `errors.New(` at any depth (sentinel).
//!    - Skip lines whose trimmed prefix is `//`.
//!    - At `brace_depth >= 1`, fire on `errors.New(`.
//!    - Always (regardless of depth) fire on `fmt.Errorf("...")` calls
//!      whose only argument is a string literal containing no `%w` /
//!      `%v` formatting verb (pure string error).
//!
//! ## Examples
//!
//! ```rust
//! use lazuli_doctor::error_handling::handler_no_string_error_001::check;
//! use lazuli_doctor::error_handling::walker::GoHandlerSourceFile;
//! use std::path::PathBuf;
//!
//! let f = GoHandlerSourceFile {
//!     feature_name: "auth".to_string(),
//!     bucket: "handlers".to_string(),
//!     relative_path: PathBuf::from("features/auth/handlers/login.go"),
//!     absolute_path: PathBuf::from("/abs/features/auth/handlers/login.go"),
//!     source: "package handlers\n\nfunc Login() error {\n  return errors.New(\"bad\")\n}\n".to_string(),
//!     loc_count: 5,
//!     is_test: false,
//! };
//! assert_eq!(check(&[f]).len(), 1);
//! ```
//!
//! ## See also
//!
//! - [`crate::error_handling::handler_error_wrap_001`] — sibling rule
//!   that flags `fmt.Errorf` calls with `%v`/`%s` instead of `%w`.

use std::path::{Path, PathBuf};

use crate::error_handling::walker::GoHandlerSourceFile;

/// One ad-hoc string-error construction in handler code.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Path relative to the workspace root.
    pub path: PathBuf,
    /// 1-based line number.
    pub line: usize,
    /// Feature this file belongs to.
    pub feature: String,
    /// Bucket the file sits in.
    pub bucket: String,
    /// Which construct triggered (`errors.New("...")` or `fmt.Errorf("...")`).
    pub construct: String,
}

impl Finding {
    /// Stable rule code.
    pub const CODE: &'static str = "HANDLER-NO-STRING-ERROR-001";

    /// Render the doctor-formatted diagnostic message.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::error_handling::handler_no_string_error_001::Finding;
    /// use std::path::PathBuf;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("features/auth/handlers/login.go"),
    ///     line: 9,
    ///     feature: "auth".to_string(),
    ///     bucket: "handlers".to_string(),
    ///     construct: "errors.New(...)".to_string(),
    /// };
    /// let msg = f.message();
    /// assert!(msg.contains(":9"));
    /// assert!(msg.contains("errors.New"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{}:{} `{}` constructs an ad-hoc string error inside a \
             handler body. Return a declared error variant (named in \
             the feature's `errors:` block, with `messageKey`, `status`, \
             and `retriable` set) — or hoist to a package-level \
             `var ErrFoo = errors.New(...)` sentinel and return that. \
             See `ERROR-DECLARED-EXHAUSTIVE-001`.",
            self.path.display(),
            self.line,
            self.construct,
        )
    }
}

/// Run the rule against a slice of pre-walked Go handler files.
///
/// Test files are skipped — Go tests often manufacture ad-hoc errors
/// for assertion fixtures and that's not a contract violation.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::error_handling::handler_no_string_error_001::check;
/// use lazuli_doctor::error_handling::walker::GoHandlerSourceFile;
/// use std::path::PathBuf;
///
/// let pkg_level = GoHandlerSourceFile {
///     feature_name: "billing".to_string(),
///     bucket: "domain".to_string(),
///     relative_path: PathBuf::from("features/billing/domain/errors.go"),
///     absolute_path: PathBuf::from("/abs/features/billing/domain/errors.go"),
///     source: "package domain\n\nvar ErrNotFound = errors.New(\"not found\")\n".to_string(),
///     loc_count: 3,
///     is_test: false,
/// };
/// assert!(check(&[pkg_level]).is_empty(), "package-level sentinel is allowed");
/// ```
pub fn check(files: &[GoHandlerSourceFile]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for file in files {
        if file.is_test {
            continue;
        }
        scan_file(
            &file.relative_path,
            &file.feature_name,
            &file.bucket,
            &file.source,
            &mut findings,
        );
    }
    findings
}

fn scan_file(path: &Path, feature: &str, bucket: &str, source: &str, out: &mut Vec<Finding>) {
    let mut block_depth: usize = 0;
    let mut brace_depth: usize = 0;

    for (idx, raw_line) in source.lines().enumerate() {
        let (stripped, depth_after) = strip_block_comments(raw_line, block_depth);
        block_depth = depth_after;

        let no_line_comment = strip_line_comment(&stripped);
        let trimmed = no_line_comment.trim_start();

        if trimmed.is_empty() || trimmed.starts_with("//") {
            // Still need to track braces across blank/comment lines.
            let (opens, closes) = count_braces_outside_strings(&no_line_comment);
            brace_depth = brace_depth.saturating_add(opens).saturating_sub(closes);
            continue;
        }

        // Sentinel-error shape: `var ErrFoo = errors.New("...")` at any
        // depth. Allow `var (` block sentinel groups too — if the line
        // starts with `var Err` we treat it as the canonical pattern.
        let sentinel = is_sentinel_var_decl(trimmed);

        // Walk the cleaned line char-by-char, tracking running brace
        // depth so a single-line `func A() error { return errors.New("a") }`
        // (depth opens to 1 at `{`, falls back to 0 at `}`) correctly
        // fires the errors.New finding at the moment we're inside the body.
        let mut depth = brace_depth;
        let mut in_str = false;
        let mut prev = b' ';
        let bytes = no_line_comment.as_bytes();
        let mut i = 0usize;
        let mut fired_errors_new = false;
        let mut fired_errorf = false;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'"' && prev != b'\\' {
                in_str = !in_str;
                prev = b;
                i += 1;
                continue;
            }
            if !in_str {
                if b == b'{' {
                    depth += 1;
                    prev = b;
                    i += 1;
                    continue;
                }
                if b == b'}' {
                    depth = depth.saturating_sub(1);
                    prev = b;
                    i += 1;
                    continue;
                }
                // Try to match `errors.New(` starting here.
                if !fired_errors_new
                    && depth >= 1
                    && !sentinel
                    && no_line_comment[i..].starts_with("errors.New(")
                    && has_call_boundary_before(&no_line_comment, i)
                {
                    out.push(Finding {
                        path: path.to_path_buf(),
                        line: idx + 1,
                        feature: feature.to_owned(),
                        bucket: bucket.to_owned(),
                        construct: "errors.New(...)".to_owned(),
                    });
                    fired_errors_new = true;
                }
                // `fmt.Errorf("literal")` at any depth — pure string error.
                if !fired_errorf
                    && no_line_comment[i..].starts_with("fmt.Errorf(")
                    && has_call_boundary_before(&no_line_comment, i)
                {
                    let after = &no_line_comment[i + "fmt.Errorf(".len()..];
                    if is_pure_string_errorf(after) {
                        out.push(Finding {
                            path: path.to_path_buf(),
                            line: idx + 1,
                            feature: feature.to_owned(),
                            bucket: bucket.to_owned(),
                            construct: "fmt.Errorf(...)".to_owned(),
                        });
                        fired_errorf = true;
                    }
                }
            }
            prev = b;
            i += 1;
        }
        brace_depth = depth;
    }
}

/// `var Err... = errors.New(...)` (single-line) OR `var Err... =
/// fmt.Errorf(...)` — these are the package-level sentinel forms the
/// rule deliberately allows. Also allow inside `var (...)` blocks via a
/// looser prefix check on `Err` identifiers.
///
/// Conservative: must START with `var Err` (case sensitive) or
/// `ErrSomething =` (interior of a `var (...)` block).
fn is_sentinel_var_decl(trimmed: &str) -> bool {
    if trimmed.starts_with("var Err") {
        return true;
    }
    // Inside `var ( ErrX = errors.New(...) )` blocks the inner lines
    // start with `ErrX` (no `var` keyword). Be conservative: require
    // the line to start with `Err` followed by uppercase letter or `_`,
    // and contain ` = ` before the constructor.
    if let Some(rest) = trimmed.strip_prefix("Err")
        && let Some(next) = rest.chars().next()
        && (next.is_ascii_uppercase() || next == '_' || next.is_ascii_alphanumeric())
        && trimmed.contains('=')
        && (trimmed.contains("errors.New(") || trimmed.contains("fmt.Errorf("))
    {
        return true;
    }
    false
}

/// Look at the substring AFTER `fmt.Errorf(` and decide if it's a pure
/// string-only call. Returns `true` when:
///
/// - The first non-whitespace token is a `"..."` literal.
/// - The format string contains no `%w` and no `%v` (and no `%s` /
///   `%+v` since those also signal an argument is being interpolated;
///   if there's no `,` after the literal that's still ad-hoc).
/// - After the closing `"`, the next non-whitespace char is `)` —
///   no further args.
///
/// The "no further args" check is what distinguishes
/// `fmt.Errorf("could not foo")` (ad-hoc, fires here) from
/// `fmt.Errorf("could not foo: %v", err)` (which is the
/// `HANDLER-ERROR-WRAP-001` rule's concern, not this one).
fn is_pure_string_errorf(after: &str) -> bool {
    let s = after.trim_start();
    if !s.starts_with('"') {
        return false;
    }
    // Find the closing unescaped quote.
    let bytes = s.as_bytes();
    let mut i = 1usize;
    let mut prev = b'"';
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' && prev != b'\\' {
            break;
        }
        prev = b;
        i += 1;
    }
    if i >= bytes.len() {
        return false;
    }
    let literal = &s[1..i];
    // Has any formatting verb that consumes an arg? Then this is NOT
    // a pure string error — let HANDLER-ERROR-WRAP-001 deal with it.
    if literal.contains("%w")
        || literal.contains("%v")
        || literal.contains("%+v")
        || literal.contains("%s")
        || literal.contains("%d")
    {
        return false;
    }
    // After closing quote, the next non-whitespace char must be `)`.
    let rest = &s[i + 1..];
    let trimmed = rest.trim_start();
    trimmed.starts_with(')')
}

/// `errors.New(` / `fmt.Errorf(` must be preceded by start-of-line or
/// a non-identifier char so identifiers that happen to end in
/// `errors` (e.g. `myErrors.New(`) don't trigger.
fn has_call_boundary_before(line: &str, idx: usize) -> bool {
    if idx == 0 {
        return true;
    }
    let prev = line.as_bytes()[idx - 1];
    !(prev.is_ascii_alphanumeric() || prev == b'_')
}

/// See `super::handler_no_panic_001::strip_block_comments` — same
/// shape, kept private here to avoid cross-rule visibility leakage.
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

/// Count `{` / `}` outside string literals on a single line. Used to
/// keep `brace_depth` accurate across blank / comment-only lines (the
/// main scan loop handles non-blank lines char-by-char with its own
/// running depth).
fn count_braces_outside_strings(line: &str) -> (usize, usize) {
    let mut opens = 0;
    let mut closes = 0;
    let mut in_str = false;
    let mut prev = b' ';
    for &b in line.as_bytes() {
        if b == b'"' && prev != b'\\' {
            in_str = !in_str;
        } else if !in_str {
            if b == b'{' {
                opens += 1;
            } else if b == b'}' {
                closes += 1;
            }
        }
        prev = b;
    }
    (opens, closes)
}

#[cfg(test)]
mod tests {
    include!("handler_no_string_error_001_tests.rs");
}
