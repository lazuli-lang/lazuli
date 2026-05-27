//! `HANDLER-NO-PANIC-001` — flag literal `panic(...)` calls in
//! non-test user-app Go handler sources.
//!
//! Scans every `.go` file the
//! [`crate::error_handling::walker::walk_workspace_go_handlers`] walker
//! yields under `features/<feature>/{handlers,domain,jobs,integrations}/`.
//! Test files (`*_test.go` — `is_test == true`) are silent because
//! `panic` is the canonical way Go tests signal a fatal precondition
//! when `t.Fatal` isn't available (e.g. inside test helpers).
//!
//! ## Why
//!
//! `panic(...)` inside a Go HTTP / job / integration handler aborts the
//! goroutine and surfaces as an opaque 500 to the caller (or a silently
//! dropped job). Lazuli's error-handling discipline says every failure
//! path must be a declared, named error variant returned via the
//! handler's `Result`-shaped Go return signature — not a panic.
//!
//! Default severity: `Warning`. Under `tdd-iron-hand` preset: `Error`.
//!
//! ## Heuristic
//!
//! Line-based, no `go/ast` dep — same posture as the Rust walker. For
//! each line:
//!
//! 1. Track block-comment depth (`/* ... */`) across lines. Skip the
//!    line if it's entirely inside a block comment.
//! 2. Strip `// line comments` so panic-keywords inside trailing
//!    comments don't fire.
//! 3. Skip lines whose trimmed prefix is `//` (full-line comment).
//! 4. Scan the surviving prefix for `panic(`, ignoring matches whose
//!    `panic(` byte index falls inside a `"..."` string literal
//!    (cheap parity heuristic: count unescaped `"` to the left).
//!
//! ## What's NOT flagged
//!
//! - `*_test.go` files (Go test convention).
//! - `panic(` text inside string literals: `log.Print("never panic(")`.
//! - `panic(` text inside block comments: `/* TODO: panic(err) here */`.
//! - `panic(` text inside line comments: `// don't panic(err)`.
//! - Identifiers ending in `panic`, e.g. `nopanic()` — the heuristic
//!   anchors on `panic(` preceded by whitespace, `;`, `{`, `}`, `(`, or
//!   start-of-line, so `runtime.panic(` and `myPanic(` are flagged but
//!   `nopanic(` and `panicOnce(` are NOT (because the match would be
//!   inside the identifier, not at a boundary).
//!
//! ## Examples
//!
//! ```rust
//! use lazuli_doctor::error_handling::handler_no_panic_001::check;
//! use lazuli_doctor::error_handling::walker::GoHandlerSourceFile;
//! use std::path::PathBuf;
//!
//! let f = GoHandlerSourceFile {
//!     feature_name: "auth".to_string(),
//!     bucket: "handlers".to_string(),
//!     relative_path: PathBuf::from("features/auth/handlers/login.go"),
//!     absolute_path: PathBuf::from("/abs/features/auth/handlers/login.go"),
//!     source: "package handlers\nfunc Login() { panic(\"nope\") }\n".to_string(),
//!     loc_count: 2,
//!     is_test: false,
//! };
//! let findings = check(&[f]);
//! assert_eq!(findings.len(), 1);
//! assert_eq!(findings[0].line, 2);
//! ```
//!
//! ## See also
//!
//! - [`crate::error_handling::preset`] — `tdd-iron-hand` escalates this
//!   rule to `Error`.
//! - [`crate::error_handling::handler_no_string_error_001`] — sibling
//!   rule covering `errors.New(...)` inside function bodies.

use std::path::{Path, PathBuf};

use crate::error_handling::walker::GoHandlerSourceFile;

/// One literal `panic(...)` call site in a non-test handler source.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Path relative to the workspace root.
    pub path: PathBuf,
    /// 1-based line number where the `panic(` token starts.
    pub line: usize,
    /// Feature this file belongs to (for grouping diagnostics).
    pub feature: String,
    /// Bucket the file sits in (`handlers`, `domain`, `jobs`,
    /// `integrations`).
    pub bucket: String,
}

impl Finding {
    /// Stable rule code.
    pub const CODE: &'static str = "HANDLER-NO-PANIC-001";

