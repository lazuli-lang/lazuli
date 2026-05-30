//! Top-level emitter walker. Drives per-feature emission and the root
//! `go.mod`. For cell E1 the per-feature output is intentionally
//! empty (just banner + `package` directive); per-kind walkers
//! (Resource, Command, Query, …) land in E2-E4 and G1-G7.
//!
//! Per proposal §2.4 every iteration is deterministic — features are
//! sorted by name before emission so output is byte-equivalent across
//! runs regardless of feature insertion order at the IR layer.

use std::collections::BTreeMap;

use lazuli_ir::{Gate, Module};

use super::audit::{emit_audit_log_ddl, emit_audit_metadata};
use super::cross_feature::CrossFeatureIndex;
use super::error_resolver::{APP_ERROR_RESOLUTION_PATH, emit_app_error_resolution};
use super::handlers::emit_handler_stubs;
use super::lint::check_generated_file;
use super::migration_ddl::emit_migrations;
use super::root::{LAZULI_APP_PATH, MAIN_GO_PATH, emit_lazuli_app_gen, emit_main_go};
use crate::{GeneratedFile, GoEmitOptions, LAZULI_GO_VERSION, LazuriteManifest};

// Wave R7-3 extract — `EmitContext`/`GoSourceContext` cluster moved
// into `module/context.rs`; `go.mod` / `go.work` emission cluster
// moved into `module/go_mod.rs`; small misc helpers (`emit_feature_stub`,
// `to_kebab`) moved into `module/helpers.rs`. R9 extract — per-feature
// walker body moved into `module/per_feature.rs` so the top-level
// orchestrator stays under the file budget. Re-exported so callers
// outside this module (every per-kind emitter under `emitter/*`) keep
// their `super::module::{GoSourceContext, EmitContext}` imports working.
mod context;
mod go_mod;
mod helpers;
mod per_feature;

pub use context::{EmitContext, GoSourceContext};
use go_mod::{
    collect_transitive_deps, emit_go_mod, emit_go_work, resolve_module_name, resolve_source_label,
};
use helpers::to_kebab;
use per_feature::emit_feature_files;

// Default Go module path (when the caller supplies none and the IR exposes no
// `app.name`) matches proposal §1.1's "fallback `lazuli/app`" rule.

