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

use crate::casing::{lower_camel, pascal_case, to_snake_case};
use crate::{
    build_module_from_path, cmd_new_frontends, collect_lzx_bundle, collect_lzx_experience_module,
    lazurite_manifest, playwright_fixture_config, project_root_for_input,
};

mod barrel;
mod design;
mod hooks;
mod mobile_views;

pub(crate) use barrel::emit_feature_barrel_ts;
use design::emit_design_files;
pub(crate) use hooks::emit_feature_react_hooks_ts;
use mobile_views::scaffold_mobile_view_files;

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
        let project_root_abs = std::fs::canonicalize(&project_root)
            .unwrap_or_else(|_| project_root.clone());
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
        if let Some(contents) =
            lazuli_codegen_ts::lzx_route_params::emit_route_params_ts(feature, module, target_prefix)
        {
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

pub(crate) fn emit_feature_sdk_ts(feature: &lazuli_ir::Feature, module: &lazuli_ir::Module) -> String {
    let mut s = String::new();
    writeln!(s, "// Code generated by lazuli; DO NOT EDIT.").ok();
    writeln!(
        s,
        "import {{ defineCommand, defineQuery, type ID, type Money, type Time }} from \"@lazuli/runtime\";"
    )
    .ok();
    writeln!(s).ok();
    write_cross_feature_imports(&mut s, feature, module);
    write_plugin_semantic_aliases(&mut s, feature);
    write_referenced_enum_aliases(&mut s, feature, module);

    let mut records: Vec<&lazuli_ir::Record> = feature.records.iter().collect();
    records.sort_by(|a, b| a.name.cmp(&b.name));
    for record in records {
        write_record_interface(&mut s, record, module);
    }

    let mut resources: Vec<&lazuli_ir::Resource> = feature.resources.iter().collect();
    resources.sort_by(|a, b| a.name.cmp(&b.name));
    for resource in resources {
        write_resource_interface(&mut s, resource, module);
    }

    let mut commands: Vec<&lazuli_ir::Command> = feature.commands.iter().collect();
    commands.sort_by(|a, b| a.name.cmp(&b.name));
    for command in commands {
        write_command_sdk(&mut s, feature, command, module);
    }

    let mut queries: Vec<&lazuli_ir::Query> = feature.queries.iter().collect();
    queries.sort_by(|a, b| a.name().cmp(b.name()));
    for query in queries {
        write_query_sdk(&mut s, feature, query, module);
    }

    // router-w4 — per-resource lifecycle_route helpers. Appended at
    // the tail so feature SDK consumers can `import { hostLifecycleRoute }
    // from '@hostpoint/sdk/host/host.gen'` and the routes.gen.tsx
    // beforeLoad closures can call the helper via the same path.
    if let Some(helpers) = lazuli_codegen_ts::emit_lifecycle_route_helpers_ts(feature) {
        writeln!(s).ok();
        s.push_str(&helpers);
    }

    s
}

/// Emit `import { X } from '../other-feature/other-feature.gen';` lines
/// for every enum/record referenced by this feature but declared in
/// another feature. Closes WAR-CODEGEN-TS-01 + WAR-CODEGEN-XFEAT-01/02:
/// previously such cross-feature references silently dropped the import
/// and produced `tsc` errors at the consumer site, forcing users to
/// duplicate enums/records across every consuming feature.
fn write_cross_feature_imports(
    s: &mut String,
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
) {
    // Map of owner-feature name → set of type names imported from it.
    let mut imports: std::collections::BTreeMap<String, BTreeSet<String>> =
        std::collections::BTreeMap::new();
    collect_cross_feature_refs(feature, module, &mut imports);

    if imports.is_empty() {
        return;
    }

    let mut emitted = false;
    for (owner_feature, names) in &imports {
        let mut sorted: Vec<&String> = names.iter().collect();
        sorted.sort();
        let joined = sorted
            .iter()
            .map(|n| pascal_case(n))
            .collect::<Vec<_>>()
            .join(", ");
        // Emit both import (for local use in resource/command shapes)
        // AND re-export (so existing consumer code that imports the
        // type from this feature's .gen.ts continues to work after a
        // duplicate alias is removed). `export type { ... }` is
        // required because enum/record cross-feature refs are
        // type-only and isolatedModules rejects bare `export { ... }`
        // when the symbol carries no value.
        writeln!(
            s,
            "import type {{ {joined} }} from \"../{owner_feature}/{owner_feature}.gen\";"
        )
        .ok();
        writeln!(
            s,
            "export type {{ {joined} }} from \"../{owner_feature}/{owner_feature}.gen\";"
        )
        .ok();
        emitted = true;
    }
    if emitted {
        writeln!(s).ok();
    }
}

/// Walk every field/slot of every record/resource/command/query in
/// `feature` and accumulate the set of enum/record names that are
/// referenced but DECLARED in another feature.
fn collect_cross_feature_refs(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    out: &mut std::collections::BTreeMap<String, BTreeSet<String>>,
) {
    let walk_type = |type_ref: &lazuli_ir::TypeRef,
                     out: &mut std::collections::BTreeMap<String, BTreeSet<String>>| {
        let mut stack: Vec<&lazuli_ir::TypeRef> = vec![type_ref];
        while let Some(t) = stack.pop() {
            match t {
                lazuli_ir::TypeRef::Many(inner) => stack.push(inner),
                lazuli_ir::TypeRef::EnumRef(qn) | lazuli_ir::TypeRef::UserDefined(qn) => {
                    if let Some(owner) = owner_feature_for_type(qn, module, feature) {
                        out.entry(owner)
                            .or_insert_with(BTreeSet::new)
                            .insert(qn.name.clone());
                    }
                }
                _ => {}
            }
        }
    };
    for record in &feature.records {
        for field in &record.fields {
            walk_type(&field.type_ref, out);
        }
    }
    for resource in &feature.resources {
        for field in &resource.fields {
            walk_type(&field.type_ref, out);
        }
    }
    for command in &feature.commands {
        for slot in command_sdk_slots(feature, command, module) {
            walk_type(&slot.type_ref, out);
        }
        if let lazuli_ir::CommandEffect::Returns(effect) = &command.effect {
            walk_type(&effect.return_type, out);
        }
    }
    for query in &feature.queries {
        for slot in query_args(feature, query, module) {
            walk_type(&slot.type_ref, out);
        }
    }
}

/// Resolve a type reference to its owner feature name, but only when
/// the type lives in a DIFFERENT feature than `consumer`. Returns None
/// when the type is local (no import needed), defined in both consumer
/// and another feature (treat the duplicate as local — happens when
/// authors copy enums between features per WAR-CODEGEN-XFEAT-01), or
/// builtin/unresolvable.
fn owner_feature_for_type(
    qn: &lazuli_ir::QualifiedName,
    module: &lazuli_ir::Module,
    consumer: &lazuli_ir::Feature,
) -> Option<String> {
    let local_hit = consumer
        .enums
        .iter()
        .any(|e| e.name.eq_ignore_ascii_case(&qn.name))
        || consumer
            .records
            .iter()
            .any(|r| r.name.eq_ignore_ascii_case(&qn.name));
    if local_hit {
        return None;
    }
    // Honor the QualifiedName.feature hint if present (preferred owner).
    if let Some(hint) = qn.feature.as_deref() {
        if module.features.iter().any(|f| f.name == hint) {
            return Some(hint.to_owned());
        }
    }
    // Otherwise, find the first feature that declares this enum/record.
    for feature in &module.features {
        if feature.name == consumer.name {
            continue;
        }
        let owns = feature
            .enums
            .iter()
            .any(|e| e.name.eq_ignore_ascii_case(&qn.name))
            || feature
                .records
                .iter()
                .any(|r| r.name.eq_ignore_ascii_case(&qn.name));
        if owns {
            return Some(feature.name.clone());
        }
    }
    None
}

/// B3 — emit `export type <Name> = string;` brand aliases for every
/// plugin-contributed `@semantic.<Name>` referenced by this feature.
/// Per `docs/proposals/semantic-types-plugin-locales.md` §Codegen the
/// TS layer is type-only — no runtime validation — so an opaque alias
/// is the right surface. The Go side keeps the validate dispatch.
///
/// Sorted output keeps generated TS byte-stable across runs.
fn write_plugin_semantic_aliases(s: &mut String, feature: &lazuli_ir::Feature) {
    let mut aliases: BTreeSet<String> = BTreeSet::new();
    collect_plugin_semantic_aliases_in_feature(feature, &mut aliases);
    if aliases.is_empty() {
        return;
    }
    writeln!(
        s,
        "// Plugin-contributed semantic types (docs/proposals/semantic-types-plugin-locales.md)."
    )
    .ok();
    for name in aliases {
        // Carrier is `Text` in v1 → `string`. The proposal closed
        // carrier catalog locks to `String`; widening needs a separate
        // proposal that also threads a non-string TS shape.
        writeln!(s, "export type {} = string;", pascal_case(&name)).ok();
    }
    writeln!(s).ok();
}

fn collect_plugin_semantic_aliases_in_feature(
    feature: &lazuli_ir::Feature,
    out: &mut BTreeSet<String>,
) {
    for resource in &feature.resources {
        for field in &resource.fields {
            collect_plugin_semantic_aliases_in_type(&field.type_ref, out);
        }
    }
    for record in &feature.records {
        for field in &record.fields {
            collect_plugin_semantic_aliases_in_type(&field.type_ref, out);
        }
    }
    for event in &feature.events {
        for field in &event.payload {
            collect_plugin_semantic_aliases_in_type(&field.type_ref, out);
        }
    }
    for command in &feature.commands {
        if let lazuli_ir::CommandInput::Typed(slots) = &command.input {
            for slot in slots {
                collect_plugin_semantic_aliases_in_type(&slot.type_ref, out);
            }
        }
    }
}

fn collect_plugin_semantic_aliases_in_type(
    type_ref: &lazuli_ir::TypeRef,
    out: &mut BTreeSet<String>,
) {
    match type_ref {
        lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::SemanticPluginType { name, .. }) => {
            out.insert(name.clone());
        }
        lazuli_ir::TypeRef::Many(inner) => {
            collect_plugin_semantic_aliases_in_type(inner, out);
        }
        _ => {}
    }
}

