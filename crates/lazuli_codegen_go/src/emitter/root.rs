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

use lazuli_ir::{
    AppCors, AppLocale, AppLogging, AppObservability, AppTracing, EncryptionAlgorithm,
    EncryptionBinding, EncryptionRotation, EncryptionSource, EncryptionTemplateAxis, Module,
};

use super::imports::ImportSet;
use super::patterns::{
    PATTERN_CORS_REGISTER, PATTERN_ENCRYPTION_REGISTER, PATTERN_MAIN_ENTRYPOINT,
    emit_pattern_header,
};
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

    // Side-effect imports for user-authored handler packages
    // (`app/features/<feature>/handlers/`, package `<feature>handlers`).
    // Each handler file ships a `func init() { lazuli.RegisterFn(
    // "<feature>.<name>", <FnName>) }` that the runtime registry
    // consults when `command.gen.go` emits
    // `Effect: lazuli.ReturnsFromRegistry[I, O]("<feature>.<name>")`.
    // Without this anonymous import, the init() never fires and
    // dispatch returns 500 "no handler registered" at first request.
    //
    // The handler package path uses the *project root* module
    // (`manifest.project_module`), not the codegen submodule
    // (`module_name`). In Lazurite submodule mode the gen lives at
    // `<project>/generated/<feature>` while handlers live at
    // `<project>/app/features/<feature>/handlers` — different modules
    // resolved through the workspace `go.work use` directive.
    //
    // Emitted unconditionally per feature: Go's `go build` accepts
    // anonymous imports of empty packages, and missing handler
    // packages surface as build errors that point straight at the
    // gap (vs. silent runtime failures from absent RegisterFn calls).
    let mut handler_imports: BTreeMap<&str, String> = BTreeMap::new();
    let project_module = manifest
        .map(|m| m.project_module.trim())
        .filter(|m| !m.is_empty());
    if let Some(root) = project_module {
        // Only emit imports for features that actually declare
        // handlers (commands going through ReturnsFromRegistry,
        // @fn.X/@hook.X refs, etc.) — features without handlers
        // don't have an `app/features/<f>/handlers/` directory on
        // disk and importing them would fail `go build`. The set
        // is computed by walking the same IR sites
        // `emit_handler_stubs` uses (single source of truth).
        let features_with_handlers = super::handlers::features_with_handlers(module);
        for feature in &module.features {
            if !features_with_handlers.contains(&feature.name) {
                continue;
            }
            let path = format!("{}/app/features/{}/handlers", root, feature.name);
            handler_imports.insert(feature.name.as_str(), path);
        }
    }

    // The codegen-emitted `app/` package (under `dist/go/app/`)
    // currently carries two init() registrations:
    //
    //   - `app/error_resolution.gen.go` — `lazuli.RegisterFeatureErrors`
    //     calls (per-feature errors block lowering).
    //   - `app/app_integrations.gen.go` — `lazuli.RegisterAppIntegration`
    //     calls (registry.bindings → adapter wiring for the
    //     `lazuli.ObjectStore(...)` facade etc.).
    //
    // Both files live in `package app` and depend on a side-effect
    // import from `main` to fire their init(). Emitted only when at
    // least one of those files would land — keeps main.go free of
    // dead imports for capsules without errors blocks or bindings.
    let has_app_errors = module.features.iter().any(|f| f.errors.is_some());
    let has_app_integrations = module
        .app
        .as_ref()
        .is_some_and(|a| a.integrations.iter().any(|i| i.adapter.is_some()))
        || module
            .registry
            .as_ref()
            .is_some_and(|r| r.integrations.iter().any(|i| i.adapter.is_some()));
    let emit_app_pkg_import = has_app_errors || has_app_integrations;

    p.banner(source_label, "main");

    // Side-effect imports require an `_` alias to bypass Go's
    // "unused import" check. The shared `ImportSet::emit` always
    // renders bare `"<path>"` rows, so we hand-roll the import block
    // here to preserve the side-effect form. The visual grouping
    // (stdlib / lazuli runtime / feature pkgs) mirrors the rest of
    // the codebase.
    emit_main_imports(
        &mut p,
        &feature_imports,
        &handler_imports,
        manifest,
        emit_app_pkg_import,
        module_name,
    );
    p.blank();

    // Boot block — the Lazuli Go lib's current `Boot(ctx, dbURL)`
    // signature opens shared runtime state, but does not itself mount
    // the HTTP registry. When the runtime team grows the
    // `lazuli.AppContract` umbrella the call changes to
    // `Boot(ctx, lazuliApp)`; until then we read the DB URL from
    // `LAZULI_DB` and fall back to a local Postgres default to keep the
    // smoke-test workflow ergonomic.
    emit_pattern_header(&mut p, PATTERN_MAIN_ENTRYPOINT);
    p.line("func main() {");
    p.indent();
    p.line("ctx := context.Background()");
    let default_observability = AppObservability::default();
    let error_sources = module
        .app
        .as_ref()
        .and_then(|app| app.observability.as_ref())
        .map(|observability| observability.error_source.as_slice())
        .unwrap_or(default_observability.error_source.as_slice());
    let panic_recover = module
        .app
        .as_ref()
        .and_then(|app| app.observability.as_ref())
        .map(|observability| observability.panic_recover)
        .unwrap_or(default_observability.panic_recover);
    p.line("ctx = lazuli.SetEnvironment(ctx, os.Getenv(\"LAZULI_ENV\"))");
    p.line(&format!(
        "ctx = lazuli.SetObservabilityPolicy(ctx, []string{{{}}})",
        format_string_slice(error_sources)
    ));
    p.line(&format!(
        "ctx = lazuli.SetPanicRecoverPolicy(ctx, {panic_recover})"
    ));
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
    p.line("// lazuli.Mux() walks that registry and attaches command, query, and");
    p.line("// healthz routes before the process starts accepting requests.");
    p.line("handler := lazuli.Mux()");
    p.blank();
    p.line("addr := os.Getenv(\"LAZULI_ADDR\")");
    p.line("if addr == \"\" {");
    p.indent();
    p.line("addr = \":8080\"");
    p.dedent();
    p.line("}");
    p.line("slog.Info(\"lazuli http listening\", \"addr\", addr)");
    p.line("if err := http.ListenAndServe(addr, handler); err != nil {");
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
    handler_imports: &BTreeMap<&str, String>,
    manifest: Option<&LazuriteManifest>,
    emit_app_pkg_import: bool,
    module_name: &str,
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
    if emit_app_pkg_import {
        p.blank();
        p.line("// App-level wiring package — `dist/go/app/` carries the");
        p.line("// codegen-emitted `RegisterFeatureErrors` + `RegisterAppIntegration`");
        p.line("// calls. Side-effect import fires those init() blocks at boot.");
        p.line(&format!("_ \"{}/app\"", module_name));
    }
    if !handler_imports.is_empty() {
        p.blank();
        p.line("// User-authored handler packages — init() blocks self-register");
        p.line("// with `lazuli.RegisterFn(...)` so generated command Effect");
        p.line("// `lazuli.ReturnsFromRegistry[I, O](\"<feature>.<name>\")` can resolve.");
        for (_name, path) in handler_imports {
            p.line(&format!("_ \"{}\"", path));
        }
    }
    if let Some(manifest) = manifest {
        // Plugin side-effect imports — one `_ "<go_module>"` per
        // declared plugin so the plugin package's `init()` runs and
        // populates the runtime adapter registry (`lazuli.RegisterAdapter`)
        // BEFORE the first facade call resolves. The go module path is
        // resolved by the CLI translator: for `Plugin::Remote { module,
        // .. }` it equals the Lazurite.toml `module`; for `Plugin::Local
        // { path }` it is read from `<path>/go.mod`'s first-line
        // `module ...` directive. Plugins where the CLI could not
        // resolve a go_module are skipped — the runtime surfaces them
        // as `ErrAdapterMissing` at facade resolve time, which points
        // straight at the missing plugin manifest / go.mod gap.
        //
        // Deferred resolution in `runtime/go/lazuli/app_integration.go`
        // means init() order between plugins and the codegen-emitted
        // `app/app_integrations.gen.go` does NOT matter — each plugin
        // can register its adapter independently and the facade calls
        // resolve at request time. The side-effect imports below are
        // the missing piece that brings the plugin packages into the
        // binary's transitive import graph in the first place.
        let plugin_modules: Vec<String> = manifest
            .plugins
            .iter()
            .filter_map(|(_ref_str, plugin)| plugin.go_module.clone())
            .collect();
        if !plugin_modules.is_empty() {
            p.blank();
            p.line("// Plugin imports — side-effect aliases so each plugin's package");
            p.line("// init() runs and calls lazuli.RegisterAdapter(...) into the");
            p.line("// runtime registry. Deferred resolution in the runtime means");
            p.line("// init order across plugins + app_integrations.gen.go no longer");
            p.line("// matters — see runtime/go/lazuli/app_integration.go.");
            for module in plugin_modules {
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
    // Encryption bucket cycle — emit `var EncryptionBindings = ...`
    // when the capsule declares one or more `encryption.key @key.<scope>`
    // bindings. Each binding wires to the runtime registry via an
    // `init()` block calling `encryption.Register(...)`. See
    // `docs/proposals/encryption-vocab.md` §Codegen.
    let emit_encryption = !manifest.encryption_bindings.is_empty();

    if !emit_locale
        && !emit_logging
        && !emit_tracing
        && !emit_name
        && !emit_cors_todo
        && !emit_encryption
    {
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
    if emit_encryption {
        imports.add("lazuli.dev/runtime/lazuli/encryption");
    }
    if emit_cors_todo {
        // `lazuli.AppCors` + `lazuli.SetCorsContract` live in the runtime root.
        imports.add("lazuli.dev/runtime/lazuli");
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
    if emit_encryption {
        maybe_blank(&mut p, &mut first_block);
        emit_encryption_bindings(&mut p, &manifest.encryption_bindings);
    }
    if emit_cors_todo {
        maybe_blank(&mut p, &mut first_block);
        emit_cors_contract(
            &mut p,
            manifest.cors.as_ref().expect("cors guarded"),
            manifest.locale.is_some(),
        );
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

/// Encryption bucket cycle — emit `var EncryptionBindings = ...` plus
/// the `init()` that registers each binding with the runtime
/// registry. The runtime side
/// (`runtime/go/lazuli/encryption/registry.go`) supplies
/// `encryption.Binding`, `encryption.Register`, and the closed
/// catalogs (`SourceEnv`/`SourceSecrets`, `AxisTenantID`/
/// `AxisUserID`/`AxisRecordID`, `AlgorithmAES256GCM`,
/// `RotationManual`).
///
/// Codegen never names AES, GCM, or any concrete crypto primitive —
/// the runtime resolves the cipher behind the `encryption.Binding`
/// catalog token. Per `docs/proposals/encryption-vocab.md` §Codegen,
/// the wire-thin principle: this emitter is ~50 LOC of `import + call`,
/// not a homegrown crypto envelope.
fn emit_encryption_bindings(p: &mut GoPrinter, bindings: &[EncryptionBinding]) {
    p.line(
        "// EncryptionBindings is the lowered `app.encryption` catalog from app.lzi.",
    );
    p.line(
        "// One entry per `@key.<scope>` referenced by any `@cap.Encrypted` /",
    );
    p.line("// `@cap.E2ee` field. The `init()` below registers each binding with");
    p.line("// the runtime registry so `encryption.For(ctx, \"@key.<scope>\")` resolves");
    p.line("// the per-tenant cipher on demand.");
    p.line("var EncryptionBindings = []encryption.Binding{");
    p.indent();
    for binding in bindings {
        emit_encryption_binding_literal(p, binding);
    }
    p.dedent();
    p.line("}");
    p.blank();
    emit_pattern_header(p, PATTERN_ENCRYPTION_REGISTER);
    p.line("func init() {");
    p.indent();
    p.line("for _, b := range EncryptionBindings {");
    p.indent();
    p.line("encryption.Register(b)");
    p.dedent();
    p.line("}");
    p.dedent();
    p.line("}");
}

/// Emit the lowered `app.cors` block as a `lazuli.AppCors` value
/// + `init()` that registers it with the runtime middleware via
/// `lazuli.SetCorsContract`. The runtime CORS middleware (wire of
/// `rs/cors`) consumes the registered contract at request time.
///
/// `app.cors`:
///   allow_origins production "https://app.example.com"
///   allow_origins local      "http://localhost:5173"
///   allow_credentials true
///   max_age "1h"
///
/// Emits as:
///   var CorsContract = lazuli.AppCors{
///       AllowOrigins: map[string][]string{
///           "production": {"https://app.example.com"},
///           "local":      {"http://localhost:5173"},
///       },
///       AllowCredentials: true,
///       MaxAge:           3600,
///   }
///   func init() { lazuli.SetCorsContract(&CorsContract) }
fn emit_cors_contract(p: &mut GoPrinter, cors: &AppCors, has_locale: bool) {
    p.line("// CorsContract is the lowered `app.cors` block from app.lzi.");
    p.line("// Origins are keyed by environment (matches `app.environments`).");
    p.line("// The runtime middleware resolves the active set against");
    p.line("// `LAZULI_ENV` at request time, wires `github.com/rs/cors` for");
    p.line("// preflight + Access-Control-* headers.");
    p.line("var CorsContract = lazuli.AppCors{");
    p.indent();
    if cors.allow_origins.is_empty() {
        p.line("AllowOrigins: map[string][]string{},");
    } else {
        p.line("AllowOrigins: map[string][]string{");
        p.indent();
        // Merge any duplicate environment entries (DSL allows multiple
        // `allow_origins <env>` lines per env).
        let mut by_env: std::collections::BTreeMap<&str, Vec<&str>> =
            std::collections::BTreeMap::new();
        for rule in &cors.allow_origins {
            let entry = by_env.entry(rule.environment.as_str()).or_default();
            for o in &rule.origins {
                entry.push(o.as_str());
            }
        }
        for (env, origins) in by_env {
            let origin_list = origins
                .into_iter()
                .map(|o| format!("{:?}", o))
                .collect::<Vec<_>>()
                .join(", ");
            p.line(&format!("{:?}: {{{}}},", env, origin_list));
        }
        p.dedent();
        p.line("},");
    }
    if cors.allow_credentials {
        p.line("AllowCredentials: true,");
    }
    if let Some(max_age) = cors.max_age.as_deref() {
        if let Some(seconds) = parse_duration_to_seconds(max_age) {
            p.line(&format!("MaxAge: {},", seconds));
        }
    }
    p.dedent();
    p.line("}");
    p.blank();
    emit_pattern_header(p, PATTERN_CORS_REGISTER);
    p.line("func init() {");
    p.indent();
    p.line("lazuli.SetCorsContract(&CorsContract)");
    if has_locale {
        // IR Error-Vocab — register the lowered locale contract so the
        // HTTP error boundary can negotiate `Accept-Language` against
        // the supported set + default. Without this call the resolver
        // falls back to `en-US` and the proposal's pt-BR floor never
        // reaches the wire on a default-Locale install (proposal §2.E).
        p.line("lazuli.RegisterAppLocaleContract(LocaleContract)")
    }
    p.dedent();
    p.line("}");
}

/// Parse a DSL duration literal ("1h", "30 minutes", "10s", "1d") to
/// seconds. Returns `None` on shapes the runtime middleware can't accept
/// (negative, zero, malformed). Mirror the small surface the proposal
/// commits to — extend additively as more unit shapes appear in pilots.
fn parse_duration_to_seconds(literal: &str) -> Option<i64> {
    let trimmed = literal.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Strip a leading numeric prefix (digits + optional decimal not supported).
    let (num_str, unit_str) = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .map(|i| (&trimmed[..i], trimmed[i..].trim()))
        .unwrap_or((trimmed, ""));
    let n: i64 = num_str.parse().ok()?;
    if n < 0 {
        return None;
    }
    let mult: i64 = match unit_str.to_ascii_lowercase().as_str() {
        "" | "s" | "sec" | "secs" | "second" | "seconds" => 1,
        "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3600,
        "d" | "day" | "days" => 86_400,
        _ => return None,
    };
    Some(n.saturating_mul(mult))
}

fn emit_encryption_binding_literal(p: &mut GoPrinter, binding: &EncryptionBinding) {
    p.line("{");
    p.indent();
    let mut rows: Vec<(String, String)> = Vec::new();
    rows.push(("Scope:".to_owned(), format!("{:?},", binding.scope)));
    let (source_const, template) = match &binding.source {
        EncryptionSource::Env(t) => ("encryption.SourceEnv", t),
        EncryptionSource::Secrets(t) => ("encryption.SourceSecrets", t),
    };
    rows.push(("Source:".to_owned(), format!("{},", source_const)));
    rows.push((
        "Template:".to_owned(),
        format!("{:?},", template.literal),
    ));
    let axis_consts: Vec<&'static str> = template
        .axes
        .iter()
        .map(|axis| encryption_axis_const(*axis))
        .collect();
    rows.push((
        "Axes:".to_owned(),
        format!("[]encryption.TemplateAxis{{{}}},", axis_consts.join(", ")),
    ));
    rows.push((
        "Algorithm:".to_owned(),
        format!("{},", encryption_algorithm_const(binding.algorithm)),
    ));
    rows.push((
        "Rotation:".to_owned(),
        format!("{},", encryption_rotation_const(binding.rotation)),
    ));
    emit_aligned_struct_value_rows(p, &rows);
    p.dedent();
    p.line("},");
}

fn encryption_axis_const(axis: EncryptionTemplateAxis) -> &'static str {
    match axis {
        EncryptionTemplateAxis::TenantId => "encryption.AxisTenantID",
        EncryptionTemplateAxis::UserId => "encryption.AxisUserID",
        EncryptionTemplateAxis::RecordId => "encryption.AxisRecordID",
    }
}

fn encryption_algorithm_const(algorithm: EncryptionAlgorithm) -> &'static str {
    match algorithm {
        EncryptionAlgorithm::Aes256Gcm => "encryption.AlgorithmAES256GCM",
    }
}

fn encryption_rotation_const(rotation: EncryptionRotation) -> &'static str {
    match rotation {
        EncryptionRotation::Manual => "encryption.RotationManual",
    }
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

fn format_string_slice(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("{value:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AppCors, AppLocale, AppLogging, AppManifest, AppTracing, Defaults, EncryptionAlgorithm,
        EncryptionBinding, EncryptionRotation, EncryptionSource, EncryptionTemplate, Feature,
        LocaleFallback, Module, Policies,
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
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
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
            errors: None,
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
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn manifest(name: &str) -> AppManifest {
        AppManifest {
            name: name.to_owned(),
            title: None,
            version: None,
            lazuli_version: None,
            targets: Vec::new(),
            default_locale: None,
            default_timezone: None,
            auth_failed_redirect: None,
            not_found: None,
            error_pages: Vec::new(),
            uses: Vec::new(),
            packs: Vec::new(),
            bindings: Vec::new(),
            architecture: None,
            services: Vec::new(),
            communication: None,
            environments: Vec::new(),
            urls: Vec::new(),
            cors: None,
            headers: None,
            cookie: None,
            proxy: None,
            limits: None,
            env: Vec::new(),
            integrations: Vec::new(),
            capabilities: Vec::new(),
            runtime: Vec::new(),
            deploy: None,
            logging: None,
            tracing: None,
            observability: None,
            locale: None,
            encryption_bindings: Vec::new(),
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
            design: None,
            rbac: None,
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
        assert!(out.contains("handler := lazuli.Mux()"));
        assert!(out.contains("http.ListenAndServe(addr, handler)"));
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
        assert!(
            out.contains(
                "// lazuli.Mux() walks that registry and attaches command, query, and"
            )
        );
        assert!(out.contains("// healthz routes before the process starts accepting requests."));
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
    fn lazuli_app_gen_emits_cors_contract() {
        let mut app = manifest("AcmeCRM");
        app.cors = Some(AppCors {
            allow_origins: vec![
                lazuli_ir::AppCorsOriginRule {
                    environment: "production".to_owned(),
                    origins: vec!["https://app.example.com".to_owned()],
                },
                lazuli_ir::AppCorsOriginRule {
                    environment: "local".to_owned(),
                    origins: vec![
                        "http://localhost:5173".to_owned(),
                        "http://localhost:5174".to_owned(),
                    ],
                },
            ],
            allow_credentials: true,
            max_age: Some("1h".to_owned()),
            span_ref: None,
        });
        let module = module_with(Vec::new(), Some(app));
        let out = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");

        // Real codegen now — no TODO marker, an actual lazuli.AppCors
        // value + init() that registers with the runtime middleware.
        assert!(
            out.contains("var CorsContract = lazuli.AppCors{"),
            "expected CorsContract var, got:\n{}",
            out
        );
        assert!(out.contains("\"production\": {\"https://app.example.com\"}"));
        assert!(
            out.contains("\"local\": {\"http://localhost:5173\", \"http://localhost:5174\"}")
        );
        assert!(out.contains("AllowCredentials: true,"));
        assert!(out.contains("MaxAge: 3600,"));
        assert!(out.contains("lazuli.SetCorsContract(&CorsContract)"));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli\""));
    }

    #[test]
    fn parse_duration_to_seconds_unit_table() {
        assert_eq!(parse_duration_to_seconds("30s"), Some(30));
        assert_eq!(parse_duration_to_seconds("10 minutes"), Some(600));
        assert_eq!(parse_duration_to_seconds("1h"), Some(3600));
        assert_eq!(parse_duration_to_seconds("2 hours"), Some(7200));
        assert_eq!(parse_duration_to_seconds("1d"), Some(86_400));
        assert_eq!(parse_duration_to_seconds("nonsense"), None);
        assert_eq!(parse_duration_to_seconds(""), None);
        assert_eq!(parse_duration_to_seconds("-5s"), None);
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

    // Encryption bucket cycle — `var EncryptionBindings` + `init()`
    // registers each binding with the runtime registry. The emitter
    // never names AES or any concrete crypto; it threads catalog
    // tokens (`encryption.AlgorithmAES256GCM`, etc.) through to the
    // runtime so the wire-thin principle holds.
    #[test]
    fn lazuli_app_gen_emits_encryption_bindings() {
        let mut app = manifest("AcmeCRM");
        app.encryption_bindings.push(EncryptionBinding {
            scope: "@key.tenant".to_owned(),
            source: EncryptionSource::Env(EncryptionTemplate::parse(
                "CRYPT_KEY_TENANT_{tenant_id}",
            )),
            algorithm: EncryptionAlgorithm::Aes256Gcm,
            rotation: EncryptionRotation::Manual,
            rotation_profile: None,
            span_ref: None,
        });
        let module = module_with(Vec::new(), Some(app));
        let out = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");

        // Import shows up.
        assert!(
            out.contains("\"lazuli.dev/runtime/lazuli/encryption\""),
            "expected encryption runtime import in lazuli_app.gen.go:\n{out}"
        );
        // Catalog declared.
        assert!(out.contains("var EncryptionBindings = []encryption.Binding{"));
        // Closed-catalog tokens — codegen never names AES, GCM, nonces.
        assert!(out.contains("Source:    encryption.SourceEnv,"));
        assert!(out.contains("Algorithm: encryption.AlgorithmAES256GCM,"));
        assert!(out.contains("Rotation:  encryption.RotationManual,"));
        // Template axis lifted from the literal.
        assert!(out.contains("Axes:      []encryption.TemplateAxis{encryption.AxisTenantID},"));
        // Template literal preserved verbatim for runtime substitution.
        assert!(out.contains("Template:  \"CRYPT_KEY_TENANT_{tenant_id}\","));
        // `init()` walks the catalog so registration happens at boot.
        assert!(out.contains("func init() {"));
        assert!(out.contains("encryption.Register(b)"));
    }

    #[test]
    fn lazuli_app_gen_no_bindings_omits_encryption_import() {
        let module = module_with(Vec::new(), Some(manifest("AcmeCRM")));
        let out = emit_lazuli_app_gen(&module, "AcmeCRM").expect("must emit");
        assert!(!out.contains("encryption.Binding"));
        assert!(!out.contains("encryption.Register"));
        assert!(!out.contains("lazuli/encryption"));
    }
}
