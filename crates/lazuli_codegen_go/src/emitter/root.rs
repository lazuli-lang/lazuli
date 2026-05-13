//! Cell I2 — Root module-level emission. Walks the module once and
//! emits the two singletons that live at the root of the generated
//! Go tree (alongside `go.mod`):
//!
//! - `main.go` — `func main()` entry point. Side-effect imports for
//!   every feature package so each feature's `init()` registers its
//!   resources/commands/queries with the Lazuli Go runtime registry.
//!   Calls `lazuli.Boot(ctx, dbURL)` with the same shape the spike
//!   already proves in `dist/go/main.go`, then gives the runtime a
//!   single registry-driven mount point before serving HTTP.
//! - `lazuli_app.gen.go` — App-level contract values. Lowers
//!   `module.app: Option<AppManifest>` into per-bucket contract
//!   declarations using the contract types the Lazuli Go lib already
//!   exposes:
//!     - `app.locale` → `i18n.LocaleContract`
//!     - `app.logging` → `observability.LoggingContract`
//!     - `app.tracing` → `observability.TracingContract`
//!
//! Proposal references:
//! - §3.13 — Root-level files table.
//! - §3.13.1 — `app.routes` lowering (deferred; `lazuli.AppContract`
//!   wrapper + `lazuli.AppRoute` / `lazuli.AppCors` types do not exist
//!   on the Lazuli Go lib yet, so we emit TODO comments and skip the
//!   missing fields gracefully per the cell I2 brief). The shape the
//!   proposal sketches lands here once the runtime team adds the
//!   `lazuli.AppContract` umbrella.
//! - §5.1 — root layout (`go.mod`, `main.go`, `lazuli_app.gen.go`,
//!   per-feature dirs).
//! - §11 — boundary discipline: emitter never edits
//!   `runtime/go/lazuli/**`; missing types are surfaced as TODO
//!   comments, never silently faked.
//!
//! ## Determinism
//!
//! Side-effect imports are sorted by feature name via a `BTreeMap`
//! (matching the per-feature walker in `module.rs`). Sub-contract
//! emission walks `module.app: Option<AppManifest>` in a fixed field
//! order — features may flip `app: None` on or off but the field
//! ordering inside the file is wall-clock stable.

use std::collections::BTreeMap;

use lazuli_ir::{AppLocale, AppLogging, AppTracing, Module};

use super::imports::ImportSet;
use super::printer::GoPrinter;
use crate::LazuriteManifest;

/// File path emitted at the root for `main.go`.
pub(crate) const MAIN_GO_PATH: &str = "main.go";

/// File path emitted at the root for `lazuli_app.gen.go`.
pub(crate) const LAZULI_APP_PATH: &str = "lazuli_app.gen.go";

