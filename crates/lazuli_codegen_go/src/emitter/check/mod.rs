//! `lazuli generate go --check` closed error catalog.
//!
//! The pass is intentionally codegen-local: it catches refs that the
//! Go emitter cannot resolve without reaching into doctor or the
//! filesystem. Discovery-backed checks (`@adapter.*` register sites
//! and `@fn.*` stubs) are recognized here but remain no-op stubs until
//! extension discovery lands.

use std::collections::BTreeSet;

use lazuli_ir::Module;

mod refs;
mod registries;

use refs::collect_feature_refs;
use registries::{
    declared_plugin_names, known_cap_ref, known_runtime_ref, known_semantic_ref, plugin_declared,
};

pub const CODE_PLUGIN: &str = "CODEGEN-GO-PLUGIN-001";
pub const CODE_UNRESOLVED: &str = "CODEGEN-GO-UNRESOLVED-002";
pub const CODE_ADAPTER: &str = "CODEGEN-GO-ADAPTER-003";
pub const CODE_SEMANTIC: &str = "CODEGEN-GO-SEMANTIC-004";
pub const CODE_CAP: &str = "CODEGEN-GO-CAP-005";
pub const CODE_FN: &str = "CODEGEN-GO-FN-006";
/// `TypeRef::Unresolved` with a non-`@` raw name reached the codegen.
/// The analyzer could not resolve a record/resource/enum reference, so
/// the emitter would either inline a Go-invalid name or silently fall
/// back to a placeholder. Fail loudly instead so users see broken
/// references at `lazuli generate go` time, not at runtime.
pub const CODE_TYPE_UNRESOLVED: &str = "CODEGEN-GO-TYPE-007";

/// Synthetic ref literal used to flag a `TypeRef::Unresolved(raw)` so
/// `run_checks` can emit `CODE_TYPE_UNRESOLVED` without changing the
/// signature of the recursive collectors. The prefix is illegal in any
/// authored DSL (no `@` host, double underscore) so it cannot collide
/// with a real reference.
pub(super) const UNRESOLVED_TYPE_PREFIX: &str = "__lazuli_type_unresolved__/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckIssue {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub feature: Option<String>,
    pub site: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RefUse {
    pub(super) literal: String,
    pub(super) feature: Option<String>,
    pub(super) site: Option<String>,
}

pub fn run_checks(module: &Module) -> Vec<CheckIssue> {
    let declared_plugins = declared_plugin_names(module);
    let mut refs = Vec::new();

    for feature in &module.features {
        collect_feature_refs(feature, &mut refs);
    }

    let mut issues = Vec::new();
    let mut seen = BTreeSet::new();
    for reference in refs {
        if let Some(name) = reference.literal.strip_prefix("@lazuli/plugin-") {
            if !plugin_declared(&declared_plugins, name) {
                push_issue(
                    &mut issues,
                    &mut seen,
                    CODE_PLUGIN,
                    Severity::Error,
                    format!(
                        "plugin reference {} not declared in app.lzi registry",
                        reference.literal
                    ),
                    &reference,
                );
            }
        } else if let Some(name) = reference.literal.strip_prefix("@runtime/") {
            if !known_runtime_ref(name) {
                push_issue(
                    &mut issues,
                    &mut seen,
                    CODE_UNRESOLVED,
                    Severity::Error,
                    format!(
                        "runtime reference {} is not in the closed Go runtime catalog",
                        reference.literal
                    ),
                    &reference,
                );
            }
        } else if let Some(name) = reference.literal.strip_prefix("@semantic.") {
            if !known_semantic_ref(name) {
                push_issue(
                    &mut issues,
                    &mut seen,
                    CODE_SEMANTIC,
                    Severity::Error,
                    format!(
                        "semantic reference {} is outside the closed Go semantic table",
                        reference.literal
                    ),
                    &reference,
                );
            }
        } else if let Some(name) = reference.literal.strip_prefix("@cap.") {
            if !known_cap_ref(name) {
                push_issue(
                    &mut issues,
                    &mut seen,
                    CODE_CAP,
                    Severity::Error,
                    format!(
                        "capability reference {} is outside Hashed/Encrypted/Token/File",
                        reference.literal
                    ),
                    &reference,
                );
            }
        } else if let Some(name) = reference.literal.strip_prefix(UNRESOLVED_TYPE_PREFIX) {
            // Bare unresolved type identifier — analyzer could not map
            // it to a resource/record/enum. Without this hard fail the
            // emitter inlines a sanitised placeholder and the Go build
            // breaks with a confusing "undeclared name" downstream.
            push_issue(
                &mut issues,
                &mut seen,
                CODE_TYPE_UNRESOLVED,
                Severity::Error,
                format!(
                    "type reference `{}` does not resolve to a known resource, record, or enum",
                    name
                ),
                &reference,
            );
        } else if reference.literal.starts_with("@adapter.") {
            // Stub for CODEGEN-GO-ADAPTER-003. RegisterAdapter discovery
            // needs filesystem/runtime integration context that this pure
            // Module pass does not receive yet.
        } else if reference.literal.starts_with("@fn.") {
            // Stub for CODEGEN-GO-FN-006. Extension stub discovery lands
            // with the follow-up §10.5 resolver.
        }
    }

    issues
}

