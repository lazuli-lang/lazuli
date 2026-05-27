//! `INTERNAL-PANIC-UNWRAP-001` — flag panic-prone constructs in
//! framework non-test code.
//!
//! Scans every `crates/lazuli_*/src/*.rs` library file (skipping
//! `tests/`, `examples/`, `benches/`, and inside `#[cfg(test)] mod`
//! blocks) for any of:
//!
//! - `.unwrap()` / `.expect("...")` on `Option` / `Result`
//! - `panic!(...)`, `todo!(...)`, `unimplemented!(...)`, `unreachable!(...)`
//!
//! Most `lazuli_doctor` consumers are LSP-side live-squiggle paths or
//! `cargo run -- doctor --self` in CI. Both want a non-`syn` line-based
//! heuristic so cold start stays sub-second. The rule walks the file
//! line-by-line tracking a single piece of state: depth inside a
//! `#[cfg(test)]`-attributed `mod ... { ... }` block. Inside such
//! blocks, the rule is silent — tests are expected to panic on
//! assertion failure.
//!
//! ## Why
//!
//! `.unwrap()` in framework library code is a latent panic that ships
//! to the user as a runtime crash with `panicked at 'called Option::unwrap()
//! on a None value'`. Every framework panic should either be:
//!
//! - propagated via `?` with `.context(...)` (the anyhow path), OR
//! - a typed error from `thiserror::Error` returned via `Result<_, E>`, OR
//! - a documented invariant the caller already guarantees (in which
//!   case `severity_override` with `reason` is the escape hatch).
//!
//! Default severity: `Warning`. Under `tdd-iron-hand` preset: `Error`.
//!
//! ## What's NOT flagged
//!
//! - Code inside `#[cfg(test)] mod tests { ... }` (test panics are
//!   normal).
//! - Code in files under `tests/` / `examples/` / `benches/` (the
//!   walker tags `is_library_src = false`; this rule respects it).
//! - `Result::ok()` / `Result::err()` (not panic-prone).
//! - Macros in strings or comments — the heuristic uses simple substring
//!   matching, not lexical analysis. False positives in `r#"..."#` fixture
//!   content are flagged; author tags via `severity_override`.
//!
//! ## Examples
//!
//! ```no_run
//! use lazuli_doctor::error_handling::panic_unwrap_001::check;
//! use lazuli_doctor::internal_hygiene::walker::walk_workspace_rust_sources;
//! use std::path::Path;
//!
//! let files = walk_workspace_rust_sources(Path::new("/path/to/lazuli"));
//! let findings = check(&files);
//! for f in &findings {
//!     println!("{}:{} {}", f.path.display(), f.line, f.construct);
//! }
//! ```
//!
//! ## See also
//!
//! - [`crate::error_handling::preset`] — preset wiring; iron-hand
//!   escalates this rule to `Error`.

use std::path::{Path, PathBuf};

use crate::internal_hygiene::walker::RustSourceFile;

/// One panic-prone construct outside a test context.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Path relative to the workspace root.
    pub path: PathBuf,
    /// 1-based line number.
    pub line: usize,
    /// The construct found (`.unwrap()`, `panic!`, `todo!`, etc.).
    pub construct: String,
}

impl Finding {
    /// Stable rule code.
    pub const CODE: &'static str = "INTERNAL-PANIC-UNWRAP-001";

    /// Render the doctor-formatted diagnostic message.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::error_handling::panic_unwrap_001::Finding;
    /// use std::path::PathBuf;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("crates/lazuli_ir/src/lib.rs"),
    ///     line: 42,
    ///     construct: ".unwrap()".to_string(),
    /// };
    /// assert!(f.message().contains(":42"));
    /// assert!(f.message().contains(".unwrap()"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{}:{} `{}` is panic-prone in non-test framework code. \
             Propagate via `?` with `.context(...)` or return a typed \
             error per CLAUDE.md error-handling discipline. If the \
             invariant is documented at the call site, add a \
             `severity_override` with `reason`.",
            self.path.display(),
            self.line,
            self.construct,
        )
    }
}

