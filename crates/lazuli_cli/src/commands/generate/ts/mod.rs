//! `lazuli generate ts` — TypeScript user-code emitter.
//!
//! Carved out of `main.rs` as part of Wave R3-D (Rails-style refactor).
//! This module owns every TypeScript artifact that `lazuli generate ts`
//! writes under `dist/ts-{web,mobile}/`:
//!
//! - **Design tokens** (`emit_design_files`): Tailwind v4 CSS,
//!   `tokens.css`, `tokens.ts`, allow-list JSON. Derived from
//!   `module.design`.
//! - **Mobile-target runtime** (`emit_mobile_runtime_layout`):
//!   `dist/ts-mobile/runtime/layout.tsx`. Emitted once per project
//!   when an Expo frontend is declared.
//! - **Route-guard artifacts** (`emit_route_guard_artifacts`):
//!   audience-aware view gating; per-frontend (web + mobile).
//! - **Routes** (`emit_routes_artifacts`): typed route catalogs for
//!   each frontend, with `routes.ts` and JSON sidecars.
//! - **Preflight** (`emit_preflight_ts`): semantic preflight checks
//!   for `command`/`query` slots.
//! - **Per-feature artifacts** (`emit_feature_ts_artifacts`): for
//!   every `feature <name>` the IR carries, emit the SDK
//!   (`<feature>.ts`), barrel (`index.ts`), Zod (`<feature>.zod.ts`),
//!   React Query hooks (`<feature>.hooks.ts`), and the slot
//!   interfaces / record interfaces / enum aliases / cross-feature
//!   imports they need.
//! - **Mobile view scaffolds** (`scaffold_mobile_view_files`): the
//!   per-view Expo file under `app/clients/mobile/app/`. User-owned;
//!   we only write when the file doesn't exist.
//! - **Playwright fixtures**: per-view E2E shells emitted into
//!   `e2e/<feature>/<view>.spec.ts`.
//!
//! Internal organization (post-extraction; original `main.rs` cluster
//! preserved verbatim in this single file for now; subsequent passes
//! may split it into sibling modules — see `docs/proposals/rails-style-refactor-2026-05-24.md`
//! §Wave R3-D follow-up):
//!
//! - `generate_ts` is the entry point invoked by
//!   `commands::generate::generate_command` for `GenerateKind::Ts`.
//! - `emit_feature_sdk_ts`, `emit_feature_zod_ts`,
//!   `emit_feature_react_hooks_ts`, `emit_feature_barrel_ts` are
//!   the four per-feature emitters; the rest of the cluster is leaf
//!   helpers (identifier builders, TypeScript type mappers, Zod
//!   expression composers, enum/policy/audit formatters).
//!
//! ABI: `lazuli generate --kind ts ...` is byte-identical with the
//! pre-split build. The cluster boundary was chosen so no caller
//! outside this file references any helper here (verified via `grep`
//! against the rest of `main.rs`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::casing::{pascal_case, to_snake_case};
use crate::{
    build_module_from_path, cmd_new_frontends, collect_lzx_bundle, collect_lzx_experience_module,
    lazurite_manifest, playwright_fixture_config, project_root_for_input,
};

mod barrel;
mod cross_feature;
mod design;
mod format;
mod hooks;
mod ident;
mod mobile_views;
mod sdk;
mod types;
mod zod;

