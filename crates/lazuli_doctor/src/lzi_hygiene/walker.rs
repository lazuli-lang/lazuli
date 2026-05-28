//! Walker for user-app `.lzi` files.
//!
//! Yields one [`LziSourceFile`] per discovered `.lzi` source the rules
//! should audit. Path-level exemptions are baked in here:
//!
//! - `vocab-fixtures/`, `anti-patterns/`, `proposals/` directories
//!   (intentionally-deviant test fixtures)
//! - Generated files carrying `# @generated` or `# Code generated` in
//!   the first non-blank line (future codegen targets)
//! - Top-level toplevel files: `app.lzi`, `registry.lzi`,
//!   `workspace.lzi`, `profiles.lzi`. These declare `app`, `registry`,
//!   `workspace`, `profiles` toplevels respectively — not `feature`.
//!   Counting features on them would always return zero; that's not
//!   a violation.
//! - `contracts/<service>.lzi` — declares `contract`, not `feature`.

use std::fs;
use std::path::{Path, PathBuf};

/// One `.lzi` source file discovered by the walker.
#[derive(Debug, Clone)]
pub struct LziSourceFile {
    /// Path relative to the workspace root.
    pub relative_path: PathBuf,
    /// Absolute path on disk.
    pub absolute_path: PathBuf,
    /// Full file contents, UTF-8. BOM stripped.
    pub source: String,
    /// Line count of `source`.
    pub loc_count: usize,
}

/// Walk `<workspace_root>/{features,**}/*.lzi`, returning every
/// discoverable `.lzi` file with metadata, minus exempt paths and
/// generated files.
///
/// ## Examples
///
/// ```no_run
/// use lazuli_doctor::lzi_hygiene::walker::walk_lzi_sources;
/// use std::path::Path;
///
/// let files = walk_lzi_sources(Path::new("/path/to/lazuli-app"));
/// for f in &files {
///     println!("{}: {} LOC", f.relative_path.display(), f.loc_count);
/// }
/// ```
pub fn walk_lzi_sources(workspace_root: &Path) -> Vec<LziSourceFile> {
    let mut out = Vec::new();
    walk_dir_recursive(workspace_root, workspace_root, &mut out);
    out
}

fn walk_dir_recursive(workspace_root: &Path, dir: &Path, out: &mut Vec<LziSourceFile>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.is_dir() {
            if matches!(
                name,
                "target"
                    | ".git"
                    | ".worktrees"
                    | ".claude"
                    | "node_modules"
                    | "dist"
                    | ".lazuli"
                    | "vendor"
            ) {
                continue;
            }
            walk_dir_recursive(workspace_root, &path, out);
            continue;
        }
        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("lzi") {
            continue;
        }
        let relative_path = path
            .strip_prefix(workspace_root)
            .unwrap_or(&path)
            .to_path_buf();
        if is_exempt_path(&relative_path, name) {
            continue;
        }
        let source = match fs::read_to_string(&path) {
            Ok(s) => strip_bom(s),
            Err(_) => continue,
        };
        if is_generated_source(&source) {
            continue;
        }
        let loc_count = source.lines().count();
        out.push(LziSourceFile {
            relative_path,
            absolute_path: path,
            source,
            loc_count,
        });
    }
}

/// Decide whether a relative `.lzi` path is exempt from hygiene rules.
///
/// Exempts:
/// - top-level toplevel files (`app.lzi`, `registry.lzi`,
///   `workspace.lzi`, `profiles.lzi`) — they declare non-`feature`
///   toplevels.
/// - `contracts/<service>.lzi` — `contract` toplevel, not `feature`.
/// - intentionally-deviant fixtures: `vocab-fixtures/`, `anti-patterns/`,
///   `proposals/`.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::lzi_hygiene::walker::is_exempt_path;
/// use std::path::Path;
///
/// // Top-level toplevel files exempt.
/// assert!(is_exempt_path(Path::new("app.lzi"), "app.lzi"));
/// assert!(is_exempt_path(Path::new("contracts/billing.lzi"), "billing.lzi"));
/// // Fixture sub-trees exempt.
/// assert!(is_exempt_path(
///     Path::new("examples/vocab-fixtures/case.lzi"),
///     "case.lzi"
/// ));
/// // Plain feature file NOT exempt.
/// assert!(!is_exempt_path(
///     Path::new("features/billing/billing.lzi"),
///     "billing.lzi"
/// ));
/// ```
pub fn is_exempt_path(relative_path: &Path, file_name: &str) -> bool {
    if matches!(
        file_name,
        "app.lzi" | "registry.lzi" | "workspace.lzi" | "profiles.lzi"
    ) {
        return true;
    }
    let s = relative_path.to_string_lossy().replace('\\', "/");
    let comps: Vec<&str> = s.split('/').collect();
    for c in &comps {
        if matches!(
            *c,
            "vocab-fixtures" | "anti-patterns" | "proposals" | "fixtures" | "tests"
        ) {
            // Any `tests/` or `fixtures/` subtree is test material.
            // Catches `crates/<x>/tests/fixtures/<*>.lzi` (framework
            // unit-test inputs) as well as user-app `tests/` dirs.
            return true;
        }
    }
    // `crates/lazuli_*/...` — framework internal source. Walking these
    // surfaces test fixtures embedded in the framework's own crate
    // tests. The framework workspace itself doesn't ship user-shaped
    // `.lzi` files; any `.lzi` under `crates/` is fixture data.
    if comps.first().is_some_and(|c| *c == "crates") {
        return true;
    }
    // `contracts/<service>.lzi` shape: file lives directly under a
    // `contracts` directory.
    if comps.len() >= 2 && comps[comps.len() - 2] == "contracts" {
        return true;
    }
    false
}

