//! Cell B4-runtime-facade — `app/app_integrations.gen.go` emission.
//! Walks `module.app.registry.integrations` (synthesised from
//! `registry.bindings.<name>: <Kind> / adapter @lazuli/plugin-<x>` sugar by
//! the app-manifest parser) and emits one `lazuli.RegisterAppIntegration`
//! call per binding so the runtime can resolve binding-name →
//! adapter at boot.
//!
//! The emitted file looks like:
//!
//! ```go
//! func init() {
//!     lazuli.RegisterAppIntegration("object_store", "@lazuli/plugin-object-store")
//! }
//! ```
//!
//! The two-step (adapter ref → binding name) keeps codegen wire-thin:
//! the plugin package's own `init()` populates the adapter registry
//! via `lazuli.RegisterAdapter`; the codegen-emitted call below
//! records the binding name (`object_store`) → adapter ref pair so
//! the runtime can resolve the binding at request time.
//!
//! Deferred resolution: the emitted call passes the adapter REF STRING
//! (not a `MustResolveAdapter`-resolved value). The runtime's
//! `ResolveAppIntegration` performs the adapter lookup lazily, at the
//! first facade call, so init() ordering between this file and each
//! plugin package no longer matters. See
//! `runtime/go/lazuli/app_integration.go` for the deferred mechanic.
//!
//! Skipped entirely when the module declares no `integrations`
//! carrying an adapter, so the output listing stays signal-rich.
//!
//! Spec: docs/proposals/hostpoint-complete-roadmap-2026-05-18.md §3.5.

use std::collections::BTreeMap;

use lazuli_ir::Module;

use super::imports::ImportSet;
use super::printer::GoPrinter;

/// File path emitted at the module root for `app/app_integrations.gen.go`.
pub const APP_INTEGRATIONS_PATH: &str = "app/app_integrations.gen.go";