pub(crate) use barrel::emit_feature_barrel_ts;
// Siblings (sdk.rs) reach for these via `use super::{…}`; re-export.
pub(super) use cross_feature::{
    write_cross_feature_imports, write_plugin_semantic_aliases, write_referenced_enum_aliases,
};
use design::emit_design_files;
pub(crate) use format::find_enum_decl;
// Re-exported under the parent module so siblings (sdk, zod, hooks)
// keep their `use super::{escape_js_string, ...}` import shape after the
// format-helpers carve-out.
pub(super) use format::{
    enum_variant_ts_literal, escape_js_string, format_audit_ts, format_policy_ts,
    format_string_array, wire_key_fields_literal, write_deprecated_const_alias,
};
pub(crate) use hooks::emit_feature_react_hooks_ts;
// Same shape for ident.rs: siblings reach for these via
// `use super::{command_ident, …}` so we re-export here.
pub(super) use ident::{
    command_export_ident, command_ident, command_input_iface, legacy_query_ident, query_ident,
};
use mobile_views::scaffold_mobile_view_files;
pub(crate) use sdk::emit_feature_sdk_ts;
// Same shape for types.rs: siblings reach for these via
// `use super::{command_sdk_slots, …}` so we re-export here.
pub(super) use types::{
    TsSlot, command_input_slots, command_is_pure_read, command_output_ts_type, command_sdk_slots,
    find_resource, is_resource_ref, pick_query_resource_ts, query_args, resource_field_ts_name,
    resource_field_ts_type, ts_type_for_type_ref,
};
pub(crate) use zod::{
    command_schema_ident, command_zod_slots, emit_feature_zod_ts, zod_base_for_type_ref,
};

