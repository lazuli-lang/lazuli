//! TEST-VIEW-E2E-MISSING-001 — view declared in `.lzx` without paired
//! Playwright spec at `e2e/<experience>/<view>.spec.ts`.
//!
//! Fires once per `view <name>` whose canonical spec path does not exist
//! on disk. The check is FILE-PAIR ONLY: doctor inspects the filesystem
//! and reports the gap; it does NOT invoke Playwright, does NOT parse
//! `.spec.ts` contents, and does NOT validate test quality. Wire-thin —
//! Lazuli scaffolds, Playwright runs.
//!
//! Canonical path convention (NOT configurable in `Lazurite.toml`):
//!
//! ```text
//! <project_root>/e2e/<experience>/<view>.spec.ts
//! ```
//!
//! The `<experience>` segment is the `experience <name>` block in `.lzx`
//! (the `LzxExperience.name`); this is what the proposal calls "feature"
//! since `experiences` mirror feature names in the canonical pilot
//! layout. The `<view>` segment is the `view <name>` declaration.
//!
//! Severity per [tdd-bdd-first-2026-05-23.md §Wave 3.5](../../../../docs/proposals/tdd-bdd-first-2026-05-23.md):
//!
//! | profile | severity |
//! |---|---|
//! | prototype | info |
//! | strict | warning |
//! | production | error |
//!
//! The rule emits one `Finding` per missing spec; the doctor wrapper
//! attaches the appropriate severity from the active `SecurityProfile`.
//!
//! Reference: docs/proposals/tdd-bdd-first-2026-05-23.md §Wave 3.5

use std::path::{Path, PathBuf};

use lazuli_syntax::LzxDocument;

// ── output ────────────────────────────────────────────────────────────────────

/// One TEST-VIEW-E2E-MISSING-001 finding: a `view <name>` in `.lzx`
/// without a paired Playwright spec at the canonical path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzx` file. Anchor for the diagnostic — points at the
    /// view declaration, not the missing `.spec.ts`.
    pub path: PathBuf,
    /// 1-based line number of the `view <name>` block inside `path`.
    pub line: usize,
    /// 1-based column number of the `view <name>` token.
    pub column: usize,
    /// Experience name (e.g. `customer`). Forms the directory segment
    /// of the canonical spec path.
    pub experience: String,
    /// View name (e.g. `list`, `detail`).
    pub view: String,
    /// Canonical spec path that was checked and found missing. Held so
    /// the message can name it without recomputing.
    pub expected_spec: PathBuf,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "TEST-VIEW-E2E-MISSING-001";

    /// Render the user-facing diagnostic body — names the expected
    /// spec path and suggests `lazuli generate playwright`.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::test_discipline::test_view_e2e_missing_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("shop.lzx"),
    ///     line: 12,
    ///     column: 3,
    ///     experience: "customer".into(),
    ///     view: "dashboard".into(),
    ///     expected_spec: PathBuf::from("e2e/customer/dashboard.spec.ts"),
    /// };
    /// assert!(f.message().contains("dashboard.spec.ts"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "view `{}.{}` has no paired E2E spec at `{}`; run \
             `lazuli generate playwright` to scaffold it (file-pair only; \
             doctor does not run Playwright).",
            self.experience,
            self.view,
            self.expected_spec.display()
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Canonical spec path for a `(experience, view)` pair under
/// `project_root`. Public so that
/// `lazuli_cli::cmd_generate_playwright` can use the same builder when
/// it scaffolds files — drift between generator and lint would
/// otherwise be silent.
///
/// ## Examples
///
/// ```rust
/// use std::path::Path;
/// use lazuli_doctor::test_discipline::test_view_e2e_missing_001::expected_spec_path;
///
/// let p = expected_spec_path(Path::new("/proj"), "customer", "dashboard");
/// assert!(p.ends_with("e2e/customer/dashboard.spec.ts"));
/// ```
pub fn expected_spec_path(project_root: &Path, experience: &str, view: &str) -> PathBuf {
    expected_spec_path_with_root(project_root, None, experience, view)
}

