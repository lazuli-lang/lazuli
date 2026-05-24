//! Canonical handler-path resolution.
//!
//! Single source of truth for the `<app_dir>/features/<feature>/handlers/<name>.{go,_test.go}`
//! layout enforced by CLAUDE.md §"Handlers". Both the doctor rules
//! (HANDLER-MISSING-001 / TEST-HANDLER-MISSING-001) and the CLI generator
//! (`lazuli generate handler`) consume these helpers so the layout never
//! drifts between consumer and enforcer.
//!
//! The path is **NOT** configurable. Convention over configuration per
//! CLAUDE.md §83-105: every handler lives in its feature's `handlers/`
//! sub-folder, named after the `@fn`/`@validator`/`@hook` it implements,
//! in snake_case. The codegen pascal-cases the file stem to produce the
//! Go symbol; that's the only transformation.
//!
//! Reference: docs/proposals/tdd-bdd-first-2026-05-23.md §5.2.

use std::path::{Path, PathBuf};

/// Resolve the canonical handler `.go` path for `feature::handler_name`
/// under the given app root.
///
/// `app_root` is the directory containing `features/` — usually
/// `<project_root>/app` when `Lazurite.toml [lazurite] app_dir = "app"`,
/// or `<project_root>` itself when no `app_dir` is set.
pub fn resolve(app_root: &Path, feature: &str, handler_name: &str) -> PathBuf {
    app_root
        .join("features")
        .join(feature)
        .join("handlers")
        .join(format!("{handler_name}.go"))
}

/// Resolve the canonical handler `_test.go` paired path. Same convention
/// as [`resolve`] but with the Go test suffix.
pub fn resolve_test(app_root: &Path, feature: &str, handler_name: &str) -> PathBuf {
    app_root
        .join("features")
        .join(feature)
        .join("handlers")
        .join(format!("{handler_name}_test.go"))
}

/// The `handlers/` directory for a feature. Useful when the generator
/// needs to `create_dir_all` before writing files.
pub fn handlers_dir(app_root: &Path, feature: &str) -> PathBuf {
    app_root.join("features").join(feature).join("handlers")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_uses_canonical_layout() {
        let root = Path::new("/tmp/proj/app");
        let go = resolve(root, "post", "validate_title");
        assert_eq!(
            go,
            Path::new("/tmp/proj/app/features/post/handlers/validate_title.go")
        );
    }

    #[test]
    fn resolve_test_uses_canonical_layout() {
        let root = Path::new("/tmp/proj/app");
        let test = resolve_test(root, "post", "validate_title");
        assert_eq!(
            test,
            Path::new("/tmp/proj/app/features/post/handlers/validate_title_test.go")
        );
    }

    #[test]
    fn handlers_dir_uses_canonical_layout() {
        let root = Path::new("/tmp/proj/app");
        let dir = handlers_dir(root, "post");
        assert_eq!(dir, Path::new("/tmp/proj/app/features/post/handlers"));
    }

    #[test]
    fn snake_case_handler_names_preserved() {
        // The CLI's pascal_case() converts only inside the .go BODY.
        // File stem stays snake_case to match @fn.<name> verbatim.
        let go = resolve(Path::new("/r"), "auth", "verify_password_v2");
        assert!(go.ends_with("verify_password_v2.go"));
    }
}
