//! Shared test helpers for the `coverage::*` calculator unit tests.
//!
//! Lives behind `#[cfg(test)]` only — never compiled into the
//! production build. Constructing a minimal `Feature` requires
//! initializing 30+ fields, none of which a coverage calculator
//! actually inspects beyond the slot it walks; this module
//! centralizes the boilerplate so each calculator's tests stay
//! focused on what they measure.

use lazuli_ir::{
    Command, CommandEffect, CommandInput, CommandKind, Defaults, Feature, Policies, PolicyRef,
    TestBlock,
};

pub fn cmd_with_policy(name: &str, policy: PolicyRef, tests: Option<TestBlock>) -> Command {
    Command {
        name: name.to_string(),
        public_contract: None,
        kind: CommandKind::Update,
        route: Vec::new(),
        input: CommandInput::Empty,
        target: None,
        lets: Vec::new(),
        effect: CommandEffect::None,
        policy,
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
        tests,
        previous_names: Vec::new(),
        span_ref: None,
        triggers: Vec::new(),
        synthesized_from_cap_file: None,
        owner_scope_sql: None,
    }
}

pub fn empty_feature(name: &str) -> Feature {
    Feature {
        name: name.into(),
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
        synth_origins: std::collections::BTreeMap::new(),
        span_ref: None,
    }
}
