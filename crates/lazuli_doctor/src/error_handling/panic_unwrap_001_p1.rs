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
/// - File name is `tests.rs`, `*_tests.rs`, `tests_*.rs`, or `*_test.rs`
///   (W5 Rails-style sibling test files; W3-era underscore-prefix variant
///   also seen in `doctor/folder/<rule>/tests_basic.rs` etc.).
/// - Any path component equals `tests` or `lib_tests` (rails-style test
///   sub-tree, including the LSP's `src/lib_tests/group_NN_*.rs` split).
fn is_rails_test_file(path: &std::path::Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if name == "tests.rs" {
        return true;
    }
    if name.ends_with("_tests.rs")
        || name.ends_with("_test.rs")
        || (name.starts_with("tests_") && name.ends_with(".rs"))
        || name == "test_support.rs"
        || (name.starts_with("test_support") && name.ends_with(".rs"))
    {
        return true;
    }
    // SPEC-19 `_p<N>` chunks of a `_tests.rs` sibling (e.g.
    // `handler_signature_mismatch_001_tests_p1.rs`) are test code too: the
    // `#[cfg(test)] mod tests { include!(...) }` wrapper lives in the canonical
    // `<base>_tests.rs`, so the chunk file carries no `#[cfg(test)]` of its own
    // and the depth-tracker can't see it. Strip a trailing `_p<digits>` and
    // re-check for the `_tests` sibling marker.
    if let Some(stem) = name.strip_suffix(".rs")
        && let Some((head, digits)) = stem.rsplit_once("_p")
        && !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && head.ends_with("_tests")
    {
        return true;
    }
    let s = path.to_string_lossy().replace('\\', "/");
    for component in s.split('/') {
        if component == "tests" || component == "lib_tests" {
            return true;
        }
    }
    false
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
        if trimmed.starts_with("#[cfg(test)]")
            || trimmed.starts_with("#[cfg(any(test")
            || trimmed.starts_with("#[cfg(all(test")
        {
            in_test_attr = true;
        } else if in_test_attr
            && (trimmed.starts_with("mod ")
                || trimmed.starts_with("pub mod ")
                || trimmed.starts_with("pub(crate) mod ")
                || trimmed.starts_with("pub(super) mod "))
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
///
/// String literals are stripped before pattern matching — `line.contains
/// ("panic!(")` in a rule implementation's source should NOT self-fire.
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

    // Strip string-literal content so e.g. `line.contains("panic!(")`
    // in a rule's own implementation doesn't self-trigger.
    let stripped = strip_string_literals(line);

    // Order: most specific first, to avoid `expect` matching `expect_err`.
    if stripped.contains("panic!(") {
        return Some("panic!(...)");
    }
    if stripped.contains("unimplemented!(") {
        return Some("unimplemented!(...)");
    }
    if stripped.contains("unreachable!(") {
        return Some("unreachable!(...)");
    }
    if stripped.contains("todo!(") {
        return Some("todo!(...)");
    }
    if let Some(idx) = stripped.find(".unwrap()") {
        // Discriminate from `.unwrap_or()`, `.unwrap_or_else()`,
        // `.unwrap_or_default()`, etc. — those are explicit fallbacks
        // and not panic-prone.
        let after = &stripped[idx + ".unwrap".len()..];
        if after.starts_with("()") {
            return Some(".unwrap()");
        }
    }
    if stripped.contains(".expect(") && !stripped.contains(".expect_err(") {
        return Some(".expect(...)");
    }

    None
}
