//! Cell E4 — shared fixture builders for inline `#[cfg(test)] mod tests`
//! blocks across the `query/` submodule.
//!
//! Lifted out of `mod.rs` (wave R8-3) so the test modules co-located with
//! each emitter (`list`, `lookup`, `sql`, plus the entry-point tests in
//! `mod.rs`) don't each carry a copy of the same ~150 LOC of struct
//! literals. This file is `#[cfg(test)]`-only — it never ships in a
//! release build.
//!
//! These are **fixture builders**, not tests. They construct empty /
//! minimal IR shapes that individual tests then mutate before feeding
//! into the orchestrator. Each builder mirrors the IR struct's `Default`
//! contract where one exists, and fills the rest with empty `Vec`s /
//! `None`s. Mirrors `command/test_support.rs` verbatim.

#![cfg(test)]
#![allow(dead_code)]

use lazuli_ir::{
    AppManifest, Defaults, Feature, Field, Module, Policies, PolicyRef, QualifiedName, Record,
    Resource, TypeRef, TypedSlot,
};

use super::super::cross_feature::CrossFeatureIndex;
use super::super::module::EmitContext;
use super::emit_query_file;

pub(super) fn emit(feature: &Feature) -> Option<String> {
    let module = module_with_features(vec![feature.clone()]);
    let index = CrossFeatureIndex::build(&module);
    let emit_ctx = EmitContext::no_source("customer/query.gen.go");
    emit_query_file("examples/x.lzi", feature, "lazuli/test", &index, &emit_ctx)
}

pub(super) fn emit_from_module(module: &Module, feature_name: &str) -> Option<String> {
    let feature = module
        .features
        .iter()
        .find(|feature| feature.name == feature_name)
        .expect("feature exists");
    let index = CrossFeatureIndex::build(module);
    let emit_ctx = EmitContext::no_source("customer/query.gen.go");
    emit_query_file("examples/x.lzi", feature, "lazuli/test", &index, &emit_ctx)
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

pub(super) fn base_feature(name: &str) -> Feature {
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

pub(super) fn field(name: &str, type_ref: TypeRef, required: bool) -> Field {
    Field {
        name: name.to_owned(),
        type_ref,
        required,
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

pub(super) fn resource(name: &str, fields: Vec<Field>) -> Resource {
    Resource {
        name: name.to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields,
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

pub(super) fn record(name: &str) -> Record {
    Record {
        name: name.to_owned(),
        public_contract: None,
        fields: Vec::new(),
        discriminator_field: None,
        span_ref: None,
    }
}

pub(super) fn slot(name: &str, type_ref: TypeRef, required: bool) -> TypedSlot {
    TypedSlot {
        name: name.to_owned(),
        type_ref,
        required,
        constraints: lazuli_ir::FieldConstraints::default(),
        validate_skip: false,
    }
}

pub(super) fn qname(name: &str) -> QualifiedName {
    QualifiedName {
        feature: None,
        name: name.to_owned(),
    }
}

// Silence `unused_imports` when not all fixture functions are used from a
// given sibling — the dead-code allow above already silences unused
// functions; this just keeps `PolicyRef` import honest when needed.
#[allow(dead_code)]
pub(super) fn _policy_ref_witness() -> PolicyRef {
    PolicyRef::None
}
