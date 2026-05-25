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
use std::path::Path;

use crate::casing::to_kebab_case;
use crate::lazurite_manifest;
use crate::path_utils::{absolutize_for_codegen, absolutize_project_root, relative_path};

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
        // Resolve the Lazuli runtime/go path. Lazurite.toml's
        // `[lazuli] path` is authoritative (the user explicitly
        // points at a local checkout); fall back to the ancestor
        // heuristic for legacy projects that haven't set it.
        let detected =
            out_dir.and_then(|out_dir| detect_runtime_dev_replace(project_root, out_dir));
        let manifest_runtime = manifest.lazuli.path.as_ref().map(|p| {
            // `path` is the lazuli source ROOT (e.g. `../lazuli`);
            // the runtime/go module lives at `<root>/runtime/go`.
            let runtime_rel = format!("{}/runtime/go", p.trim_end_matches('/'));
            // dist/go/go.mod sits TWO levels deeper than the project
            // root (project → dist → dist/go), so prepend `../../`
            // for the go.mod replace.
            RuntimeDevReplace {
                go_mod: format!("../../{}", runtime_rel),
                go_work: runtime_rel,
            }
        });
        let resolved = manifest_runtime.or(detected);
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

fn detect_runtime_dev_replace(project_root: &Path, out_dir: &Path) -> Option<RuntimeDevReplace> {
    let project_abs = absolutize_project_root(project_root);
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
        return Some(RuntimeDevReplace {
            go_mod: relative_path(&out_abs, &runtime_dir),
            go_work: relative_path(&project_abs, &runtime_dir),
        });
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
