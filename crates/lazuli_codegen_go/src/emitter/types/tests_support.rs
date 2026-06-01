//! Shared fixture helpers for the inline test modules.
//!
//! Lives at module scope (not `#[cfg(test)]`-gated as a sibling file) so
//! `builtins.rs` and `named.rs` can pull the same factory helpers without
//! re-declaring them. The `#[cfg(test)]` guard sits at the `mod`
//! declaration in `mod.rs`.

use lazuli_ir::{AppManifest, Defaults, Feature, Module, Policies, Record, Resource};

use super::TypeCtx;
use crate::emitter::cross_feature::CrossFeatureIndex;

pub(super) fn empty_feature(name: &str) -> Feature {
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
        synth_origins: std::collections::BTreeMap::new(),
    }
}

pub(super) fn module_with_features(features: Vec<Feature>) -> Module {
    Module {
        workspace: None,
        contracts: Vec::new(),
        app: Some(AppManifest {
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
        }),
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        features,
    }
}

pub(super) fn make_record(name: &str) -> Record {
    Record {
        name: name.to_owned(),
        public_contract: None,
        fields: Vec::new(),
        discriminator_field: None,
        span_ref: None,
    }
}

pub(super) fn make_resource(name: &str) -> Resource {
    Resource {
        name: name.to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields: Vec::new(),
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

pub(super) fn cross_ref_module() -> Module {
    let mut customer = empty_feature("customer");
    customer.resources.push(make_resource("Customer"));
    let mut org = empty_feature("org");
    org.resources.push(make_resource("User"));
    module_with_features(vec![customer, org])
}

pub(super) fn type_ctx<'a>(
    current_feature: &'a str,
    module_name: &'a str,
    cross_index: &'a CrossFeatureIndex<'a>,
) -> TypeCtx<'a> {
    TypeCtx {
        current_feature,
        module_name,
        cross_index,
    }
}