/// L0 #3 — emit TypeScript user-code for a Lazuli/Lazurite project.
/// Walks the package, runs every TS-side emitter (design tokens, per-feature
/// SDK, .lzx view hooks, slot interfaces, Zod schemas), and writes to
/// `dist/ts-<frontend>/`. Honors `Lazurite.toml [frontends.<name>]`.
pub(crate) fn generate_ts(input: &Path, output: Option<&Path>, check: bool) -> Result<()> {
    let project_root = project_root_for_input(input);
    let manifest = lazurite_manifest::load(&project_root).with_context(|| {
        format!(
            "failed to read {}",
            project_root.join("Lazurite.toml").display()
        )
    })?;
    let module = build_module_from_path(input)?;
    let lzx_bundle = collect_lzx_bundle(input);

    let mut files: Vec<lazuli_codegen_ts::GeneratedFile> = Vec::new();

    // Design tokens emission — same artifacts the legacy `generate_ts`
    // would have produced. Skips silently when `module.design` is None
    // (project hasn't authored design.lzi yet).
    if let Some(design) = module.design.as_ref() {
        files.extend(emit_design_files(design, &manifest));
    }

    // Mobile-target runtime: emit `dist/ts-mobile/runtime/layout.tsx`
    // once when the project declares an Expo frontend
    // (`docs/proposals/mobile-target.md` §5.4). The user-owned
    // `app/clients/mobile/app/_layout.tsx` is a one-line re-export of
    // this body; regen always rewrites this file.
    if manifest_has_expo_frontend(&manifest) {
        files.push(lazuli_codegen_ts::GeneratedFile {
            path: "dist/ts-mobile/runtime/layout.tsx".to_owned(),
            contents: lazuli_codegen_ts::mobile_runtime::emit_mobile_runtime_layout(),
        });
    }

    files.extend(
        lazuli_codegen_ts::lzx_audience_slot::emit_route_guard_artifacts(
            module.app.as_ref().or(lzx_bundle.app.as_ref()),
            &lzx_bundle.routes,
            &lzx_bundle.surfaces,
            &lzx_bundle.experiences,
            &module.features,
            lazuli_codegen_ts::lzx_audience_slot::RouteGuardTarget::Web,
        ),
    );
    files.push(lazuli_codegen_ts::GeneratedFile {
        path: "dist/ts-web/tests/fixtures.gen.ts".to_owned(),
        contents: lazuli_codegen_ts::playwright::emit_playwright_fixtures(
            &module,
            &lzx_bundle.routes,
            &lzx_bundle.surfaces,
            &lzx_bundle.experiences,
            &playwright_fixture_config(&project_root, manifest.as_ref()),
        ),
    });

    if let Some(contents) = lazuli_codegen_ts::emit_semantic_formatters_ts(&module) {
        for target_prefix in app_ts_target_prefixes(&module, &manifest) {
            files.push(lazuli_codegen_ts::GeneratedFile {
                path: format!("dist/{target_prefix}/runtime/formatters.gen.ts"),
                contents: contents.clone(),
            });
        }
    }

    // router-w1 (Wave 1): emit_routes_todo flipped — per-target routes.gen.tsx.
    let lzx_module = collect_lzx_experience_module(input);
    files.extend(lazuli_codegen_ts::routes::emit_routes_artifacts(
        lzx_module.app.as_ref().or(module.app.as_ref()),
        &lzx_module.routes,
        &lzx_module.surfaces,
        &lzx_module.experiences,
        &module.features,
        lazuli_codegen_ts::routes::RoutesTarget::Web,
    ));
    files.extend(lazuli_codegen_ts::routes::emit_routes_artifacts(
        lzx_module.app.as_ref().or(module.app.as_ref()),
        &lzx_module.routes,
        &lzx_module.surfaces,
        &lzx_module.experiences,
        &module.features,
        lazuli_codegen_ts::routes::RoutesTarget::Mobile,
    ));

    // Per-feature: SDK (audience-filtered if frontend declares audiences),
    // Zod schemas, .lzx view hooks (one file per audience/view tuple),
    // slot interfaces (one per @client.<slot> binding).
    let mut features: Vec<&lazuli_ir::Feature> = module.features.iter().collect();
    features.sort_by(|a, b| a.name.cmp(&b.name));
    for feature in features {
        files.extend(emit_feature_ts_artifacts(feature, &module, &manifest));
    }

    // LAZ-SEMANTIC-AUTO-VALIDATE — top-level dist/ts-web/preflight.gen.ts
    // side-effect-imports every per-feature preflight. Apps consume it
    // once (e.g. in main.tsx) so the registry is hot before any
    // useLazuliCommand renders.
    if let Some(contents) = lazuli_codegen_ts::emit_preflight_index_ts(&module) {
        files.push(lazuli_codegen_ts::GeneratedFile {
            path: "dist/ts-web/preflight.gen.ts".to_owned(),
            contents,
        });
    }

    // Plugin catalog — single JSON file consolidating every plugin's
    // manifest + README excerpt + Go/TS exports. Consumed by apps,
    // docs sites, the LSP, and the planned `lazuli plugins` CLI.
    // Spec: docs/proposals/plugin-catalog-file-2026-05-23.md.
    //
    // Vite aliases used to be emitted alongside the catalog as
    // `dist/ts-web/lazurite.vite.mjs`, but that meant consumer
    // `vite.config.ts` files imported from a build artifact (and
    // failed on fresh checkouts before the first `lazuli generate`).
    // Replaced by the `@lazuli/vite` runtime package, which reads
    // Lazurite.toml at vite-config-load time on the actual host.
    if let Some(m) = manifest.as_ref() {
        let project_root_abs =
            std::fs::canonicalize(&project_root).unwrap_or_else(|_| project_root.clone());
        if let Some(contents) = crate::plugin_catalog::emit_plugin_catalog(m, &project_root_abs) {
            files.push(lazuli_codegen_ts::GeneratedFile {
                path: "dist/plugin-catalog.json".to_owned(),
                contents,
            });
        }
    }

    if check {
        println!("lazuli generate ts --check");
        println!("would emit {} file(s):", files.len());
        for file in &files {
            println!("  {}", file.path);
        }
        return Ok(());
    }

    // Emitters return project-relative paths (e.g. `dist/ts-web/slug/...`).
    // When the user passes `--output <dir>` we honour it as a literal base
    // (legacy override + tests); otherwise default to project root so the
    // `dist/<target>/` prefix encoded in each path lands at its canonical
    // location. The manifest's `[frontends.<x>].out` is declarative — it
    // describes WHERE the dist directory lives but is NOT a join prefix.
    let out_dir = output.map(Path::to_path_buf).unwrap_or(project_root);

    fs::create_dir_all(&out_dir)
        .with_context(|| format!("creating output directory {}", out_dir.display()))?;

    for file in &files {
        crate::write_generated_file(&out_dir, &file.path, &file.contents)?;
    }

    // Per-view mobile scaffolds. Each mobile surface view writes one
    // `app/clients/mobile/app/<audience>/<expo-route>.tsx` placeholder
    // ONCE (idempotent — never overwrites user edits, mirroring
    // `cmd_new_frontends::scaffold_frontend_mobile`). Author replaces
    // the placeholder JSX with real RN components as soon as the
    // component library is chosen. See
    // `docs/proposals/mobile-target.md` §5.2.
    let scaffold_count = scaffold_mobile_view_files(&module, &out_dir)?;

    println!(
        "wrote {} file(s) to {} ({} mobile view scaffold{} written)",
        files.len(),
        out_dir.display(),
        scaffold_count,
        if scaffold_count == 1 { "" } else { "s" }
    );
    Ok(())
}