/// Emit the root `main.go` for the module. Always returns a file —
/// even modules with zero features need a `main()` so `go build ./...`
/// has a binary entry point.
pub fn emit_main_go(
    module: &Module,
    module_name: &str,
    source_label: &str,
    manifest: Option<&LazuriteManifest>,
) -> String {
    let mut p = GoPrinter::new();

    // Side-effect imports per feature. `BTreeMap` makes iteration
    // order independent of IR `Vec` insertion order, matching the
    // per-feature walker in `module.rs`. Each feature's `init()`
    // registers its resources / commands / queries with the runtime
    // global registry (`lazuli.Register(...)`).
    let mut feature_imports: BTreeMap<&str, String> = BTreeMap::new();
    for feature in &module.features {
        let path = format!("{}/{}", module_name, feature.name);
        feature_imports.insert(feature.name.as_str(), path);
    }

    p.banner(source_label, "main");

    // Side-effect imports require an `_` alias to bypass Go's
    // "unused import" check. The shared `ImportSet::emit` always
    // renders bare `"<path>"` rows, so we hand-roll the import block
    // here to preserve the side-effect form. The visual grouping
    // (stdlib / lazuli runtime / feature pkgs) mirrors the rest of
    // the codebase.
    emit_main_imports(&mut p, &feature_imports, manifest);
    p.blank();

    // Boot block — the Lazuli Go lib's current `Boot(ctx, dbURL)`
    // signature opens shared runtime state, but does not itself mount
    // the HTTP registry. When the runtime team grows the
    // `lazuli.AppContract` umbrella the call changes to
    // `Boot(ctx, lazuliApp)`; until then we read the DB URL from
    // `LAZULI_DB` and fall back to a local Postgres default to keep the
    // smoke-test workflow ergonomic.
    p.line("func main() {");
    p.indent();
    p.line("ctx := context.Background()");
    p.line("slog.SetDefault(slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo})))");
    p.blank();
    p.line("dbURL := os.Getenv(\"LAZULI_DB\")");
    p.line("if dbURL == \"\" {");
    p.indent();
    p.line("dbURL = \"postgres://lazuli:lazuli@localhost:5432/lazuli?sslmode=disable\"");
    p.dedent();
    p.line("}");
    p.blank();
    p.line("if err := lazuli.Boot(ctx, dbURL); err != nil {");
    p.indent();
    p.line("slog.Error(\"lazuli boot failed\", \"error\", err)");
    p.line("os.Exit(1)");
    p.dedent();
    p.line("}");
    p.blank();
    p.line("slog.Info(\"lazuli runtime booted\",");
    p.indent();
    p.line("\"resources\", len(lazuli.Resources()),");
    p.line("\"commands\", len(lazuli.Commands()),");
    p.line("\"queries\", len(lazuli.Queries()),");
    p.dedent();
    p.line(")");
    p.blank();
    p.line("// Feature packages are imported above for init-time registry registration.");
    p.line("// MountAll walks that registry and attaches API, command, and agent HTTP");
    p.line("// handlers before the process starts accepting requests.");
    p.line("mux := lazuli.Mux()");
    p.line("lazuli.MountAll(mux)");
    p.blank();
    p.line("addr := os.Getenv(\"LAZULI_ADDR\")");
    p.line("if addr == \"\" {");
    p.indent();
    p.line("addr = \":8080\"");
    p.dedent();
    p.line("}");
    p.line("slog.Info(\"lazuli http listening\", \"addr\", addr)");
    p.line("if err := http.ListenAndServe(addr, mux); err != nil {");
    p.indent();
    p.line("slog.Error(\"lazuli http server exited\", \"error\", err)");
    p.line("os.Exit(1)");
    p.dedent();
    p.line("}");
    p.dedent();
    p.line("}");

    p.finish()
}

/// Hand-rolled import block for `main.go`. The shared `ImportSet`
/// doesn't model side-effect (`_`) aliases — feature packages are
/// imported for their `init()` registrations only, never referenced
/// by name. We render the three groups manually with the alias
/// applied to the feature-pkg rows. Feature paths arrive already
/// joined with `<module_name>/<feature>` so this helper only needs
/// the per-feature `<path>` values.
fn emit_main_imports(
    p: &mut GoPrinter,
    feature_imports: &BTreeMap<&str, String>,
    manifest: Option<&LazuriteManifest>,
) {
    p.line("import (");
    p.indent();
    // Stdlib group — fixed entries known at compile time.
    p.line("\"context\"");
    p.line("\"log/slog\"");
    p.line("\"net/http\"");
    p.line("\"os\"");
    p.blank();
    // Lazuli runtime group — only the top-level `lazuli` package
    // (`Boot`, `Resources`, `Mux`, ...). Sub-packages would land here
    // when a future surface (e.g. plugin registration) needs them.
    p.line("\"lazuli.dev/runtime/lazuli\"");
    if !feature_imports.is_empty() {
        p.blank();
        for (_name, path) in feature_imports {
            // `_` alias triggers init() without exposing identifiers.
            p.line(&format!("_ \"{}\"", path));
        }
    }
    if let Some(manifest) = manifest {
        let plugins = manifest
            .plugins
            .iter()
            .filter_map(|(_ref_str, plugin)| plugin.module.as_deref())
            .collect::<Vec<_>>();
        if !plugins.is_empty() {
            p.blank();
            p.line("// Plugin imports - registered at init time via lazuli.RegisterAdapter.");
            for module in plugins {
                p.line(&format!("_ \"{}\"", module));
            }
        }
    }
    p.dedent();
    p.line(")");
}