/// Same as [`expected_spec_path`] but lets the caller override the `e2e/`
/// root via `[testing.playwright].discovery_root` (multi-client pilots
/// like `app/clients/<name>/e2e/`). When `discovery_root` is `None`,
/// falls back to `<project_root>/e2e/` (original behavior).
///
/// 2026-05-27 — added so multi-client layouts (where e2e lives under
/// `app/clients/<name>/e2e/`) stop emitting false `TEST-VIEW-E2E-
/// MISSING-001` findings pointing at the bare `./e2e/` path that
/// doesn't exist. The sibling `coverage::view_e2e_pair` already
/// honored discovery_root; this brings the diagnostic version into
/// parity.
///
/// ## Examples
///
/// ```rust
/// use std::path::Path;
/// use lazuli_doctor::test_discipline::test_view_e2e_missing_001::expected_spec_path_with_root;
///
/// // Default: project_root/e2e/<experience>/<view>.spec.ts
/// let default_p = expected_spec_path_with_root(
///     Path::new("/proj"), None, "customer", "dashboard");
/// assert!(default_p.ends_with("e2e/customer/dashboard.spec.ts"));
///
/// // Override: <discovery_root>/<experience>/<view>.spec.ts
/// let override_p = expected_spec_path_with_root(
///     Path::new("/proj"),
///     Some(Path::new("app/clients/web/e2e")),
///     "customer",
///     "dashboard",
/// );
/// assert!(override_p.ends_with("app/clients/web/e2e/customer/dashboard.spec.ts"));
/// ```
pub fn expected_spec_path_with_root(
    project_root: &Path,
    discovery_root: Option<&Path>,
    experience: &str,
    view: &str,
) -> PathBuf {
    let base: PathBuf = match discovery_root {
        Some(rel) if rel.is_absolute() => rel.to_path_buf(),
        Some(rel) => project_root.join(rel),
        None => project_root.join("e2e"),
    };
    base.join(experience).join(format!("{view}.spec.ts"))
}

/// Run TEST-VIEW-E2E-MISSING-001 against one parsed `.lzx` document.
///
/// * `document` — parsed `.lzx` (must have `experiences[].views[]`
///   populated).
/// * `source_path` — `.lzx` file the document came from. Used as the
///   diagnostic anchor.
/// * `source` — raw `.lzx` text. Used to compute the 1-based line of
///   each `view <name>` block from its byte span.
/// * `project_root` — directory holding `e2e/`. The check resolves
///   spec paths relative to this root.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::test_discipline::test_view_e2e_missing_001::check;
///
/// let findings = check(&document, Path::new("shop.lzx"), &source, Path::new("/proj"));
/// for f in findings {
///     eprintln!("{}.{}: missing spec at {}", f.experience, f.view, f.expected_spec.display());
/// }
/// ```
pub fn check(
    document: &LzxDocument,
    source_path: &Path,
    source: &str,
    project_root: &Path,
) -> Vec<Finding> {
    check_with_discovery_root(document, source_path, source, project_root, None)
}

/// Same as [`check`] but accepts an optional discovery_root override
/// from `[testing.playwright].discovery_root`. Also probes flat
/// fallback paths (`<root>/<view>.spec.ts`,
/// `<root>/<feature>-<view>.spec.ts`) before declaring a view
/// uncovered — mirrors `coverage::view_e2e_pair::view_has_spec` so the
/// diagnostic and coverage views stay aligned.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::test_discipline::test_view_e2e_missing_001::check_with_discovery_root;
///
/// // Multi-client pilot: spec files live under app/clients/web/e2e/
/// // Without the discovery_root override, the rule would falsely flag
/// // every view as missing because ./e2e/ doesn't exist.
/// let findings = check_with_discovery_root(
///     &document,
///     Path::new("shop.lzx"),
///     &source,
///     Path::new("/proj"),
///     Some(Path::new("app/clients/web/e2e")),
/// );
/// ```
pub fn check_with_discovery_root(
    document: &LzxDocument,
    source_path: &Path,
    source: &str,
    project_root: &Path,
    discovery_root: Option<&Path>,
) -> Vec<Finding> {
    let scan_root: PathBuf = match discovery_root {
        Some(rel) if rel.is_absolute() => rel.to_path_buf(),
        Some(rel) => project_root.join(rel),
        None => project_root.join("e2e"),
    };
    let mut findings = Vec::new();
    for experience in &document.experiences {
        for view in &experience.views {
            // Probe nested + flat (snake) + flat (kebab) + flat
            // (<experience>-<view>) — same set the coverage layer
            // accepts. ANY match = covered.
            let view_snake = &view.name;
            let view_kebab = view_snake.replace('_', "-");
            let candidates = [
                scan_root
                    .join(&experience.name)
                    .join(format!("{view_snake}.spec.ts")),
                scan_root.join(format!("{view_snake}.spec.ts")),
                scan_root.join(format!("{view_kebab}.spec.ts")),
                scan_root.join(format!("{}-{view_snake}.spec.ts", experience.name)),
                scan_root.join(format!("{}-{view_kebab}.spec.ts", experience.name)),
            ];
            if candidates.iter().any(|p| p.exists()) {
                continue;
            }
            let spec_path = scan_root
                .join(&experience.name)
                .join(format!("{view_snake}.spec.ts"));
            let (line, column) = span_line_col(source, view.span.start);
            findings.push(Finding {
                path: source_path.to_path_buf(),
                line,
                column,
                experience: experience.name.clone(),
                view: view.name.clone(),
                expected_spec: spec_path,
            });
        }
    }
    findings
}

