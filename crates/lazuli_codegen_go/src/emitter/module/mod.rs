//! Top-level emitter walker. Drives per-feature emission and the root
//! `go.mod`. For cell E1 the per-feature output is intentionally
//! empty (just banner + `package` directive); per-kind walkers
//! (Resource, Command, Query, …) land in E2-E4 and G1-G7.
//!
//! Per proposal §2.4 every iteration is deterministic — features are
//! sorted by name before emission so output is byte-equivalent across
//! runs regardless of feature insertion order at the IR layer.

use std::collections::BTreeMap;

use lazuli_ir::{FileId, Gate, Module, SourceMap, SpanRef};

use super::api::emit_api_file;
use super::audit::{emit_audit_log_ddl, emit_audit_metadata};
use super::auth::emit_auth_file;
use super::auth_refresh::emit_auth_refresh_file;
use super::auto_photo::emit_auto_photo_file;
use super::command::emit_command_file;
use super::cross_feature::CrossFeatureIndex;
use super::deps::{GO_POSTGIS_DEP, TransitiveDep};
use super::enums::emit_enum_file;
use super::error_resolver::{
    APP_ERROR_RESOLUTION_PATH, emit_app_error_resolution, emit_feature_errors_file,
};
use super::events::emit_events_file;
use super::handlers::emit_handler_stubs;
use super::imports::ImportSet;
use super::job::emit_job_file;
use super::lint::check_generated_file;
use super::mcp_server::emit_mcp_server_file;
use super::migration::emit_migration_file;
use super::migration_ddl::emit_migrations;
use super::notification::emit_notification_file;
use super::poller::emit_poller_file;
use super::printer::GoPrinter;
use super::query::emit_query_file;
use super::report::emit_reports_file;
use super::resource::emit_resource_file;
use super::root::{LAZULI_APP_PATH, MAIN_GO_PATH, emit_lazuli_app_gen, emit_main_go};
use super::storage::emit_storage_file;
use super::translation::emit_translation_files;
use super::webhook::emit_webhook_file;
use crate::{GeneratedFile, GoEmitOptions, LAZULI_GO_VERSION, LazuriteManifest};
use lazuli_ir::{BuiltinType, TypeRef};

// Wave R7-3 extract — `EmitContext`/`GoSourceContext` cluster moved
// into `module/context.rs`; `go.mod` / `go.work` emission cluster
// moved into `module/go_mod.rs`; small misc helpers (`emit_feature_stub`,
// `to_kebab`) moved into `module/helpers.rs`. Re-exported so callers
// outside this module (every per-kind emitter under `emitter/*`) keep
// their `super::module::{GoSourceContext, EmitContext}` imports working.
mod context;
mod go_mod;
mod helpers;

pub use context::{EmitContext, GoSourceContext};
use go_mod::{collect_transitive_deps, emit_go_mod, emit_go_work, resolve_module_name, resolve_source_label};
use helpers::{emit_feature_stub, to_kebab};

/// Default Go module path used when the caller did not supply one and
/// the IR exposes no `app.name`. Matches proposal §1.1's "fallback
/// `lazuli/app`" rule.