/// Emit `lazuli_app.gen.go` for the module. Walks
/// `module.app: Option<AppManifest>` and lowers the sub-blocks the
/// Lazuli Go lib already models into per-contract `var` declarations.
/// Returns `None` when no `AppManifest` is declared AND no sub-block
/// would render — keeps the listing signal-rich.
pub fn emit_lazuli_app_gen(module: &Module, source_label: &str) -> Option<String> {
    let manifest = module.app.as_ref()?;

    // Pre-walk: which sub-blocks will we emit? Used both to decide
    // whether to bail out entirely (no observable surface → no file)
    // and to populate the per-block import set.
    let emit_locale = manifest.locale.is_some();
    let emit_logging = manifest.logging.is_some();
    let emit_tracing = manifest.tracing.is_some();
    let emit_name = !manifest.name.trim().is_empty();
    let emit_cors_todo = manifest.cors.is_some();
    let emit_routes_todo = false; // `Module.app.routes` is on `ExperienceModule`, not `AppManifest`.

    if !emit_locale && !emit_logging && !emit_tracing && !emit_name && !emit_cors_todo {
        return None;
    }

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    if emit_locale {
        imports.add("lazuli.dev/runtime/lazuli/i18n");
    }
    if emit_logging || emit_tracing {
        imports.add("lazuli.dev/runtime/lazuli/observability");
    }

    p.banner(source_label, "main");
    if !imports.is_empty() {
        imports.emit(&mut p);
        p.blank();
    }

    // Blank-line BETWEEN blocks, never after the last one — `gofmt`
    // strips trailing newlines from composite files and we want
    // byte-equivalent output without invoking the formatter
    // (proposal §11 invariant). `first_block` toggles after the first
    // emission lands.
    let mut first_block = true;
    let maybe_blank = |p: &mut GoPrinter, first_block: &mut bool| {
        if *first_block {
            *first_block = false;
        } else {
            p.blank();
        }
    };

    // `AppName` constant lands first so reading the file top-down
    // surfaces the app identity before the per-bucket contracts.
    if emit_name {
        maybe_blank(&mut p, &mut first_block);
        p.line("// AppName is the lowered `app <Name>` identifier from app.lzi.");
        p.line(&format!("const AppName = {:?}", manifest.name));
    }

    if let Some(locale) = manifest.locale.as_ref() {
        maybe_blank(&mut p, &mut first_block);
        emit_locale_contract(&mut p, locale);
    }
    if let Some(logging) = manifest.logging.as_ref() {
        maybe_blank(&mut p, &mut first_block);
        emit_logging_contract(&mut p, logging);
    }
    if let Some(tracing) = manifest.tracing.as_ref() {
        maybe_blank(&mut p, &mut first_block);
        emit_tracing_contract(&mut p, tracing);
    }
    if emit_cors_todo {
        maybe_blank(&mut p, &mut first_block);
        // `app.cors` lives on `ir::AppCors` but the Lazuli Go lib does
        // not yet expose a `lazuli.CorsContract`. Surface the gap so the
        // runtime team can land the matching shape (proposal §3.13.1
        // sketches `lazuli.AppCors`). When the type exists this block
        // upgrades to a real `var CorsContract = lazuli.CorsContract{...}`.
        p.line("// TODO(runtime): emit `var CorsContract = lazuli.CorsContract{...}` once");
        p.line("// the Lazuli Go lib exposes the typed CORS shape sketched in");
        p.line("// docs/proposals/codegen-lazuli-go.md §3.13.1. `module.app.cors` is");
        p.line("// declared but no matching runtime type exists yet.");
    }
    if emit_routes_todo {
        maybe_blank(&mut p, &mut first_block);
        // Top-level `app.routes` declarations live on `ExperienceModule`,
        // not the backend `Module`. When the experience layer threads
        // routes back into the backend module (Phase L Tier 4 follow-up
        // continuation) this block upgrades to a real
        // `var Routes = []lazuli.AppRoute{...}`.
        p.line("// TODO(runtime): emit `var Routes = []lazuli.AppRoute{...}` once");
        p.line("// `Module.app.routes` lifts from `ExperienceModule`. The shape");
        p.line("// lives in docs/proposals/codegen-lazuli-go.md §3.13.1.");
    }

    Some(p.finish())
}

