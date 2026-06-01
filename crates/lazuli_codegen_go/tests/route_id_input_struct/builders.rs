//! Boilerplate IR builders shared by the route_id_input_struct tests.
//! Mirrors the helpers in tests/emit_v1.rs.

use std::collections::BTreeMap;

use lazuli_ir::{
    AppManifest, Command, CommandEffect, CommandInput, CommandKind, Defaults, Feature, Field,
    FieldConstraints, Module, Policies, PolicyRef, QualifiedName, Resource, TypeRef,
};

pub fn empty_feature(name: &str) -> Feature {
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
        synth_origins: BTreeMap::new(),
    }
}

pub fn minimal_app_manifest(name: &str) -> AppManifest {
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

pub fn module_with(feature: Feature) -> Module {
    Module {
        workspace: None,
        contracts: Vec::new(),
        app: Some(minimal_app_manifest("test_app")),
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        features: vec![feature],
    }
}

pub fn local_qname(name: &str) -> QualifiedName {
    QualifiedName {
        feature: None,
        name: name.to_owned(),
    }
}

pub fn simple_field(name: &str, type_ref: TypeRef) -> Field {
    Field {
        name: name.to_owned(),
        type_ref,
        required: true,
        unique: false,
        slug: false,
        default: None,
        derived_from: None,
        computed_date: None,
        constraints: FieldConstraints::default(),
        full_text: false,
        previous_names: Vec::new(),
        pii: None,
        owner_axis: None,
        cross_feature_target: None,
        span_ref: None,
    }
}

pub fn resource_with(name: &str, field_name: &str, field_ty: TypeRef) -> Resource {
    Resource {
        name: name.to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields: vec![simple_field(field_name, field_ty)],
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

pub fn base_command(name: &str) -> Command {
    Command {
        name: name.to_owned(),
        public_contract: None,
        kind: CommandKind::Create,
        route: Vec::new(),
        input: CommandInput::Empty,
        target: None,
        lets: Vec::new(),
        effect: CommandEffect::None,
        policy: PolicyRef::None,
        policy_expr: None,
        policy_when_denied: None,
        emits: Vec::new(),
        rate_limit: None,
        audit: None,
        approval: None,
        invalidates: Vec::new(),
        external_calls: Vec::new(),
        timeout: None,
        retry: None,
        idempotency: None,
        write_window: None,
        deprecated: None,
        handler: None,
        tests: None,
        triggers: vec![],
        synthesized_from_cap_file: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
        derived_from: None,
    }
}

pub fn command_gen(files: &[lazuli_codegen_go::GeneratedFile], feature: &str) -> String {
    files
        .iter()
        .find(|file| file.path == format!("{feature}/command.gen.go"))
        .unwrap_or_else(|| panic!("expected {feature}/command.gen.go"))
        .contents
        .clone()
}