    /// Render the doctor-formatted diagnostic message.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::error_handling::handler_no_panic_001::Finding;
    /// use std::path::PathBuf;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("features/auth/handlers/login.go"),
    ///     line: 12,
    ///     feature: "auth".to_string(),
    ///     bucket: "handlers".to_string(),
    /// };
    /// let msg = f.message();
    /// assert!(msg.contains(":12"));
    /// assert!(msg.contains("panic("));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{}:{} `panic(...)` in non-test Go handler aborts the \
             goroutine and surfaces as an opaque 500 (or a dropped \
             job). Return a declared error variant via the handler's \
             `Result`-shaped Go signature per Lazuli error-handling \
             discipline. Test files (`*_test.go`) are exempt.",
            self.path.display(),
            self.line,
        )
    }
}

/// Run the rule against a slice of pre-walked Go handler files.
///
/// `*_test.go` files (`is_test == true`) are silently skipped — Go test
/// convention permits panics.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::error_handling::handler_no_panic_001::check;
/// use lazuli_doctor::error_handling::walker::GoHandlerSourceFile;
/// use std::path::PathBuf;
///
/// let f = GoHandlerSourceFile {
///     feature_name: "billing".to_string(),
///     bucket: "jobs".to_string(),
///     relative_path: PathBuf::from("features/billing/jobs/reconcile.go"),
///     absolute_path: PathBuf::from("/abs/features/billing/jobs/reconcile.go"),
///     source: "package jobs\n\nfunc Run() { panic(\"boom\") }\n".to_string(),
///     loc_count: 3,
///     is_test: false,
/// };
/// assert_eq!(check(&[f]).len(), 1);
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

fn scan_file(
    path: &Path,
    feature: &str,
    bucket: &str,
    source: &str,
    out: &mut Vec<Finding>,
) {
    let mut block_comment_depth: usize = 0;

    for (idx, raw_line) in source.lines().enumerate() {
        let (stripped, depth_after) = strip_block_comments(raw_line, block_comment_depth);
        block_comment_depth = depth_after;

        // After block-comment stripping, drop `// line comments`.
        let no_line_comment = strip_line_comment(&stripped);
        let trimmed = no_line_comment.trim_start();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        if line_contains_panic_call(&no_line_comment) {
            out.push(Finding {
                path: path.to_path_buf(),
                line: idx + 1,
                feature: feature.to_owned(),
                bucket: bucket.to_owned(),
            });
        }
    }
}