fn write_referenced_enum_aliases(
    s: &mut String,
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
) {
    let mut referenced = BTreeSet::new();
    collect_referenced_feature_enums(feature, module, &mut referenced);
    // Closes WAR-CODEGEN-TS-01: also emit enums referenced by OTHER
    // features (via cross-feature import). Without this, the owner
    // feature's .gen.ts wouldn't export the type the consumer imports.
    for other in &module.features {
        if other.name == feature.name {
            continue;
        }
        let mut other_refs = BTreeSet::new();
        collect_referenced_feature_enums(other, module, &mut other_refs);
        for r in other_refs {
            if feature.enums.iter().any(|e| e.name == r) {
                referenced.insert(r);
            }
        }
        // Also walk cross-feature import collection — captures enums
        // used in command inputs/outputs that the simple "referenced"
        // walk may have missed for the consumer side.
        let mut cross = std::collections::BTreeMap::new();
        collect_cross_feature_refs(other, module, &mut cross);
        if let Some(names) = cross.get(&feature.name) {
            for n in names {
                if feature.enums.iter().any(|e| e.name == *n) {
                    referenced.insert(n.clone());
                }
            }
        }
    }

    let mut emitted = false;
    let mut enums: Vec<&lazuli_ir::EnumDecl> = feature.enums.iter().collect();
    enums.sort_by(|a, b| a.name.cmp(&b.name));
    for enum_decl in enums {
        if !referenced.contains(&enum_decl.name) {
            continue;
        }
        let type_name = pascal_case(&enum_decl.name);
        let const_name = enum_value_constant_name(&enum_decl.name);
        let options_name = enum_option_constant_name(&enum_decl.name);
        let values = enum_decl
            .variants
            .iter()
            .map(enum_variant_ts_literal)
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(s, "export const {const_name} = [{values}] as const;").ok();
        writeln!(s, "export type {type_name} = typeof {const_name}[number];").ok();
        if enum_has_option_metadata(enum_decl) {
            write_enum_options_alias(s, enum_decl, &type_name, &options_name);
        }
        emitted = true;
    }
    if emitted {
        writeln!(s).ok();
    }
}

fn collect_referenced_feature_enums(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    out: &mut BTreeSet<String>,
) {
    for record in &feature.records {
        for field in &record.fields {
            collect_enum_ref(&field.type_ref, feature, out);
        }
    }
    for resource in &feature.resources {
        for field in &resource.fields {
            collect_enum_ref(&field.type_ref, feature, out);
        }
    }
    for command in &feature.commands {
        for slot in command_sdk_slots(feature, command, module) {
            collect_enum_ref(&slot.type_ref, feature, out);
        }
        if let lazuli_ir::CommandEffect::Returns(effect) = &command.effect {
            collect_enum_ref(&effect.return_type, feature, out);
        }
    }
    for query in &feature.queries {
        for slot in query_args(feature, query, module) {
            collect_enum_ref(&slot.type_ref, feature, out);
        }
    }
}

fn collect_enum_ref(
    type_ref: &lazuli_ir::TypeRef,
    feature: &lazuli_ir::Feature,
    out: &mut BTreeSet<String>,
) {
    match type_ref {
        lazuli_ir::TypeRef::EnumRef(name) if enum_ref_matches_feature(feature, name) => {
            out.insert(name.name.clone());
        }
        // UserDefined-tagged enum fields. Parallel to the
        // `UserDefined → enum_decl` fallback in `ts_type_for_type_ref`:
        // when the analyzer leaves an enum reference as
        // `UserDefined("CustomerTier")` (default-bearing fields seem
        // to take this path), the emitter resolves it to a real enum
        // — but the alias only lands at the top of the file if we ALSO
        // record the reference here. Without this branch the generated
        // TS references an undeclared `CustomerTier` symbol.
        lazuli_ir::TypeRef::UserDefined(name) if enum_ref_matches_feature(feature, name) => {
            out.insert(name.name.clone());
        }
        lazuli_ir::TypeRef::Many(inner) => collect_enum_ref(inner, feature, out),
        // Bare-name `Unresolved` fallback for the same reason — kept
        // narrow so we never invent a reference: the bare name MUST
        // already exist in the same feature's enum catalog.
        lazuli_ir::TypeRef::Unresolved(raw) if !raw.starts_with('@') => {
            if feature
                .enums
                .iter()
                .any(|enum_decl| enum_decl.name.eq_ignore_ascii_case(raw))
            {
                out.insert(raw.clone());
            }
        }
        _ => {}
    }
}

fn enum_ref_matches_feature(feature: &lazuli_ir::Feature, name: &lazuli_ir::QualifiedName) -> bool {
    name.feature
        .as_ref()
        .is_none_or(|owner| owner == &feature.name)
        && feature
            .enums
            .iter()
            .any(|enum_decl| enum_decl.name.eq_ignore_ascii_case(&name.name))
}

fn write_record_interface(s: &mut String, record: &lazuli_ir::Record, module: &lazuli_ir::Module) {
    // Field keys in camelCase — idiomatic JS/TS. The wire JSON
    // contract stays snake_case (Go runtime); `LazuliClient` re-keys
    // at the boundary via `runtime/ts/lazuli/src/case-mapper.ts`.
    writeln!(s, "export interface {} {{", pascal_case(&record.name)).ok();
    let mut fields: Vec<&lazuli_ir::Field> = record.fields.iter().collect();
    fields.sort_by(|a, b| a.name.cmp(&b.name));
    for field in fields {
        let ty = ts_type_for_type_ref(&field.type_ref, module);
        let camel = lazuli_codegen_ts::lower_camel_export(&field.name);
        if field.required {
            writeln!(s, "  {}: {};", camel, ty).ok();
        } else {
            writeln!(s, "  {}?: {} | null;", camel, ty).ok();
        }
    }
    writeln!(s, "}}").ok();
    writeln!(s).ok();
}

fn write_resource_interface(
    s: &mut String,
    resource: &lazuli_ir::Resource,
    module: &lazuli_ir::Module,
) {
    writeln!(s, "export interface {} {{", pascal_case(&resource.name)).ok();
    writeln!(s, "  id: ID;").ok();
    let mut fields: Vec<&lazuli_ir::Field> = resource.fields.iter().collect();
    fields.sort_by(|a, b| a.name.cmp(&b.name));
    for field in fields {
        if matches!(
            field.name.as_str(),
            "id" | "created_at" | "updated_at" | "deleted_at"
        ) {
            continue;
        }
        let name = resource_field_ts_name(field, module);
        let camel = lazuli_codegen_ts::lower_camel_export(&name);
        let ty = resource_field_ts_type(field, module);
        if field.required {
            writeln!(s, "  {camel}: {ty};").ok();
        } else {
            writeln!(s, "  {camel}?: {ty} | null;").ok();
        }
    }
    writeln!(s, "  createdAt: Time;").ok();
    writeln!(s, "  updatedAt: Time;").ok();
    if resource.soft_delete {
        writeln!(s, "  deletedAt?: Time | null;").ok();
    }
    writeln!(s, "}}").ok();
    writeln!(s).ok();
}

