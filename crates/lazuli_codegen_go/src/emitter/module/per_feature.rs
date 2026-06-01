//! Per-feature file walker — extracted from `module/mod.rs` so the
//! orchestrator stays under the file budget. Emits every `<feature>/*.gen.go`
//! that the various kind emitters (Resource, Command, Query, Auth, Job,
//! Webhook, Notification, Poller, MCP server, Translation, Events,
//! Storage, Auto-Photo, Api, Reports, Migration) are responsible for.
//!
//! The orchestrator (`emit_module`) iterates the feature list and calls
//! `emit_feature_files` once per feature, passing in everything that
//! survives between iterations (cross-feature index, module name, gate
//! map, source label / context). The helper returns the freshly emitted
//! `GeneratedFile` records and the caller pushes them into the global
//! list.

use std::collections::BTreeMap;

use lazuli_ir::{Feature, Gate};

use crate::GeneratedFile;
use crate::emitter::api::emit_api_file;
use crate::emitter::auth::emit_auth_file;
use crate::emitter::auth_refresh::emit_auth_refresh_file;
use crate::emitter::auto_photo::emit_auto_photo_file;
use crate::emitter::command::emit_command_file;
use crate::emitter::cross_feature::CrossFeatureIndex;
use crate::emitter::enums::emit_enum_file;
use crate::emitter::error_resolver::emit_feature_errors_file;
use crate::emitter::events::emit_events_file;
use crate::emitter::job::emit_job_file;
use crate::emitter::mcp_server::emit_mcp_server_file;
use crate::emitter::migration::emit_migration_file;
use crate::emitter::notification::emit_notification_file;
use crate::emitter::poller::emit_poller_file;
use crate::emitter::query::emit_query_file;
use crate::emitter::referential_guard::emit_referential_guard_file;
use crate::emitter::report::emit_reports_file;
use crate::emitter::resource::emit_resource_file;
use crate::emitter::storage::emit_storage_file;
use crate::emitter::translation::emit_translation_files;
use crate::emitter::webhook::emit_webhook_file;

use super::context::{EmitContext, GoSourceContext};
use super::helpers::emit_feature_stub;