/// `true` when the source's first non-blank line is a `# @generated`
/// or `# Code generated` marker. Mirrors the analogous carve-out in
/// [`crate::internal_hygiene::file_size_001::is_generated_rust`].
fn is_generated_source(source: &str) -> bool {
    for line in source.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let lower = t.to_ascii_lowercase();
        return lower.starts_with("# @generated")
            || lower.starts_with("# code generated")
            || lower.starts_with("// @generated")
            || lower.starts_with("// code generated");
    }
    false
}

fn strip_bom(s: String) -> String {
    if let Some(stripped) = s.strip_prefix('\u{feff}') {
        stripped.to_owned()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn make_mock_app(root: &Path) {
        // Two feature files we expect to be walked.
        let feat_dir = root.join("features/billing");
        fs::create_dir_all(&feat_dir).unwrap();
        fs::File::create(feat_dir.join("billing.lzi"))
            .unwrap()
            .write_all(b"feature billing\n  description \"\"\n")
            .unwrap();
        let feat_dir2 = root.join("features/auth");
        fs::create_dir_all(&feat_dir2).unwrap();
        fs::File::create(feat_dir2.join("auth.lzi"))
            .unwrap()
            .write_all(b"feature auth\n")
            .unwrap();

        // Toplevel exempt files.
        fs::File::create(root.join("app.lzi"))
            .unwrap()
            .write_all(b"app Acme\n")
            .unwrap();
        fs::File::create(root.join("registry.lzi"))
            .unwrap()
            .write_all(b"registry\n  description \"\"\n")
            .unwrap();

        // Contracts dir exempt.
        let contracts = root.join("contracts");
        fs::create_dir_all(&contracts).unwrap();
        fs::File::create(contracts.join("payments.lzi"))
            .unwrap()
            .write_all(b"contract payments\n")
            .unwrap();

        // Fixture sub-tree exempt.
        let fixtures = root.join("examples/vocab-fixtures");
        fs::create_dir_all(&fixtures).unwrap();
        fs::File::create(fixtures.join("case.lzi"))
            .unwrap()
            .write_all(b"feature case\n")
            .unwrap();

        // Generated file exempt.
        let gen_dir = root.join("features/codegen");
        fs::create_dir_all(&gen_dir).unwrap();
        fs::File::create(gen_dir.join("emitted.lzi"))
            .unwrap()
            .write_all(b"# @generated\nfeature emitted\n")
            .unwrap();
    }

    #[test]
    fn walker_includes_only_feature_files() {
        let tmp = tempfile::tempdir().unwrap();
        make_mock_app(tmp.path());
        let files = walk_lzi_sources(tmp.path());
        let names: Vec<_> = files
            .iter()
            .filter_map(|f| f.relative_path.file_name().and_then(|s| s.to_str()))
            .collect();
        assert!(names.contains(&"billing.lzi"), "{names:?}");
        assert!(names.contains(&"auth.lzi"), "{names:?}");
        assert!(!names.contains(&"app.lzi"));
        assert!(!names.contains(&"registry.lzi"));
        assert!(!names.contains(&"payments.lzi"));
        assert!(!names.contains(&"case.lzi")); // vocab-fixtures exempt
        assert!(!names.contains(&"emitted.lzi")); // @generated exempt
    }

    #[test]
    fn walker_returns_empty_when_no_lzi_anywhere() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(walk_lzi_sources(tmp.path()).is_empty());
    }

    #[test]
    fn walker_skips_artefact_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target/lzi-junk");
        fs::create_dir_all(&target).unwrap();
        fs::File::create(target.join("foo.lzi")).unwrap();
        let dist = tmp.path().join("dist");
        fs::create_dir_all(&dist).unwrap();
        fs::File::create(dist.join("emitted.lzi")).unwrap();
        let feat_dir = tmp.path().join("features/billing");
        fs::create_dir_all(&feat_dir).unwrap();
        fs::File::create(feat_dir.join("billing.lzi"))
            .unwrap()
            .write_all(b"feature billing\n")
            .unwrap();
        let files = walk_lzi_sources(tmp.path());
        assert_eq!(files.len(), 1);
        assert!(
            files[0]
                .relative_path
                .to_string_lossy()
                .ends_with("billing.lzi")
        );
    }

    #[test]
    fn is_exempt_path_matches_canonical_carve_outs() {
        assert!(is_exempt_path(Path::new("app.lzi"), "app.lzi"));
        assert!(is_exempt_path(Path::new("registry.lzi"), "registry.lzi"));
        assert!(is_exempt_path(Path::new("workspace.lzi"), "workspace.lzi"));
        assert!(is_exempt_path(Path::new("profiles.lzi"), "profiles.lzi"));
        assert!(is_exempt_path(
            Path::new("contracts/billing.lzi"),
            "billing.lzi"
        ));
        assert!(is_exempt_path(
            Path::new("examples/anti-patterns/saturated.lzi"),
            "saturated.lzi"
        ));
        // Framework-internal test fixtures: anywhere under `crates/`,
        // anywhere under `tests/`, anywhere under `fixtures/`.
        assert!(is_exempt_path(
            Path::new("crates/lazuli_cli/tests/fixtures/cache/namespace_collision.lzi"),
            "namespace_collision.lzi"
        ));
        assert!(is_exempt_path(
            Path::new("some/path/tests/foo.lzi"),
            "foo.lzi"
        ));
        assert!(is_exempt_path(
            Path::new("some/path/fixtures/foo.lzi"),
            "foo.lzi"
        ));
        // Plain user-app feature file NOT exempt.
        assert!(!is_exempt_path(
            Path::new("features/billing/billing.lzi"),
            "billing.lzi"
        ));
    }
}
