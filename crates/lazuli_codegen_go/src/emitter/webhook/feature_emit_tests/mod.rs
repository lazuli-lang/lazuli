//! Tests for `emit_webhook_file`. Co-located with the production emitter
//! via `#[cfg(test)] mod feature_emit_tests;` in `mod.rs`, so the test path
//! stays `emitter::webhook::feature_emit_tests::*`. Sub-modules group tests
//! by sub-concern (per-field contract shape vs higher-level wiring).

#![cfg(test)]

mod contract_tests;
mod wiring_tests;

use super::*;
use lazuli_ir::{
    AppManifest, Defaults, Feature, Module, Path, PathRef, Policies, VerifyScheme, VerifySpec,
    Webhook,
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

pub(super) fn module_with_feature(feature: Feature) -> Module {
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
        features: vec![feature],
    }
}

pub(super) fn emit(feature: &Feature) -> Option<String> {
    let module = module_with_feature(feature.clone());
    let index = CrossFeatureIndex::build(&module);
    let emit_ctx = EmitContext::no_source("customer/webhook.gen.go");
    emit_webhook_file("examples/x.lzi", feature, "lazuli/test", &index, &emit_ctx)
}

pub(super) fn path(segments: &[&str]) -> Path {
    Path::from_segments(segments.iter().copied())
}

pub(super) fn base_webhook(name: &str) -> Webhook {
    Webhook {
        name: name.to_owned(),
        route: format!("/webhooks/{name}"),
        verify: PathRef::convention(format!("./webhooks/{name}_verify.go")),
        structured_verify: None,
        tenant_from: None,
        scope_global: None,
        idempotency: None,
        policy: None,
        policy_expr: None,
        policy_when_denied: None,
        handler: PathRef::authored(format!("./webhooks/{name}.go")),
        returns: None,
        emits: Vec::new(),
        emit_predicates: Vec::new(),
        payload_from: None,
        replay: None,
        dlq: None,
        retry: None,
        previous_names: Vec::new(),
        span_ref: None,
    }
}

pub(super) fn hmac_verify() -> VerifySpec {
    VerifySpec {
        scheme: VerifyScheme::Hmac,
        algorithm: "sha256".to_owned(),
        secret_env: "MERCADOPAGO_HMAC_SECRET".to_owned(),
        header: "X-Signature".to_owned(),
    }
}
