//! Wire the locally-checked-out Lazuli Go runtime into a freshly
//! scaffolded project's `go.work` and `Lazurite.toml`.
//!
//! `lazuli.dev/runtime` is never published to a real Go module proxy:
//! the runtime is always resolved as a local workspace replacement so
//! developers can edit it in-tree against the same lazuli checkout
//! that produced the CLI binary. This module exists so the `lazuli
//! new` happy path can boot a fresh project that builds on the first
//! `go build` without manual `replace` directives.
//!
//! Resolution order for the runtime directory:
//!
//! 1. `LAZULI_RUNTIME_PATH` env var (escape hatch for non-standard
//!    layouts and CI).
//! 2. Ancestors of the running `lazuli` binary — when developing
//!    from this repo, the binary lives at
//!    `<repo>/target/{debug,release}/lazuli(.exe)`, and the runtime
//!    sits at `<repo>/runtime/go/`.
//!
//! Returns `None` for installed (system) Lazuli binaries with no
//! discoverable runtime checkout; the scaffold then leaves `go.work`
//! as-is and the user wires the path manually per the README hint.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::path_utils::{absolutize_project_root, relative_path};

/// Walk the resolution order and return the first directory that
/// looks like the Lazuli runtime (canonical `go.mod` declaring
/// `module lazuli.dev/runtime`).
pub(crate) fn locate_lazuli_runtime_dir() -> Option<PathBuf> {
    if let Ok(env_path) = std::env::var("LAZULI_RUNTIME_PATH") {
        let candidate = PathBuf::from(env_path);
        if is_lazuli_runtime_dir(&candidate) {
            return Some(candidate);
        }
    }
    let exe = std::env::current_exe().ok()?;
    for ancestor in exe.ancestors() {
        let candidate = ancestor.join("runtime").join("go");
        if is_lazuli_runtime_dir(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// A directory qualifies as the Lazuli runtime when it contains a
/// `go.mod` whose `module` line is exactly `lazuli.dev/runtime`. This
/// guards against picking up an unrelated `runtime/go/` directory in
/// some other project that happens to sit above the binary.
fn is_lazuli_runtime_dir(candidate: &Path) -> bool {
    let go_mod = candidate.join("go.mod");
    let Ok(contents) = fs::read_to_string(&go_mod) else {
        return false;
    };
    contents
        .lines()
        .any(|line| line.trim() == "module lazuli.dev/runtime")
}

/// Append `use <runtime_dir>` to the scaffold's `go.work` and write
/// the same path into Lazurite.toml as `[lazuli] path = "<root>"`
/// (without the trailing `/runtime/go` — Lazurite.toml points at the
/// lazuli source root). The scaffold ships a `go.work` with `.` and
/// `./dist/go`; this adds the local runtime as a third entry so
/// `go mod tidy` / `go build` resolve `lazuli.dev/runtime` without
/// hitting the network.
///
/// Path discipline: relative when project and runtime share a common
/// ancestor (the canonical sibling layout), absolute otherwise (e.g.
/// different Windows drives). Avoids baking machine-specific paths
/// like `c:/Users/lucas/lazuli/...` into the scaffold output — those
/// would leak the scaffolder's filesystem layout into every new
/// project and break cross-developer builds on day one.
///
/// Once `[lazuli] path` is wired, downstream `lazuli generate go`
/// runs treat it as authoritative and emit `go.work` entries from
/// `Lazurite.toml [plugins]` plus this runtime entry. Subsequent
/// regens stay portable without manual intervention.
pub(crate) fn inject_runtime_into_go_work(project: &Path, runtime_dir: &Path) -> Result<()> {
    let go_work_path = project.join("go.work");
    let original = fs::read_to_string(&go_work_path)
        .with_context(|| format!("failed to read {}", go_work_path.display()))?;

    let project_abs = absolutize_project_root(project);
    let runtime_abs = if runtime_dir.is_absolute() {
        runtime_dir.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| Path::new(".").to_path_buf())
            .join(runtime_dir)
    };

    // Prefer relative; fall back to absolute when no common prefix
    // exists (e.g. Windows cross-drive). `relative_path` already
    // walks components and emits forward-slash output.
    let rel = relative_path(&project_abs, &runtime_abs);
    let go_work_entry = if rel.starts_with("..") || rel == "." || rel.starts_with('.') {
        rel.clone()
    } else {
        // No common ancestor — relative_path would return a string
        // that isn't a valid relative path. Use absolute as a
        // last-resort fallback.
        runtime_abs.to_string_lossy().replace('\\', "/")
    };

    // Lazurite.toml `[lazuli] path` points at the lazuli source ROOT
    // (e.g. `../lazuli`), not at `runtime/go`. Strip the trailing
    // `/runtime/go` so the field carries the canonical value the
    // rest of the codegen expects.
    let lazuli_root_entry = go_work_entry
        .strip_suffix("/runtime/go")
        .unwrap_or(&go_work_entry)
        .to_owned();

    // Idempotency: if the user already wired the runtime in go.work,
    // don't write again.
    if !original.contains(&go_work_entry) {
        let updated = if let Some(close_idx) = original.find(")") {
            let (head, tail) = original.split_at(close_idx);
            format!("{head}    {go_work_entry}\n{tail}")
        } else {
            format!("{original}\nuse {go_work_entry}\n")
        };
        fs::write(&go_work_path, updated)
            .with_context(|| format!("failed to write {}", go_work_path.display()))?;
    }

    // Also wire `[lazuli] path` into Lazurite.toml so downstream
    // tooling (@lazuli/vite, `lazuli generate go`, `lazuli generate
    // ts`) reads the same source-of-truth.
    let lazurite_path = project.join("Lazurite.toml");
    if let Ok(manifest_src) = fs::read_to_string(&lazurite_path) {
        if manifest_src.contains("path = \"") && manifest_src.contains("[lazuli]") {
            // Field already declared; leave it.
        } else {
            let updated_manifest =
                inject_lazuli_path_into_lazurite(&manifest_src, &lazuli_root_entry);
            if let Err(err) = fs::write(&lazurite_path, updated_manifest) {
                eprintln!("warning: failed to write [lazuli] path into Lazurite.toml: {err:#}");
            }
        }
    }

    Ok(())
}

/// Add `path = "<value>"` as the FIRST field of the `[lazuli]` section
/// in Lazurite.toml. Idempotent: returns the original source unchanged
/// when the section already declares a `path` (any value). When the
/// `[lazuli]` section is absent, appends a fresh one at end-of-file.
fn inject_lazuli_path_into_lazurite(src: &str, path: &str) -> String {
    if let Some(section_idx) = src.find("[lazuli]") {
        // Insertion point: end of the `[lazuli]\n` header line. The
        // new `path = ...` becomes the first field, sitting flush
        // against `runtime = "..."` and friends.
        let after_header_idx = section_idx + "[lazuli]".len();
        let newline_offset = src[after_header_idx..]
            .find('\n')
            .map(|n| after_header_idx + n + 1)
            .unwrap_or(src.len());
        let (head, tail) = src.split_at(newline_offset);
        return format!("{head}path = \"{path}\"\n{tail}");
    }
    // No [lazuli] section: append a complete block at EOF.
    let sep = if src.ends_with('\n') { "" } else { "\n" };
    format!("{src}{sep}\n[lazuli]\npath = \"{path}\"\n")
}
