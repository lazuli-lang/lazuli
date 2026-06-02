//! `HANDLER-ERROR-WRAP-001` — flag `fmt.Errorf` calls that interpolate
//! an `error` via `%v` / `%s` / `%+v` instead of the chain-preserving
//! `%w` verb.
//!
//! Scans every non-test `.go` file the
//! [`crate::error_handling::walker::walk_workspace_go_handlers`] walker
//! yields under `features/<feature>/{handlers,domain,jobs,integrations}/`.
//!
//! ## Why
//!
//! Go's `fmt.Errorf("...: %w", err)` returns an error that wraps the
//! cause — `errors.Is(wrapped, sentinel)` and `errors.As(wrapped,
//! &target)` walk the chain and find the inner cause. Using `%v` /
//! `%s` / `%+v` instead stringifies the cause and DROPS the chain —
//! the wrapper becomes a leaf error and `errors.Is` / `errors.As`
//! return false. That breaks every consumer that needs to discriminate
//! between declared error variants (HTTP status mapping, retriable
//! classification, audit emit, frontend `messageKey` routing).
//!
//! Default severity: `Warning`. Under `tdd-iron-hand` preset: `Error`.
//!
//! ## Heuristic
//!
//! Line-based, no `go/ast` dep. Per line (after stripping block /
//! line comments):
//!
//! 1. Locate `fmt.Errorf(` (at a call boundary so identifiers like
//!    `myfmt.Errorf(` don't trigger).
//! 2. Extract the format-string literal between the first pair of
//!    unescaped `"..."`.
//! 3. If the literal contains `%w`, the call is already correct —
//!    silent.
//! 4. If the literal contains any of `%v`, `%s`, `%+v` AND there is at
//!    least one comma after the closing quote (signaling a trailing
//!    arg), fire.
//!
//! ## What's NOT flagged
//!
//! - `fmt.Errorf("...: %w", err)` — wrapping is correct.
//! - `fmt.Errorf("literal")` — pure string error (no args). That's
//!   the `HANDLER-NO-STRING-ERROR-001` rule's concern.
//! - `fmt.Sprintf("...: %v", err)` — not an error constructor.
//! - Multi-line `fmt.Errorf(` calls whose format string is on a
//!   different line than the verbs. The line-based heuristic only
//!   inspects the literal that starts on the same line; the trade-off
//!   matches sibling rules.
//! - Test files (`*_test.go` — Go convention).
//!
//! ## Examples
//!
//! ```rust
//! use lazuli_doctor::error_handling::handler_error_wrap_001::check;
//! use lazuli_doctor::error_handling::walker::GoHandlerSourceFile;
//! use std::path::PathBuf;
//!
//! let f = GoHandlerSourceFile {
//!     feature_name: "billing".to_string(),
//!     bucket: "jobs".to_string(),
//!     relative_path: PathBuf::from("features/billing/jobs/charge.go"),
//!     absolute_path: PathBuf::from("/abs/features/billing/jobs/charge.go"),
//!     source: "package jobs\nfunc Run(err error) error {\n  return fmt.Errorf(\"charge: %v\", err)\n}\n".to_string(),
//!     loc_count: 4,
//!     is_test: false,
//! };
//! let findings = check(&[f]);
//! assert_eq!(findings.len(), 1);
//! assert_eq!(findings[0].verb, "%v");
//! ```
//!
//! ## See also
//!
//! - [`crate::error_handling::handler_no_string_error_001`] — sibling
//!   rule covering `fmt.Errorf("literal")` calls with no formatting verb.

use std::path::{Path, PathBuf};

use crate::error_handling::walker::GoHandlerSourceFile;

/// One `fmt.Errorf` call with a non-`%w` error formatting verb.
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
    /// Which non-`%w` verb the rule saw (`"%v"`, `"%s"`, `"%+v"`).
    pub verb: String,
}

impl Finding {
    /// Stable rule code.
    pub const CODE: &'static str = "HANDLER-ERROR-WRAP-001";

    /// Render the doctor-formatted diagnostic message.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::error_handling::handler_error_wrap_001::Finding;
    /// use std::path::PathBuf;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("features/billing/jobs/charge.go"),
    ///     line: 17,
    ///     feature: "billing".to_string(),
    ///     bucket: "jobs".to_string(),
    ///     verb: "%v".to_string(),
    /// };
    /// let msg = f.message();
    /// assert!(msg.contains(":17"));
    /// assert!(msg.contains("%v"));
    /// assert!(msg.contains("%w"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{}:{} `fmt.Errorf` uses `{}` to interpolate the cause — \
             this stringifies it and drops the error chain. Use `%w` \
             so `errors.Is` / `errors.As` can walk the chain and the \
             feature's declared error variants stay routable.",
            self.path.display(),
            self.line,
            self.verb,
        )
    }
}