/// Per-feature TS emission walker. Wires the .lzx view emitters from
/// Wave 3 Cell B (`lazuli_codegen_ts::lzx::emit_surface_views`).
fn emit_feature_ts_artifacts(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    manifest: &Option<lazurite_manifest::Manifest>,
) -> Vec<lazuli_codegen_ts::GeneratedFile> {
    let mut out = Vec::new();
    let target_prefixes = feature_ts_target_prefixes(feature, manifest);
    if !feature.resources.is_empty()
        || !feature.records.is_empty()
        || !feature.commands.is_empty()
        || !feature.queries.is_empty()
    {
        for target_prefix in &target_prefixes {
            out.push(lazuli_codegen_ts::GeneratedFile {
                path: format!(
                    "dist/{}/{}/{}.gen.ts",
                    target_prefix, feature.name, feature.name
                ),
                contents: emit_feature_sdk_ts(feature, module),
            });
            out.push(lazuli_codegen_ts::GeneratedFile {
                path: format!(
                    "dist/{}/{}/{}.zod.ts",
                    target_prefix, feature.name, feature.name
                ),
                contents: emit_feature_zod_ts(feature, module),
            });
            // A.1 emits per-feature React hook wrappers (use<X> for every
            // command + query) into <feature>.react.gen.ts.
            if !feature.commands.is_empty() || !feature.queries.is_empty() {
                out.push(lazuli_codegen_ts::GeneratedFile {
                    path: format!(
                        "dist/{}/{}/{}.react.gen.ts",
                        target_prefix, feature.name, feature.name
                    ),
                    contents: emit_feature_react_hooks_ts(feature, module),
                });
            }
            // C.2 emits cap.File upload-orchestration hooks into a sibling
            // file (avoid collision with A.1's react.gen.ts above).
            if *target_prefix == "ts-web" {
                if let Some(contents) = lazuli_codegen_ts::emit_cap_file_hooks_ts(feature) {
                    out.push(lazuli_codegen_ts::GeneratedFile {
                        path: format!(
                            "dist/{}/{}/{}.cap-file.react.gen.ts",
                            target_prefix, feature.name, feature.name
                        ),
                        contents,
                    });
                }
                // LAZ-SEMANTIC-AUTO-VALIDATE Wave 2 — per-feature preflight
                // registrations for commands with @semantic.X fields whose
                // plugin declares a TS validator.
                if let Some(contents) = lazuli_codegen_ts::emit_preflight_ts(feature) {
                    out.push(lazuli_codegen_ts::GeneratedFile {
                        path: format!(
                            "dist/{}/{}/{}.preflight.gen.ts",
                            target_prefix, feature.name, feature.name
                        ),
                        contents,
                    });
                }
            }
            // A.1 also emits a per-feature barrel `index.ts` so consumers
            // import named hooks via `from '@app/sdk/<feature>'`.
            out.push(lazuli_codegen_ts::GeneratedFile {
                path: format!("dist/{}/{}/index.ts", target_prefix, feature.name),
                contents: emit_feature_barrel_ts(feature),
            });
        }
    }
    for target_prefix in &target_prefixes {
        if let Some(contents) = lazuli_codegen_ts::lzx_route_params::emit_route_params_ts(
            feature,
            module,
            target_prefix,
        ) {
            out.push(lazuli_codegen_ts::GeneratedFile {
                path: format!(
                    "dist/{}/{}/{}.routes.gen.ts",
                    target_prefix, feature.name, feature.name
                ),
                contents,
            });
        }
    }
    let app_name = manifest
        .as_ref()
        .map(|m| m.project.name.as_str())
        .unwrap_or("");
    for surface in &feature.surfaces {
        let target = match surface.target {
            lazuli_ir::SurfaceTarget::Web => {
                lazuli_codegen_ts::lzx::lzx_router_adapter::RouterTarget::ViteReact
            }
            lazuli_ir::SurfaceTarget::Mobile => {
                lazuli_codegen_ts::lzx::lzx_router_adapter::RouterTarget::Expo
            }
        };
        // surface carries its feature owner; emitter resolves refs internally.
        let _ = feature;
        out.extend(lazuli_codegen_ts::lzx::emit_surface_views(
            surface, target, app_name,
        ));
    }
    out
}

