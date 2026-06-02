//! Path utilities shared by the codegen entry points (`commands::generate::*`)
//! and the project scaffold (`commands::new::*`).
//!
//! Previously inlined in `main.rs`. Pulled out so the extracted
//! clusters can call into them without `pub(crate)` re-exports
//! cluttering `main.rs`. Each function is pure: no filesystem
//! mutation, no panics on weird input; the worst-case fallbacks use
//! `Path::new(".")` or the supplied project root.

use std::ffi::OsStr;
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

/// Compare two path components for the common-prefix walk in
/// [`relative_path`]. Windows paths are case-insensitive — and
/// `std::env::current_exe()` can report a different drive-letter case
/// (`C:`) than a user-supplied `c:/…` — so fold ASCII case there; Unix
/// stays case-sensitive.
fn path_component_eq(a: &OsStr, b: &OsStr) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
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
        && path_component_eq(&from_components[common], &to_components[common])
    {
        common += 1;
    }

    // No shared leading component → the two paths live under different roots
    // (most often different Windows drive letters). A `..`-relative path
    // can't bridge that, and splicing the target's `Prefix`/`RootDir`
    // components (`C:`, `\`) into the middle yields garbage like
    // `../../../../C:/\/tmp/…` that is not a valid path. Return the absolute
    // target with `/` separators; callers detect the non-`..` prefix and use
    // it as an absolute fallback.
    if common == 0 {
        return to_dir.to_string_lossy().replace('\\', "/");
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

/// True when `path` is an ABSOLUTE, machine-specific path that must
/// never be baked into a committed `go.mod` / `go.work` runtime wiring.
///
/// Recognizes (forward- OR back-slash, since `relative_path` normalizes
/// to `/` but the input may not be normalized yet):
/// - Windows drive-rooted — `C:/x`, `C:\x` (`^[A-Za-z]:[\\/]`).
/// - POSIX root — `/x` (`^/`).
/// - UNC — `\\host\share` (`^\\`).
///
/// A relative path (`../lazuli/runtime/go`, `./x`, `runtime/go`) is
/// `false`. This is the single predicate the codegen resolver consults
/// before emitting a runtime replace, and the doctor rule
/// `RUNTIME-WIRING-ABSOLUTE-PATH-001` mirrors it (the doctor crate
/// cannot depend on `lazuli_cli`, so the logic is duplicated there as a
/// tiny pure fn — keep the two in lock-step).
pub(crate) fn is_absolute_runtime_path(path: &str) -> bool {
    let p = path.trim();
    let bytes = p.as_bytes();
    // UNC: `\\host\share`. (Forward-slash `//` is not a Go-module path
    // form we emit, so we don't treat a leading `//` as UNC.)
    if p.starts_with("\\\\") {
        return true;
    }
    // POSIX root.
    if p.starts_with('/') {
        return true;
    }
    // Windows drive: `X:` followed by `\` or `/`.
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_absolute_runtime_path_matches_windows_drive() {
        assert!(is_absolute_runtime_path("C:/Users/lucas/lazuli/runtime/go"));
        assert!(is_absolute_runtime_path(r"C:\Users\lucas\lazuli\runtime\go"));
        assert!(is_absolute_runtime_path("d:/x"));
    }

    #[test]
    fn is_absolute_runtime_path_matches_posix_and_unc() {
        assert!(is_absolute_runtime_path("/home/x/runtime/go"));
        assert!(is_absolute_runtime_path(r"\\host\share\runtime\go"));
    }

    #[test]
    fn is_absolute_runtime_path_relative_is_false() {
        assert!(!is_absolute_runtime_path("../lazuli/runtime/go"));
        assert!(!is_absolute_runtime_path("../../lazuli/runtime/go"));
        assert!(!is_absolute_runtime_path("../../../lazuli/runtime/go"));
        assert!(!is_absolute_runtime_path("./x"));
        assert!(!is_absolute_runtime_path("runtime/go"));
        assert!(!is_absolute_runtime_path("."));
        // A relative path that merely CONTAINS a colon (not a drive root).
        assert!(!is_absolute_runtime_path("a:b/runtime/go"));
    }

    #[test]
    fn relative_path_same_prefix_ascends_and_descends() {
        assert_eq!(relative_path(Path::new("/a/b/c"), Path::new("/a/b/d")), "../d");
        assert_eq!(
            relative_path(Path::new("/a/b"), Path::new("/a/b/c/d")),
            "c/d"
        );
        assert_eq!(relative_path(Path::new("/a/b"), Path::new("/a/b")), ".");
    }

    #[test]
    fn relative_path_no_common_prefix_returns_forward_slashed_target() {
        // Different roots → no `..` bridge; return the target verbatim rather
        // than a spliced franken-path. (On Windows this is the cross-drive
        // case; the same guard is exercised here with rootless relatives.)
        assert_eq!(relative_path(Path::new("a/b"), Path::new("x/y")), "x/y");
    }

    #[cfg(windows)]
    #[test]
    fn relative_path_windows_folds_drive_letter_case() {
        // Regression: `current_exe()` reports `C:` while the user passed `c:`.
        // The walk must fold case and emit a clean relative path, NOT the
        // `../../../../C:/\/tmp/…` garbage the case-sensitive compare produced.
        assert_eq!(
            relative_path(
                Path::new(r"c:\tmp\proj"),
                Path::new(r"C:\tmp\lazuli\runtime\go")
            ),
            "../lazuli/runtime/go"
        );
    }
}
