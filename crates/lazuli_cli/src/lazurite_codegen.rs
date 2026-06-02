//! Lazurite manifest → `lazuli_codegen_go::LazuriteManifest` adapter.
//!
//! Carved out of `main.rs` as part of Wave R6-5 (Rails-style refactor).
//! This module owns the plumbing that lowers an authored
//! `Lazurite.toml` into the codegen-side shape the Go emitter consumes:
//!
//! - plugin module-path resolution (local `go.mod` lookup),
//! - `[lazuli] path` runtime-replace resolution and the legacy ancestor
//!   heuristic for dev-replace,
//! - default Go module-name derivation from `app.name`.
//!
//! No behavior change vs. the pre-split build — `lazuli generate go`
//! produces byte-identical output.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::casing::to_kebab_case;
use crate::lazurite_manifest;
use crate::path_utils::{
    absolutize_for_codegen, absolutize_project_root, is_absolute_runtime_path, relative_path,
};

pub(crate) fn codegen_lazurite_manifest(
    manifest: &lazurite_manifest::Manifest,
    project_root: &Path,
    out_dir: Option<&Path>,
) -> lazuli_codegen_go::LazuriteManifest {
    let plugins = manifest
        .plugins
        .iter()
        .map(|(plugin_ref, plugin)| {
            let (module, version, path) = match plugin {
                lazurite_manifest::Plugin::Remote { module, version } => {
                    (Some(module.clone()), Some(version.clone()), None)
                }
                lazurite_manifest::Plugin::Local { path } => (None, None, Some(path.clone())),
            };
            // Resolve the plugin's Go module path so codegen can emit a
            // side-effect import in main.go. For Remote plugins the
            // Lazurite.toml `module` IS the Go module path; for Local
            // plugins we read the first-line `module ...` from
            // `<path>/go.mod`. This closes the init-order panic class
            // by guaranteeing the plugin's package init() lands in the
            // binary's transitive import graph — see
            // `runtime/go/lazuli/app_integration.go` for the deferred
            // resolution that lets Local plugins register their adapter
            // before the first facade call.
            let go_module = match plugin {
                lazurite_manifest::Plugin::Remote { module, .. } => Some(module.clone()),
                lazurite_manifest::Plugin::Local { path } => {
                    read_plugin_go_module(project_root, path)
                }
            };
            (
                plugin_ref.clone(),
                lazuli_codegen_go::LazuritePlugin {
                    module,
                    version,
                    path,
                    go_module,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    // Frente 1 — `[generate.go]` defaults apply transparently. Pilots
    // can omit the block entirely; the canonical
    // `emit_main = true / submodule = true / out = "dist/go"` shape lifts
    // from `GenerateGo::default()`.
    let generate_go = {
        let go = manifest.generate_go_or_default();
        // Resolve the Lazuli runtime/go path, in priority order
        // (SPEC 0030 — portable runtime wiring):
        //   1. `Lazurite.toml [lazuli] path` (authoritative — the
        //      author explicitly points at a local checkout).
        //   2. `LAZULI_RUNTIME_PATH` env (build-time fallback for
        //      CI / non-standard layouts).
        //   3. The legacy ancestor heuristic (sibling layout has no
        //      shared ancestor, so this only ever fires for the rare
        //      runtime-under-project layout).
        // Every branch computes a PROJECT-ROOT-RELATIVE path with the
        // real relativizer (`relative_path`) and REFUSES to emit an
        // absolute path: an absolute `replace`/`use` baked into a
        // committed artifact breaks `go build` on every other machine
        // (`RUNTIME-WIRING-ABSOLUTE-PATH-001`). When only an absolute
        // path is available (e.g. Windows cross-drive), we emit NOTHING
        // and tell the author to set a relative `[lazuli] path` or
        // `LAZULI_RUNTIME_PATH`.
        let manifest_runtime = out_dir.and_then(|out_dir| {
            manifest
                .lazuli
                .path
                .as_ref()
                .and_then(|p| resolve_runtime_replace_from_lazuli_root(project_root, out_dir, p))
        });
        let env_runtime = out_dir.and_then(|out_dir| {
            resolve_runtime_replace_from_env(project_root, out_dir)
        });
        let detected =
            out_dir.and_then(|out_dir| detect_runtime_dev_replace(project_root, out_dir));
        let resolved = manifest_runtime.or(env_runtime).or(detected);
        Some(lazuli_codegen_go::LazuriteGenerateGo {
            emit_main: go.emit_main,
            submodule: go.submodule,
            dev_replace: go
                .dev_replace
                .clone()
                .or_else(|| resolved.as_ref().map(|paths| paths.go_mod.clone())),
            dev_work_replace: go
                .dev_replace
                .clone()
                .or_else(|| resolved.map(|paths| paths.go_work)),
        })
    };
    let dev = manifest
        .dev
        .as_ref()
        .map(|dev| lazuli_codegen_go::LazuriteDev {
            plugin_paths: dev.plugin_paths.clone(),
        });

    lazuli_codegen_go::LazuriteManifest {
        project_module: manifest.project.module.clone(),
        plugins,
        generate_go,
        dev,
    }
}

/// Read the first-line `module <path>` directive from a local plugin's
/// `go.mod`. Used by `codegen_lazurite_manifest` to discover the Go
/// module path the codegen needs to emit a `_ "<module>"` side-effect
/// import in main.go (so the plugin's package init() runs and its
/// `lazuli.RegisterAdapter(...)` populates the registry).
///
/// Returns `None` when:
/// - the path does not resolve to a directory containing `go.mod`
/// - the file is unreadable
/// - no `module` directive is found in the first ~20 lines
///
/// `None` is a soft failure: the emitter skips that plugin's import,
/// which surfaces as the existing `ErrAdapterMissing` at facade resolve
/// time rather than as a codegen panic. This matches the proposal's
/// "additive, never break the build" discipline.
fn read_plugin_go_module(project_root: &Path, plugin_path: &str) -> Option<String> {
    let plugin_root = if Path::new(plugin_path).is_absolute() {
        std::path::PathBuf::from(plugin_path)
    } else {
        project_root.join(plugin_path)
    };
    let go_mod = plugin_root.join("go.mod");
    let contents = std::fs::read_to_string(&go_mod).ok()?;
    for line in contents.lines().take(40) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("module ") {
            // Trim trailing comments (`// indirect` style) and whitespace.
            let module = rest.split("//").next()?.trim();
            if !module.is_empty() {
                return Some(module.to_owned());
            }
        }
    }
    None
}

struct RuntimeDevReplace {
    go_mod: String,
    go_work: String,
}

/// Relativize a resolved runtime/go directory into the go.mod replace
/// path (relative to `dist/go/`, i.e. the out dir) and the go.work `use`
/// path (relative to the project root). Returns `None` — with a loud
/// `eprintln!` fix hint — when EITHER side can only be expressed as an
/// absolute path (no relative bridge exists, e.g. Windows cross-drive).
/// An absolute path must never be baked into a committed artifact
/// (`RUNTIME-WIRING-ABSOLUTE-PATH-001`).
fn relativize_runtime_dir(
    project_root: &Path,
    out_dir: &Path,
    runtime_dir: &Path,
    source_hint: &str,
) -> Option<RuntimeDevReplace> {
    let project_abs = absolutize_project_root(project_root);
    let out_abs = absolutize_for_codegen(project_root, out_dir);
    let go_mod = relative_path(&out_abs, runtime_dir);
    let go_work = relative_path(&project_abs, runtime_dir);
    if is_absolute_runtime_path(&go_mod) || is_absolute_runtime_path(&go_work) {
        eprintln!(
            "warning: cannot emit a portable runtime replace from {source_hint} \
             (resolved to an absolute path with no relative bridge to the project, \
             e.g. a different Windows drive). Skipping the `replace lazuli.dev/runtime` \
             directive — set a relative `[lazuli] path` in Lazurite.toml or export \
             LAZULI_RUNTIME_PATH. (RUNTIME-WIRING-ABSOLUTE-PATH-001)"
        );
        return None;
    }
    Some(RuntimeDevReplace { go_mod, go_work })
}

/// Resolve the runtime replace from the `[lazuli] path` source root
/// (e.g. `../lazuli` or `../../lazuli`). The runtime/go module lives at
/// `<root>/runtime/go`. The root may be relative (canonical) or absolute
/// (legacy / cross-machine) — either way we anchor it against the
/// project root, then relativize, so the EMITTED path is always relative
/// or absent. Computed with the real relativizer rather than blind
/// string concat so the per-pilot depth (hostpoint `../lazuli`, pauta
/// `../../lazuli`) comes out correct without hardcoding a depth.
fn resolve_runtime_replace_from_lazuli_root(
    project_root: &Path,
    out_dir: &Path,
    lazuli_root: &str,
) -> Option<RuntimeDevReplace> {
    let project_abs = absolutize_project_root(project_root);
    let root_path = Path::new(lazuli_root);
    let root_abs = if root_path.is_absolute() {
        root_path.to_path_buf()
    } else {
        project_abs.join(root_path)
    };
    let runtime_dir = normalize_dots(&root_abs.join("runtime").join("go"));
    relativize_runtime_dir(project_root, out_dir, &runtime_dir, "[lazuli] path")
}

/// Resolve the runtime replace from the `LAZULI_RUNTIME_PATH` env var —
/// the build-time fallback consulted by `lazuli generate go` when
/// `[lazuli] path` is unset (SPEC 0030; previously the env was honored
/// only by `lazuli new`). The env value points directly at the
/// `runtime/go` dir (matching `commands/new/runtime_wiring.rs`'s
/// `locate_lazuli_runtime_dir`). We RELATIVIZE it at emit time and never
/// bake the absolute env value into the committed artifact — if it can
/// only be expressed absolutely, we emit nothing + diagnose.
fn resolve_runtime_replace_from_env(
    project_root: &Path,
    out_dir: &Path,
) -> Option<RuntimeDevReplace> {
    let env_path = std::env::var("LAZULI_RUNTIME_PATH").ok()?;
    if env_path.trim().is_empty() {
        return None;
    }
    let candidate = PathBuf::from(env_path);
    // Guard: only honor a dir that actually IS the lazuli runtime, so a
    // stale env var doesn't wire a bogus replace.
    let go_mod = candidate.join("go.mod");
    let is_runtime = std::fs::read_to_string(&go_mod)
        .map(|c| c.lines().any(|l| l.trim() == "module lazuli.dev/runtime"))
        .unwrap_or(false);
    if !is_runtime {
        return None;
    }
    let candidate_abs = if candidate.is_absolute() {
        candidate
    } else {
        absolutize_project_root(project_root).join(candidate)
    };
    relativize_runtime_dir(
        project_root,
        out_dir,
        &normalize_dots(&candidate_abs),
        "LAZULI_RUNTIME_PATH",
    )
}

/// Collapse `.` and `..` components in an ABSOLUTE path lexically (no
/// filesystem access — the runtime checkout may not exist on this
/// machine at codegen time, e.g. a `--check` run). Needed because
/// `relative_path` walks raw components: a literal `..` left in the
/// joined path (from a relative `[lazuli] path`) would otherwise pollute
/// the common-prefix walk and yield a wrong relative result.
fn normalize_dots(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out: Vec<Component> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                // Pop a normal segment if we have one; otherwise keep the
                // `..` (can't ascend past a root/prefix lexically).
                if matches!(out.last(), Some(Component::Normal(_))) {
                    out.pop();
                } else {
                    out.push(comp);
                }
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out.iter().map(|c| c.as_os_str()).collect()
}

fn detect_runtime_dev_replace(project_root: &Path, out_dir: &Path) -> Option<RuntimeDevReplace> {
    let out_abs = absolutize_for_codegen(project_root, out_dir);
    for parent in out_abs.ancestors() {
        let runtime_dir = parent.join("runtime").join("go");
        let go_mod = runtime_dir.join("go.mod");
        let Ok(contents) = std::fs::read_to_string(&go_mod) else {
            continue;
        };
        if !contents
            .lines()
            .any(|line| line.trim() == "module lazuli.dev/runtime")
        {
            continue;
        }
        // Relativize + guard: an absolute result (cross-drive) is
        // refused so no absolute path leaks into a committed artifact.
        return relativize_runtime_dir(project_root, out_dir, &runtime_dir, "ancestor heuristic");
    }
    None
}

/// Derive the Go module name from the IR's `app.name` (kebab-cased,
/// per proposal §1.1). Falls back to `lazuli/app` when no manifest
/// surfaces a name.
pub(crate) fn default_go_module_name(module: &lazuli_ir::Module) -> String {
    let name = module
        .app
        .as_ref()
        .map(|app| app.name.as_str())
        .unwrap_or("app");
    format!("lazuli/{}", to_kebab_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an absolute project root under the platform's drive/root so
    /// `relativize_runtime_dir` produces `..`-relative output (not the
    /// cross-drive absolute fallback). On Windows we pin drive `C:` so the
    /// computed path is deterministic.
    fn abs_root(rel_segments: &[&str]) -> PathBuf {
        #[cfg(windows)]
        let mut p = PathBuf::from(r"C:\");
        #[cfg(not(windows))]
        let mut p = PathBuf::from("/");
        for seg in rel_segments {
            p.push(seg);
        }
        p
    }

    // SPEC 0030 — sibling layout (hostpoint): project and lazuli are
    // siblings, so `[lazuli] path = "../lazuli"`.
    #[test]
    fn lazuli_path_sibling_layout_is_relative() {
        let project = abs_root(&["Users", "lucas", "hostpoint"]);
        let out = project.join("dist").join("go");
        let r = resolve_runtime_replace_from_lazuli_root(&project, &out, "../lazuli")
            .expect("sibling layout resolves");
        assert_eq!(r.go_work, "../lazuli/runtime/go");
        assert_eq!(r.go_mod, "../../../lazuli/runtime/go");
        assert!(!is_absolute_runtime_path(&r.go_work));
        assert!(!is_absolute_runtime_path(&r.go_mod));
    }

    // SPEC 0030 — nested layout (pauta): project lives two levels under
    // the shared ancestor of lazuli, so `[lazuli] path = "../../lazuli"`.
    // The depth math must come out DIFFERENT from the sibling case.
    #[test]
    fn lazuli_path_nested_layout_is_relative_and_deeper() {
        let project = abs_root(&["Users", "lucas", "dev", "pauta-web-monorepo"]);
        let out = project.join("dist").join("go");
        let r = resolve_runtime_replace_from_lazuli_root(&project, &out, "../../lazuli")
            .expect("nested layout resolves");
        assert_eq!(r.go_work, "../../lazuli/runtime/go");
        assert_eq!(r.go_mod, "../../../../lazuli/runtime/go");
        assert!(!is_absolute_runtime_path(&r.go_work));
        assert!(!is_absolute_runtime_path(&r.go_mod));
        // Distinct from the sibling case — proves the depth generalizes.
        assert_ne!(r.go_work, "../lazuli/runtime/go");
    }

    // SPEC 0030 — an ABSOLUTE `[lazuli] path` that has no relative bridge
    // to the project (different Windows drive) must emit NOTHING, not an
    // absolute path baked into the committed artifact.
    #[cfg(windows)]
    #[test]
    fn lazuli_path_cross_drive_absolute_emits_nothing() {
        let project = PathBuf::from(r"D:\work\proj");
        let out = project.join("dist").join("go");
        let r = resolve_runtime_replace_from_lazuli_root(&project, &out, r"C:\Users\lucas\lazuli");
        assert!(
            r.is_none(),
            "cross-drive absolute path must not be emitted into a committed artifact"
        );
    }

    // SPEC 0030 fix #3 — `LAZULI_RUNTIME_PATH` is consulted by the
    // codegen resolver (not just `lazuli new`) as the build-time fallback
    // when `[lazuli] path` is unset, and its result is RELATIVIZED (never
    // baked absolute). Uses a real on-disk fake runtime so the
    // `module lazuli.dev/runtime` guard passes.
    #[test]
    fn env_override_resolves_relativized_when_config_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(tmp.path()).unwrap();
        // project at <base>/proj ; runtime at <base>/lazuli/runtime/go
        let project = base.join("proj");
        let out = project.join("dist").join("go");
        let runtime = base.join("lazuli").join("runtime").join("go");
        std::fs::create_dir_all(&runtime).unwrap();
        std::fs::write(runtime.join("go.mod"), "module lazuli.dev/runtime\n\ngo 1.26.0\n")
            .unwrap();

        // Serialize env mutation; restore afterwards.
        let prev = std::env::var("LAZULI_RUNTIME_PATH").ok();
        unsafe {
            std::env::set_var("LAZULI_RUNTIME_PATH", &runtime);
        }
        let r = resolve_runtime_replace_from_env(&project, &out);
        match prev {
            Some(v) => unsafe { std::env::set_var("LAZULI_RUNTIME_PATH", v) },
            None => unsafe { std::env::remove_var("LAZULI_RUNTIME_PATH") },
        }

        let r = r.expect("env override resolves to the fake runtime");
        assert_eq!(r.go_work, "../lazuli/runtime/go");
        assert_eq!(r.go_mod, "../../../lazuli/runtime/go");
        assert!(!is_absolute_runtime_path(&r.go_work));
        assert!(!is_absolute_runtime_path(&r.go_mod));
    }

    // An ABSOLUTE `[lazuli] path` that DOES share a root with the project
    // is relativized at emit time (never baked as absolute).
    #[test]
    fn lazuli_path_absolute_same_root_is_relativized() {
        let project = abs_root(&["Users", "lucas", "hostpoint"]);
        let out = project.join("dist").join("go");
        let lazuli_abs = abs_root(&["Users", "lucas", "lazuli"]);
        let r = resolve_runtime_replace_from_lazuli_root(
            &project,
            &out,
            &lazuli_abs.to_string_lossy(),
        )
        .expect("same-root absolute path relativizes");
        assert_eq!(r.go_work, "../lazuli/runtime/go");
        assert!(!is_absolute_runtime_path(&r.go_work));
        assert!(!is_absolute_runtime_path(&r.go_mod));
    }
}
