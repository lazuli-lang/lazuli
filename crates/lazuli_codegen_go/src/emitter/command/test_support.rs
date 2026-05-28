//! Cell E3 — shared fixture builders for inline `#[cfg(test)] mod tests`
//! blocks across the `command/` submodule.
//!
//! Lifted out of `file_emit.rs` (wave R8-2b) so the two test modules
//! ({`feature_emit_tests`, `tests`}) don't each carry a copy of the
//! same ~250 LOC of struct literals. This file is `#[cfg(test)]`-only —
//! it never ships in a release build.
//!
//! These are **fixture builders**, not tests. They construct empty /
//! minimal IR shapes that individual tests then mutate before feeding
//! into the orchestrator. Each builder mirrors the IR struct's `Default`
//! contract where one exists, and fills the rest with empty `Vec`s /
//! `None`s.

#![cfg(test)]
#![allow(dead_code)]

use lazuli_ir::{
    AppManifest, BuiltinType, Command, CommandEffect, CommandInput, CommandKind, Defaults, Feature,
    Field, FieldConstraints, Module, Policies, PolicyRef, QualifiedName, Resource, TypeRef,
    TypedSlot,
};

use super::super::cross_feature::CrossFeatureIndex;
use super::super::module::EmitContext;
use super::file_emit::emit_command_file;

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
        append_only: false,
    }
}

/// A minimal text `Field` used by the scope tests to push columns onto
/// a `simple_resource()` so `@scope.*` / `owner_scope_sql` resolution
/// has something to bind to. Lifted from the inline `scope::tests`
/// helper so sibling test-host files (`scope_owner_tests.rs`,
/// `scope_where_keys_tests.rs`, `owner_scope_sql_tests.rs`) can share
/// it without each carrying a copy.
pub(super) fn scope_field(name: &str) -> Field {
    Field {
        name: name.to_owned(),
        type_ref: TypeRef::Builtin(BuiltinType::Text),
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

pub(super) fn typed_slot(name: &str, builtin: BuiltinType, required: bool) -> TypedSlot {
    TypedSlot {
        name: name.to_owned(),
        type_ref: TypeRef::Builtin(builtin),
        required,
        constraints: lazuli_ir::FieldConstraints::default(),
        validate_skip: false,
    }
}

pub(super) fn local_qname(name: &str) -> QualifiedName {
    QualifiedName {
        feature: None,
        name: name.to_owned(),
    }
}

/// Emit `command.gen.go` for the given feature. Ensures a `Customer`
/// resource exists somewhere in the module so command effects resolve
/// at codegen time. This mirrors the `emit` helper that used to live
/// inline in the `command/file_emit.rs` `mod tests` block.
pub(super) fn emit_with_customer_fallback(feature: &Feature) -> Option<String> {
    let mut features = vec![feature.clone()];
    if !feature
        .commands
        .iter()
        .all(|c| matches!(c.effect, CommandEffect::None))
    {
        features[0].resources.push(simple_resource("Customer"));
    }
    let module = module_with_features(features);
    let index = CrossFeatureIndex::build(&module);
    let emit_ctx = EmitContext::no_source("customer/command.gen.go");
    emit_command_file(
        "examples/x.lzi",
        &module.features[0],
        "lazuli/test",
        &index,
        &emit_ctx,
    )
}

pub(super) fn base_command(name: &str) -> Command {
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