/// Remove the slice of a line that's inside a `/* ... */` block
/// comment, tracking depth across lines. Returns the surviving prefix
/// (with the in-comment slice replaced by spaces so column positions of
/// surviving content don't shift) plus the block-comment depth after
/// this line.
///
/// Go does not nest block comments per spec, but a depth counter is a
/// safer-than-bool heuristic and matches the panic_unwrap_001 posture.
fn strip_block_comments(line: &str, mut depth: usize) -> (String, usize) {
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if depth == 0 {
            // Look for `/*` opener — but only outside a string literal.
            // Cheap heuristic: if we're inside a `"..."` literal on this
            // line, treat `/*` as content.
            if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' && !in_string_at(line, i) {
                depth += 1;
                out.push(' ');
                out.push(' ');
                i += 2;
                continue;
            }
            out.push(bytes[i] as char);
            i += 1;
        } else {
            // Inside block comment — replace with space.
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
/// literal on this line. Counts unescaped `"` to the left of `idx`;
/// odd count means we're inside a literal.
///
/// Cheap heuristic — does not understand Go raw strings (backticks);
/// the alternative would be a full mini-lexer. The trade-off is the
/// same one panic_unwrap_001 makes for Rust raw strings.
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

/// Return `true` if the line contains a `panic(` token outside any
/// string literal, anchored at a boundary so identifiers like
/// `nopanic(` and `panicOnce(` don't fire.
fn line_contains_panic_call(line: &str) -> bool {
    let needle = "panic(";
    let mut start = 0;
    while let Some(rel) = line[start..].find(needle) {
        let abs = start + rel;
        if !in_string_at(line, abs) && has_call_boundary_before(line, abs) {
            return true;
        }
        start = abs + needle.len();
    }
    false
}

/// `panic(` must be preceded by start-of-line or one of: whitespace,
/// `;`, `{`, `}`, `(`, `,`, `=`, `&`, `|`, `!`, `:`, `.`. The `.`
/// allows `runtime.panic(` (qualified). An identifier char (letter,
/// digit, `_`) means we're matching the tail of a longer identifier
/// like `noPanic` — silent.
fn has_call_boundary_before(line: &str, idx: usize) -> bool {
    if idx == 0 {
        return true;
    }
    let prev = line.as_bytes()[idx - 1];
    !(prev.is_ascii_alphanumeric() || prev == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handler_file(source: &str, is_test: bool) -> GoHandlerSourceFile {
        GoHandlerSourceFile {
            feature_name: "auth".to_owned(),
            bucket: "handlers".to_owned(),
            relative_path: PathBuf::from("features/auth/handlers/login.go"),
            absolute_path: PathBuf::from("/abs/features/auth/handlers/login.go"),
            source: source.to_owned(),
            loc_count: source.lines().count(),
            is_test,
        }
    }

    #[test]
    fn bare_panic_fires() {
        let f = handler_file(
            "package handlers\n\nfunc Login() {\n    panic(\"never\")\n}\n",
            false,
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 4);
        assert_eq!(findings[0].feature, "auth");
        assert_eq!(findings[0].bucket, "handlers");
    }

    #[test]
    fn panic_in_test_file_does_not_fire() {
        let f = handler_file(
            "package handlers\n\nfunc TestLogin(t *T) { panic(\"oops\") }\n",
            true,
        );
        assert!(check(&[f]).is_empty(), "panic in _test.go is allowed");
    }

    #[test]
    fn panic_in_line_comment_does_not_fire() {
        let f = handler_file(
            "package handlers\n\nfunc Login() {\n    // never panic(here)\n    return\n}\n",
            false,
        );
        assert!(check(&[f]).is_empty(), "panic inside // comment is silent");
    }

    #[test]
    fn panic_in_block_comment_does_not_fire() {
        let f = handler_file(
            "package handlers\n\n/*\nTODO: panic(err)\nlater\n*/\nfunc Login() {}\n",
            false,
        );
        assert!(check(&[f]).is_empty(), "panic inside /* */ is silent");
    }

    #[test]
    fn panic_inside_string_literal_does_not_fire() {
        let f = handler_file(
            "package handlers\n\nfunc Login() {\n    log.Print(\"never panic(err)\")\n}\n",
            false,
        );
        assert!(
            check(&[f]).is_empty(),
            "panic token inside string literal is silent"
        );
    }

    #[test]
    fn identifier_ending_in_panic_does_not_fire() {
        let f = handler_file(
            "package handlers\n\nfunc Login() {\n    nopanic()\n    safePanic()\n}\n",
            false,
        );
        let findings = check(&[f]);
        // `nopanic(` is silent (would match inside identifier — boundary check rejects).
        // `safePanic(` is silent — capital P doesn't match `panic`.
        assert!(
            findings.is_empty(),
            "identifiers ending in panic/Panic are not flagged; got {findings:?}"
        );
    }

    #[test]
    fn qualified_panic_call_fires() {
        let f = handler_file(
            "package handlers\n\nfunc Login() {\n    runtime.panic(\"qualified\")\n}\n",
            false,
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1, "qualified runtime.panic(...) fires");
    }

    #[test]
    fn multiple_panics_each_get_a_finding() {
        let f = handler_file(
            "package handlers\n\nfunc A() { panic(1) }\nfunc B() { panic(2) }\n",
            false,
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].line, 3);
        assert_eq!(findings[1].line, 4);
    }

    #[test]
    fn block_comment_spanning_panic_is_silent() {
        let f = handler_file(
            "package handlers\n\nfunc A() {\n  /* start\n  panic(\"hidden\")\n  end */\n}\n",
            false,
        );
        assert!(check(&[f]).is_empty(), "panic across /* ... */ is silent");
    }

    #[test]
    fn trailing_line_comment_with_panic_is_silent() {
        let f = handler_file(
            "package handlers\n\nfunc A() {\n  log.Print(\"ok\") // panic(here)\n}\n",
            false,
        );
        assert!(
            check(&[f]).is_empty(),
            "trailing // comment with panic is silent"
        );
    }

    #[test]
    fn message_includes_path_and_line() {
        let f = handler_file(
            "package handlers\n\nfunc A() { panic(1) }\n",
            false,
        );
        let finding = check(&[f]).into_iter().next().unwrap();
        let msg = finding.message();
        assert!(msg.contains("login.go"));
        assert!(msg.contains(":3"));
        assert!(msg.contains("panic("));
    }
}