/// Run the rule against a slice of pre-walked Go handler files.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::error_handling::handler_error_wrap_001::check;
/// use lazuli_doctor::error_handling::walker::GoHandlerSourceFile;
/// use std::path::PathBuf;
///
/// let ok = GoHandlerSourceFile {
///     feature_name: "billing".to_string(),
///     bucket: "jobs".to_string(),
///     relative_path: PathBuf::from("features/billing/jobs/charge.go"),
///     absolute_path: PathBuf::from("/abs/features/billing/jobs/charge.go"),
///     source: "package jobs\nfunc Run(err error) error {\n  return fmt.Errorf(\"charge: %w\", err)\n}\n".to_string(),
///     loc_count: 4,
///     is_test: false,
/// };
/// assert!(check(&[ok]).is_empty(), "%w wrapping is correct");
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

    for (idx, raw_line) in source.lines().enumerate() {
        let (stripped, depth_after) = strip_block_comments(raw_line, block_depth);
        block_depth = depth_after;

        let no_line_comment = strip_line_comment(&stripped);
        let trimmed = no_line_comment.trim_start();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        // Find every `fmt.Errorf(` call on the line; check each.
        let mut search_from = 0usize;
        while let Some(rel) = no_line_comment[search_from..].find("fmt.Errorf(") {
            let abs = search_from + rel;
            search_from = abs + "fmt.Errorf(".len();
            if in_string_at(&no_line_comment, abs) {
                continue;
            }
            if !has_call_boundary_before(&no_line_comment, abs) {
                continue;
            }
            let after = &no_line_comment[abs + "fmt.Errorf(".len()..];
            if let Some(verb) = bad_wrap_verb(after) {
                out.push(Finding {
                    path: path.to_path_buf(),
                    line: idx + 1,
                    feature: feature.to_owned(),
                    bucket: bucket.to_owned(),
                    verb: verb.to_owned(),
                });
            }
        }
    }
}

/// Inspect the substring AFTER `fmt.Errorf(`. Return the offending
/// verb (`"%v"`, `"%s"`, or `"%+v"`) if:
///
/// - The format-string literal between the first `"..."` contains one
///   of those verbs but does NOT contain `%w`.
/// - At least one `,` exists after the closing quote (i.e. an arg is
///   being passed in to be interpolated).
///
/// Returns `None` if the call uses `%w`, has no offending verb, or
/// has no trailing arg (pure string).
fn bad_wrap_verb(after: &str) -> Option<&'static str> {
    let s = after.trim_start();
    if !s.starts_with('"') {
        return None;
    }
    // Find closing unescaped quote.
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
        return None;
    }
    let literal = &s[1..i];
    // `%w` already correct.
    if literal.contains("%w") {
        return None;
    }
    // After the closing quote, look for a comma before the matching `)`
    // — without one there's no arg, so it's pure-string territory
    // (covered by HANDLER-NO-STRING-ERROR-001, not this rule).
    let rest = &s[i + 1..];
    if !rest.contains(',') {
        return None;
    }
    // Prefer `%+v` if present (most specific), then `%v`, then `%s`.
    if literal.contains("%+v") {
        return Some("%+v");
    }
    if literal.contains("%v") {
        return Some("%v");
    }
    if literal.contains("%s") {
        return Some("%s");
    }
    None
}

/// `fmt.Errorf(` must be preceded by start-of-line or a non-identifier
/// char so identifiers like `myfmt.Errorf(` don't trigger.
fn has_call_boundary_before(line: &str, idx: usize) -> bool {
    if idx == 0 {
        return true;
    }
    let prev = line.as_bytes()[idx - 1];
    !(prev.is_ascii_alphanumeric() || prev == b'_')
}

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
            // Copy the whole UTF-8 char at `i`, not `bytes[i] as char`
            // (which would mangle a multibyte char into Latin-1 mojibake
            // and shift downstream byte offsets). `i` is a char boundary
            // here because the comment markers we skip are ASCII.
            let ch = char_at(line, i);
            out.push(ch);
            i += ch.len_utf8();
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

