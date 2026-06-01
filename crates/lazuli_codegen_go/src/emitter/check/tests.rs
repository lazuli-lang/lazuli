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
        knowledge: None,
        defaults: Defaults {
            tenancy: None,
            timestamps: false,
            policy: None,
            rate_limit: None,
            audit: None,
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
        computed_date: None,
        constraints: lazuli_ir::FieldConstraints::default(),
        full_text: false,
        previous_names: Vec::new(),
        pii: None,
        owner_axis: None,
        cross_feature_target: None,
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
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        restrict_on_delete: Vec::new(),
        append_only: false,
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
