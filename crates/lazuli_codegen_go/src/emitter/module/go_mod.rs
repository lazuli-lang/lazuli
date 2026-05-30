//! `go.mod` + `go.work` emission and the module-name / source-label
//! / transitive-deps helpers that feed it.
//!
//! Every `emit_module` invocation produces exactly one `go.mod` (and,
//! in workspace mode, one `go.work`). Both are pure-string builders:
//! input is `module`, `LazuriteManifest`, transitive dep list +
//! dev-replace path; output is the file body as a `String`. No source
//! map / span / per-feature context is involved, so the emission
//! cluster lives one module away from the per-feature walker.
//!
//! Wave R7-3 extract: lifted out of `module/mod.rs`.

use lazuli_ir::Module;

use super::super::deps::{GO_POSTGIS_DEP, TransitiveDep};
use super::super::printer::GoPrinter;
use super::to_kebab;
use crate::{GoEmitOptions, LazuriteManifest};
use lazuli_ir::{BuiltinType, TypeRef};

const DEFAULT_MODULE_NAME: &str = "lazuli/app";

/// Default Go toolchain pin emitted into `go.mod`. Matches
/// `runtime/go/go.mod` (currently `go 1.26.0`) so the generated
/// module shares the same toolchain expectation as the hand-written
/// Lazuli Go library. Go 1.26 ships `net/http.CrossOriginProtection`
/// (used by `runtime/go/lazuli/http_csrf.go`) plus the routing
/// enhancements depended on across `runtime/go/lazuli/http.go`.
const DEFAULT_GO_TOOLCHAIN: &str = "go 1.26.0";

/// Workspace/submodule builds resolve the runtime through `go.work`
/// or a local replace, so the generated module only needs to put the
/// runtime on Go's build list. The zero version keeps that require
/// self-contained and avoids pretending a proxy tag is needed in dev.
const WORKSPACE_RUNTIME_REQUIRE_VERSION: &str = "v0.0.0";

pub(super) fn resolve_module_name(
    module: &Module,
    options: &GoEmitOptions,
    manifest: Option<&LazuriteManifest>,
) -> String {
    if let Some(module_name) = manifest
        .map(|m| m.project_module.trim())
        .filter(|module_name| !module_name.is_empty())
    {
        return module_name.to_owned();
    }
    if let Some(name) = options
        .module_name
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return name.to_owned();
    }
    match module.app.as_ref() {
        Some(app) if !app.name.trim().is_empty() => format!("lazuli/{}", to_kebab(&app.name)),
        _ => DEFAULT_MODULE_NAME.to_owned(),
    }
}

pub(super) fn resolve_source_label(module: &Module) -> String {
    match module.app.as_ref() {
        Some(app) => app.name.clone(),
        None => "lazuli module".to_owned(),
    }
}