fn emit_locale_contract(p: &mut GoPrinter, locale: &AppLocale) {
    p.line("// LocaleContract is the lowered `app.locale` block from app.lzi.");
    p.line("// Codegen surfaces the typed catalog so the runtime negotiation");
    p.line("// middleware can resolve a request locale without re-parsing the IR.");
    p.line("var LocaleContract = i18n.LocaleContract{");
    p.indent();
    p.line(&format!("Default: {:?},", locale.default));
    if !locale.supported.is_empty() {
        p.line("Supported: []string{");
        p.indent();
        for tag in &locale.supported {
            p.line(&format!("{:?},", tag));
        }
        p.dedent();
        p.line("},");
    }
    if !locale.fallbacks.is_empty() {
        p.line("Fallbacks: []i18n.Fallback{");
        p.indent();
        for fb in &locale.fallbacks {
            p.line(&format!("{{From: {:?}, To: {:?}}},", fb.from, fb.to));
        }
        p.dedent();
        p.line("},");
    }
    p.dedent();
    p.line("}");
}

fn emit_logging_contract(p: &mut GoPrinter, logging: &AppLogging) {
    p.line("// LoggingContract is the lowered `app.logging` block from app.lzi.");
    p.line("// The runtime materialises the slog handler stack from this contract;");
    p.line("// adapter selection lives in registry.capabilities.");
    p.line("var LoggingContract = observability.LoggingContract{");
    p.indent();
    // Gather rows first so we can column-align `Key:` widths the way
    // `gofmt` would. Mirrors `resource.rs`'s kv-row padding approach.
    let mut rows: Vec<(String, String)> = Vec::new();
    if let Some(level) = logging.level.as_deref() {
        rows.push((
            "Level:".to_owned(),
            format!("observability.{},", log_level_const(level)),
        ));
    }
    if let Some(format) = logging.format.as_deref() {
        rows.push((
            "Format:".to_owned(),
            format!("observability.{},", log_format_const(format)),
        ));
    }
    if let Some(redact) = logging.redact.as_deref() {
        rows.push((
            "Redact:".to_owned(),
            format!("observability.{},", redact_strategy_const(redact)),
        ));
    }
    if let Some(rate) = logging.sample_rate {
        rows.push(("SampleRate:".to_owned(), format!("{},", format_f64(rate))));
    }
    emit_aligned_struct_value_rows(p, &rows);
    p.dedent();
    p.line("}");
}

fn emit_tracing_contract(p: &mut GoPrinter, tracing: &AppTracing) {
    p.line("// TracingContract is the lowered `app.tracing` block from app.lzi.");
    p.line("// The runtime resolves the exporter adapter at boot; `Propagate` and");
    p.line("// `SampleRate` drive the head-sampling decision.");
    p.line("var TracingContract = observability.TracingContract{");
    p.indent();
    let mut rows: Vec<(String, String)> = Vec::new();
    if let Some(propagate) = tracing.propagate {
        rows.push(("Propagate:".to_owned(), format!("{},", propagate)));
    }
    if let Some(rate) = tracing.sample_rate {
        rows.push(("SampleRate:".to_owned(), format!("{},", format_f64(rate))));
    }
    if let Some(exporter) = tracing.exporter.as_deref() {
        // Adapter refs (`@adapter.otlp`) are emitted verbatim — the
        // runtime resolves them at boot via `RegisterAdapter`.
        rows.push(("Exporter:".to_owned(), format!("{:?},", exporter)));
    }
    emit_aligned_struct_value_rows(p, &rows);
    p.dedent();
    p.line("}");
}

