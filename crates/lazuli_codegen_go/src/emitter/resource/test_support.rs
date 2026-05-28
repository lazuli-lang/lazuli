//! Shared test fixtures for the `resource` siblings. Builds the
//! minimal `Module` / `Feature` / `Resource` skeletons every sibling
//! `*_tests.rs` reaches for, plus the `emit` shortcut that wraps a
//! single feature into a module and runs `emit_resource_file`.
//!
//! Pure construction; no asserts here.

#![cfg(test)]
#![allow(dead_code)]

use super::CrossFeatureIndex;
use super::emit_resource_file;
use lazuli_ir::{
    AppManifest, BuiltinType, CapabilityRef, Defaults, E2eeCapability, EncryptedCapability,
    Feature, Field, HashAlgorithm, HashedCapability, Module, Policies, Resource, TokenCapability,
    TokenStore, TypeRef,
};

/// Test helper: build a single-feature module around `feature`,
/// construct the cross-feature index against it, and emit the
/// resource file. Pre-Phase-Prep tests didn't need the index;
/// the helper keeps the suite concise while threading the new
/// arguments through.
pub(super) fn emit(feature: &Feature) -> Option<String> {
    let module = single_feature_module(feature.clone());
    let index = CrossFeatureIndex::build(&module);
    emit_resource_file("examples/x.lzi", feature, "lazuli/test", &index)
}

pub(super) fn single_feature_module(feature: Feature) -> Module {
    Module {
        workspace: None,
        contracts: Vec::new(),
        app: Some(default_app_manifest("test")),
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        features: vec![feature],
    }
}

pub(super) fn default_app_manifest(name: &str) -> AppManifest {
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

pub(super) fn base_feature(name: &str) -> Feature {
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

pub(super) fn simple_field(name: &str, builtin: BuiltinType, required: bool) -> Field {
    Field {
        name: name.to_owned(),
        type_ref: TypeRef::Builtin(builtin),
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

pub(super) fn simple_resource(name: &str, fields: Vec<Field>) -> Resource {
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
        append_only: false,
    }
}

// -----------------------------------------------------------------
// Secret-bearing capability field constructors. Used by the JSON-skip
// sentinel tests + a few encryption-helper tests; kept here so each
// test sibling can `use` them by name rather than redeclaring.
// -----------------------------------------------------------------

pub(super) fn hashed_field(name: &str, required: bool) -> Field {
    Field {
        name: name.to_owned(),
        type_ref: TypeRef::Capability(CapabilityRef::Hashed(HashedCapability {
            algorithm: HashAlgorithm::Argon2id,
        })),
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

pub(super) fn encrypted_field(name: &str, required: bool) -> Field {
    Field {
        name: name.to_owned(),
        type_ref: TypeRef::Capability(CapabilityRef::Encrypted(EncryptedCapability {
            key: "@key.tenant".to_owned(),
        })),
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

pub(super) fn token_field(name: &str, required: bool) -> Field {
    Field {
        name: name.to_owned(),
        type_ref: TypeRef::Capability(CapabilityRef::Token(TokenCapability {
            ttl: "24h".to_owned(),
            single_use: false,
            store: TokenStore::Hashed,
        })),
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

pub(super) fn e2ee_field(name: &str, required: bool) -> Field {
    Field {
        name: name.to_owned(),
        type_ref: TypeRef::Capability(CapabilityRef::E2ee(E2eeCapability {
            key: "@key.user".to_owned(),
        })),
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