/// Emit every per-feature `<feature>/*.gen.go` file for a single
/// feature. The orchestrator pushes the returned vector into its
/// global `files` list. Files that the kind emitters skip (because
/// the feature declares no items of that kind) are not appended.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_feature_files(
    feature: &Feature,
    source_label: &str,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
    source_context: Option<&GoSourceContext<'_>>,
    gate_map: Option<&BTreeMap<String, Vec<Gate>>>,
) -> Vec<GeneratedFile> {
    let mut files = Vec::new();

    let path = format!("{name}/{name}.gen.go", name = feature.name);
    let contents = emit_feature_stub(source_label, &feature.name);
    files.push(GeneratedFile { path, contents });

    // Cell E2 — `Resource` + `Record` emission lands in a sibling
    // file. Features that declare neither skip the file entirely
    // (an empty body would leave a stray `package <feature>` and
    // gofmt would tolerate it but the file would carry no signal).
    if let Some(contents) = emit_resource_file(source_label, feature, module_name, cross_index) {
        let resource_path = format!("{name}/resource.gen.go", name = feature.name);
        files.push(GeneratedFile {
            path: resource_path,
            contents,
        });
    }

    // Spec 0014 — referential-guard precondition functions. Per-feature
    // `guards.gen.go` carrying one `guard<Protected><Relation>Refs` EXISTS
    // probe per `restrict on_delete references … via …` clause. Skipped
    // when no resource in the feature declares a guard (signal-rich listing).
    if let Some(contents) = emit_referential_guard_file(source_label, feature) {
        let guards_path = format!("{name}/guards.gen.go", name = feature.name);
        files.push(GeneratedFile {
            path: guards_path,
            contents,
        });
    }

    // Cell E2.5 — `EnumDecl` emission. Per-feature typed aliases
    // plus aligned const blocks land in a sibling `enum.gen.go`.
    // Skipped entirely when the feature declares no enums so the
    // output listing stays signal-rich.
    if let Some(contents) = emit_enum_file(source_label, feature) {
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
    if let Some(contents) = emit_feature_errors_file(source_label, feature) {
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
        let emit_ctx =
            EmitContext::for_feature(source_context, source_label, &feature.name, &command_path)
                .with_gates(gate_map);
        if let Some(contents) =
            emit_command_file(source_label, feature, module_name, cross_index, &emit_ctx)
        {
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
            EmitContext::for_feature(source_context, source_label, &feature.name, &query_path)
                .with_gates(gate_map);
        if let Some(contents) =
            emit_query_file(source_label, feature, module_name, cross_index, &emit_ctx)
        {
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
    if let Some(contents) = crate::emitter::register::emit_register_file(source_label, feature) {
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
            EmitContext::for_feature(source_context, source_label, &feature.name, &auth_path);
        if let Some(contents) =
            emit_auth_file(source_label, feature, module_name, cross_index, &emit_ctx)
        {
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
    if let Some(contents) = emit_auth_refresh_file(source_label, feature) {
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
            EmitContext::for_feature(source_context, source_label, &feature.name, &job_path)
                .with_gates(gate_map);
        if let Some(contents) =
            emit_job_file(source_label, feature, module_name, cross_index, &emit_ctx)
        {
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
        let emit_ctx =
            EmitContext::for_feature(source_context, source_label, &feature.name, &webhook_path)
                .with_gates(gate_map);
        if let Some(contents) =
            emit_webhook_file(source_label, feature, module_name, cross_index, &emit_ctx)
        {
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
            source_label,
            &feature.name,
            &notification_path,
        );
        if let Some(contents) =
            emit_notification_file(source_label, feature, module_name, cross_index, &emit_ctx)
        {
            files.push(GeneratedFile {
                path: notification_path,
                contents,
            });
        }
    }

    // Cell P.C — Poller v0 spine emission. Per-feature
    // `RegisterPollers(*poller.Registry)` with `poller.Spec[...]`
    // literals in `poller.gen.go`. Per docs/proposals/poller-vocab.md §6.1.
    if let Some(contents) = emit_poller_file(source_label, feature) {
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
    if let Some(contents) = emit_mcp_server_file(source_label, feature) {
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
        source_label,
        feature,
        module_name,
        cross_index,
    ));

    // Cell G3b — EventGroup emission. Per-feature `lazuli.EventGroup`
    // values + payload structs in `events.gen.go`.
    if let Some(contents) = emit_events_file(source_label, feature, module_name, cross_index) {
        let events_path = format!("{name}/events.gen.go", name = feature.name);
        files.push(GeneratedFile {
            path: events_path,
            contents,
        });
    }

    // Cell G4 — Storage emission. Per-feature `storage.FileContract`
    // values for every `@cap.File(...)` site in `storage.gen.go`.
    if let Some(contents) = emit_storage_file(source_label, feature, module_name, cross_index) {
        let storage_path = format!("{name}/storage.gen.go", name = feature.name);
        files.push(GeneratedFile {
            path: storage_path,
            contents,
        });
    }

    // FR-3b.2 — auto-photo init() registration emission. One per
    // feature with at least one synthesized @cap.File command group.
    if let Some(contents) = emit_auto_photo_file(source_label, feature, module_name, cross_index) {
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
            EmitContext::for_feature(source_context, source_label, &feature.name, &api_path)
                .with_gates(gate_map);
        if let Some(contents) =
            emit_api_file(source_label, feature, module_name, cross_index, &emit_ctx)
        {
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
    if let Some(contents) = emit_reports_file(source_label, feature) {
        let reports_path = format!("{name}/reports.gen.go", name = feature.name);
        files.push(GeneratedFile {
            path: reports_path,
            contents,
        });
    }

    // Cell TM — TenantMigration emission. Per-feature
    // `migrations.MigrationContract` values in `migration.gen.go`.
    if let Some(contents) = emit_migration_file(source_label, feature, module_name, cross_index) {
        let migration_path = format!("{name}/migration.gen.go", name = feature.name);
        files.push(GeneratedFile {
            path: migration_path,
            contents,
        });
    }

    files
}
