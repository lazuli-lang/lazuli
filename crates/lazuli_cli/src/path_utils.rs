//! Path utilities shared by the codegen entry points (`commands::generate::*`)
//! and the project scaffold (`commands::new::*`).
//!
//! Previously inlined in `main.rs`. Pulled out so the extracted
//! clusters can call into them without `pub(crate)` re-exports
//! cluttering `main.rs`. Each function is pure: no filesystem
//! mutation, no panics on weird input; the worst-case fallbacks use
//! `Path::new(".")` or the supplied project root.

use std::path::{Path, PathBuf};

/// Resolve `project_root` to an absolute path. Used by `generate go`
/// and `generate ts` to anchor codegen outputs even when callers pass
/// a relative project root (e.g. `--input ./examples/full-capsule`).
pub(crate) fn absolutize_project_root(project_root: &Path) -> PathBuf {
    if project_root.is_absolute() {
        project_root.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| Path::new(".").to_path_buf())
            .join(project_root)
    }
}

/// Like `absolutize_project_root` but for a file path beneath the
/// project. If the path is already absolute, return it verbatim; if it
/// is relative-under-the-project, join it onto the absolute CWD so
/// downstream codegen can write deterministically regardless of where
/// the CLI was invoked. Falls back to `project_root.join(path)` when
/// `path` is relative but does not start with `project_root` (e.g. a
/// `dist/...` target authored relative to the workspace).
pub(crate) fn absolutize_for_codegen(project_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else if path.starts_with(project_root) {
        std::env::current_dir()
            .unwrap_or_else(|_| project_root.to_path_buf())
            .join(path)
    } else {
        project_root.join(path)
    }
}

/// Compute a relative path string between two directories, using `..`
/// segments to ascend the common ancestor. Output uses `/` separators
/// even on Windows so TypeScript `import` paths stay portable.
///
/// Used by the TS emitter to write cross-feature `import` statements
/// (`../../<feature>/<artifact>`) without baking the absolute path of
/// the host machine into generated artifacts.
pub(crate) fn relative_path(from_dir: &Path, to_dir: &Path) -> String {
    let from_components = from_dir
        .components()
        .map(|component| component.as_os_str().to_owned())
        .collect::<Vec<_>>();
    let to_components = to_dir
        .components()
        .map(|component| component.as_os_str().to_owned())
        .collect::<Vec<_>>();

    let mut common = 0;
    while common < from_components.len()
        && common < to_components.len()
        && from_components[common] == to_components[common]
    {
        common += 1;
    }

    let mut parts = Vec::new();
    for _ in common..from_components.len() {
        parts.push("..".to_owned());
    }
    for component in &to_components[common..] {
        parts.push(component.to_string_lossy().into_owned());
    }

    if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    }
}