fn write_command_sdk(
    s: &mut String,
    feature: &lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) {
    let feature_pascal = pascal_case(&feature.name);
    let input_iface = command_input_iface(&command.name, &feature_pascal);
    let output_ty = command_output_ts_type(feature, command, module);
    let command_export = command_export_ident(feature, command, module);
    let legacy_command_ident = command_ident(&feature.name, &command.name);

    writeln!(s, "export interface {input_iface} {{").ok();
    for slot in command_sdk_slots(feature, command, module) {
        let optional = if slot.required { "" } else { "?" };
        let camel = lazuli_codegen_ts::lower_camel_export(&slot.name);
        writeln!(
            s,
            "  {}{}: {};",
            camel,
            optional,
            ts_type_for_type_ref(&slot.type_ref, module)
        )
        .ok();
    }
    writeln!(s, "}}").ok();
    writeln!(s).ok();

    let invalidates: Vec<String> = command
        .invalidates
        .iter()
        .map(|i| {
            // Wire registry key: `<feature>.<query_name>` (cell B1 dropped
            // `.query.` infix). The pseudo-feature `query` (legacy parser
            // output for `query.<name>` same-feature shorthand) and the
            // None fallback both resolve to the host feature.
            let feature_name = match i.query.feature.as_deref() {
                Some("query") | None => feature.name.as_str(),
                Some(feat) => feat,
            };
            format!("{}.{}", feature_name, i.query.name)
        })
        .collect();
    // Wave 0 (ir-returns-list-2026-05-22 §2.2): pure-read commands lower
    // to `defineQuery` so the React app gets react-query semantics
    // (cache, refetch, suspense, useLazuliQuery). The wire is identical;
    // only the client-side factory differs. Non-read commands stay on
    // `defineCommand` and keep carrying invalidates / policy / rate-limit
    // / audit metadata for `useLazuliCommand` callers.
    if command_is_pure_read(command) {
        writeln!(
            s,
            "export const {} = defineQuery<{}, {}>(\"{}.{}\");",
            command_export,
            input_iface,
            output_ty,
            feature.name,
            command.name
        )
        .ok();
        writeln!(s).ok();
        if legacy_command_ident != command_export {
            write_deprecated_const_alias(s, &legacy_command_ident, &command_export);
        }
        return;
    }
    writeln!(
        s,
        "export const {} = defineCommand<{}, {}>(\"{}.{}\", {{",
        command_export,
        input_iface,
        output_ty,
        feature.name,
        command.name
    )
    .ok();
    writeln!(s, "  invalidates: {},", format_string_array(&invalidates)).ok();
    // Operational metadata (review bug #7, 2026-05-15) — the Go side
    // already carries Policy / RateLimit / Audit on `lazuli.Command[I,O]`.
    // The TS SDK previously lost them, so clients had no way to drive
    // policy-aware affordances or rate-limit-aware backoff without a
    // separate metadata call.
    if let Some(policy_literal) = format_policy_ts(&command.policy, feature) {
        writeln!(s, "  policy: {policy_literal},").ok();
    }
    if let Some(rate_limit) = command.rate_limit.as_ref() {
        // `ir-rate-limit-env-aware` cell 1 — SDK shim: surface the
        // default literal. Cell 2 extends the wire shape to carry the
        // env-qualified slice for client-side affordance.
        writeln!(
            s,
            "  rateLimit: \"{}\",",
            escape_js_string(&rate_limit.default)
        )
        .ok();
    }
    if let Some(audit_literal) = format_audit_ts(command.audit.as_ref()) {
        writeln!(s, "  audit: {audit_literal},").ok();
    }
    writeln!(s, "}});").ok();
    writeln!(s).ok();
    if legacy_command_ident != command_export {
        write_deprecated_const_alias(s, &legacy_command_ident, &command_export);
    }
}

fn write_query_sdk(
    s: &mut String,
    feature: &lazuli_ir::Feature,
    query: &lazuli_ir::Query,
    module: &lazuli_ir::Module,
) {
    let args = query_args(feature, query, module);
    let args_ty = if args.is_empty() {
        "{}".to_owned()
    } else {
        let fields = args
            .iter()
            .map(|slot| {
                let optional = if slot.required { "" } else { "?" };
                format!(
                    "{}{}: {}",
                    slot.name,
                    optional,
                    ts_type_for_type_ref(&slot.type_ref, module)
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        format!("{{ {fields} }}")
    };
    // Pick the resource most likely matching the query's intent.
    // Previous heuristic was `feature.resources.first()` which produced
    // wildly wrong types when the first resource isn't the "main" one
    // (e.g. `host.lookupHostByMyHost` typed as `IntermediationTermsAcceptance`
    // because that's the first resource declared in `host.lzi`; see
    // WAR-VOCAB-HOSTHOME-01). New heuristic: find a resource whose
    // PascalCase name appears as a token in the query name, falling
    // back to the first resource when no match is found.
    let resource_ty = pick_query_resource_ts(feature, query.name()).unwrap_or_else(|| {
        feature
            .resources
            .first()
            .map(|r| pascal_case(&r.name))
            .unwrap_or_else(|| "unknown".to_owned())
    });
    let resource_pascal = resource_ty.clone();
    let returns = match query {
        lazuli_ir::Query::Lookup(_) => resource_ty,
        lazuli_ir::Query::List(_) => format!("{resource_ty}[]"),
        lazuli_ir::Query::Sql(q) => ts_type_for_type_ref(&q.returns, module),
    };
    let query_ref_kind = match query {
        lazuli_ir::Query::List(_) => lazuli_ir::QueryKind::List,
        lazuli_ir::Query::Lookup(_) => lazuli_ir::QueryKind::Lookup,
        lazuli_ir::Query::Sql(q) => match q.sql_kind {
            lazuli_ir::SqlQueryKind::Sql => lazuli_ir::QueryKind::Sql,
            lazuli_ir::SqlQueryKind::View => lazuli_ir::QueryKind::View,
        },
    };
    // Query-side operational metadata (review bug #7, 2026-05-15).
    // Today `lazuli_ir::Query` carries no explicit policy/rate_limit at
    // the variant level — `query.list/lookup/sql` are universally
    // readable inside a tenant (see audience_sdk.rs's note). The TS
    // signature already accepts a `DefineQueryOptions` block so when
    // policy lands on Query the codegen will populate it here without
    // a runtime contract change.
    writeln!(
        s,
        // Wire registry key: `<feature>.<query_name>` (cell B1 dropped
        // `.query.` infix — the `/q/` HTTP prefix already disambiguates kind).
        "export const {} = defineQuery<{}, {}>(\"{}.{}\");",
        query_ident(&feature.name, &resource_pascal, query_ref_kind, query.name()),
        args_ty,
        returns,
        feature.name,
        query.name()
    )
    .ok();
    writeln!(s).ok();
    let legacy_ident = legacy_query_ident(&feature.name, query_ref_kind, query.name());
    let current_ident = query_ident(&feature.name, &resource_pascal, query_ref_kind, query.name());
    if legacy_ident != current_ident {
        write_deprecated_const_alias(s, &legacy_ident, &current_ident);
    }
}

pub(crate) fn emit_feature_zod_ts(feature: &lazuli_ir::Feature, module: &lazuli_ir::Module) -> String {
    let mut s = String::new();
    writeln!(s, "// Code generated by lazuli; DO NOT EDIT.").ok();
    writeln!(s, "import {{ z }} from \"zod\";").ok();
    writeln!(s).ok();

    let feature_pascal = pascal_case(&feature.name);
    let mut commands: Vec<&lazuli_ir::Command> = feature.commands.iter().collect();
    commands.sort_by(|a, b| a.name.cmp(&b.name));
    for command in commands {
        let schema_ident = command_schema_ident(&command.name, &feature_pascal);
        writeln!(s, "export const {schema_ident} = z.object({{").ok();
        for slot in command_zod_slots(feature, command, module) {
            // Zod schemas mirror the camelCase SDK interfaces emitted
            // in `messaging.gen.ts` etc. The wire JSON contract stays
            // snake_case; `LazuliClient` rekeys at the boundary
            // (`case-mapper.ts`). Apps validating client-side state
            // (forms, local cache) speak in camelCase, matching
            // the typed interface.
            writeln!(
                s,
                "  {}: {},",
                lazuli_codegen_ts::lower_camel_export(&slot.name),
                zod_expr_for_slot(&slot.type_ref, &slot.constraints, !slot.required, module)
            )
            .ok();
        }
        writeln!(s, "}});").ok();
        writeln!(s).ok();
    }

    s
}

#[derive(Clone)]
pub(crate) struct TsSlot {
    pub(crate) name: String,
    pub(crate) type_ref: lazuli_ir::TypeRef,
    pub(crate) required: bool,
    pub(crate) constraints: lazuli_ir::FieldConstraints,
}

pub(super) fn command_sdk_slots(
    feature: &lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) -> Vec<TsSlot> {
    let mut slots = Vec::new();
    for route in &command.route {
        slots.push(TsSlot {
            name: route.name.clone(),
            type_ref: route.type_ref.clone(),
            required: route.from.is_none(),
            constraints: lazuli_ir::FieldConstraints::default(),
        });
    }
    slots.extend(command_input_slots(feature, command, module));
    slots
}

pub(crate) fn command_zod_slots(
    feature: &lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) -> Vec<TsSlot> {
    let input_slots = command_input_slots(feature, command, module);
    if input_slots.is_empty() {
        command
            .route
            .iter()
            .map(|route| TsSlot {
                name: route.name.clone(),
                type_ref: route.type_ref.clone(),
                required: route.from.is_none(),
                constraints: lazuli_ir::FieldConstraints::default(),
            })
            .collect()
    } else {
        input_slots
    }
}

fn command_input_slots(
    feature: &lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) -> Vec<TsSlot> {
    match &command.input {
        lazuli_ir::CommandInput::Empty => Vec::new(),
        lazuli_ir::CommandInput::Typed(slots) => slots
            .iter()
            .map(|slot| TsSlot {
                name: slot.name.clone(),
                type_ref: slot.type_ref.clone(),
                required: slot.required,
                constraints: slot.constraints.clone(),
            })
            .collect(),
        lazuli_ir::CommandInput::Short(names) => {
            let resource = command_resource(feature, command, module);
            names
                .iter()
                .map(|name| {
                    let field = resource.and_then(|r| r.fields.iter().find(|f| f.name == *name));
                    TsSlot {
                        name: name.clone(),
                        type_ref: field
                            .map(|f| f.type_ref.clone())
                            .unwrap_or(lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Text)),
                        required: field.map(|f| f.required).unwrap_or(true),
                        constraints: field.map(|f| f.constraints.clone()).unwrap_or_default(),
                    }
                })
                .collect()
        }
    }
}

pub(super) fn query_args(
    feature: &lazuli_ir::Feature,
    query: &lazuli_ir::Query,
    module: &lazuli_ir::Module,
) -> Vec<TsSlot> {
    match query {
        lazuli_ir::Query::List(q) => q.params.iter().map(ts_slot_from_typed).collect(),
        lazuli_ir::Query::Sql(q) => q.params.iter().map(ts_slot_from_typed).collect(),
        lazuli_ir::Query::Lookup(q) => {
            let mut slots: Vec<TsSlot> = q.params.iter().map(ts_slot_from_typed).collect();
            if slots.is_empty() {
                for key in &q.keys {
                    if let lazuli_ir::Expr::Path(path) = &key.equals {
                        if path.segments.first().is_some_and(|s| s == "input") {
                            if let Some(name) = path.segments.get(1) {
                                slots.push(query_input_slot(feature, module, name));
                            }
                        }
                    }
                }
            }
            if slots.is_empty() {
                collect_input_slots_from_filters(feature, module, &q.filters, &mut slots);
            }
            if slots.is_empty() {
                if let Some(name) = q.name.strip_prefix("by_") {
                    slots.push(query_input_slot(feature, module, name));
                }
            }
            slots
        }
    }
}

fn collect_input_slots_from_filters(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    filters: &[lazuli_ir::Filter],
    slots: &mut Vec<TsSlot>,
) {
    for filter in filters {
        collect_input_slots_from_predicate(feature, module, &filter.predicate, slots);
    }
}

fn collect_input_slots_from_predicate(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    predicate: &lazuli_ir::Predicate,
    slots: &mut Vec<TsSlot>,
) {
    match predicate {
        lazuli_ir::Predicate::Comparison { left, right, .. } => {
            collect_input_slot_from_expr(feature, module, left, slots);
            collect_input_slot_from_expr(feature, module, right, slots);
        }
        lazuli_ir::Predicate::Has {
            collection,
            element,
        } => {
            collect_input_slot_from_expr(feature, module, collection, slots);
            collect_input_slot_from_expr(feature, module, element, slots);
        }
        lazuli_ir::Predicate::And(predicates) | lazuli_ir::Predicate::Or(predicates) => {
            for predicate in predicates {
                collect_input_slots_from_predicate(feature, module, predicate, slots);
            }
        }
    }
}

fn collect_input_slot_from_expr(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    expr: &lazuli_ir::Expr,
    slots: &mut Vec<TsSlot>,
) {
    let lazuli_ir::Expr::Path(path) = expr else {
        return;
    };
    if !path
        .segments
        .first()
        .is_some_and(|segment| segment == "input")
    {
        return;
    }
    let Some(name) = path.segments.get(1) else {
        return;
    };
    if slots.iter().any(|slot| slot.name == *name) {
        return;
    }
    slots.push(query_input_slot(feature, module, name));
}

fn query_input_slot(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    name: &str,
) -> TsSlot {
    let field = feature
        .resources
        .first()
        .and_then(|resource| resource.fields.iter().find(|field| field.name == name))
        .or_else(|| {
            module
                .features
                .iter()
                .flat_map(|feature| feature.resources.iter())
                .flat_map(|resource| resource.fields.iter())
                .find(|field| field.name == name)
        });
    TsSlot {
        name: name.to_owned(),
        type_ref: field
            .map(|field| field.type_ref.clone())
            .or_else(|| {
                name.eq_ignore_ascii_case("id")
                    .then_some(lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Id))
            })
            .unwrap_or(lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Text)),
        required: true,
        constraints: field
            .map(|field| field.constraints.clone())
            .unwrap_or_default(),
    }
}

