//! Shared fixtures for root emitter tests. Split out so each test
//! file stays under the 500 LOC budget.

use super::*;
use crate::LazuriteManifest;
use lazuli_ir::{
    AppCors, AppLocale, AppLogging, AppManifest, AppTracing, Defaults, EncryptionAlgorithm,
    EncryptionBinding, EncryptionRotation, EncryptionSource, EncryptionTemplate, Feature,
    LocaleFallback, Module, Policies,
};
use std::collections::BTreeMap;


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

pub(super) fn manifest(name: &str) -> AppManifest {
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

pub(super) fn module_with(features: Vec<Feature>, app: Option<AppManifest>) -> Module {
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

pub(super) fn lazurite_manifest(project_module: &str) -> LazuriteManifest {
    LazuriteManifest {
        project_module: project_module.to_owned(),
        plugins: BTreeMap::new(),
        generate_go: None,
        dev: None,
    }
}