/// Run the rule against a slice of pre-walked library files.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::error_handling::panic_unwrap_001::check;
/// use lazuli_doctor::internal_hygiene::walker::RustSourceFile;
/// use std::path::PathBuf;
///
/// let f = RustSourceFile {
///     crate_name: "lazuli_test".to_string(),
///     relative_path: PathBuf::from("crates/lazuli_test/src/lib.rs"),
///     absolute_path: PathBuf::from("/abs/crates/lazuli_test/src/lib.rs"),
///     source: "pub fn frob() { let x: Option<u32> = None; x.unwrap(); }\n".to_string(),
///     loc_count: 1,
///     is_library_src: true,
/// };
/// let findings = check(&[f]);
/// assert_eq!(findings.len(), 1);
/// assert_eq!(findings[0].construct, ".unwrap()");
/// ```
pub fn check(files: &[RustSourceFile]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for file in files {
        if !file.is_library_src {
            continue;
        }
        if is_rails_test_file(&file.relative_path) {
            // Rails-style test siblings (`<name>_tests.rs`) and any file
            // under `src/tests/` are test code. The rule's existing
            // `#[cfg(test)] mod` depth tracking handles inline `mod tests
            // { ... }` blocks; this skip handles the W5-era pattern where
            // a whole sibling file IS the test module, included by a
            // parent via `include!()`.
            continue;
        }
        scan_file(&file.relative_path, &file.source, &mut findings);
    }
    findings
}

/// Returns `true` for files conventionally containing test code:
/// - File name ends with `_tests.rs` (W5 Rails-style sibling test files).
/// - Path contains `/src/tests/` or `\src\tests\` (rails-style test subtree).
fn is_rails_test_file(path: &std::path::Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if name.ends_with("_tests.rs") || name == "tests.rs" {
        return true;
    }
    let s = path.to_string_lossy().replace('\\', "/");
    s.contains("/src/tests/")
}

fn scan_file(path: &Path, source: &str, out: &mut Vec<Finding>) {
    let mut in_test_attr = false; // saw `#[cfg(test)]` recently, awaiting `mod`
    let mut test_mod_depth = 0usize; // brace depth inside `#[cfg(test)] mod X { ... }`
    let mut brace_depth_at_test_start = 0usize;
    let mut current_brace_depth = 0usize;

    for (idx, line) in source.lines().enumerate() {
        let stripped = strip_line_comments(line);
        let trimmed = stripped.trim_start();

        // Track `#[cfg(test)]` attribute on the line just above a `mod`.
        if trimmed.starts_with("#[cfg(test)]") || trimmed.starts_with("#[cfg(any(test")
            || trimmed.starts_with("#[cfg(all(test")
        {
            in_test_attr = true;
        } else if in_test_attr && (trimmed.starts_with("mod ") || trimmed.starts_with("pub mod ")
            || trimmed.starts_with("pub(crate) mod ") || trimmed.starts_with("pub(super) mod "))
        {
            // Entering a test module. Track its closing brace.
            if test_mod_depth == 0 {
                brace_depth_at_test_start = current_brace_depth;
            }
            test_mod_depth += 1;
            in_test_attr = false;
        } else if !trimmed.starts_with("#[") {
            // Attribute streak broken without consuming the mod — reset.
            in_test_attr = false;
        }

        // Adjust brace depth based on this line BEFORE checking patterns.
        let (opens, closes) = count_braces(&stripped);
        let depth_before = current_brace_depth;
        current_brace_depth = depth_before.saturating_add(opens).saturating_sub(closes);

        // If we exited the test mod block, clear it.
        if test_mod_depth > 0 && current_brace_depth <= brace_depth_at_test_start {
            test_mod_depth = 0;
        }

        // Inside a #[cfg(test)] mod ... — never fire.
        if test_mod_depth > 0 {
            continue;
        }
        // The line that opens the test mod (`#[cfg(test)] mod tests {`)
        // straddles the boundary; treat its body as in-test once depth
        // has moved past `brace_depth_at_test_start`.
        if in_test_attr {
            continue;
        }

        if let Some(construct) = detect_panic_construct(&stripped) {
            out.push(Finding {
                path: path.to_path_buf(),
                line: idx + 1,
                construct: construct.to_owned(),
            });
        }
    }
}