fn ts_slot_from_typed(slot: &lazuli_ir::TypedSlot) -> TsSlot {
    TsSlot {
        name: slot.name.clone(),
        type_ref: slot.type_ref.clone(),
        required: slot.required,
        constraints: slot.constraints.clone(),
    }
}

/// Pick the most likely resource for a `query.list` / `query.lookup` /
/// `query.sql` return type. Walks the feature's resources, returns the
/// one whose snake-cased name appears as a substring of the query
/// name (e.g. `my_host` → "host" → Host; `property_detail` → "property"
/// → Property). Returns None when no resource matches; caller falls
/// back to `feature.resources.first()`. Closes WAR-VOCAB-HOSTHOME-01.
///
/// Wave §A2 (mine_query disambiguation, 2026-05-23): now matches the
/// plural form of each resource's snake name as well so
/// `mine_properties` → "property" + "properties" → Property. Without
/// this, `mine_properties` fell through to `feature.resources.first()`
/// which in `catalog.lzi` happens to be `UploadedAsset` — emitting
/// the wrong TS return type. Hostpoint workaround was an explicit
/// `as unknown as Property[]` cast in HostHome.tsx.
pub(super) fn pick_query_resource_ts(feature: &lazuli_ir::Feature, query_name: &str) -> Option<String> {
    let query_lc = query_name.to_ascii_lowercase();
    // Prefer the longest match (so "service_transaction" beats
    // "service" + "transaction" tie). Sort by length desc.
    let mut candidates: Vec<&lazuli_ir::Resource> = feature.resources.iter().collect();
    candidates.sort_by(|a, b| b.name.len().cmp(&a.name.len()));
    for resource in candidates {
        let snake = to_snake_case(&resource.name);
        if query_lc.contains(&snake) {
            return Some(pascal_case(&resource.name));
        }
        // Plural-aware match: a `query.list mine_properties` should
        // bind to the `Property` resource even though the snake form
        // is the singular `property`.
        let snake_plural = pluralize_snake(&snake);
        if !snake_plural.is_empty() && query_lc.contains(&snake_plural) {
            return Some(pascal_case(&resource.name));
        }
        // Also try a token-by-token match for compound names like
        // "ServiceTransaction" vs query "transaction_detail".
        let last_token = snake.rsplit('_').next().unwrap_or("");
        if !last_token.is_empty() && last_token.len() > 3 && query_lc.contains(last_token) {
            return Some(pascal_case(&resource.name));
        }
        // Plural-aware last-token match — same fix one level deeper.
        let last_token_plural = pluralize_snake(last_token);
        if !last_token_plural.is_empty()
            && last_token_plural.len() > 4
            && query_lc.contains(&last_token_plural)
        {
            return Some(pascal_case(&resource.name));
        }
    }
    None
}

/// Cheap English-only pluralizer for snake-case identifiers. Handles
/// the three patterns that actually appear in pilot vocabularies:
///   - ends in `y` preceded by consonant → drop `y`, append `ies`
///     (property → properties, story → stories).
///   - ends in `s`/`x`/`z` or `ch`/`sh`     → append `es`
///     (process → processes, box → boxes).
///   - otherwise                          → append `s` (host → hosts).
///
/// Returns empty when the input is empty. Not a general-purpose
/// pluralizer — does not handle irregular forms (man → men, child →
/// children). Pilots whose vocab uses those should declare an explicit
/// `returns <Resource>` on the query rather than rely on this
/// heuristic.
fn pluralize_snake(word: &str) -> String {
    if word.is_empty() {
        return String::new();
    }
    let len = word.len();
    let last = word.as_bytes()[len - 1];
    if last == b'y' && len >= 2 {
        let prev = word.as_bytes()[len - 2];
        let is_consonant = !matches!(prev, b'a' | b'e' | b'i' | b'o' | b'u');
        if is_consonant {
            let mut out = word[..len - 1].to_string();
            out.push_str("ies");
            return out;
        }
    }
    if word.ends_with('s')
        || word.ends_with('x')
        || word.ends_with('z')
        || word.ends_with("ch")
        || word.ends_with("sh")
    {
        return format!("{word}es");
    }
    format!("{word}s")
}

/// Wave 0 (ir-returns-list-2026-05-22 §2.2): a command is a *pure read*
/// when its sole declared effect is `Returns(_)`, carries no declared
/// side-effects (no event emits, no lifecycle triggers, no invalidations,
/// no external calls), is NOT synthesized from `@cap.File` (those are
/// upload-protocol commands with implicit side effects the analyzer
/// doesn't surface as `emits`/`triggers`), AND its name starts with a
/// read-verb prefix (`list_`, `get_`, `lookup_`, `search_`, `find_`,
/// `count_`).
///
/// Pure-read commands lower to `defineQuery<I, O>` on the TS side
/// (consumable via `useLazuliQuery`) so the React app gets cache +
/// refetch + suspense semantics for free, instead of `defineCommand`
/// (which forces `useLazuliCommand` and imperative call sites). The
/// wire payload is identical — only the client-side factory differs.
///
/// The name-prefix gate exists because pilots and the analyzer leave
/// the IR side-effect surface empty for many side-effecting commands —
/// e.g. `account.login` (mints a session but has no `emits` because the
/// session table is private), `request_profile_photo_upload` (mints a
/// presigned URL but has no `triggers`). Trusting only the IR's empty
/// side-effect set produced false positives (W0-5 surfaced this:
/// hostpoint app failed to typecheck because login + photo-upload
/// commands shipped as `defineQuery`, breaking existing
/// `useLazuliCommand` callsites). The name-prefix gate makes the
/// classification conservative — false negatives (a read that doesn't
/// follow the naming convention) ship as `defineCommand`, which still
/// works; false positives ship a wire mismatch, which doesn't.
pub(super) fn command_is_pure_read(command: &lazuli_ir::Command) -> bool {
    if !matches!(command.effect, lazuli_ir::CommandEffect::Returns(_)) {
        return false;
    }
    if !command.emits.is_empty()
        || !command.triggers.is_empty()
        || !command.invalidates.is_empty()
        || !command.external_calls.is_empty()
    {
        return false;
    }
    // cap_file synth: Request/Confirm/Clear are upload-protocol writes;
    // only GetUrl is a pure read (mints a signed download URL, no
    // mutation). c-2 worker surfaced this nuance; integrated 2026-05-22.
    if command
        .synthesized_from_cap_file
        .as_ref()
        .is_some_and(|marker| marker.role != lazuli_ir::AutoPhotoCommandRole::GetUrl)
    {
        return false;
    }
    const READ_VERB_PREFIXES: &[&str] = &[
        "list_", "get_", "lookup_", "search_", "find_", "count_",
    ];
    READ_VERB_PREFIXES
        .iter()
        .any(|prefix| command.name.starts_with(prefix))
}