/// Walk the IR module and produce every `.gen.go` plus the root
/// `go.mod`. Per cell E1 this only emits the file skeleton; kinds
/// land in subsequent cells.
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
    // facade binding). See docs/proposals/hostpoint-complete-roadmap-2026-05-18.md §3.5.
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
    if let Some(facts) = &options.plan_gate {
        if let Some(contents) = crate::emitter::plan::emit_plan_catalog_file(facts) {
            files.push(GeneratedFile {
                path: "plan/catalog.gen.go".to_owned(),
                contents,
            });
        }
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
        let path = format!("{name}/{name}.gen.go", name = feature.name);
        let contents = emit_feature_stub(&source_label, &feature.name);
        files.push(GeneratedFile { path, contents });

        // Cell E2 — `Resource` + `Record` emission lands in a sibling
        // file. Features that declare neither skip the file entirely
        // (an empty body would leave a stray `package <feature>` and
        // gofmt would tolerate it but the file would carry no signal).
        if let Some(contents) =
            emit_resource_file(&source_label, feature, &module_name, &cross_index)
        {
            let resource_path = format!("{name}/resource.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: resource_path,
                contents,
            });
        }

        // Cell E2.5 — `EnumDecl` emission. Per-feature typed aliases
        // plus aligned const blocks land in a sibling `enum.gen.go`.
        // Skipped entirely when the feature declares no enums so the
        // output listing stays signal-rich.
        if let Some(contents) = emit_enum_file(&source_label, feature) {
            let enum_path = format!("{name}/enum.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: enum_path,
                contents,
            });
        }

        // Cell CODEGEN-1 (IR Error-Vocab) — per-feature `errors.gen.go`
        // emitted when the feature declares an `errors` block. Carries
        // the lowered `FeatureErrorContract` (exposure rules + per-code
        // message overrides). Skipped when `Feature.errors.is_none()`
        // so the output listing stays signal-rich. See
        // `docs/proposals/ir-error-messages-vocab.md` §4.1.2.
        if let Some(contents) = emit_feature_errors_file(&source_label, feature) {
            let errors_path = format!("{name}/errors.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: errors_path,
                contents,
            });
        }

        // Cell E3 — `Command` emission. Walks every command on the
        // feature into a sibling `command.gen.go`. Features without
        // commands skip the file entirely (mirrors the resource /
        // enum skip rule so the output listing stays signal-rich).
        {
            let command_path = format!("{name}/command.gen.go", name = feature.name);
            let emit_ctx = EmitContext::for_feature(
                source_context,
                &source_label,
                &feature.name,
                &command_path,
            )
            .with_gates(gate_map);
            if let Some(contents) = emit_command_file(
                &source_label,
                feature,
                &module_name,
                &cross_index,
                &emit_ctx,
            ) {
                files.push(GeneratedFile {
                    path: command_path,
                    contents,
                });
            }
        }

        // Cell E4 — Query.{List, Lookup, Sql} emission. Per-feature
        // typed Args struct + `lazuli.Query[A, R]` value per query
        // into `query.gen.go`.
        {
            let query_path = format!("{name}/query.gen.go", name = feature.name);
            let emit_ctx =
                EmitContext::for_feature(source_context, &source_label, &feature.name, &query_path)
                    .with_gates(gate_map);
            if let Some(contents) = emit_query_file(
                &source_label,
                feature,
                &module_name,
                &cross_index,
                &emit_ctx,
            ) {
                files.push(GeneratedFile {
                    path: query_path,
                    contents,
                });
            }
        }

        // WAR-RUNTIME-COMMAND-01 (register half) — `register.gen.go`
        // emits a single `func init()` that calls `lazuli.Register(...)`
        // for every Resource/Command/Query in this feature. Required
        // before `lazuli.Mux()` can route HTTP. Skipped when the feature
        // declares none of those (output stays signal-rich).
        if let Some(contents) = crate::emitter::register::emit_register_file(&source_label, feature)
        {
            let register_path = format!("{name}/register.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: register_path,
                contents,
            });
        }

        // Cell G1 — Auth emission. Per-feature `auth` block lowered
        // to `auth.PasswordContract` / `SessionsContract` / `MfaContract`
        // / `OAuthContract` typed values in `auth.gen.go`.
        {
            let auth_path = format!("{name}/auth.gen.go", name = feature.name);
            let emit_ctx =
                EmitContext::for_feature(source_context, &source_label, &feature.name, &auth_path);
            if let Some(contents) = emit_auth_file(
                &source_label,
                feature,
                &module_name,
                &cross_index,
                &emit_ctx,
            ) {
                files.push(GeneratedFile {
                    path: auth_path,
                    contents,
                });
            }
        }

        // IR Auth Refresh CODEGEN-1 — the framework-emitted
        // `auth.refresh` command handler wire. Skipped unless
        // `auth.sessions.rotation` is enabled so un-authored shapes
        // produce no extra `.gen.go` file.
        if let Some(contents) = emit_auth_refresh_file(&source_label, feature) {
            let auth_refresh_path = format!("{name}/auth.refresh.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: auth_refresh_path,
                contents,
            });
        }

        // Cell G2a — Job emission. Per-feature `lazuli.JobContract`
        // values in `job.gen.go`.
        {
            let job_path = format!("{name}/job.gen.go", name = feature.name);
            let emit_ctx =
                EmitContext::for_feature(source_context, &source_label, &feature.name, &job_path)
                    .with_gates(gate_map);
            if let Some(contents) = emit_job_file(
                &source_label,
                feature,
                &module_name,
                &cross_index,
                &emit_ctx,
            ) {
                files.push(GeneratedFile {
                    path: job_path,
                    contents,
                });
            }
        }

        // Cell G2b — Webhook v0 spine emission. Per-feature
        // `lazuli.WebhookContract` values in `webhook.gen.go`.
        {
            let webhook_path = format!("{name}/webhook.gen.go", name = feature.name);
            let emit_ctx = EmitContext::for_feature(
                source_context,
                &source_label,
                &feature.name,
                &webhook_path,
            )
            .with_gates(gate_map);
            if let Some(contents) = emit_webhook_file(
                &source_label,
                feature,
                &module_name,
                &cross_index,
                &emit_ctx,
            ) {
                files.push(GeneratedFile {
                    path: webhook_path,
                    contents,
                });
            }
        }

        // Cell G2c — Notification v0 spine emission. Per-feature
        // `lazuli.NotificationContract` values in `notification.gen.go`.
        {
            let notification_path = format!("{name}/notification.gen.go", name = feature.name);
            let emit_ctx = EmitContext::for_feature(
                source_context,
                &source_label,
                &feature.name,
                &notification_path,
            );
            if let Some(contents) = emit_notification_file(
                &source_label,
                feature,
                &module_name,
                &cross_index,
                &emit_ctx,
            ) {
                files.push(GeneratedFile {
                    path: notification_path,
                    contents,
                });
            }
        }

        // Cell P.C — Poller v0 spine emission. Per-feature
        // `RegisterPollers(*poller.Registry)` with `poller.Spec[...]`
        // literals in `poller.gen.go`. Per docs/proposals/poller-vocab.md §6.1.
        if let Some(contents) = emit_poller_file(&source_label, feature) {
            let poller_path = format!("{name}/poller.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: poller_path,
                contents,
            });
        }

        // MCP bucket cycle (M8) — emit `<feature>/mcp_server.gen.go`
        // containing one `mcp.ServerRegistration` literal per declared
        // `mcp_server <name>` block plus an `init()` that registers
        // them with the global runtime registry
        // (`lazuli.dev/runtime/lazuli/mcp.RegisterServer`). The Lazuli
        // boot path enumerates the registry and starts each transport.
        if let Some(contents) = emit_mcp_server_file(&source_label, feature) {
            let mcp_path = format!("{name}/mcp_server.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: mcp_path,
                contents,
            });
        }

        // Cell G3a (Wave 3.5) — Translation emission. Lowers
        // `feature.translation.keys[]` into one `translation.gen.go`
        // that calls `lazuli.RegisterFeatureTranslationCatalog(...)`
        // plus one `i18n/<feature>.<locale>.json` per locale the
        // feature authored variants for. The runtime loader merges
        // every authored bare key as `<feature>.<bare_key>` into the
        // default resolver's `Catalogs` map so L1/L2 lookups hit
        // authored text (proposal §2.E, §5.1).
        files.extend(emit_translation_files(
            &source_label,
            feature,
            &module_name,
            &cross_index,
        ));

        // Cell G3b — EventGroup emission. Per-feature `lazuli.EventGroup`
        // values + payload structs in `events.gen.go`.
        if let Some(contents) = emit_events_file(&source_label, feature, &module_name, &cross_index)
        {
            let events_path = format!("{name}/events.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: events_path,
                contents,
            });
        }

        // Cell G4 — Storage emission. Per-feature `storage.FileContract`
        // values for every `@cap.File(...)` site in `storage.gen.go`.
        if let Some(contents) =
            emit_storage_file(&source_label, feature, &module_name, &cross_index)
        {
            let storage_path = format!("{name}/storage.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: storage_path,
                contents,
            });
        }
        // FR-3b.2 — auto-photo init() registration emission. One per
        // feature with at least one synthesized @cap.File command group.
        if let Some(contents) =
            emit_auto_photo_file(&source_label, feature, &module_name, &cross_index)
        {
            let auto_photo_path = format!("{name}/auto_photo.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: auto_photo_path,
                contents,
            });
        }

        // Cell G5 — Api emission. Per-feature `lazuli.Api[I, O]`
        // values in `api.gen.go` (Lazuli Go lib gap §4.2 — emitter
        // ships TODO comments inside the value literal).
        {
            let api_path = format!("{name}/api.gen.go", name = feature.name);
            let emit_ctx =
                EmitContext::for_feature(source_context, &source_label, &feature.name, &api_path)
                    .with_gates(gate_map);
            if let Some(contents) = emit_api_file(
                &source_label,
                feature,
                &module_name,
                &cross_index,
                &emit_ctx,
            ) {
                files.push(GeneratedFile {
                    path: api_path,
                    contents,
                });
            }
        }

        // R.C — Report emission. Per-feature `report.Contract` values
        // (one per `report <name>`) + Run<Name> entry points in
        // `reports.gen.go`. The auto-mounted HTTP routes are wired by
        // the runtime + main.go (out of scope for the emitter walker).
        // See `docs/proposals/report-vocab.md` v0.2 §Codegen.
        if let Some(contents) = emit_reports_file(&source_label, feature) {
            let reports_path = format!("{name}/reports.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: reports_path,
                contents,
            });
        }

        // Cell TM — TenantMigration emission. Per-feature
        // `migrations.MigrationContract` values in `migration.gen.go`.
        if let Some(contents) =
            emit_migration_file(&source_label, feature, &module_name, &cross_index)
        {
            let migration_path = format!("{name}/migration.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: migration_path,
                contents,
            });
        }
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
            panic!("{err}");
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
