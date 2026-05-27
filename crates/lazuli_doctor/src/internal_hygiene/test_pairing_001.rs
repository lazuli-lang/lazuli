//! `INTERNAL-TEST-PAIRING-001` — flag `.rs` files without inline
//! `#[cfg(test)] mod tests` or sibling `tests/<name>.rs`.
//!
//! Mirrors Rails' `test/` directory mirror convention (every
//! `app/models/user.rb` has `test/models/user_test.rb`) but adapted
//! to Rust idiom: prefer inline `#[cfg(test)] mod tests` when feasible,
//! fall back to integration tests under `crates/<name>/tests/`.
//!
//! ## When the rule fires
//!
//! A file fires if ALL of these are true:
//! - Lives under `crates/lazuli_*/src/`
//! - Is NOT `lib.rs`, `main.rs`, or `mod.rs` (those are wiring; tests
//!   for their contents live in the modules they re-export)
//! - Contains at least one `pub fn` / `pub struct` / `pub enum` /
//!   `pub trait` (something testable; pure type definitions without
//!   methods are exempted)
//! - Does NOT contain `#[cfg(test)]` anywhere
//! - Does NOT have a sibling `<name>_test.rs` in the same dir
//! - Does NOT have a corresponding `crates/<crate>/tests/<name>.rs`
//!
//! Default severity: `Warning`. Under `tdd-iron-hand` preset: `Error`.
//!
//! ## Why exempt `lib.rs` / `main.rs` / `mod.rs`
//!
//! These are wiring files — they re-export submodules, mount routes,
//! or hold the crate's prelude. Their `pub` items are forwarded from
//! deeper modules where the actual tests live. Requiring inline tests
//! in wiring files would either force test duplication or push wiring
//! contents into nested modules unnecessarily.
//!
//! ## Examples
//!
//! ```no_run
//! use lazuli_doctor::internal_hygiene::test_pairing_001::check;
//! use lazuli_doctor::internal_hygiene::walker::walk_workspace_rust_sources;
//! use std::path::Path;
//!
//! let files = walk_workspace_rust_sources(Path::new("/path/to/lazuli"));
//! let findings = check(&files);
//! // Each finding names a file that needs either an inline mod tests
//! // or a sibling _test.rs / tests/ file.
//! ```
//!
//! ## See also
//!
//! - [`crate::test_discipline::test_handler_missing_001`] — the
//!   analogous rule for `.lzi` handler tests on the user side.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::internal_hygiene::walker::RustSourceFile;

/// One `.rs` library file with `pub` items but no associated tests.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Path relative to the workspace root.
    pub path: PathBuf,
    /// Sibling files that would satisfy the pairing (informational —
    /// names the path the author would create).
    pub suggested_sibling: PathBuf,
}

impl Finding {
    /// Stable diagnostic code emitted with every finding.
    pub const CODE: &'static str = "INTERNAL-TEST-PAIRING-001";

    /// Human-readable message naming the offending file + the
    /// canonical sibling path the author can create.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::internal_hygiene::test_pairing_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("crates/lazuli_widget/src/widget.rs"),
    ///     suggested_sibling: PathBuf::from(
    ///         "crates/lazuli_widget/src/widget_test.rs",
    ///     ),
    /// };
    /// assert!(f.message().contains("widget.rs"));
    /// assert!(f.message().contains("widget_test.rs"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{} declares `pub` items but has no inline `#[cfg(test)] mod tests` and no \
             sibling `{}`. Add inline tests or an integration test file.",
            self.path.display(),
            self.suggested_sibling.display(),
        )
    }
}