fn command_output_ts_type(
    _feature: &lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) -> String {
    match &command.effect {
        lazuli_ir::CommandEffect::Creates(effect) => resource_ts_name(&effect.resource, module),
        lazuli_ir::CommandEffect::Updates(effect) => resource_ts_name(&effect.resource, module),
        lazuli_ir::CommandEffect::Deletes(effect) => resource_ts_name(&effect.resource, module),
        // For `returns User` we want the full resource interface (User)
        // not the FK collapse to `ID`. `ts_type_for_type_ref` collapses
        // any `UserDefined(<Resource>)` to `ID` because that's correct
        // for resource-field positions (FK column). But the return
        // position carries the typed row — same fix as the Go side
        // (`types::go_return_type_for`).
        lazuli_ir::CommandEffect::Returns(effect) => {
            ts_return_type_for_type_ref(&effect.return_type, module)
        }
        // CommandEffect::None means the command has an `@fn.*` handler
        // with no declared return effect — the Go side returns `struct{}`
        // (empty object). TS surface mirrors that as `void`. Previously
        // this fell back to `feature.resources.first()`, which produced
        // wildly wrong types (e.g. every catalog command typed as
        // `UploadedAsset` — see WAR-VOCAB-HOSTPROPDETAIL-02).
        lazuli_ir::CommandEffect::None => "void".to_owned(),
    }
}

/// Variant of [`ts_type_for_type_ref`] that resolves resource refs to
/// their full interface name (`User`) instead of the FK collapse (`ID`).
/// Used by [`command_output_ts_type`] for `Returns` — the handler emits
/// the typed row, not the row id. Mirrors the Go side's
/// `go_return_type_for` / `command_output_type` split (see
/// `crates/lazuli_codegen_go/src/emitter/types.rs`).
fn ts_return_type_for_type_ref(
    type_ref: &lazuli_ir::TypeRef,
    module: &lazuli_ir::Module,
) -> String {
    match type_ref {
        lazuli_ir::TypeRef::UserDefined(name) if is_resource_ref(type_ref, module) => {
            // Skip the FK collapse — return the resource interface name.
            find_resource(module, name)
                .map(|r| pascal_case(&r.name))
                .unwrap_or_else(|| pascal_case(&name.name))
        }
        lazuli_ir::TypeRef::Many(inner) => {
            format!("{}[]", ts_return_type_for_type_ref(inner, module))
        }
        // Everything else (builtins, capabilities, enums, records,
        // unresolved) shares the same shape as field-position resolution.
        other => ts_type_for_type_ref(other, module),
    }
}

fn command_resource<'a>(
    feature: &'a lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &'a lazuli_ir::Module,
) -> Option<&'a lazuli_ir::Resource> {
    match &command.effect {
        lazuli_ir::CommandEffect::Creates(effect) => find_resource(module, &effect.resource),
        lazuli_ir::CommandEffect::Updates(effect) => find_resource(module, &effect.resource),
        lazuli_ir::CommandEffect::Deletes(effect) => find_resource(module, &effect.resource),
        lazuli_ir::CommandEffect::Returns(_) | lazuli_ir::CommandEffect::None => {
            feature.resources.first()
        }
    }
}

fn find_resource<'a>(
    module: &'a lazuli_ir::Module,
    name: &lazuli_ir::QualifiedName,
) -> Option<&'a lazuli_ir::Resource> {
    module
        .features
        .iter()
        .filter(|feature| name.feature.as_ref().is_none_or(|n| n == &feature.name))
        .flat_map(|feature| feature.resources.iter())
        .find(|resource| resource.name.eq_ignore_ascii_case(&name.name))
}

fn resource_ts_name(name: &lazuli_ir::QualifiedName, module: &lazuli_ir::Module) -> String {
    find_resource(module, name)
        .map(|r| pascal_case(&r.name))
        .unwrap_or_else(|| pascal_case(&name.name))
}

fn resource_field_ts_name(field: &lazuli_ir::Field, module: &lazuli_ir::Module) -> String {
    if is_resource_ref(&field.type_ref, module) && !field.name.ends_with("_id") {
        format!("{}_id", field.name)
    } else {
        field.name.clone()
    }
}

fn resource_field_ts_type(field: &lazuli_ir::Field, module: &lazuli_ir::Module) -> String {
    if is_resource_ref(&field.type_ref, module) {
        "ID".to_owned()
    } else {
        ts_type_for_type_ref(&field.type_ref, module)
    }
}

fn is_resource_ref(type_ref: &lazuli_ir::TypeRef, module: &lazuli_ir::Module) -> bool {
    match type_ref {
        lazuli_ir::TypeRef::UserDefined(name) => module
            .features
            .iter()
            .flat_map(|feature| feature.resources.iter())
            .any(|resource| resource.name.eq_ignore_ascii_case(&name.name)),
        _ => false,
    }
}

fn ts_type_for_type_ref(type_ref: &lazuli_ir::TypeRef, module: &lazuli_ir::Module) -> String {
    match type_ref {
        lazuli_ir::TypeRef::Builtin(builtin) => match builtin {
            lazuli_ir::BuiltinType::Id => "ID".to_owned(),
            lazuli_ir::BuiltinType::Text
            | lazuli_ir::BuiltinType::SemanticEmail
            | lazuli_ir::BuiltinType::SemanticPhone
            | lazuli_ir::BuiltinType::SemanticUrl
            | lazuli_ir::BuiltinType::SemanticUuid
            | lazuli_ir::BuiltinType::SemanticCurrency
            | lazuli_ir::BuiltinType::CapSecret => "string".to_owned(),
            // B3 — plugin-contributed `@semantic.<Name>` projects to
            // the brand alias name (e.g. `BrazilianCPF`). The SDK
            // emitter (`emit_feature_sdk_ts`) writes the
            // `export type <Name> = string;` line at file head so
            // every consuming interface picks up the alias.
            lazuli_ir::BuiltinType::SemanticPluginType { name, .. } => pascal_case(name),
            lazuli_ir::BuiltinType::Boolean => "boolean".to_owned(),
            lazuli_ir::BuiltinType::Integer
            | lazuli_ir::BuiltinType::Decimal => "number".to_owned(),
            // Per `semantic-types-money-brazilian.md` v0.3 — Money is
            // the rich struct on the TS side too. `Money` interface
            // lives in `@lazuli/runtime`; downstream consumers get the
            // shape via the typed import.
            lazuli_ir::BuiltinType::SemanticMoney { .. } => "Money".to_owned(),
            lazuli_ir::BuiltinType::Date | lazuli_ir::BuiltinType::DateTime => "Time".to_owned(),
            lazuli_ir::BuiltinType::Json
            | lazuli_ir::BuiltinType::SemanticGeoPoint
            | lazuli_ir::BuiltinType::CapFile => "unknown".to_owned(),
        },
        lazuli_ir::TypeRef::Capability(capability) => match capability {
            lazuli_ir::CapabilityRef::Hashed(_)
            | lazuli_ir::CapabilityRef::Encrypted(_)
            | lazuli_ir::CapabilityRef::E2ee(_)
            | lazuli_ir::CapabilityRef::Token(_)
            | lazuli_ir::CapabilityRef::PII(_) => "string".to_owned(),
            lazuli_ir::CapabilityRef::File(_) => "unknown".to_owned(),
        },
        lazuli_ir::TypeRef::Many(inner) => format!("{}[]", ts_type_for_type_ref(inner, module)),
        lazuli_ir::TypeRef::EnumRef(name) => find_enum_decl(module, name)
            .map(|enum_decl| pascal_case(&enum_decl.name))
            .unwrap_or_else(|| "unknown".to_owned()),
        lazuli_ir::TypeRef::UserDefined(name) => {
            if is_resource_ref(type_ref, module) {
                "ID".to_owned()
            } else if let Some(enum_decl) = find_enum_decl(module, name) {
                // Enum referenced via UserDefined path. The parser
                // sometimes tags an enum field as UserDefined when the
                // analyzer hasn't promoted it to EnumRef (review bug #3,
                // 2026-05-15: `tier: CustomerTier = free` and
                // `source: CustomerSource = manual` both flowed as
                // UserDefined-with-no-record-match and lowered to
                // `unknown` — even though `CustomerTier`/`CustomerSource`
                // are declared above the resource block).
                pascal_case(&enum_decl.name)
            } else {
                module
                    .features
                    .iter()
                    .flat_map(|feature| feature.records.iter())
                    .find(|record| record.name.eq_ignore_ascii_case(&name.name))
                    .map(|record| pascal_case(&record.name))
                    .unwrap_or_else(|| "unknown".to_owned())
            }
        }
        lazuli_ir::TypeRef::Unresolved(raw) => {
            if raw.starts_with("@cap.Hashed")
                || raw.starts_with("@cap.Encrypted")
                || raw.starts_with("@cap.Token")
                || raw == "@semantic.Email"
            {
                return "string".to_owned();
            }
            // Bare PascalCase fallback: the analyzer occasionally leaves
            // a `Unresolved("Foo")` even when `Foo` is a declared enum /
            // record / resource somewhere in the module (review bug #3,
            // 2026-05-15: `tier: CustomerTier = manual` flowed as
            // `Unresolved("CustomerTier")` and lowered to `unknown`
            // even though `CustomerTier` is declared three lines above).
            // Recover by walking the module's catalogs here so the TS
            // SDK preserves typing instead of falling to opaque
            // `unknown` whenever the analyzer's resolve pass misses an
            // edge case.
            if !raw.starts_with('@') {
                let synthetic = lazuli_ir::QualifiedName {
                    feature: None,
                    name: raw.clone(),
                };
                if let Some(enum_decl) = find_enum_decl(module, &synthetic) {
                    return pascal_case(&enum_decl.name);
                }
                if let Some(record) = module
                    .features
                    .iter()
                    .flat_map(|feature| feature.records.iter())
                    .find(|record| record.name.eq_ignore_ascii_case(raw))
                {
                    return pascal_case(&record.name);
                }
                if module
                    .features
                    .iter()
                    .flat_map(|feature| feature.resources.iter())
                    .any(|resource| resource.name.eq_ignore_ascii_case(raw))
                {
                    return "ID".to_owned();
                }
            }
            "unknown".to_owned()
        }
    }
}