/// True when the manifest declares at least one Expo frontend. Drives
/// the singleton `dist/ts-mobile/runtime/layout.tsx` emission per
/// `docs/proposals/mobile-target.md` §5.4. Manifest-less generation
/// (legacy/test paths) returns false — the runtime layout only matters
/// when an Expo-targeted scaffold consumes it.
fn manifest_has_expo_frontend(manifest: &Option<lazurite_manifest::Manifest>) -> bool {
    manifest
        .as_ref()
        .map(|m| {
            m.frontends
                .values()
                .any(|f| matches!(f.target, lazurite_manifest::FrontendTarget::Expo))
        })
        .unwrap_or(false)
}

fn feature_ts_target_prefixes(
    feature: &lazuli_ir::Feature,
    manifest: &Option<lazurite_manifest::Manifest>,
) -> BTreeSet<&'static str> {
    let mut targets = BTreeSet::new();
    if let Some(manifest) = manifest {
        for frontend in manifest.frontends.values() {
            match frontend.target {
                lazurite_manifest::FrontendTarget::TanstackVite => {
                    targets.insert("ts-web");
                }
                lazurite_manifest::FrontendTarget::Expo => {
                    targets.insert("ts-mobile");
                }
            }
        }
    }
    for surface in &feature.surfaces {
        match surface.target {
            lazuli_ir::SurfaceTarget::Web => {
                targets.insert("ts-web");
            }
            lazuli_ir::SurfaceTarget::Mobile => {
                targets.insert("ts-mobile");
            }
        }
    }
    if targets.is_empty() {
        targets.insert("ts-web");
    }
    targets
}

fn app_ts_target_prefixes(
    module: &lazuli_ir::Module,
    manifest: &Option<lazurite_manifest::Manifest>,
) -> BTreeSet<&'static str> {
    let mut targets = BTreeSet::new();
    if let Some(manifest) = manifest {
        for frontend in manifest.frontends.values() {
            match frontend.target {
                lazurite_manifest::FrontendTarget::TanstackVite => {
                    targets.insert("ts-web");
                }
                lazurite_manifest::FrontendTarget::Expo => {
                    targets.insert("ts-mobile");
                }
            }
        }
    }
    for feature in &module.features {
        for surface in &feature.surfaces {
            match surface.target {
                lazuli_ir::SurfaceTarget::Web => {
                    targets.insert("ts-web");
                }
                lazuli_ir::SurfaceTarget::Mobile => {
                    targets.insert("ts-mobile");
                }
            }
        }
    }
    if targets.is_empty() {
        targets.insert("ts-web");
    }
    targets
}
