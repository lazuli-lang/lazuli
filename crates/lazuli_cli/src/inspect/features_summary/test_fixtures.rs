//! Shared test fixtures for the `features_summary` test modules.
//!
//! The integration tests for the renderer span three concerns:
//! crud / me annotations (`annotations.rs`), owner-axis flag plumbing
//! (`owner_scope.rs`), and the orchestrator itself (`mod.rs`). All of
//! them construct minimal `Feature` / `Resource` / `Command` / `Query`
//! values to drive `render_features_summary`. The constructors live
//! here as `pub(super)` items so sibling test modules can reach for
//! `super::test_fixtures::*` without duplicating the literals.

#![cfg(test)]

use lazuli_ir::{
    BuiltinType, Command, CommandEffect, CommandInput, CommandKind, ConventionRef, Defaults,
    Feature, Field, FieldConstraints, ListQuery, LookupQuery, OwnerAxis, Policies, PolicyRef,
    QualifiedName, Query, Resource, TypeRef,
};
use std::collections::BTreeMap;

/// Build a baseline empty feature with the required slots filled
/// minimally — used by the §8 and §9 fixtures below.
pub(super) fn empty_feature(name: &str) -> Feature {
    Feature {
        name: name.to_owned(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        defaults: Defaults::default(),
        uses: Vec::new(),
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
        requirements: Vec::new(),
        enums: Vec::new(),
        resources: Vec::new(),
        events: Vec::new(),
        rules: Vec::new(),
        policies: Policies::default(),
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
        pollers: Vec::new(),
        auth: None,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        agents: Vec::new(),
        reports: Vec::new(),
        channels: Vec::new(),
        caches: Vec::new(),
        aggregates: Vec::new(),
        mcp_servers: Vec::new(),
        previous_names: Vec::new(),
        synth_origins: BTreeMap::new(),
        span_ref: None,
    }
}

pub(super) fn customer_resource(conventions: Vec<ConventionRef>) -> Resource {
    Resource {
        name: "Customer".to_owned(),
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
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        append_only: false,
        invariants: Vec::new(),
        lock: None,
        composite_key: None,
        conventions,
    }
}

/// Minimal `Command` value — just enough to give the renderer a
/// `name` to print. Effect/input are the inert variants.
pub(super) fn minimal_command(name: &str) -> Command {
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
        previous_names: Vec::new(),
        span_ref: None,
        triggers: Vec::new(),
        synthesized_from_cap_file: None,
        owner_scope_sql: None,
        derived_from: None,
    }
}

pub(super) fn list_query(name: &str) -> Query {
    Query::List(ListQuery {
        name: name.to_owned(),
        public_contract: None,
        params: Vec::new(),
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        order: Vec::new(),
        paginate: None,
        modifier: None,
        cache: None,
        policy: PolicyRef::None,
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
    })
}

pub(super) fn lookup_query(name: &str) -> Query {
    Query::Lookup(LookupQuery {
        name: name.to_owned(),
        public_contract: None,
        params: Vec::new(),
        keys: Vec::new(),
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        policy: PolicyRef::None,
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
    })
}

/// Build a `host: Host required` FK field carrying
/// `@owner_axis(through: user)`. Pattern lifted verbatim from the
/// the canonical pilot's `Property.host` field (§1.5).
pub(super) fn owner_axis_host_field() -> Field {
    Field {
        name: "host".to_owned(),
        type_ref: TypeRef::UserDefined(QualifiedName {
            feature: None,
            name: "Host".to_owned(),
        }),
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
        owner_axis: Some(OwnerAxis {
            through_column: "user".to_owned(),
        }),
        cross_feature_target: None,
        span_ref: None,
    }
}

/// Build a tenant-only `name: Text required` field used to pad out
/// the owner-scope fixtures so the resource has at least one
/// non-FK field too.
pub(super) fn text_field(name: &str) -> Field {
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

pub(super) fn property_resource_with_owner_axis(conventions: Vec<ConventionRef>) -> Resource {
    Resource {
        name: "Property".to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields: vec![owner_axis_host_field(), text_field("name")],
        constraints: Vec::new(),
        validate: None,
        validates: Vec::new(),
        retention: None,
        previous_names: Vec::new(),
        span_ref: None,
        lifecycle: None,
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        append_only: false,
        invariants: Vec::new(),
        lock: None,
        composite_key: None,
        conventions,
    }
}