/// Lower a `PolicyRef` to a TypeScript object literal matching the
/// `PolicySpec` shape exported by `@lazuli/runtime/spec`. Returns `None`
/// when the policy is omitted or explicitly `None` so the caller can
/// elide the `policy: ...` line entirely (review bug #7).
fn format_policy_ts(policy: &lazuli_ir::PolicyRef, feature: &lazuli_ir::Feature) -> Option<String> {
    // Re-prepend `@` when the parser dropped it. PolicyRef::Local
    // carries either the bare category name (`"update"`) or the
    // partial-qualified form (`"policy.update"`); PolicyRef::Atom can
    // arrive with or without the `@` host prefix. Normalize to the
    // DSL-faithful surface (`@policy.update`, `@role.admin`, …) so
    // clients see what they wrote.
    fn ensure_at_prefix(s: &str) -> String {
        if s.starts_with('@') {
            s.to_owned()
        } else {
            format!("@{}", s)
        }
    }
    let (name, atoms): (String, Vec<&str>) = match policy {
        lazuli_ir::PolicyRef::None => return None,
        lazuli_ir::PolicyRef::Local(local) => {
            let qualified = if local.contains('.') {
                ensure_at_prefix(local)
            } else {
                format!("@policy.{}", local)
            };
            let resolved_atoms: Vec<&str> = feature
                .policies
                .categories
                .iter()
                .find(|cat| cat.name == *local)
                .map(|cat| cat.atoms.iter().map(String::as_str).collect())
                .unwrap_or_default();
            (qualified, resolved_atoms)
        }
        lazuli_ir::PolicyRef::Atom(atom) => {
            let qualified = ensure_at_prefix(atom);
            // When the parser stored a `@policy.<name>` reference as
            // an Atom (vs Local), the literal `atom` itself is the
            // POLICY NAME, not an actual `@role.X`/`@scope.X`/`@actor.X`
            // atom. Resolve via the feature's policies dictionary to
            // recover the real atoms; fall back to treating it as a
            // standalone atom only when no category matches.
            let body = atom.trim_start_matches('@');
            let local_name = body.strip_prefix("policy.").unwrap_or("");
            let resolved_atoms: Vec<&str> = if !local_name.is_empty() {
                feature
                    .policies
                    .categories
                    .iter()
                    .find(|cat| cat.name == local_name)
                    .map(|cat| cat.atoms.iter().map(String::as_str).collect())
                    .unwrap_or_default()
            } else {
                vec![atom.as_str()]
            };
            (qualified, resolved_atoms)
        }
        lazuli_ir::PolicyRef::External { feature, name } => {
            (
                format!("{}.{}", feature, ensure_at_prefix(name)),
                Vec::new(),
            )
        }
        lazuli_ir::PolicyRef::Unresolved(raw) => (raw.clone(), Vec::new()),
    };
    let atoms_lit = if atoms.is_empty() {
        "[]".to_owned()
    } else {
        let entries: Vec<String> = atoms
            .iter()
            .filter_map(|atom| parse_policy_atom_ts(atom))
            .collect();
        format!("[{}]", entries.join(", "))
    };
    Some(format!(
        "{{ name: \"{}\", atoms: {} }}",
        escape_js_string(&name),
        atoms_lit
    ))
}

/// Parse a raw policy atom string like `@role.admin` (or `role.admin`
/// when the parser dropped the host prefix) into the TS
/// `{ namespace: "role", name: "admin" }` literal. Returns `None` when
/// the atom does not parse — caller drops it from the literal rather
/// than emitting an invalid spec.
fn parse_policy_atom_ts(raw: &str) -> Option<String> {
    let body = raw.trim_start_matches('@');
    let (namespace, name) = body.split_once('.')?;
    if namespace.is_empty() || name.is_empty() {
        return None;
    }
    Some(format!(
        "{{ namespace: \"{}\", name: \"{}\" }}",
        escape_js_string(namespace),
        escape_js_string(name)
    ))
}

/// Lower an `AuditSpec` to a TypeScript literal matching the
/// `AuditSpec` union exported by `@lazuli/runtime/spec`:
///   - `Some({subjects: [], ..})`        → `"default"` sentinel
///   - `Some({subjects: ["actor", ..]})` → string array literal
///   - `None`                             → caller elides the field
fn format_audit_ts(audit: Option<&lazuli_ir::AuditSpec>) -> Option<String> {
    let audit = audit?;
    if audit.subjects.is_empty() {
        return Some("\"default\"".to_owned());
    }
    let entries: Vec<String> = audit
        .subjects
        .iter()
        .map(|s| format!("\"{}\"", escape_js_string(s)))
        .collect();
    Some(format!("[{}]", entries.join(", ")))
}

/// Escape a string for embedding in a TS double-quoted literal. Conservative:
/// covers `"`, `\`, and control chars that would terminate or break the
/// literal. Newlines collapse to `\n`; nothing else is interpreted.
fn escape_js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", ch as u32));
            }
            _ => out.push(ch),
        }
    }
    out
}

pub(crate) fn find_enum_decl<'a>(
    module: &'a lazuli_ir::Module,
    name: &lazuli_ir::QualifiedName,
) -> Option<&'a lazuli_ir::EnumDecl> {
    module
        .features
        .iter()
        .filter(|feature| {
            name.feature
                .as_ref()
                .is_none_or(|owner| owner == &feature.name)
        })
        .flat_map(|feature| feature.enums.iter())
        .find(|enum_decl| enum_decl.name.eq_ignore_ascii_case(&name.name))
}

fn enum_value_constant_name(type_ref: &str) -> String {
    let local = type_ref
        .rsplit(['.', ':'])
        .find(|part| !part.is_empty())
        .unwrap_or(type_ref);
    let mut out = String::with_capacity(local.len() + "_VALUES".len());
    let mut prev_lower_or_digit = false;

    for ch in local.chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && prev_lower_or_digit && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_uppercase());
            prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else {
            if !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            prev_lower_or_digit = false;
        }
    }

    while out.ends_with('_') {
        out.pop();
    }

    out.push_str("_VALUES");
    out
}

fn enum_option_constant_name(type_ref: &str) -> String {
    let mut out = enum_value_constant_name(type_ref);
    if out.ends_with("_VALUES") {
        let prefix_len = out.len() - "_VALUES".len();
        out.truncate(prefix_len);
        out.push_str("_OPTIONS");
    }
    out
}

fn enum_variant_ts_literal(variant: &lazuli_ir::EnumVariant) -> String {
    match &variant.storage_value {
        Some(lazuli_ir::StorageValue::String(value)) => format_ts_string(value),
        Some(lazuli_ir::StorageValue::Integer(value)) => value.to_string(),
        None => format_ts_string(&variant.name.to_ascii_lowercase()),
    }
}

fn enum_has_option_metadata(enum_decl: &lazuli_ir::EnumDecl) -> bool {
    enum_decl.variants.iter().any(|variant| {
        variant.label_key.is_some() || variant.hint_key.is_some() || variant.icon_key.is_some()
    })
}

