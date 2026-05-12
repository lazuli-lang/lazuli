//! Top-level emitter walker. Drives per-feature emission and the root
//! `go.mod`. For cell E1 the per-feature output is intentionally
//! empty (just banner + `package` directive); per-kind walkers
//! (Resource, Command, Query, …) land in E2-E4 and G1-G7.
//!
//! Per proposal §2.4 every iteration is deterministic — features are
//! sorted by name before emission so output is byte-equivalent across
//! runs regardless of feature insertion order at the IR layer.

use std::collections::BTreeMap;

use lazuli_ir::Module;

use super::api::emit_api_file;
use super::audit::{emit_audit_log_ddl, emit_audit_metadata};
use super::auth::emit_auth_file;
use super::command::emit_command_file;
use super::cross_feature::CrossFeatureIndex;
use super::enums::emit_enum_file;
use super::events::emit_events_file;
use super::imports::ImportSet;
use super::job::emit_job_file;
use super::migration::emit_migration_file;
use super::migration_ddl::emit_migrations;
use super::notification::emit_notification_file;
use super::printer::GoPrinter;
use super::query::emit_query_file;
use super::resource::emit_resource_file;
use super::root::{emit_lazuli_app_gen, emit_main_go, LAZULI_APP_PATH, MAIN_GO_PATH};
use super::storage::emit_storage_file;
use super::translation::emit_translation_files;
use super::webhook::emit_webhook_file;
use crate::{GeneratedFile, GoEmitOptions, LAZULI_GO_VERSION};

/// Default Go module path used when the caller did not supply one and
/// the IR exposes no `app.name`. Matches proposal §1.1's "fallback
/// `lazuli/app`" rule.
const DEFAULT_MODULE_NAME: &str = "lazuli/app";

/// Default Go toolchain pin emitted into `go.mod`. Matches
/// `runtime/go/go.mod` (currently `go 1.25.0`) so the generated
/// module shares the same toolchain expectation as the hand-written
/// Lazuli Go library.
const DEFAULT_GO_TOOLCHAIN: &str = "go 1.25";