// ── internals ─────────────────────────────────────────────────────────────────

/// Resolve a byte offset into 1-based `(line, column)`. Mirrors
/// `lazuli_cli::doctor::line_col_for_offset` so callers without that
/// helper (e.g. the rule's own unit tests) get identical answers.
fn span_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (idx, ch) in source.char_indices() {
        if idx >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use tempfile::TempDir;

    const SAMPLE_LZX: &str = "\
experience customer
  imports customer

  view list
    source customer.query.list

  view detail
    source customer.query.by_id
";

    fn parse(source: &str) -> LzxDocument {
        lazuli_syntax::parse_lzx_document(source).expect("parse lzx fixture")
    }

    #[test]
    fn fires_for_every_view_without_paired_spec() {
        let tmp = TempDir::new().expect("tempdir");
        let project_root = tmp.path();
        let document = parse(SAMPLE_LZX);
        let findings = check(
            &document,
            Path::new("app/customer/customer.lzx"),
            SAMPLE_LZX,
            project_root,
        );
        assert_eq!(findings.len(), 2, "two views, two missing specs");
        assert!(findings.iter().any(|f| f.view == "list"));
        assert!(findings.iter().any(|f| f.view == "detail"));
        assert_eq!(Finding::CODE, "TEST-VIEW-E2E-MISSING-001");
        assert!(
            findings[0]
                .message()
                .contains("file-pair only; doctor does not run Playwright"),
            "message should reaffirm wire-thin boundary: {}",
            findings[0].message()
        );
    }

    #[test]
    fn silent_when_every_view_has_paired_spec() {
        let tmp = TempDir::new().expect("tempdir");
        let project_root = tmp.path();
        let e2e_dir = project_root.join("e2e").join("customer");
        fs::create_dir_all(&e2e_dir).expect("create e2e dir");
        fs::write(e2e_dir.join("list.spec.ts"), "// authored").expect("write list spec");
        fs::write(e2e_dir.join("detail.spec.ts"), "// authored").expect("write detail spec");

        let document = parse(SAMPLE_LZX);
        let findings = check(
            &document,
            Path::new("app/customer/customer.lzx"),
            SAMPLE_LZX,
            project_root,
        );
        assert!(findings.is_empty(), "no missing specs, no findings");
    }

    #[test]
    fn fires_only_for_missing_specs_when_some_exist() {
        let tmp = TempDir::new().expect("tempdir");
        let project_root = tmp.path();
        let e2e_dir = project_root.join("e2e").join("customer");
        fs::create_dir_all(&e2e_dir).expect("create e2e dir");
        // Only `list` has a paired spec; `detail` should still fire.
        fs::write(e2e_dir.join("list.spec.ts"), "// authored").expect("write list spec");

        let document = parse(SAMPLE_LZX);
        let findings = check(
            &document,
            Path::new("app/customer/customer.lzx"),
            SAMPLE_LZX,
            project_root,
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].view, "detail");
        assert_eq!(findings[0].experience, "customer");
        assert!(
            findings[0]
                .expected_spec
                .ends_with(Path::new("e2e/customer/detail.spec.ts")),
            "expected_spec resolves under project_root: {}",
            findings[0].expected_spec.display()
        );
    }

    #[test]
    fn empty_experience_emits_nothing() {
        // No `view <name>` blocks → nothing to pair.
        let source = "\
experience empty
  imports nothing
";
        let tmp = TempDir::new().expect("tempdir");
        let document = parse(source);
        let findings = check(
            &document,
            Path::new("app/empty/empty.lzx"),
            source,
            tmp.path(),
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn expected_spec_path_follows_convention() {
        let p = expected_spec_path(Path::new("/proj"), "customer", "detail");
        assert_eq!(
            p,
            PathBuf::from("/proj")
                .join("e2e")
                .join("customer")
                .join("detail.spec.ts")
        );
    }

    #[test]
    fn line_column_anchor_points_at_view_block() {
        // Manual offset: in SAMPLE_LZX, the first `view list` is on line 4.
        let document = parse(SAMPLE_LZX);
        let tmp = TempDir::new().expect("tempdir");
        let findings = check(&document, Path::new("customer.lzx"), SAMPLE_LZX, tmp.path());
        let list = findings
            .iter()
            .find(|f| f.view == "list")
            .expect("list finding");
        assert_eq!(list.line, 4, "view `list` declared on line 4 of fixture");
        assert!(list.column >= 1);
    }
}