fn push_issue(
    issues: &mut Vec<CheckIssue>,
    seen: &mut BTreeSet<(&'static str, String, Option<String>, Option<String>)>,
    code: &'static str,
    severity: Severity,
    message: String,
    reference: &RefUse,
) {
    let key = (
        code,
        reference.literal.clone(),
        reference.feature.clone(),
        reference.site.clone(),
    );
    if seen.insert(key) {
        issues.push(CheckIssue {
            code,
            severity,
            message,
            feature: reference.feature.clone(),
            site: reference.site.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AppIntegration, AppManifest, AppRegistry, BuiltinType, Defaults, Feature, Field, Policies,
        QualifiedName, Resource, TypeRef,
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

    fn empty_app() -> AppManifest {
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

    fn empty_registry() -> AppRegistry {
        AppRegistry {
            env: Vec::new(),
            integrations: Vec::new(),
            capabilities: Vec::new(),
            packs: Vec::new(),
            tools: Vec::new(),
            webhook_events: Vec::new(),
            secret_rotations: Vec::new(),
        }
    }

    fn module_with_feature(feature: Feature) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(empty_app()),
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: vec![feature],
        }
    }

    fn field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    fn resource_with_field(type_ref: TypeRef) -> Resource {
        Resource {
            name: "Customer".to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![field("value", type_ref)],
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: vec![],

            lock: None,

            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
        }
    }

    fn codes(issues: &[CheckIssue]) -> Vec<&'static str> {
        issues.iter().map(|issue| issue.code).collect()
    }

    #[test]
    fn missing_plugin_reference_reports_plugin_001() {
        let mut feature = empty_feature("billing");
        feature.uses.push("@lazuli/plugin-mercadopago".to_owned());

        let issues = run_checks(&module_with_feature(feature));

        assert_eq!(codes(&issues), vec![CODE_PLUGIN]);
        assert_eq!(issues[0].severity, Severity::Error);
        assert_eq!(issues[0].feature.as_deref(), Some("billing"));
    }

    #[test]
    fn declared_plugin_registry_entry_suppresses_plugin_001() {
        let mut feature = empty_feature("billing");
        feature.uses.push("@lazuli/plugin-mercadopago".to_owned());
        let mut registry = empty_registry();
        registry.integrations.push(AppIntegration {
            name: "mercadopago".to_owned(),
            kind: "PaymentGateway".to_owned(),
            adapter: Some("@lazuli/plugin-mercadopago".to_owned()),
            adapter_provenance: Some("plugin".to_owned()),
            environments: Vec::new(),
            credentials: None,
            data_classification: None,
        });
        let mut module = module_with_feature(feature);
        module.registry = Some(registry);

        let issues = run_checks(&module);

        assert!(issues.is_empty());
    }

    #[test]
    fn unknown_semantic_reference_reports_semantic_004() {
        let mut feature = empty_feature("customer");
        feature
            .resources
            .push(resource_with_field(TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "@semantic.Locale".to_owned(),
            })));

        let issues = run_checks(&module_with_feature(feature));

        assert_eq!(codes(&issues), vec![CODE_SEMANTIC]);
        assert_eq!(issues[0].site.as_deref(), Some("resource Customer.value"));
    }

    #[test]
    fn unknown_capability_reference_reports_cap_005() {
        let mut feature = empty_feature("customer");
        feature
            .resources
            .push(resource_with_field(TypeRef::Unresolved(
                "@cap.E2ee".to_owned(),
            )));

        let issues = run_checks(&module_with_feature(feature));

        assert_eq!(codes(&issues), vec![CODE_CAP]);
        assert_eq!(issues[0].severity, Severity::Error);
    }

    #[test]
    fn legacy_cap_secret_reports_cap_005() {
        let mut feature = empty_feature("customer");
        feature.resources.push(resource_with_field(TypeRef::Builtin(
            BuiltinType::CapSecret,
        )));

        let issues = run_checks(&module_with_feature(feature));

        assert_eq!(codes(&issues), vec![CODE_CAP]);
    }

    #[test]
    fn valid_semantic_and_capability_catalog_entries_do_not_report() {
        let mut feature = empty_feature("customer");
        feature
            .resources
            .push(resource_with_field(TypeRef::Many(Box::new(
                TypeRef::Unresolved("@semantic.Currency".to_owned()),
            ))));
        feature
            .uses
            .push("@cap.File(max_size:25mb,accept:text/csv)".to_owned());

        let issues = run_checks(&module_with_feature(feature));

        assert!(issues.is_empty());
    }

    // ---------------------------------------------------------------
    // CODE_TYPE_UNRESOLVED (CODEGEN-GO-TYPE-007) — analyzer failed to
    // resolve a bare type identifier. The codegen MUST fail loudly
    // instead of inlining a sanitised placeholder that breaks the Go
    // build downstream.
    // ---------------------------------------------------------------

    #[test]
    fn bare_unresolved_type_reference_reports_type_007() {
        let mut feature = empty_feature("orders");
        feature
            .resources
            .push(resource_with_field(TypeRef::Unresolved(
                "MysteryShape".to_owned(),
            )));

        let issues = run_checks(&module_with_feature(feature));

        assert_eq!(codes(&issues), vec![CODE_TYPE_UNRESOLVED]);
        assert_eq!(issues[0].severity, Severity::Error);
        assert_eq!(issues[0].site.as_deref(), Some("resource Customer.value"));
        assert!(
            issues[0].message.contains("MysteryShape"),
            "diagnostic must name the unresolved identifier; got: {}",
            issues[0].message
        );
    }

    #[test]
    fn bare_unresolved_inside_many_still_reports_type_007() {
        let mut feature = empty_feature("orders");
        feature
            .resources
            .push(resource_with_field(TypeRef::Many(Box::new(
                TypeRef::Unresolved("MysteryShape".to_owned()),
            ))));

        let issues = run_checks(&module_with_feature(feature));

        assert_eq!(codes(&issues), vec![CODE_TYPE_UNRESOLVED]);
    }

    #[test]
    fn at_prefixed_unresolved_keeps_specific_namespace_code() {
        // Regression guard: when the unresolved string starts with `@`,
        // the bare-name branch must NOT fire — the namespace-specific
        // code (CODE_SEMANTIC/CAP/etc.) wins.
        let mut feature = empty_feature("customer");
        feature
            .resources
            .push(resource_with_field(TypeRef::Unresolved(
                "@semantic.Locale".to_owned(),
            )));

        let issues = run_checks(&module_with_feature(feature));

        assert_eq!(codes(&issues), vec![CODE_SEMANTIC]);
        assert!(
            !issues.iter().any(|i| i.code == CODE_TYPE_UNRESOLVED),
            "namespace refs must not double-fire on TYPE-007"
        );
    }
}