/// Walk the IR module and produce every `.gen.go` plus the root
/// `go.mod`. Per cell E1 this only emits the file skeleton; kinds
/// land in subsequent cells.
pub fn emit_module(module: &Module, options: &GoEmitOptions) -> Vec<GeneratedFile> {
    let module_name = resolve_module_name(module, options);
    let lazuli_go_version = if options.lazuli_go_version.is_empty() {
        LAZULI_GO_VERSION.to_owned()
    } else {
        options.lazuli_go_version.clone()
    };

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

    // Root `go.mod` first so byte-comparison fixtures find it at
    // index 0.
    files.push(GeneratedFile {
        path: "go.mod".to_owned(),
        contents: emit_go_mod(&module_name, &lazuli_go_version),
    });

    // Phase Prep §1.1 mini-cell pré-E3 — build the cross-feature
    // resolver index once per run, before any per-feature walker
    // sees a type. The index lifts cross-feature `UserDefined` /
    // `EnumRef` references (analyzer leaves these with
    // `qname.feature = None`) to `<owner>.<Name>` plus a
    // `<module>/<owner>` import.
    let cross_index = CrossFeatureIndex::build(module);

    let source_label = resolve_source_label(module);

    // Cell I2 — root `main.go` (always emitted) + `lazuli_app.gen.go`
    // (skipped when `module.app == None` and no observable surface).
    // Ordered after `go.mod` and before per-feature files so reading
    // the output listing top-down surfaces the binary entry first.
    files.push(GeneratedFile {
        path: MAIN_GO_PATH.to_owned(),
        contents: emit_main_go(module, &module_name, &source_label),
    });
    if let Some(contents) = emit_lazuli_app_gen(module, &source_label) {
        files.push(GeneratedFile {
            path: LAZULI_APP_PATH.to_owned(),
            contents,
        });
    }

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

        // Cell E3 — `Command` emission. Walks every command on the
        // feature into a sibling `command.gen.go`. Features without
        // commands skip the file entirely (mirrors the resource /
        // enum skip rule so the output listing stays signal-rich).
        if let Some(contents) =
            emit_command_file(&source_label, feature, &module_name, &cross_index)
        {
            let command_path = format!("{name}/command.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: command_path,
                contents,
            });
        }

        // Cell E4 — Query.{List, Lookup, Sql} emission. Per-feature
        // typed Args struct + `lazuli.Query[A, R]` value per query
        // into `query.gen.go`.
        if let Some(contents) =
            emit_query_file(&source_label, feature, &module_name, &cross_index)
        {
            let query_path = format!("{name}/query.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: query_path,
                contents,
            });
        }

        // Cell G1 — Auth emission. Per-feature `auth` block lowered
        // to `auth.PasswordContract` / `SessionsContract` / `MfaContract`
        // / `OAuthContract` typed values in `auth.gen.go`.
        if let Some(contents) =
            emit_auth_file(&source_label, feature, &module_name, &cross_index)
        {
            let auth_path = format!("{name}/auth.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: auth_path,
                contents,
            });
        }

        // Cell G2a — Job emission. Per-feature `lazuli.JobContract`
        // values in `job.gen.go`.
        if let Some(contents) =
            emit_job_file(&source_label, feature, &module_name, &cross_index)
        {
            let job_path = format!("{name}/job.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: job_path,
                contents,
            });
        }

        // Cell G2b — Webhook v0 spine emission. Per-feature
        // `lazuli.WebhookContract` values in `webhook.gen.go`.
        if let Some(contents) =
            emit_webhook_file(&source_label, feature, &module_name, &cross_index)
        {
            let webhook_path = format!("{name}/webhook.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: webhook_path,
                contents,
            });
        }

        // Cell G2c — Notification v0 spine emission. Per-feature
        // `lazuli.NotificationContract` values in `notification.gen.go`.
        if let Some(contents) =
            emit_notification_file(&source_label, feature, &module_name, &cross_index)
        {
            let notification_path =
                format!("{name}/notification.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: notification_path,
                contents,
            });
        }

        // Cell G3a — Translation emission. Per-feature `i18n.Catalog`
        // + `//go:embed i18n/*.json` + `embed.FS` in `translation.gen.go`.
        // Now also emits `i18n/_placeholder.json` companion so the
        // `//go:embed` directive resolves at compile time (cell B1).
        files.extend(emit_translation_files(
            &source_label,
            feature,
            &module_name,
            &cross_index,
        ));

        // Cell G3b — EventGroup emission. Per-feature `lazuli.EventGroup`
        // values + payload structs in `events.gen.go`.
        if let Some(contents) =
            emit_events_file(&source_label, feature, &module_name, &cross_index)
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

        // Cell G5 — Api emission. Per-feature `lazuli.Api[I, O]`
        // values in `api.gen.go` (Lazuli Go lib gap §4.2 — emitter
        // ships TODO comments inside the value literal).
        if let Some(contents) =
            emit_api_file(&source_label, feature, &module_name, &cross_index)
        {
            let api_path = format!("{name}/api.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: api_path,
                contents,
            });
        }

        // Cell TM — TenantMigration emission. Per-feature
        // `migrations.MigrationContract` values in `migration.gen.go`.
        if let Some(contents) =
            emit_migration_file(&source_label, feature, &module_name, &cross_index)
        {
            let migration_path =
                format!("{name}/migration.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: migration_path,
                contents,
            });
        }
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

    files
}

fn resolve_module_name(module: &Module, options: &GoEmitOptions) -> String {
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

fn emit_go_mod(module_name: &str, lazuli_go_version: &str) -> String {
    let mut p = GoPrinter::new();
    p.line(&format!("module {}", module_name));
    p.blank();
    p.line(DEFAULT_GO_TOOLCHAIN);
    p.blank();
    p.line("require (");
    p.indent();
    // The Lazuli Go lib publishes a single Go module at
    // `lazuli.dev/runtime`; per-bucket subpackages (`auth`, `storage`,
    // `jobs`, the top-level `lazuli` package, ...) live under it. The
    // `require` clause names the module, not the subpackage; generated
    // imports reference `lazuli.dev/runtime/lazuli` and
    // `lazuli.dev/runtime/lazuli/<bucket>` paths against that module.
    p.line(&format!("lazuli.dev/runtime {}", lazuli_go_version));
    p.dedent();
    p.line(")");
    p.finish()
}

fn emit_feature_stub(source: &str, feature_name: &str) -> String {
    let mut p = GoPrinter::new();
    p.banner(source, feature_name);
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
        assert_eq!(to_kebab("hostpoint"), "hostpoint");
    }

    #[test]
    fn to_kebab_pascal_inserts_dashes() {
        assert_eq!(to_kebab("HostPoint"), "host-point");
    }

    #[test]
    fn to_kebab_handles_underscores_and_spaces() {
        assert_eq!(to_kebab("hello_world test"), "hello-world-test");
    }
}