pub(super) fn emit_go_mod(
    module_name: &str,
    lazuli_go_version: &str,
    manifest: Option<&LazuriteManifest>,
    transitive_deps: &[&'static TransitiveDep],
    dev_replace_runtime: Option<&str>,
    workspace_mode: bool,
) -> String {
    let mut p = GoPrinter::new();
    p.line(&format!("module {}", module_name));
    p.blank();
    p.line(DEFAULT_GO_TOOLCHAIN);

    // Plugin + transitive deps. `lazuli.dev/runtime` is always required
    // because Go ignores replace/workspace entries that are not on the
    // module build list. Workspace mode uses a zero version; non-
    // workspace mode keeps the crate/runtime release pin.
    let plugin_requires: Vec<(String, String)> = if let Some(manifest) = manifest {
        manifest
            .plugins
            .iter()
            .filter_map(|(ref_str, plugin)| {
                let Some(module) = plugin.module.as_deref() else {
                    if plugin.path.is_some() {
                        eprintln!(
                            "warning: plugin `{}` has a local path but no module; skipping go.mod require",
                            ref_str
                        );
                    }
                    return None;
                };
                let version = plugin.version.as_deref().unwrap_or("v0.0.0-local");
                Some((module.to_owned(), version.to_owned()))
            })
            .collect()
    } else {
        Vec::new()
    };

    let mut sorted_deps = transitive_deps.to_vec();
    sorted_deps.sort_by_key(|dep| dep.module);
    sorted_deps.dedup_by_key(|dep| dep.module);

    let runtime_version = if workspace_mode {
        WORKSPACE_RUNTIME_REQUIRE_VERSION
    } else {
        lazuli_go_version
    };
    p.blank();
    p.line("require (");
    p.indent();
    // The Lazuli Go lib publishes a single Go module at
    // `lazuli.dev/runtime`; per-bucket subpackages (`auth`, `storage`,
    // `jobs`, the top-level `lazuli` package, ...) live under it.
    p.line(&format!("lazuli.dev/runtime {}", runtime_version));
    for (module, version) in &plugin_requires {
        p.line(&format!("{} {}", module, version));
    }
    for dep in sorted_deps {
        p.line(&format!("{} {}", dep.module, dep.version));
    }
    p.dedent();
    p.line(")");

    // Emit `replace lazuli.dev/runtime => <path>` whenever a local
    // runtime checkout is wired (workspace or not). Originally this
    // was workspace-mode-gated on the assumption that `go.work` `use`
    // entries fully satisfy the require. Empirically that's not
    // reliable: a freshly-scaffolded project whose go.work and
    // go.mod are otherwise identical to a working project fails with
    // `lazuli.dev/runtime@v0.0.0: unrecognized import path` until a
    // replace directive lands here. The replace is harmless when
    // workspace mode is also active (both point at the same path),
    // and saves the build when workspace resolution silently
    // skips the entry.
    if let Some(path) = dev_replace_runtime {
        p.blank();
        p.line(&format!("replace lazuli.dev/runtime => {}", path));
    }
    if let Some(manifest) = manifest
        && let Some(dev) = manifest.dev.as_ref()
    {
        let mut replacements = Vec::new();
        for (ref_str, path) in &dev.plugin_paths {
            let Some(plugin) = manifest.plugins.get(ref_str) else {
                continue;
            };
            let Some(module) = plugin.module.as_deref() else {
                continue;
            };
            replacements.push((module, path.as_str()));
        }
        if !replacements.is_empty() {
            p.blank();
            for (module, path) in replacements {
                p.line(&format!("replace {} => {}", module, path));
            }
        }
    }
    p.finish()
}

pub(super) fn emit_go_work(
    dev_runtime_path: Option<&str>,
    manifest: Option<&LazuriteManifest>,
) -> String {
    let mut p = GoPrinter::new();
    p.line(DEFAULT_GO_TOOLCHAIN);
    p.blank();
    p.line("use (");
    p.indent();
    p.line(".");
    p.line("./dist/go");
    if let Some(path) = dev_runtime_path {
        p.line(path);
    }
    if let Some(m) = manifest {
        let dev_overrides = m
            .dev
            .as_ref()
            .map(|d| &d.plugin_paths)
            .cloned()
            .unwrap_or_default();
        for (namespace, plugin) in &m.plugins {
            let path = dev_overrides
                .get(namespace)
                .cloned()
                .or_else(|| plugin.path.clone());
            if let Some(path) = path {
                p.line(&path);
            }
        }
    }
    p.dedent();
    p.line(")");
    p.finish()
}

pub(super) fn collect_transitive_deps(module: &Module) -> Vec<&'static TransitiveDep> {
    let mut deps = Vec::new();
    if module
        .features
        .iter()
        .flat_map(|feature| feature.resources.iter())
        .flat_map(|resource| resource.fields.iter())
        .any(|field| type_ref_contains_geopoint(&field.type_ref))
    {
        deps.push(&GO_POSTGIS_DEP);
    }
    deps
}

pub(super) fn type_ref_contains_geopoint(type_ref: &TypeRef) -> bool {
    match type_ref {
        TypeRef::Builtin(BuiltinType::SemanticGeoPoint) => true,
        TypeRef::Many(inner) => type_ref_contains_geopoint(inner),
        _ => false,
    }
}