/// Walk every library `.rs` file, emit findings for those that declare
/// `pub` items but have no inline `#[cfg(test)]` and no sibling
/// `<name>_test.rs` / `tests/<name>.rs`. Wiring files
/// (`lib.rs` / `main.rs` / `mod.rs`) and files without testable `pub`
/// items are exempt.
///
/// ## Examples
///
/// ```no_run
/// use lazuli_doctor::internal_hygiene::test_pairing_001::check;
/// use lazuli_doctor::internal_hygiene::walker::walk_workspace_rust_sources;
/// use std::path::Path;
///
/// let files = walk_workspace_rust_sources(Path::new("/path/to/lazuli"));
/// for f in check(&files) {
///     eprintln!("missing tests: {} (try {})", f.path.display(), f.suggested_sibling.display());
/// }
/// ```
pub fn check(files: &[RustSourceFile]) -> Vec<Finding> {
    let library_paths: HashSet<PathBuf> = files
        .iter()
        .filter(|f| f.is_library_src)
        .map(|f| f.relative_path.clone())
        .collect();
    let test_paths: HashSet<PathBuf> = files
        .iter()
        .filter(|f| !f.is_library_src)
        .map(|f| f.relative_path.clone())
        .collect();

    let mut findings = Vec::new();
    for file in files {
        if !file.is_library_src {
            continue;
        }
        let name = file
            .relative_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if matches!(name, "lib.rs" | "main.rs" | "mod.rs") {
            continue;
        }
        if !has_testable_pub_item(&file.source) {
            continue;
        }
        if has_inline_test_mod(&file.source) {
            continue;
        }
        if has_sibling_test_file(&file.relative_path, &library_paths) {
            continue;
        }
        if has_integration_test_file(&file.crate_name, name, &test_paths) {
            continue;
        }
        let suggested_sibling = suggest_sibling(&file.relative_path);
        findings.push(Finding {
            path: file.relative_path.clone(),
            suggested_sibling,
        });
    }
    findings
}

fn has_testable_pub_item(source: &str) -> bool {
    source.lines().any(|line| {
        let t = line.trim_start();
        if t.starts_with("pub(") {
            return false;
        }
        let rest = match t.strip_prefix("pub ") {
            Some(r) => r.trim_start(),
            None => return false,
        };
        rest.starts_with("fn ")
            || rest.starts_with("struct ")
            || rest.starts_with("enum ")
            || rest.starts_with("trait ")
    })
}

/// Substance check for inline test modules (2026-05-27 theater
/// promotion): a file that contains `#[cfg(test)] mod tests { }`
/// without any `#[test]` attribute is a placeholder, not a test
/// pair. Treat it the same as having no inline module at all so the
/// rule still fires.
///
/// The `#[test]` substring is enough — the test mod might wrap each
/// fn in `#[tokio::test]` or another harness macro that ultimately
/// expands to a `#[test]`; rather than chase macro variants, we
/// accept any literal `#[test]` OR a paren-wrapped variant
/// (`#[test(`, `#[tokio::test]`, `#[test_log::test]`) as substance
/// evidence. Anything else means the `#[cfg(test)]` block is a
/// dressing without content.
fn has_inline_test_mod(source: &str) -> bool {
    if source.contains("#[cfg(test)]") || source.contains("#[test]") {
        return source_has_test_item(source);
    }
    false
}

fn source_has_test_item(source: &str) -> bool {
    // Closed catalog of attributes that mark an actual test fn. The
    // bare `#[test]` form is by far the most common; harness macros
    // (`#[tokio::test]`, `#[test_log::test]`, `#[rstest]`) expand to
    // a `#[test]` but they don't carry it literally in the source.
    // Each entry is matched as a substring; that is enough for the
    // "block contains at least one entry" gate.
    const TEST_ATTR_MARKERS: &[&str] = &[
        "#[test]",
        "#[test(",
        "#[tokio::test",
        "#[async_std::test",
        "#[actix_rt::test",
        "#[test_log::test",
        "#[rstest",
        "#[proptest",
        "#[quickcheck",
    ];
    TEST_ATTR_MARKERS
        .iter()
        .any(|marker| source.contains(marker))
}

fn has_sibling_test_file(path: &std::path::Path, library_paths: &HashSet<PathBuf>) -> bool {
    let parent = match path.parent() {
        Some(p) => p,
        None => return false,
    };
    let stem = match path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => return false,
    };
    let sibling = parent.join(format!("{}_test.rs", stem));
    library_paths.contains(&sibling)
}