/// Walk both `module.app.integrations` and
/// `module.registry.integrations` and emit one
/// `lazuli.RegisterAppIntegration` call per `(name, adapter)` pair.
/// Returns `None` when no integration carries an `adapter` —
/// integrations declared without a plugin binding (env-var-only
/// credentials) are not facade-resolvable and would emit an
/// `init()` that never wires anything.
pub fn emit_app_integrations(source_label: &str, module: &Module) -> Option<String> {
    // Two sources carry adapter references today:
    //
    //  1. `module.app.integrations` — the legacy `app <Name>`
    //     manifest `integrations` block.
    //  2. `module.registry.integrations` — the `registry` block
    //     in `registry.lzi`, including entries synthesised by the
    //     B1 `bindings` sugar (`bindings.<name>: <Kind> / adapter
    //     @lazuli/plugin-<x>`).
    //
    // The walker merges both into a BTreeMap keyed by name; later
    // entries (registry) win on collision so the sugar shape is
    // canonical. The legacy `AppBinding` (`bindings.<feature>.<slot>
    // = integrations.<name>`) is target-side and does not name the
    // adapter directly — we resolve through the integrations table.
    let mut by_name: BTreeMap<String, String> = BTreeMap::new();
    if let Some(app) = module.app.as_ref() {
        for integration in &app.integrations {
            if let Some(adapter) = integration.adapter.as_deref() {
                by_name.insert(integration.name.clone(), adapter.to_owned());
            }
        }
    }
    if let Some(registry) = module.registry.as_ref() {
        for integration in &registry.integrations {
            if let Some(adapter) = integration.adapter.as_deref() {
                by_name.insert(integration.name.clone(), adapter.to_owned());
            }
        }
    }
    if by_name.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    imports.add("lazuli.dev/runtime/lazuli");

    p.banner(source_label, "app");
    imports.emit(&mut p);
    p.blank();

    p.line("// App integrations registry — wires each `registry.bindings.<name>`");
    p.line("// declared in registry.lzi to the adapter shipped by the bound");
    p.line("// `@lazuli/plugin-<x>` package. Facades (`lazuli.ObjectStore`,");
    p.line("// `lazuli.PaymentGateway`, ...) resolve through this registry, so");
    p.line("// handlers reach `lazuli.ObjectStore(\"object_store\")` instead of");
    p.line("// reading `S3_ENDPOINT` env vars directly. See");
    p.line("// docs/proposals/hostpoint-complete-roadmap-2026-05-18.md §3.5.");
    p.line("//lazuli:pattern app_integration_register v1");
    p.line("func init() {");
    p.indent();
    for (name, adapter) in &by_name {
        // Deferred resolution: pass the adapter REF STRING. The
        // runtime's `ResolveAppIntegration` performs the adapter
        // lookup lazily so plugin package `init()`s can land in any
        // order. See `runtime/go/lazuli/app_integration.go`.
        p.line(&format!(
            "lazuli.RegisterAppIntegration({name:?}, {adapter:?})",
        ));
    }
    p.dedent();
    p.line("}");

    Some(p.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{AppIntegration, AppManifest, AppRegistry, Module};

    fn minimal_app(integrations: Vec<AppIntegration>) -> AppManifest {
        AppManifest {
            name: "test".to_owned(),
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
            integrations,
            capabilities: Vec::new(),
            runtime: Vec::new(),
            deploy: None,
            logging: None,
            tracing: None,
            observability: None,
            locale: None,
            encryption_bindings: Vec::new(),
            route_guard: None,
            actor_query: None,
            span_ref: None,
        }
    }

    fn integration(name: &str, kind: &str, adapter: Option<&str>) -> AppIntegration {
        AppIntegration {
            name: name.to_owned(),
            kind: kind.to_owned(),
            adapter: adapter.map(str::to_owned),
            adapter_provenance: adapter.map(|_| "plugin".to_owned()),
            environments: Vec::new(),
            credentials: None,
            data_classification: None,
        }
    }

    fn module_with(integrations: Vec<AppIntegration>) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(minimal_app(integrations)),
            registry: Some(AppRegistry {
                env: Vec::new(),
                integrations: Vec::new(),
                capabilities: Vec::new(),
                packs: Vec::new(),
                tools: Vec::new(),
                webhook_events: Vec::new(),
                secret_rotations: Vec::new(),
            }),
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: Vec::new(),
        }
    }

    /// Build a module whose adapters live in `registry.integrations`
    /// (the canonical home today — the B1 sugar lowers there).
    fn module_with_registry(integrations: Vec<AppIntegration>) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(minimal_app(Vec::new())),
            registry: Some(AppRegistry {
                env: Vec::new(),
                integrations,
                capabilities: Vec::new(),
                packs: Vec::new(),
                tools: Vec::new(),
                webhook_events: Vec::new(),
                secret_rotations: Vec::new(),
            }),
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: Vec::new(),
        }
    }

    #[test]
    fn no_integrations_emits_nothing() {
        let module = module_with(Vec::new());
        assert!(emit_app_integrations("test.lzi", &module).is_none());
    }

    #[test]
    fn integration_without_adapter_skipped() {
        // Legacy `integrations` entries without `adapter @lazuli/plugin-...`
        // (env-var-only credentials) cannot resolve a facade, so they
        // contribute zero RegisterAppIntegration calls.
        let module = module_with(vec![integration("payments", "PaymentGateway", None)]);
        assert!(emit_app_integrations("test.lzi", &module).is_none());
    }

    #[test]
    fn single_integration_with_adapter_emits_register_call() {
        let module = module_with(vec![integration(
            "object_store",
            "ObjectStore",
            Some("@lazuli/plugin-object-store"),
        )]);
        let out = emit_app_integrations("test.lzi", &module).expect("emits file");
        assert!(out.contains("package app"));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli\""));
        assert!(
            out.contains(
                "lazuli.RegisterAppIntegration(\"object_store\", \"@lazuli/plugin-object-store\")"
            )
        );
        // Deferred resolution invariant: emitter must NOT call
        // `MustResolveAdapter` at codegen-emit time. The init-order
        // panic class is closed precisely by avoiding eager resolution.
        assert!(
            !out.contains("MustResolveAdapter"),
            "emitter regression: codegen must pass adapter ref string, not call MustResolveAdapter:\n{out}"
        );
        assert!(out.contains("//lazuli:pattern app_integration_register v1"));
    }

    #[test]
    fn integration_from_registry_emits_register_call() {
        // The B1 `bindings` sugar in `registry.lzi` lowers into
        // `module.registry.integrations` (with adapter_provenance
        // = "plugin"). The emitter must surface those too — hostpoint
        // wires its plugin bindings exclusively through registry.lzi.
        let module = module_with_registry(vec![integration(
            "object_store",
            "ObjectStore",
            Some("@lazuli/plugin-object-store"),
        )]);
        let out = emit_app_integrations("test.lzi", &module).expect("emits file");
        assert!(
            out.contains(
                "lazuli.RegisterAppIntegration(\"object_store\", \"@lazuli/plugin-object-store\")"
            )
        );
        assert!(
            !out.contains("MustResolveAdapter"),
            "emitter regression: registry path must also pass ref string:\n{out}"
        );
    }

    #[test]
    fn multiple_integrations_emit_deterministic_order() {
        let module = module_with(vec![
            integration("zeta", "Z", Some("@lazuli/plugin-z")),
            integration("alpha", "A", Some("@lazuli/plugin-a")),
            integration("mid", "M", Some("@lazuli/plugin-m")),
        ]);
        let out = emit_app_integrations("test.lzi", &module).expect("emits file");
        // BTreeMap walk → alpha before mid before zeta.
        let alpha = out.find("\"alpha\"").expect("alpha line");
        let mid = out.find("\"mid\"").expect("mid line");
        let zeta = out.find("\"zeta\"").expect("zeta line");
        assert!(alpha < mid && mid < zeta, "alpha/mid/zeta order");
    }
}
