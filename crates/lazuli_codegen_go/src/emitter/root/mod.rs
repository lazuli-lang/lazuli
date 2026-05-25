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
//!     - `app.encryption` → `[]encryption.Binding` + `init()` register loop
//!     - `app.cors` → `lazuli.AppCors` + `init()` middleware register
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
//! ## Layout (Rails-style split — wave R6)
//!
//! - `main_go`     — `emit_main_go` + `emit_main_imports`
//! - `app_gen`     — `emit_lazuli_app_gen` + locale/logging/tracing/cors
//! - `encryption`  — `EncryptionBindings` + `init()` registration loop
//! - `helpers`     — duration parser, log/format token catalogs, render
//!                   helpers shared across the file emitters.

mod app_gen;
mod encryption;
mod helpers;
mod main_go;

pub use app_gen::emit_lazuli_app_gen;
pub use main_go::emit_main_go;

/// File path emitted at the root for `main.go`.
pub(crate) const MAIN_GO_PATH: &str = "main.go";

/// File path emitted at the root for `lazuli_app.gen.go`.
pub(crate) const LAZULI_APP_PATH: &str = "lazuli_app.gen.go";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LazuriteManifest;
    use helpers::parse_duration_to_seconds;
    use lazuli_ir::{
        AppCors, AppLocale, AppLogging, AppManifest, AppTracing, Defaults, EncryptionAlgorithm,
        EncryptionBinding, EncryptionRotation, EncryptionSource, EncryptionTemplate, Feature,
        LocaleFallback, Module, Policies,
    };
    use std::collections::BTreeMap;

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
            resume_routers: Vec::new(),
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
            synth_origins: std::collections::BTreeMap::new(),
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
            route_guard: None,
            actor_query: None,
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

    fn lazurite_manifest(project_module: &str) -> LazuriteManifest {
        LazuriteManifest {
            project_module: project_module.to_owned(),
            plugins: BTreeMap::new(),
            generate_go: None,
            dev: None,
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

    /// DB URL resolution must follow priority chain
    /// `DATABASE_URL` (universal convention) > `LAZULI_DB` (legacy) >
    /// local Postgres default. Regression net for the boot-time
    /// portability fix that lets Lazuli apps deploy unchanged on Fly,
    /// Railway, Render, Heroku, etc. — all of which auto-inject
    /// `DATABASE_URL` via their Postgres add-ons.
    #[test]
    fn main_go_db_url_prefers_database_url_then_lazuli_db_then_default() {
        let module = module_with(Vec::new(), Some(manifest("test_app")));
        let out = emit_main_go(&module, "lazuli/test-app", "test_app", None);
        // Both env-var names appear (priority pair).
        assert!(
            out.contains("os.Getenv(\"DATABASE_URL\")"),
            "main.go must read DATABASE_URL (universal convention)"
        );
        assert!(
            out.contains("os.Getenv(\"LAZULI_DB\")"),
            "main.go must keep LAZULI_DB as legacy fallback"
        );
        // Order: DATABASE_URL appears BEFORE LAZULI_DB (lookup priority).
        let database_url_idx = out.find("os.Getenv(\"DATABASE_URL\")").unwrap();
        let lazuli_db_idx = out.find("os.Getenv(\"LAZULI_DB\")").unwrap();
        assert!(
            database_url_idx < lazuli_db_idx,
            "DATABASE_URL lookup must come before LAZULI_DB fallback"
        );
        // Local-dev default still emitted as last-resort.
        assert!(out.contains("postgres://lazuli:lazuli@localhost:5432/lazuli"));
    }

    #[test]
    fn main_go_documents_registry_driven_http_mounting() {
        let module = module_with(vec![empty_feature("customer")], Some(manifest("test_app")));
        let out = emit_main_go(&module, "lazuli/test-app", "test_app", None);

        assert!(out.contains(
            "// Feature packages are imported above for init-time registry registration."
        ));
        assert!(
            out.contains("// lazuli.Mux() walks that registry and attaches command, query, and")
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
    fn main_go_skips_handler_import_for_feature_without_handler_obligation() {
        let module = module_with(vec![empty_feature("customer")], Some(manifest("test_app")));
        let manifest = lazurite_manifest("github.com/acme/test-app");
        let out = emit_main_go(
            &module,
            "github.com/acme/test-app/generated",
            "test_app",
            Some(&manifest),
        );

        assert!(
            !out.contains("github.com/acme/test-app/app/features/customer/handlers"),
            "feature without @fn/@hook/Returns handler obligation must not import handlers:\n{out}"
        );
    }

    #[test]
    fn main_go_imports_handler_package_for_fn_reference() {
        let mut customer = empty_feature("customer");
        customer.uses.push("@fn.risk_score".to_owned());
        let module = module_with(vec![customer], Some(manifest("test_app")));
        let manifest = lazurite_manifest("github.com/acme/test-app");
        let out = emit_main_go(
            &module,
            "github.com/acme/test-app/generated",
            "test_app",
            Some(&manifest),
        );

        assert!(
            out.contains("_ \"github.com/acme/test-app/app/features/customer/handlers\""),
            "feature with @fn reference must import handler package:\n{out}"
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
        assert!(out.contains("\"local\": {\"http://localhost:5173\", \"http://localhost:5174\"}"));
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