/// Walk the IR module and produce every `.gen.go` plus the root
/// `go.mod`. Per cell E1 this only emits the file skeleton; kinds
/// land in subsequent cells.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_go::{emitter::emit_module, GoEmitOptions};
/// let files = emit_module(&module, &GoEmitOptions::default(), None, None);
/// ```
pub fn emit_module(
    module: &Module,
    options: &GoEmitOptions,
    manifest: Option<&LazuriteManifest>,
    source_context: Option<GoSourceContext<'_>>,
) -> Vec<GeneratedFile> {
    let base_module_name = resolve_module_name(module, options, manifest);
    let submodule = manifest
        .and_then(|m| m.generate_go.as_ref())
        .map(|g| g.submodule)
        .unwrap_or(true);
    let module_name = if manifest.is_some() && submodule {
        format!("{}/generated", base_module_name.trim_end_matches('/'))
    } else {
        base_module_name
    };
    let lazuli_go_version = if options.lazuli_go_version.is_empty() {
        LAZULI_GO_VERSION.to_owned()
    } else {
        options.lazuli_go_version.clone()
    };
    let transitive_deps = collect_transitive_deps(module);
    let dev_replace_runtime = manifest
        .and_then(|m| m.generate_go.as_ref())
        .and_then(|g| g.dev_replace.as_deref());
    let dev_work_runtime = manifest
        .and_then(|m| m.generate_go.as_ref())
        .and_then(|g| g.dev_work_replace.as_deref())
        .or(dev_replace_runtime);

    // BTreeMap so the iteration order is deterministic regardless of
    // how features were inserted into the IR `Vec`. Feature names
    // are unique per module.
    let mut features: BTreeMap<&str, &lazuli_ir::Feature> = BTreeMap::new();
    for feature in &module.features {
        features.insert(feature.name.as_str(), feature);
    }

    // Capacity hint: go.mod + main.go + lazuli_app.gen.go + per-feature
    // (1 stub + up to 3 kind files). The vec grows on miss; the hint
    // keeps the common case allocation-free.
    let mut files = Vec::with_capacity(features.len() * 4 + 3);

    // Workspace mode: when `manifest` is present and `submodule` is on,
    // the project root carries a `go.work` that `use`s both the
    // application module and `dist/go`. The generated module still
    // requires `lazuli.dev/runtime` so Go puts it on the build list;
    // the workspace/replace layer supplies the local source checkout.
    let workspace_mode = manifest.is_some() && submodule;
    if manifest.is_none() || submodule {
        // Root `go.mod` first so byte-comparison fixtures find it at
        // index 0. In Lazurite sub-module mode this is the generated
        // module's `dist/go/go.mod`; the CLI writes top-level
        // companions such as `go.work` at the project root.
        files.push(GeneratedFile {
            path: "go.mod".to_owned(),
            contents: emit_go_mod(
                &module_name,
                &lazuli_go_version,
                manifest,
                &transitive_deps,
                dev_replace_runtime,
                workspace_mode,
            ),
        });
    }
    if manifest.is_some() && submodule {
        files.push(GeneratedFile {
            path: "go.work".to_owned(),
            contents: emit_go_work(dev_work_runtime, manifest),
        });
    }

    // Phase Prep §1.1 mini-cell pré-E3 — build the cross-feature
    // resolver index once per run, before any per-feature walker
    // sees a type. The index lifts cross-feature `UserDefined` /
    // `EnumRef` references (analyzer leaves these with
    // `qname.feature = None`) to `<owner>.<Name>` plus a
    // `<module>/<owner>` import.
    let cross_index = CrossFeatureIndex::build(module);

    let source_label = resolve_source_label(module);

    // Cell I2 — root `main.go` (emitted unless Lazurite disables it) + `lazuli_app.gen.go`
    // (skipped when `module.app == None` and no observable surface).
    // Ordered after `go.mod` and before per-feature files so reading
    // the output listing top-down surfaces the binary entry first.
    let emit_main = manifest
        .and_then(|m| m.generate_go.as_ref())
        .map(|g| g.emit_main)
        .unwrap_or(true);
    if emit_main {
        files.push(GeneratedFile {
            path: MAIN_GO_PATH.to_owned(),
            contents: emit_main_go(module, &module_name, &source_label, manifest),
        });
    }
    if let Some(contents) = emit_lazuli_app_gen(module, &source_label) {
        files.push(GeneratedFile {
            path: LAZULI_APP_PATH.to_owned(),
            contents,
        });
    }

    // Cell B4-runtime-facade — `app/app_integrations.gen.go` wires
    // each `registry.bindings.<name>: <Kind> / adapter @lazuli/plugin-<x>`
    // declaration to the runtime adapter registry via
    // `lazuli.RegisterAppIntegration`. Skipped when no integration
    // carries an adapter (legacy env-var-only entries do not need a
    // facade binding). See docs/proposals/the canonical pilot-complete-roadmap-2026-05-18.md §3.5.
    if let Some(contents) =
        crate::emitter::app_integration::emit_app_integrations(&source_label, module)
    {
        files.push(GeneratedFile {
            path: crate::emitter::app_integration::APP_INTEGRATIONS_PATH.to_owned(),
            contents,
        });
    }

    // PG.C — emit `dist/go/plan/catalog.gen.go` when the analyzer
    // surfaced plan facts. The file is skipped when the package
    // declares no `plan` blocks (the runtime defaults to "no
    // subscription gating").
    if let Some(facts) = &options.plan_gate
        && let Some(contents) = crate::emitter::plan::emit_plan_catalog_file(facts)
    {
        files.push(GeneratedFile {
            path: "plan/catalog.gen.go".to_owned(),
            contents,
        });
    }

    // RB.C — emit `dist/go/rbac/rbac.gen.go` when the package declares
    // a `permission` / `role` catalog. See
    // `docs/proposals/rbac-catalog-vocab.md` §Codegen-Go.
    if let Some(contents) = crate::emitter::rbac::emit_rbac_file(&source_label, module) {
        files.push(GeneratedFile {
            path: "rbac/rbac.gen.go".to_owned(),
            contents,
        });
    }

    let source_context = source_context.as_ref();

    // PG.C.1 — gate map threaded through to every per-callable emit
    // context so commands / queries / jobs / webhooks / apis can emit
    // the runtime prelude when their authored body declares a gate.
    let gate_map: Option<&BTreeMap<String, Vec<Gate>>> =
        options.plan_gate.as_ref().map(|facts| &facts.gates);

    for feature in features.values() {
        files.extend(emit_feature_files(
            feature,
            &source_label,
            &module_name,
            &cross_index,
            source_context,
            gate_map,
        ));
    }

    // Cell CODEGEN-1 (IR Error-Vocab) — app-level
    // `app/error_resolution.gen.go`. Walks every feature, gathers each
    // declared `FeatureErrors`, and registers them with the runtime
    // resolver via `lazuli.RegisterFeatureErrors(...)`. Skipped when no
    // feature declares an `errors` block. See
    // `docs/proposals/ir-error-messages-vocab.md` §4.1.3.
    if let Some(contents) = emit_app_error_resolution(&source_label, module, &module_name) {
        files.push(GeneratedFile {
            path: APP_ERROR_RESOLUTION_PATH.to_owned(),
            contents,
        });
    }

    // Cell N3 — DDL migration emission. Walks all resources across all
    // features and emits `migrations/<NNN>_<feature>_<resource>.sql`
    // files at the module root. Resource-level (not feature-level) so
    // numbering stays stable across feature reorderings.
    files.extend(emit_migrations(module, &source_label));

    // Cell B15 — audit_log table DDL + per-command audit metadata
    // emission. The shared audit table lands at
    // `migrations/audit_log.sql`; per-command metadata lands beside
    // command.gen.go when the command declares `audit default`.
    files.push(emit_audit_log_ddl());
    files.extend(emit_audit_metadata(module));

    // Handler stubs at `app/features/<feature>/handlers/<name>.go` —
    // Tier 1 portable code per `docs/project-structure.md`. Returned
    // paths are project-root-relative (prefix `app/features/`); the
    // orchestrator detects that prefix and writes outside the codegen
    // `out_dir` (which is `dist/go`), preserving the "dist is
    // disposable" invariant.
    //
    // Idempotency on already-authored handlers is enforced at write
    // time by the orchestrator (skip-if-exists). The codegen here
    // always emits a fresh stub per discovered `@fn.*` / `@hook.*`
    // reference; the writer decides whether to overwrite.
    files.extend(emit_handler_stubs(
        module,
        &module_name,
        &std::collections::BTreeSet::new(),
    ));

    for file in &files {
        // Skip lint on handler stubs — they live in the user package
        // (`package <feature>`), not in `<feature>gen`, so the
        // generated-file lint (which targets `.gen.go`) is irrelevant.
        if file.path.starts_with("app/features/") {
            continue;
        }
        if let Err(err) = check_generated_file(&file.contents, &file.path) {
            // Generated-file lint failure is a framework bug, not a user
            // bug. We surface it to stderr but keep emitting — the
            // downstream `lazuli generate go` orchestrator will see the
            // bad file and report a more actionable error path. Returning
            // `Result` here would break the public Vec<GeneratedFile>
            // signature.
            eprintln!("lazuli_codegen_go: generated-file lint failure: {err}");
        }
    }

    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_kebab_lower_passthrough() {
        assert_eq!(to_kebab("marketplace"), "marketplace");
    }

    #[test]
    fn to_kebab_pascal_inserts_dashes() {
        assert_eq!(to_kebab("MarketPlace"), "market-place");
    }

    #[test]
    fn to_kebab_handles_underscores_and_spaces() {
        assert_eq!(to_kebab("hello_world test"), "hello-world-test");
    }
}
