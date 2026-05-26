//! Root `main.go` emission. Walks the module once and emits the
//! singleton entry-point at the root of the generated Go tree:
//!
//! - Side-effect imports for every feature package so each feature's
//!   `init()` registers its resources/commands/queries with the
//!   Lazuli Go runtime registry.
//! - `func main()` body — boot the runtime via `lazuli.Boot(ctx, dbURL)`
//!   with the same shape the spike already proves in
//!   `dist/go/main.go`, then mount `lazuli.Mux()` and serve HTTP.
//!
//! Determinism:
//!
//! Side-effect imports are sorted by feature name via a `BTreeMap`
//! (matching the per-feature walker in `module.rs`).

use std::collections::BTreeMap;

use lazuli_ir::{AppObservability, Module};

use super::super::patterns::{PATTERN_MAIN_ENTRYPOINT, emit_pattern_header};
use super::super::printer::GoPrinter;
use super::helpers::format_string_slice;
use crate::LazuriteManifest;

/// Emit the root `main.go` for the module. Always returns a file —
/// even modules with zero features need a `main()` so `go build ./...`
/// has a binary entry point.
///
/// ## Examples
///
/// ```ignore
/// let go_src = emit_main_go(&module, "demo", "app.lzi", None);
/// assert!(go_src.contains("func main()"));
/// ```
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
    let mut handler_imports: BTreeMap<&str, String> = BTreeMap::new();
    let project_module = manifest
        .map(|m| m.project_module.trim())
        .filter(|m| !m.is_empty());
    if let Some(root) = project_module {
        // Only emit imports for features that actually declare handler
        // obligations (commands going through ReturnsFromRegistry,
        // @fn.X/@hook.X refs, etc.). Features without handlers do not
        // have an `app/features/<f>/handlers/` directory on disk, and
        // importing them would fail `go build`. The set is computed by
        // walking the same IR sites `emit_handler_stubs` uses.
        let features_with_handlers = super::super::handlers::features_with_handlers(module);
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
    // `Boot(ctx, lazuliApp)`; until then we read the DB URL with the
    // priority chain DATABASE_URL > LAZULI_DB > local Postgres default.
    //
    // DATABASE_URL is the universal convention (Heroku / Fly / Railway /
    // Render / Supabase / Neon / RDS — every Postgres provider managed
    // or self-hosted auto-injects this name). LAZULI_DB stays as a
    // legacy fallback so existing projects keep booting; the local
    // dev default lands when both are unset.
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
    p.line("dbURL := os.Getenv(\"DATABASE_URL\")");
    p.line("if dbURL == \"\" {");
    p.indent();
    p.line("dbURL = os.Getenv(\"LAZULI_DB\")");
    p.dedent();
    p.line("}");
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

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::Module;

    fn empty_module() -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: Vec::new(),
        }
    }

    #[test]
    fn empty_module_still_emits_main_func() {
        let go = emit_main_go(&empty_module(), "demo", "app.lzi", None);
        assert!(go.contains("func main()"));
    }
}
