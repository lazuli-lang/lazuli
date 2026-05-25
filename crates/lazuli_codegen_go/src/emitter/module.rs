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

/// Default Go module path used when the caller did not supply one and
/// the IR exposes no `app.name`. Matches proposal §1.1's "fallback
/// `lazuli/app`" rule.
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

pub struct GoSourceContext<'a> {
    pub source_map: &'a SourceMap,
    pub feature_file_ids: &'a BTreeMap<String, FileId>,
}

pub struct EmitContext<'a> {
    pub source_map: Option<&'a SourceMap>,
    pub current_file_id: Option<FileId>,
    pub generated_path: &'a str,
    pub capsule_name: &'a str,
    pub current_feature: &'a str,
    /// PG.C.1 — per-callable gate directives. Populated when the caller
    /// provides `GoEmitOptions.plan_gate`; lookups go through
    /// `gates_for(<callable_kind>, <callable_name>)`. `None` when no
    /// plan-gate facts were threaded — emitters emit no prelude.
    pub gates: Option<&'a BTreeMap<String, Vec<Gate>>>,
}

impl<'a> EmitContext<'a> {
    pub fn no_source(generated_path: &'a str) -> Self {
        Self {
            source_map: None,
            current_file_id: None,
            generated_path,
            capsule_name: "",
            current_feature: "",
            gates: None,
        }
    }

    pub fn for_feature(
        source_context: Option<&'a GoSourceContext<'a>>,
        capsule_name: &'a str,
        feature_name: &'a str,
        generated_path: &'a str,
    ) -> Self {
        Self {
            source_map: source_context.map(|ctx| ctx.source_map),
            current_file_id: source_context
                .and_then(|ctx| ctx.feature_file_ids.get(feature_name))
                .copied(),
            generated_path,
            capsule_name,
            current_feature: feature_name,
            gates: None,
        }
    }

    /// PG.C.1 — look up the gate directives declared on a callable.
    /// `callable_kind` is the on-disk authoring kind (`command`,
    /// `query.list`, `query.lookup`, `query.sql`, `job`, `webhook`,
    /// `api`). Returns an empty slice when no gates apply (the common
    /// case: most callables are not gated).
    pub fn gates_for(&self, callable_kind: &str, callable_name: &str) -> &'a [Gate] {
        let Some(map) = self.gates else {
            return &[];
        };
        let key = format!(
            "{}/{}:{}",
            self.current_feature, callable_kind, callable_name
        );
        map.get(&key).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// PG.C.1 — fluent setter so call sites can chain `EmitContext::for_feature(...).with_gates(...)`.
    pub fn with_gates(mut self, gates: Option<&'a BTreeMap<String, Vec<Gate>>>) -> Self {
        self.gates = gates;
        self
    }

    pub fn emit_line_directive(&self, p: &mut GoPrinter, span: Option<SpanRef>) -> bool {
        let (Some(source_map), Some(file_id), Some(span)) =
            (self.source_map, self.current_file_id, span)
        else {
            return false;
        };
        let Some(loc) = resolve_source_loc(source_map, file_id, span) else {
            return false;
        };
        p.line_directive(&loc.file, loc.line, loc.column);
        true
    }

    pub fn reset_line_directive(&self, p: &mut GoPrinter, emitted: bool) {
        if emitted {
            p.line_directive(self.generated_path, 1, 1);
        }
    }

    pub fn source_tag_literal(&self, kind: &str, op: &str, span: Option<SpanRef>) -> String {
        let source = self.source_loc_string(span).unwrap_or_else(String::new);
        format!(
            "lazuli.SourceTag{{Capsule: {:?}, Feature: {:?}, Kind: {:?}, Op: {:?}, Source: {:?}}}",
            self.capsule_name, self.current_feature, kind, op, source
        )
    }