/// Decode the UTF-8 char that starts at byte offset `i` in `line`.
/// `i` must be a char boundary.
fn char_at(line: &str, i: usize) -> char {
    line[i..]
        .chars()
        .next()
        .expect("offset is a valid char boundary inside the line")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn handler_file(source: &str) -> GoHandlerSourceFile {
        GoHandlerSourceFile {
            feature_name: "billing".to_owned(),
            bucket: "jobs".to_owned(),
            relative_path: PathBuf::from("features/billing/jobs/charge.go"),
            absolute_path: PathBuf::from("/abs/features/billing/jobs/charge.go"),
            source: source.to_owned(),
            loc_count: source.lines().count(),
            is_test: false,
        }
    }

    #[test]
    fn percent_v_fires() {
        let f = handler_file(
            "package jobs\n\nfunc Run(err error) error {\n  return fmt.Errorf(\"charge: %v\", err)\n}\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].verb, "%v");
        assert_eq!(findings[0].line, 4);
    }

    #[test]
    fn percent_s_fires() {
        let f = handler_file(
            "package jobs\n\nfunc Run(err error) error {\n  return fmt.Errorf(\"charge: %s\", err)\n}\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].verb, "%s");
    }

    #[test]
    fn percent_plus_v_fires() {
        let f = handler_file(
            "package jobs\n\nfunc Run(err error) error {\n  return fmt.Errorf(\"charge: %+v\", err)\n}\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].verb, "%+v");
    }

    #[test]
    fn percent_w_does_not_fire() {
        let f = handler_file(
            "package jobs\n\nfunc Run(err error) error {\n  return fmt.Errorf(\"charge: %w\", err)\n}\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn pure_string_does_not_fire_here() {
        // Covered by HANDLER-NO-STRING-ERROR-001, not this rule.
        let f =
            handler_file("package jobs\n\nfunc Run() error {\n  return fmt.Errorf(\"oops\")\n}\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn fmt_errorf_with_mixed_w_and_v_is_silent() {
        // If a format has BOTH %w and %v (e.g. wrap one, format another),
        // the chain is preserved via %w — silent.
        let f = handler_file(
            "package jobs\n\nfunc Run(err error, code int) error {\n  return fmt.Errorf(\"code=%v wrap=%w\", code, err)\n}\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn fmt_errorf_in_line_comment_is_silent() {
        let f = handler_file(
            "package jobs\n\nfunc Run() error {\n  // fmt.Errorf(\"bad: %v\", err)\n  return nil\n}\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn fmt_errorf_in_block_comment_is_silent() {
        let f = handler_file(
            "package jobs\n\nfunc Run() error {\n  /*\n  return fmt.Errorf(\"bad: %v\", err)\n  */\n  return nil\n}\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn fmt_errorf_in_string_literal_is_silent() {
        let f = handler_file(
            "package jobs\n\nfunc Run() error {\n  log.Print(\"see fmt.Errorf(%v, err)\")\n  return nil\n}\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn identifier_prefix_does_not_fire() {
        let f = handler_file(
            "package jobs\n\nfunc Run(err error) error {\n  return myfmt.Errorf(\"bad: %v\", err)\n}\n",
        );
        assert!(
            check(&[f]).is_empty(),
            "qualified myfmt.Errorf does not match unqualified fmt.Errorf rule"
        );
    }

    #[test]
    fn multiple_findings_in_one_file() {
        let f = handler_file(
            "package jobs\n\nfunc A(err error) error { return fmt.Errorf(\"a: %v\", err) }\nfunc B(err error) error { return fmt.Errorf(\"b: %s\", err) }\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].verb, "%v");
        assert_eq!(findings[1].verb, "%s");
    }

    #[test]
    fn test_file_is_skipped() {
        let mut f = handler_file(
            "package jobs\n\nfunc TestRun(t *T) {\n  return fmt.Errorf(\"x: %v\", err)\n}\n",
        );
        f.is_test = true;
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn message_includes_verb_and_w_advice() {
        let f = handler_file(
            "package jobs\n\nfunc A(err error) error { return fmt.Errorf(\"a: %v\", err) }\n",
        );
        let finding = check(&[f]).into_iter().next().unwrap();
        let msg = finding.message();
        assert!(msg.contains("%v"));
        assert!(msg.contains("%w"));
        assert!(msg.contains("charge.go"));
    }
}
