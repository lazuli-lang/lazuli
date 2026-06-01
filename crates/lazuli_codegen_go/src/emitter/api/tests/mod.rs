//! Tests for the API emitter. The original `mod tests` god block lives
//! here, split by sub-concern so each file stays within the rails-style
//! LOC budget. The `emitter::api::tests::*` namespace is preserved.

#![cfg(test)]

mod contract_tests;
mod wiring_tests;

use super::*;
use lazuli_ir::{
    AppManifest, Defaults, FileCapability, FileSize, FileSizeLiteral, FileVisibility, HttpMethod,
    MimeType, Module, PathRef, Policies, PolicyRef, Record, Resource, TypeRef,
};

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

pub(super) fn minimal_app() -> AppManifest {
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

pub(super) fn module_with_features(features: Vec<Feature>) -> Module {
    Module {
        workspace: None,
        contracts: Vec::new(),
        app: Some(minimal_app()),
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        features,
    }
}

pub(super) fn emit_from_module(module: &Module, feature_index: usize) -> Option<String> {
    let index = CrossFeatureIndex::build(module);
    let emit_ctx = EmitContext::no_source("feature/api.gen.go");
    emit_api_file(
        "examples/x.lzi",
        &module.features[feature_index],
        "lazuli/test",
        &index,
        &emit_ctx,
    )
}

pub(super) fn emit(feature: &Feature) -> Option<String> {
    let module = module_with_features(vec![feature.clone()]);
    emit_from_module(&module, 0)
}

pub(super) fn simple_api(name: &str, method: HttpMethod, path: &str, output: TypeRef) -> Api {
    Api {
        name: name.to_owned(),
        method,
        path: path.to_owned(),
        policy: PolicyRef::Local("read".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        rate_limit: None,
        output,
        handler: PathRef::authored(format!("./handlers/{name}.go")),
        locale_negotiate: None,
        deprecated: None,
        span_ref: None,
    }
}

pub(super) fn simple_record(name: &str) -> Record {
    Record {
        name: name.to_owned(),
        public_contract: None,
        fields: Vec::new(),
        discriminator_field: None,
        span_ref: None,
    }
}

pub(super) fn simple_resource(name: &str) -> Resource {
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

pub(super) fn make_file_capability(
    literal: FileSizeLiteral,
    accept: Vec<(&str, &str)>,
    visibility: Option<FileVisibility>,
    signed_ttl: Option<&str>,
) -> FileCapability {
    FileCapability {
        max_size: FileSize {
            bytes: literal.bytes(),
            literal,
        },
        accept: accept
            .into_iter()
            .map(|(family, subtype)| MimeType {
                family: family.to_owned(),
                subtype: subtype.to_owned(),
            })
            .collect(),
        visibility,
        signed_ttl: signed_ttl.map(str::to_owned),
        auto_photo_policy: None,
    }
}