/// Strip `// line comments` so panic-keywords inside comments don't
/// trigger. String literals are NOT stripped — that's a known false
/// positive class (raw string fixtures containing `panic!` text).
fn strip_line_comments(line: &str) -> String {
    if let Some(idx) = line.find("//") {
        line[..idx].to_owned()
    } else {
        line.to_owned()
    }
}

fn count_braces(line: &str) -> (usize, usize) {
    let mut in_string = false;
    let mut in_raw = false;
    let mut prev = ' ';
    let mut opens = 0;
    let mut closes = 0;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if !in_string && c == 'r' && matches!(chars.peek(), Some('#') | Some('"')) {
            // Treat raw-string start as a string token; skip until closing.
            in_raw = true;
            in_string = true;
            continue;
        }
        if !in_raw && c == '"' && prev != '\\' {
            in_string = !in_string;
            prev = c;
            continue;
        }
        if in_raw && c == '"' {
            // Cheap heuristic: end of raw string. In practice this is good
            // enough for line-by-line scanning.
            in_string = false;
            in_raw = false;
            prev = c;
            continue;
        }
        if !in_string {
            if c == '{' {
                opens += 1;
            } else if c == '}' {
                closes += 1;
            }
        }
        prev = c;
    }
    (opens, closes)
}

/// Returns the canonical construct name if the line contains a flagged
/// panic-prone construct. Order matters: `unimplemented!` must be
/// checked before `unwrap` (the latter doesn't appear in it, but a
/// regression could).
fn detect_panic_construct(line: &str) -> Option<&'static str> {
    // Skip lines that ARE a const-decl naming a constant containing one
    // of these tokens (e.g. `pub const UNWRAP_DOC: &str = "...unwrap..."`).
    let trimmed = line.trim_start();
    if trimmed.starts_with("pub const ")
        || trimmed.starts_with("const ")
        || trimmed.starts_with("pub static ")
        || trimmed.starts_with("static ")
    {
        return None;
    }
    // Skip rustdoc lines.
    if trimmed.starts_with("///") || trimmed.starts_with("//!") {
        return None;
    }
    // Skip attribute lines like `#[expect(clippy::unwrap_used)]`.
    if trimmed.starts_with("#[") {
        return None;
    }

    // Order: most specific first, to avoid `expect` matching `expect_err`.
    if line.contains("panic!(") {
        return Some("panic!(...)");
    }
    if line.contains("unimplemented!(") {
        return Some("unimplemented!(...)");
    }
    if line.contains("unreachable!(") {
        return Some("unreachable!(...)");
    }
    if line.contains("todo!(") {
        return Some("todo!(...)");
    }
    if let Some(idx) = line.find(".unwrap()") {
        // Discriminate from `.unwrap_or()`, `.unwrap_or_else()`,
        // `.unwrap_or_default()`, etc. — those are explicit fallbacks
        // and not panic-prone.
        let after = &line[idx + ".unwrap".len()..];
        if after.starts_with("()") {
            return Some(".unwrap()");
        }
    }
    if line.contains(".expect(") && !line.contains(".expect_err(") {
        return Some(".expect(...)");
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(source: &str) -> RustSourceFile {
        RustSourceFile {
            crate_name: "lazuli_test".to_owned(),
            relative_path: PathBuf::from("crates/lazuli_test/src/lib.rs"),
            absolute_path: PathBuf::from("/abs/crates/lazuli_test/src/lib.rs"),
            source: source.to_owned(),
            loc_count: source.lines().count(),
            is_library_src: true,
        }
    }

    #[test]
    fn bare_unwrap_fires() {
        let f = file("pub fn x() { let r: Result<u32, ()> = Ok(0); r.unwrap(); }\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].construct, ".unwrap()");
    }

    #[test]
    fn unwrap_or_default_does_not_fire() {
        let f = file(
            "pub fn x() {\n  let r: Result<u32, ()> = Ok(0);\n  let v = r.unwrap_or_default();\n}\n",
        );
        assert!(check(&[f]).is_empty(), "unwrap_or_default is not panic-prone");
    }

    #[test]
    fn unwrap_or_else_does_not_fire() {
        let f = file("pub fn x(r: Result<u32, ()>) -> u32 { r.unwrap_or_else(|_| 0) }\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn expect_fires() {
        let f = file(r#"pub fn x() { let r: Result<u32, ()> = Ok(0); r.expect("oops"); }
"#);
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].construct, ".expect(...)");
    }

    #[test]
    fn expect_err_does_not_fire() {
        let f = file("pub fn x() { let r: Result<u32, ()> = Err(()); r.expect_err(\"ok\"); }\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn panic_macro_fires() {
        let f = file("pub fn x() { panic!(\"never\"); }\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].construct, "panic!(...)");
    }

    #[test]
    fn todo_unimplemented_unreachable_fire() {
        let f = file(
            "pub fn a() { todo!() }\npub fn b() { unimplemented!() }\npub fn c() { unreachable!() }\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 3);
    }

    #[test]
    fn unwrap_inside_cfg_test_mod_is_silent() {
        let f = file(
            "pub fn x() {}\n#[cfg(test)]\nmod tests {\n  use super::*;\n  #[test]\n  fn t() { Some(1u32).unwrap(); }\n}\n",
        );
        assert!(check(&[f]).is_empty(), "unwrap inside #[cfg(test)] mod is allowed");
    }

    #[test]
    fn unwrap_in_test_function_outside_module_still_fires() {
        // `#[test] fn t() { ... }` at crate root (no enclosing
        // #[cfg(test)] mod) — strictly speaking the body still ships.
        // Test infrastructure relies on `mod tests` discipline; freestanding
        // #[test] fns are unusual and still scanned.
        let f = file("#[test]\nfn t() { Some(1u32).unwrap(); }\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn line_comments_with_unwrap_do_not_fire() {
        let f = file("pub fn x() { /* nothing */ }\n// example: x.unwrap();\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn rustdoc_lines_with_unwrap_do_not_fire() {
        let f = file("/// Example: `value.unwrap()` in a test.\npub fn x() {}\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn const_decls_with_panic_keyword_do_not_fire() {
        let f = file(
            "pub const PANIC_DOC: &str = \"calls panic!(...) on failure\";\npub fn x() {}\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn non_library_files_are_skipped() {
        let mut f = file("pub fn x() { panic!() }\n");
        f.is_library_src = false;
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn rails_test_sibling_files_are_skipped() {
        // Files named `<name>_tests.rs` are Rails-style sibling test
        // modules included from a parent via `include!()`. The rule's
        // `#[cfg(test)] mod` depth tracking can't see the parent, so we
        // skip these whole files.
        let mut f = file("pub fn x() { panic!() }\n");
        f.relative_path = PathBuf::from("crates/lazuli_test/src/foo_tests.rs");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn files_under_src_tests_are_skipped() {
        // Rails-style: tests live under `src/tests/...` next to the
        // module they exercise. Whole sub-tree is test code.
        let mut f = file("pub fn x() { panic!() }\n");
        f.relative_path = PathBuf::from("crates/lazuli_test/src/tests/core/workflow.rs");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn plain_src_file_still_fires() {
        // Regression guard: `tests` substring in a file path that isn't
        // a sibling test file or under `src/tests/` should still fire.
        let mut f = file("pub fn x() { panic!() }\n");
        f.relative_path = PathBuf::from("crates/lazuli_test/src/contests/mod.rs");
        assert_eq!(check(&[f]).len(), 1);
    }

    #[test]
    fn message_includes_construct_and_path() {
        let f = file("pub fn x() { panic!(\"\") }\n");
        let finding = check(&[f]).into_iter().next().unwrap();
        let msg = finding.message();
        assert!(msg.contains("panic!"));
        assert!(msg.contains("lib.rs"));
        assert!(msg.contains(":1"));
    }
}