fn write_enum_options_alias(
    s: &mut String,
    enum_decl: &lazuli_ir::EnumDecl,
    type_name: &str,
    options_name: &str,
) {
    let label_required = enum_decl
        .variants
        .iter()
        .all(|variant| variant.label_key.is_some());
    let label_prop = if label_required {
        "labelKey: string;"
    } else {
        "labelKey?: string;"
    };
    writeln!(s, "export const {options_name}: ReadonlyArray<{{").ok();
    writeln!(s, "  value: {type_name};").ok();
    writeln!(s, "  {label_prop}").ok();
    writeln!(s, "  hintKey?: string;").ok();
    writeln!(s, "  iconKey?: string;").ok();
    writeln!(s, "}}> = [").ok();
    for variant in &enum_decl.variants {
        writeln!(s, "  {},", enum_variant_option_ts_literal(variant)).ok();
    }
    writeln!(s, "];").ok();
}

fn enum_variant_option_ts_literal(variant: &lazuli_ir::EnumVariant) -> String {
    let mut props = vec![format!("value: {}", enum_variant_ts_literal(variant))];
    if let Some(label_key) = &variant.label_key {
        props.push(format!("labelKey: {}", format_ts_string(label_key)));
    }
    if let Some(hint_key) = &variant.hint_key {
        props.push(format!("hintKey: {}", format_ts_string(hint_key)));
    }
    if let Some(icon_key) = &variant.icon_key {
        props.push(format!("iconKey: {}", format_ts_string(icon_key)));
    }
    format!("{{ {} }}", props.join(", "))
}

fn format_ts_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("\"{escaped}\"")
}

fn zod_expr_for_slot(
    type_ref: &lazuli_ir::TypeRef,
    constraints: &lazuli_ir::FieldConstraints,
    optional: bool,
    module: &lazuli_ir::Module,
) -> String {
    let base = zod_base_for_type_ref(type_ref, module);
    let is_text_base = zod_is_text_base(type_ref);
    let mut out = format!(
        "{}{}",
        base,
        lazuli_codegen_ts::zod_constraint_chain(constraints, is_text_base)
    );
    if optional {
        out.push_str(".optional()");
    }
    out
}

pub(crate) fn zod_base_for_type_ref(type_ref: &lazuli_ir::TypeRef, module: &lazuli_ir::Module) -> String {
    match type_ref {
        lazuli_ir::TypeRef::Builtin(builtin) => match builtin {
            lazuli_ir::BuiltinType::Boolean => "z.boolean()".to_owned(),
            lazuli_ir::BuiltinType::Integer
            | lazuli_ir::BuiltinType::Decimal
            | lazuli_ir::BuiltinType::SemanticMoney { .. } => "z.number()".to_owned(),
            lazuli_ir::BuiltinType::SemanticEmail => "z.string().email()".to_owned(),
            lazuli_ir::BuiltinType::SemanticPhone => {
                "/* TODO(@semantic.Phone): replace with pluggable locale-aware validator */ z.string().min(10).max(15)".to_owned()
            }
            lazuli_ir::BuiltinType::SemanticUuid => "z.string().uuid()".to_owned(),
            lazuli_ir::BuiltinType::SemanticUrl => "z.string().url()".to_owned(),
            lazuli_ir::BuiltinType::SemanticPluginType { name, .. } => {
                zod_base_for_plugin_semantic(name)
            }
            lazuli_ir::BuiltinType::Json
            | lazuli_ir::BuiltinType::SemanticGeoPoint
            | lazuli_ir::BuiltinType::CapFile => "z.unknown()".to_owned(),
            _ => "z.string()".to_owned(),
        },
        lazuli_ir::TypeRef::Capability(capability) => match capability {
            lazuli_ir::CapabilityRef::File(_) => "z.unknown()".to_owned(),
            _ => "z.string()".to_owned(),
        },
        // Wave 0 (ir-returns-list-2026-05-22 §2.3): closes the
        // `SCHEMA-RICH-001` list axis early. `list <X>` lifts to
        // `TypeRef::Many(X)` in the analyzer; emit `z.array(<inner>)`
        // so form/wire schemas validate list-of-record at runtime
        // instead of accepting any `unknown[]` shape.
        lazuli_ir::TypeRef::Many(inner) => {
            format!("z.array({})", zod_base_for_type_ref(inner, module))
        }
        lazuli_ir::TypeRef::EnumRef(name) => zod_base_for_enum_ref(module, name),
        lazuli_ir::TypeRef::UserDefined(name) => find_enum_decl(module, name)
            .map(zod_base_for_enum_decl)
            .unwrap_or_else(|| "z.unknown()".to_owned()),
        lazuli_ir::TypeRef::Unresolved(raw) => {
            if !raw.starts_with('@') {
                let synthetic = lazuli_ir::QualifiedName {
                    feature: None,
                    name: raw.clone(),
                };
                if let Some(enum_decl) = find_enum_decl(module, &synthetic) {
                    return zod_base_for_enum_decl(enum_decl);
                }
            }
            "z.unknown()".to_owned()
        }
    }
}

fn zod_base_for_enum_ref(
    module: &lazuli_ir::Module,
    name: &lazuli_ir::QualifiedName,
) -> String {
    find_enum_decl(module, name)
        .map(zod_base_for_enum_decl)
        .unwrap_or_else(|| {
            format!(
                "/* TODO: cross-feature enum {}; generated as string until the enum catalog is visible */ z.string()",
                sanitize_ts_block_comment(&qualified_type_label(name))
            )
        })
}

fn zod_base_for_enum_decl(enum_decl: &lazuli_ir::EnumDecl) -> String {
    let values = enum_decl
        .variants
        .iter()
        .map(enum_variant_ts_literal)
        .collect::<Vec<_>>();

    if values.is_empty() {
        return "z.never()".to_owned();
    }

    let has_numeric_storage = enum_decl
        .variants
        .iter()
        .any(|variant| matches!(&variant.storage_value, Some(lazuli_ir::StorageValue::Integer(_))));
    if !has_numeric_storage {
        return format!("z.enum([{}])", values.join(", "));
    }

    let literals = values
        .iter()
        .map(|value| format!("z.literal({value})"))
        .collect::<Vec<_>>();
    if literals.len() == 1 {
        literals[0].clone()
    } else {
        format!("z.union([{}])", literals.join(", "))
    }
}

fn zod_base_for_plugin_semantic(name: &str) -> String {
    match name {
        "BrazilianCPF" => {
            "/* @semantic.BrazilianCPF: basic digit-only pattern; checksum validator belongs to the plugin */ z.string().regex(/^\\d{11}$/)".to_owned()
        }
        "BrazilianCNPJ" => {
            "/* @semantic.BrazilianCNPJ: basic digit-only pattern; checksum validator belongs to the plugin */ z.string().regex(/^\\d{14}$/)".to_owned()
        }
        other => format!(
            "/* TODO(@semantic.{}): pluggable Zod validator */ z.string()",
            sanitize_ts_block_comment(other)
        ),
    }
}

fn qualified_type_label(name: &lazuli_ir::QualifiedName) -> String {
    match &name.feature {
        Some(feature) => format!("{feature}.{}", name.name),
        None => name.name.clone(),
    }
}

fn sanitize_ts_block_comment(value: &str) -> String {
    value.replace("*/", "* /").replace('\r', " ").replace('\n', " ")
}

fn zod_is_text_base(type_ref: &lazuli_ir::TypeRef) -> bool {
    matches!(
        type_ref,
        lazuli_ir::TypeRef::Builtin(
            lazuli_ir::BuiltinType::Id
                | lazuli_ir::BuiltinType::Text
                | lazuli_ir::BuiltinType::Date
                | lazuli_ir::BuiltinType::DateTime
                | lazuli_ir::BuiltinType::SemanticEmail
                | lazuli_ir::BuiltinType::SemanticPhone
                | lazuli_ir::BuiltinType::SemanticUrl
                | lazuli_ir::BuiltinType::SemanticUuid
                | lazuli_ir::BuiltinType::SemanticCurrency
                | lazuli_ir::BuiltinType::SemanticPluginType { .. }
                | lazuli_ir::BuiltinType::CapSecret
        ) | lazuli_ir::TypeRef::EnumRef(_)
            | lazuli_ir::TypeRef::Capability(
                lazuli_ir::CapabilityRef::Hashed(_)
                    | lazuli_ir::CapabilityRef::Encrypted(_)
                    | lazuli_ir::CapabilityRef::E2ee(_)
                    | lazuli_ir::CapabilityRef::Token(_)
            )
    )
}

pub(super) fn command_ident(feature: &str, command_name: &str) -> String {
    let resource_pascal = pascal_case(feature);
    let feature_lc = feature.to_ascii_lowercase();
    let mut parts = command_name.split('_');
    let verb = parts.next().unwrap_or("");
    let mut out = verb.to_ascii_lowercase();
    out.push_str(&resource_pascal);
    // Closes WAR-CODEGEN-TS-02: when the command name already contains
    // the feature name as a token (e.g. `save_host_basic_details` in
    // feature `host`), skip the duplicate token so we get
    // `saveHostBasicDetails` instead of `saveHostHostBasicDetails`.
    let mut skipped_dup = false;
    for word in parts {
        if !skipped_dup && word.eq_ignore_ascii_case(&feature_lc) {
            skipped_dup = true;
            continue;
        }
        out.push_str(&pascal_case(word));
    }
    out
}