/// Render `Key: value,` struct-literal rows with the keys padded to
/// the widest key in the block. Matches `gofmt`'s alignment rule for
/// composite-literal initialisers so the emitter output passes a
/// `gofmt -d` diff cleanly.
fn emit_aligned_struct_value_rows(p: &mut GoPrinter, rows: &[(String, String)]) {
    if rows.is_empty() {
        return;
    }
    let key_width = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, value) in rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }
}

/// Map the closed-catalog `app.logging.level` token to the Lazuli Go
/// lib's `observability.LogLevel*` constant. Doctor enforces the
/// catalog at compile time; unknown tokens fall back to `LogLevelInfo`
/// (the runtime default) plus a defensive comment so the file still
/// compiles when an upstream stage drifts.
fn log_level_const(token: &str) -> &'static str {
    match token {
        "debug" => "LogLevelDebug",
        "info" => "LogLevelInfo",
        "warn" => "LogLevelWarn",
        "error" => "LogLevelError",
        _ => "LogLevelInfo",
    }
}

fn log_format_const(token: &str) -> &'static str {
    match token {
        "json" => "LogFormatJSON",
        "text" => "LogFormatText",
        _ => "LogFormatJSON",
    }
}

fn redact_strategy_const(token: &str) -> &'static str {
    match token {
        "pii" => "RedactPII",
        "none" => "RedactNone",
        _ => "RedactPII",
    }
}