fn has_integration_test_file(
    crate_name: &str,
    file_name: &str,
    test_paths: &HashSet<PathBuf>,
) -> bool {
    // E.g. `crates/<crate>/tests/<stem>.rs` or
    // `crates/<crate>/tests/<stem>_test.rs`.
    let stem = file_name.trim_end_matches(".rs");
    let candidates = [
        PathBuf::from(format!("crates/{}/tests/{}.rs", crate_name, stem)),
        PathBuf::from(format!("crates/{}/tests/{}_test.rs", crate_name, stem)),
    ];
    candidates.iter().any(|c| test_paths.contains(c))
}

fn suggest_sibling(path: &std::path::Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new(""));
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    parent.join(format!("{}_test.rs", stem))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib_file(path: &str, source: &str) -> RustSourceFile {
        RustSourceFile {
            crate_name: "lazuli_test".to_owned(),
            relative_path: PathBuf::from(path),
            absolute_path: PathBuf::from(format!("/abs/{}", path)),
            source: source.to_owned(),
            loc_count: source.lines().count(),
            is_library_src: true,
        }
    }

    fn test_file(path: &str) -> RustSourceFile {
        RustSourceFile {
            crate_name: "lazuli_test".to_owned(),
            relative_path: PathBuf::from(path),
            absolute_path: PathBuf::from(format!("/abs/{}", path)),
            source: String::new(),
            loc_count: 0,
            is_library_src: false,
        }
    }

    #[test]
    fn pub_item_without_tests_fires() {
        let f = lib_file("crates/lazuli_test/src/widget.rs", "pub fn x() {}\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].suggested_sibling,
            PathBuf::from("crates/lazuli_test/src/widget_test.rs")
        );
    }

    #[test]
    fn inline_cfg_test_silences() {
        let f = lib_file(
            "crates/lazuli_test/src/widget.rs",
            "pub fn x() {}\n#[cfg(test)] mod tests { #[test] fn t() {} }\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn empty_cfg_test_mod_still_fires() {
        // Theater regression guard (2026-05-27): a `#[cfg(test)]`
        // module that contains zero `#[test]` items is a placeholder.
        // The rule MUST still fire so the author wires real tests.
        let f = lib_file(
            "crates/lazuli_test/src/widget.rs",
            "pub fn x() {}\n#[cfg(test)] mod tests {}\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(
            findings[0].suggested_sibling,
            PathBuf::from("crates/lazuli_test/src/widget_test.rs")
        );
    }

    #[test]
    fn tokio_test_macro_counts_as_test_item() {
        // `#[tokio::test]` doesn't carry a literal `#[test]` in the
        // source but expands to one. Treat as substance.
        let f = lib_file(
            "crates/lazuli_test/src/widget.rs",
            "pub fn x() {}\n#[cfg(test)] mod tests { #[tokio::test] async fn t() {} }\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn sibling_test_file_silences() {
        let f = lib_file("crates/lazuli_test/src/widget.rs", "pub fn x() {}\n");
        let sibling = lib_file("crates/lazuli_test/src/widget_test.rs", "");
        assert!(check(&[f, sibling]).is_empty());
    }

    #[test]
    fn integration_test_file_silences() {
        let f = lib_file("crates/lazuli_test/src/widget.rs", "pub fn x() {}\n");
        let integ = test_file("crates/lazuli_test/tests/widget.rs");
        assert!(check(&[f, integ]).is_empty());
    }

    #[test]
    fn lib_mod_main_files_exempted() {
        let files = [
            lib_file("crates/lazuli_test/src/lib.rs", "pub fn x() {}\n"),
            lib_file("crates/lazuli_test/src/main.rs", "pub fn y() {}\n"),
            lib_file("crates/lazuli_test/src/sub/mod.rs", "pub fn z() {}\n"),
        ];
        assert!(check(&files).is_empty());
    }

    #[test]
    fn file_without_pub_items_exempted() {
        let f = lib_file(
            "crates/lazuli_test/src/internal.rs",
            "fn private_helper() {}\nstruct Private;\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn message_includes_path_and_sibling() {
        let f = lib_file("crates/lazuli_test/src/widget.rs", "pub fn x() {}\n");
        let finding = check(&[f]).into_iter().next().unwrap();
        let msg = finding.message();
        assert!(msg.contains("widget.rs"));
        assert!(msg.contains("widget_test.rs"));
    }
}