fn command_export_ident(
    feature: &lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) -> String {
    if command_is_pure_read(command) {
        if let Some(resource_pascal) = command_return_resource_pascal(command, module) {
            let resource_plural = lazuli_codegen_ts::pluralize(&resource_pascal);
            if command.name.eq_ignore_ascii_case("list") {
                return format!("list{resource_plural}");
            }
            if let Some(rest) = strip_query_verb_prefix(&command.name, "list_") {
                let rest_pascal = pascal_case(rest);
                return format!(
                    "list{}",
                    list_subject_pascal(&rest_pascal, &resource_pascal, &resource_plural)
                );
            }
        }
    }

    command_ident(&feature.name, &command.name)
}

fn command_return_resource_pascal(
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) -> Option<String> {
    let lazuli_ir::CommandEffect::Returns(effect) = &command.effect else {
        return None;
    };
    resource_pascal_from_return_type(&effect.return_type, module)
}

fn resource_pascal_from_return_type(
    type_ref: &lazuli_ir::TypeRef,
    module: &lazuli_ir::Module,
) -> Option<String> {
    match type_ref {
        lazuli_ir::TypeRef::Many(inner) => resource_pascal_from_return_type(inner, module),
        lazuli_ir::TypeRef::UserDefined(name) if is_resource_ref(type_ref, module) => {
            find_resource(module, name).map(|resource| pascal_case(&resource.name))
        }
        _ => None,
    }
}

pub(super) fn query_ident(
    _feature: &str,
    resource_pascal: &str,
    kind: lazuli_ir::QueryKind,
    query_name: &str,
) -> String {
    match kind {
        lazuli_ir::QueryKind::List | lazuli_ir::QueryKind::Sql | lazuli_ir::QueryKind::View => {
            let resource_plural = lazuli_codegen_ts::pluralize(resource_pascal);
            if query_name.eq_ignore_ascii_case("list") {
                format!("list{resource_plural}")
            } else if query_name.eq_ignore_ascii_case("fulltext") {
                format!("search{resource_plural}Fulltext")
            } else if let Some(rest) = strip_query_verb_prefix(query_name, "list_") {
                // `conventions [crud]` synth produces `list_<resource>s`;
                // without the dedup the legacy shape would emit
                // `listListTravelersTravelers` from `list_travelers`.
                list_prefixed_ident(rest, resource_pascal, &resource_plural)
            } else {
                let short_pascal = pascal_case(query_name);
                format!(
                    "list{}",
                    list_subject_pascal(&short_pascal, resource_pascal, &resource_plural)
                )
            }
        }
        lazuli_ir::QueryKind::Lookup => {
            if let Some(rest) = strip_query_verb_prefix(query_name, "lookup_") {
                // `conventions [crud, me]` synth produces `lookup_<r>` and
                // `lookup_my_<r>`; without the dedup the legacy
                // `lookup<R>By<X>` shape would emit
                // `lookupHostByLookupMyHost` from `lookup_my_host`.
                format!("lookup{}", pascal_case(rest))
            } else {
                let stripped = query_name.strip_prefix("by_").unwrap_or(query_name);
                format!("lookup{}By{}", resource_pascal, pascal_case(stripped))
            }
        }
    }
}

fn legacy_query_ident(feature: &str, kind: lazuli_ir::QueryKind, query_name: &str) -> String {
    let resource_pascal = pascal_case(feature);
    match kind {
        lazuli_ir::QueryKind::List | lazuli_ir::QueryKind::Sql | lazuli_ir::QueryKind::View => {
            if query_name.eq_ignore_ascii_case("list") {
                format!("list{}s", resource_pascal)
            } else if query_name.eq_ignore_ascii_case("fulltext") {
                format!("search{}sFulltext", resource_pascal)
            } else if let Some(rest) = strip_query_verb_prefix(query_name, "list_") {
                format!("list{}", pascal_case(rest))
            } else {
                format!("list{}{}s", pascal_case(query_name), resource_pascal)
            }
        }
        lazuli_ir::QueryKind::Lookup => {
            if let Some(rest) = strip_query_verb_prefix(query_name, "lookup_") {
                format!("lookup{}", pascal_case(rest))
            } else {
                let stripped = query_name.strip_prefix("by_").unwrap_or(query_name);
                format!("lookup{}By{}", resource_pascal, pascal_case(stripped))
            }
        }
    }
}

fn list_prefixed_ident(rest: &str, resource_pascal: &str, resource_plural: &str) -> String {
    let rest_pascal = pascal_case(rest);
    format!(
        "list{}",
        list_subject_pascal(&rest_pascal, resource_pascal, resource_plural)
    )
}

fn list_subject_pascal(
    short_pascal: &str,
    resource_pascal: &str,
    resource_plural: &str,
) -> String {
    let legacy_plural = format!("{resource_pascal}s");
    if short_pascal == resource_plural || short_pascal.ends_with(resource_plural) {
        short_pascal.to_owned()
    } else if short_pascal == legacy_plural {
        resource_plural.to_owned()
    } else if let Some(stem) = short_pascal.strip_suffix(&legacy_plural) {
        format!("{stem}{resource_plural}")
    } else if short_pascal == resource_pascal {
        resource_plural.to_owned()
    } else if let Some(stem) = short_pascal.strip_suffix(resource_pascal) {
        format!("{stem}{resource_plural}")
    } else if let Some(cleaned) = remove_embedded_resource_plural(short_pascal, resource_pascal) {
        format!("{cleaned}{resource_plural}")
    } else {
        format!("{short_pascal}{resource_plural}")
    }
}

fn remove_embedded_resource_plural(short_pascal: &str, resource_pascal: &str) -> Option<String> {
    let tokens = pascal_tokens(short_pascal);
    let resource_tokens = pascal_tokens(resource_pascal);
    let resource_last = resource_tokens.last()?;
    let resource_last_plural = lazuli_codegen_ts::pluralize(resource_last);
    let legacy_resource_last_plural = format!("{resource_last}s");
    let mut remove = vec![false; tokens.len()];

    for (index, token) in tokens.iter().enumerate() {
        if token == &resource_last_plural || token == &legacy_resource_last_plural {
            remove[index] = true;
        }
    }
    if !remove.iter().any(|remove| *remove) {
        return None;
    }

    for (index, token) in tokens.iter().enumerate() {
        let adjacent_removed =
            (index > 0 && remove[index - 1]) || remove.get(index + 1).copied().unwrap_or(false);
        if token == "As" && adjacent_removed {
            remove[index] = true;
        }
    }

    let cleaned = tokens
        .iter()
        .zip(remove)
        .filter_map(|(token, remove)| (!remove).then_some(token.as_str()))
        .collect::<String>();
    (!cleaned.is_empty()).then_some(cleaned)
}

fn pascal_tokens(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut start = 0;
    for (index, ch) in value.char_indices().skip(1) {
        if ch.is_ascii_uppercase() {
            tokens.push(value[start..index].to_owned());
            start = index;
        }
    }
    tokens.push(value[start..].to_owned());
    tokens.retain(|token| !token.is_empty());
    tokens
}

fn write_deprecated_const_alias(s: &mut String, old_name: &str, new_name: &str) {
    writeln!(s, "/** @deprecated use `{new_name}` */").ok();
    writeln!(s, "export const {old_name} = {new_name};").ok();
    writeln!(s).ok();
}

/// Strip a verb prefix (`lookup_` / `list_`) from a query name, returning
/// `Some(rest)` only when the remainder pascal-cases to a non-empty
/// segment. Returns `None` for bare prefix (`lookup_`), missing prefix,
/// or empty/whitespace remainder — callers fall back to the legacy hook
/// shape. Mirrors `lazuli_codegen_ts::lzx::strip_verb_prefix`; duplicated
/// here to keep the CLI's identifier-casing rules self-contained.
fn strip_query_verb_prefix<'a>(name: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = name.strip_prefix(prefix)?;
    if rest.is_empty() {
        return None;
    }
    if pascal_case(rest).is_empty() {
        return None;
    }
    Some(rest)
}

fn command_input_iface(command_name: &str, feature_pascal: &str) -> String {
    let feature_lc = feature_pascal.to_ascii_lowercase();
    let mut parts = command_name.split('_');
    let verb = parts.next().unwrap_or("");
    let mut out = pascal_case(verb);
    out.push_str(feature_pascal);
    // Mirror command_ident's WAR-CODEGEN-TS-02 dedup so the *Input
    // interface name matches the command identifier shape.
    let mut skipped_dup = false;
    for word in parts {
        if !skipped_dup && word.eq_ignore_ascii_case(&feature_lc) {
            skipped_dup = true;
            continue;
        }
        out.push_str(&pascal_case(word));
    }
    out.push_str("Input");
    out
}

pub(crate) fn command_schema_ident(command_name: &str, feature_pascal: &str) -> String {
    let iface = command_input_iface(command_name, feature_pascal);
    lower_camel(&iface) + "Schema"
}

fn format_string_array(items: &[String]) -> String {
    if items.is_empty() {
        return "[]".to_owned();
    }
    let parts: Vec<String> = items.iter().map(|s| format!("\"{s}\"")).collect();
    format!("[{}]", parts.join(", "))
}