/// Render an `f64` as a Go literal. We always emit at least one
/// decimal place so the literal is unambiguously a float (Go's `1`
/// would be an untyped int that the field decl couldn't accept
/// without conversion).
fn format_f64(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{:.1}", value)
    } else {
        // `{}` for `f64` produces the shortest round-trip form, which
        // is what we want for deterministic output. The trailing
        // newline/precision is decided by the formatter; no extra
        // padding needed.
        format!("{}", value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AppCors, AppLocale, AppLogging, AppManifest, AppTracing, Defaults, Feature, LocaleFallback,
        Module, Policies,
    };

    fn empty_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies {
                categories: Vec::new(),
                fields: Vec::new(),
                span_ref: None,
            },
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn manifest(name: &str) -> AppManifest {
        AppManifest {
            name: name.to_owned(),
            title: None,
            version: None,
            targets: Vec::new(),
            default_locale: None,
            default_timezone: None,
            auth_failed_redirect: None,
            not_found: None,
            uses: Vec::new(),
            packs: Vec::new(),
            bindings: Vec::new(),
            architecture: None,
            services: Vec::new(),
            communication: None,
            environments: Vec::new(),
            urls: Vec::new(),
            cors: None,
            env: Vec::new(),
            integrations: Vec::new(),
            capabilities: Vec::new(),
            runtime: Vec::new(),
            deploy: None,
            logging: None,
            tracing: None,
            locale: None,
            span_ref: None,
        }
    }

    fn module_with(features: Vec<Feature>, app: Option<AppManifest>) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app,
            registry: None,
            profiles: Vec::new(),
            features,
        }
    }

    #[test]
    fn main_go_empty_module_emits_boot_skeleton() {
        let module = module_with(Vec::new(), Some(manifest("test_app")));
        let out = emit_main_go(&module, "lazuli/test-app", "test_app", None);
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("\npackage main\n"));
        assert!(out.contains("\"context\""));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli\""));
        assert!(out.contains("func main() {"));
        assert!(out.contains("lazuli.Boot(ctx, dbURL)"));
        assert!(out.contains("mux := lazuli.Mux()"));
        assert!(out.contains("lazuli.MountAll(mux)"));
        assert!(out.contains("http.ListenAndServe(addr, mux)"));
        assert!(!out.contains("http.ListenAndServe(addr, lazuli.Mux())"));
        // No feature side-effect imports when the module has no features.
        assert!(!out.contains("_ \"lazuli/"));
    }

    #[test]
    fn main_go_documents_registry_driven_http_mounting() {
        let module = module_with(vec![empty_feature("customer")], Some(manifest("test_app")));
        let out = emit_main_go(&module, "lazuli/test-app", "test_app", None);

        assert!(out.contains(
            "// Feature packages are imported above for init-time registry registration."
        ));
        assert!(out
            .contains("// MountAll walks that registry and attaches API, command, and agent HTTP"));
        assert!(out.contains("// handlers before the process starts accepting requests."));
    }

    #[test]
    fn main_go_two_features_emits_sorted_side_effect_imports() {
        let module = module_with(
            vec![empty_feature("zebra"), empty_feature("alpha")],
            Some(manifest("test_app")),
        );
        let out = emit_main_go(&module, "lazuli/test-app", "test_app", None);

        // Both feature pkg paths present, with `_` alias.
        assert!(out.contains("_ \"lazuli/test-app/alpha\""));
        assert!(out.contains("_ \"lazuli/test-app/zebra\""));

        // Lexicographic ordering: alpha before zebra.
        let alpha = out.find("alpha").expect("alpha import present");
        let zebra = out.find("zebra").expect("zebra import present");
        assert!(
            alpha < zebra,
            "expected alpha import before zebra:\n{}",
            out
        );
    }

    #[test]
    fn lazuli_app_gen_emits_app_name_for_minimal_manifest() {
        let module = module_with(Vec::new(), Some(manifest("AcmeCRM")));
        let out = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");
        assert!(out.contains("package main"));
        assert!(
            out.contains("const AppName = \"AcmeCRM\""),
            "expected `AppName` constant in lazuli_app.gen.go:\n{}",
            out
        );
    }

    #[test]
    fn lazuli_app_gen_emits_locale_contract() {
        let mut app = manifest("AcmeCRM");
        app.locale = Some(AppLocale {
            default: "pt-BR".to_owned(),
            supported: vec!["pt-BR".to_owned(), "en-US".to_owned()],
            fallbacks: vec![LocaleFallback {
                from: "en-US".to_owned(),
                to: "pt-BR".to_owned(),
            }],
        });
        let module = module_with(Vec::new(), Some(app));
        let out = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");

        assert!(out.contains("\"lazuli.dev/runtime/lazuli/i18n\""));
        assert!(out.contains("var LocaleContract = i18n.LocaleContract{"));
        assert!(out.contains("Default: \"pt-BR\","));
        assert!(out.contains("\"pt-BR\","));
        assert!(out.contains("\"en-US\","));
        assert!(out.contains("[]i18n.Fallback{"));
        assert!(out.contains("{From: \"en-US\", To: \"pt-BR\"},"));
    }

    #[test]
    fn lazuli_app_gen_emits_logging_contract_with_known_constants() {
        let mut app = manifest("AcmeCRM");
        app.logging = Some(AppLogging {
            level: Some("info".to_owned()),
            format: Some("json".to_owned()),
            redact: Some("pii".to_owned()),
            sample_rate: Some(0.5),
            span_ref: None,
        });
        let module = module_with(Vec::new(), Some(app));
        let out = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");

        assert!(out.contains("\"lazuli.dev/runtime/lazuli/observability\""));
        assert!(out.contains("var LoggingContract = observability.LoggingContract{"));
        // Keys are padded to the widest in the block (`SampleRate:` at
        // 11 chars). Sample-rate stays unpadded; the others get
        // trailing spaces before the value column.
        assert!(out.contains("Level:      observability.LogLevelInfo,"));
        assert!(out.contains("Format:     observability.LogFormatJSON,"));
        assert!(out.contains("Redact:     observability.RedactPII,"));
        assert!(out.contains("SampleRate: 0.5,"));
    }

    #[test]
    fn lazuli_app_gen_emits_tracing_contract_with_exporter() {
        let mut app = manifest("AcmeCRM");
        app.tracing = Some(AppTracing {
            propagate: Some(true),
            sample_rate: Some(0.05),
            exporter: Some("@adapter.otlp".to_owned()),
            span_ref: None,
        });
        let module = module_with(Vec::new(), Some(app));
        let out = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");

        assert!(out.contains("var TracingContract = observability.TracingContract{"));
        // `SampleRate:` (11 chars) is the widest key — `Propagate:`
        // (10 chars) and `Exporter:` (9 chars) pad accordingly.
        assert!(out.contains("Propagate:  true,"));
        assert!(out.contains("SampleRate: 0.05,"));
        assert!(out.contains("Exporter:   \"@adapter.otlp\","));
    }

    #[test]
    fn lazuli_app_gen_flags_cors_as_todo() {
        let mut app = manifest("AcmeCRM");
        app.cors = Some(AppCors {
            allow_origins: Vec::new(),
            allow_credentials: false,
            max_age: None,
            span_ref: None,
        });
        let module = module_with(Vec::new(), Some(app));
        let out = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");

        // CORS Go contract doesn't exist yet — emit TODO comments,
        // never a fake type ref.
        assert!(
            out.contains("TODO(runtime): emit `var CorsContract"),
            "expected CORS TODO comment, got:\n{}",
            out
        );
    }

    #[test]
    fn lazuli_app_gen_skips_file_when_manifest_absent() {
        let module = module_with(Vec::new(), None);
        assert!(emit_lazuli_app_gen(&module, "X").is_none());
    }

    #[test]
    fn lazuli_app_gen_skips_file_when_manifest_empty() {
        // Manifest with empty name + no sub-blocks → nothing meaningful
        // to render. The minimal `name = ""` case shouldn't materialise
        // an empty file with only the banner.
        let mut app = manifest("");
        app.locale = None;
        app.logging = None;
        app.tracing = None;
        let module = module_with(Vec::new(), Some(app));
        assert!(
            emit_lazuli_app_gen(&module, "anonymous").is_none(),
            "expected None when manifest carries no observable surface"
        );
    }

    #[test]
    fn deterministic_main_go_across_runs() {
        let module = module_with(
            vec![
                empty_feature("zebra"),
                empty_feature("alpha"),
                empty_feature("mango"),
            ],
            Some(manifest("test_app")),
        );
        let a = emit_main_go(&module, "lazuli/test-app", "test_app", None);
        let b = emit_main_go(&module, "lazuli/test-app", "test_app", None);
        assert_eq!(a, b);
    }

    #[test]
    fn deterministic_lazuli_app_gen_across_runs() {
        let mut app = manifest("AcmeCRM");
        app.locale = Some(AppLocale {
            default: "pt-BR".to_owned(),
            supported: vec!["pt-BR".to_owned(), "en-US".to_owned()],
            fallbacks: Vec::new(),
        });
        app.logging = Some(AppLogging {
            level: Some("info".to_owned()),
            format: Some("json".to_owned()),
            redact: None,
            sample_rate: None,
            span_ref: None,
        });
        let module = module_with(Vec::new(), Some(app));
        let a = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");
        let b = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");
        assert_eq!(a, b);
    }

    #[test]
    fn unknown_log_level_falls_back_to_info() {
        let mut app = manifest("AcmeCRM");
        app.logging = Some(AppLogging {
            level: Some("verbose".to_owned()), // not in closed catalog
            format: None,
            redact: None,
            sample_rate: None,
            span_ref: None,
        });
        let module = module_with(Vec::new(), Some(app));
        let out = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");
        // Unknown token defers to the runtime default constant — the
        // file always compiles. Doctor surfaces the structural smell.
        // The block only has one key here, so no padding is needed.
        assert!(out.contains("Level: observability.LogLevelInfo,"));
    }
}