    pub fn emit_with_source_field(
        &self,
        p: &mut GoPrinter,
        kind: &str,
        op: &str,
        span: Option<SpanRef>,
    ) {
        p.line("WithSource: func(ctx context.Context) context.Context {");
        p.indent();
        p.line("ctx = lazuli.WithSource(ctx, lazuli.SourceTag{");
        p.indent();
        p.line(&format!("Capsule: {:?},", self.capsule_name));
        p.line(&format!("Feature: {:?},", self.current_feature));
        p.line(&format!("Kind:    {:?},", kind));
        p.line(&format!("Op:      {:?},", op));
        p.line(&format!(
            "Source:  {:?},",
            self.source_loc_string(span).unwrap_or_else(String::new)
        ));
        p.dedent();
        p.line("})");
        p.line("return ctx");
        p.dedent();
        p.line("},");
    }

    fn source_loc_string(&self, span: Option<SpanRef>) -> Option<String> {
        let (Some(source_map), Some(file_id), Some(span)) =
            (self.source_map, self.current_file_id, span)
        else {
            return None;
        };
        let loc = resolve_source_loc(source_map, file_id, span)?;
        Some(format!("{}:{}:{}", loc.file, loc.line, loc.column))
    }
}

struct ResolvedLoc {
    file: String,
    line: u32,
    column: u32,
}

fn resolve_source_loc(
    source_map: &SourceMap,
    file_id: FileId,
    span: SpanRef,
) -> Option<ResolvedLoc> {
    let file = source_map.files.iter().find(|file| file.id == file_id)?;
    let source_len = file.line_offsets.last().copied()?;
    let start = u32::try_from(span.start).ok()?;
    if start > source_len {
        return None;
    }

    let searchable_offsets = if file.line_offsets.len() > 1 {
        &file.line_offsets[..file.line_offsets.len() - 1]
    } else {
        &file.line_offsets[..]
    };

    let line_idx = match searchable_offsets.binary_search(&start) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let line_start = searchable_offsets.get(line_idx)?;
    Some(ResolvedLoc {
        file: file.path.clone(),
        line: (line_idx + 1) as u32,
        column: start - line_start + 1,
    })
}

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

fn resolve_module_name(
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

fn resolve_source_label(module: &Module) -> String {
    match module.app.as_ref() {
        Some(app) => app.name.clone(),
        None => "lazuli module".to_owned(),
    }
}

fn emit_go_mod(
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
    if let Some(manifest) = manifest {
        if let Some(dev) = manifest.dev.as_ref() {
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
    }
    p.finish()
}

fn emit_go_work(dev_runtime_path: Option<&str>, manifest: Option<&LazuriteManifest>) -> String {
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
            .map(|p| p.clone())
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

fn collect_transitive_deps(module: &Module) -> Vec<&'static TransitiveDep> {
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

fn type_ref_contains_geopoint(type_ref: &TypeRef) -> bool {
    match type_ref {
        TypeRef::Builtin(BuiltinType::SemanticGeoPoint) => true,
        TypeRef::Many(inner) => type_ref_contains_geopoint(inner),
        _ => false,
    }
}

fn emit_feature_stub(source: &str, feature_name: &str) -> String {
    let mut p = GoPrinter::new();
    p.banner(source, &super::casing::gen_package_name(feature_name));
    // E1 stub: imports are recorded but unused because no kinds emit
    // yet. We deliberately do not produce an `import (...)` block
    // until the first kind walker (cell E2) introduces a real use —
    // a leading empty block would fail `gofmt`.
    let _placeholder = ImportSet::new();
    p.finish()
}

/// Lower-snake / lower-kebab caser shared with the CLI helper. Mirrors
/// `lazuli_codegen_go::to_kebab_case` (legacy demo) and the CLI's
/// `to_kebab_case` so the derived module name matches across surfaces.
fn to_kebab(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut prev_lower = false;
    for ch in value.chars() {
        if ch == '_' || ch == ' ' {
            out.push('-');
            prev_lower = false;
            continue;
        }
        if ch.is_ascii_uppercase() {
            if prev_lower && !out.is_empty() {
                out.push('-');
            }
            for low in ch.to_lowercase() {
                out.push(low);
            }
            prev_lower = false;
            continue;
        }
        out.push(ch);
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    out
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
