//! Round-trip test for the IR Error-Vocab cell (Cell IR-1).
//!
//! Constructs a `Feature` with every new error-vocab field populated
//! (`Feature.errors`, `PolicyCategory.when_denied`, `Command.policy_when_denied`,
//! `Api.policy_when_denied`, `Webhook.policy_when_denied`,
//! `Job.policy_when_denied`, `Channel.policy_when_denied`,
//! `Workflow.policy_when_denied`, `Agent.policy_when_denied`, plus every
//! `Query` variant). Serializes via serde to JSON, deserializes back, and
//! asserts equality.
//!
//! See `docs/proposals/ir-error-messages-vocab.md` §11 Cell IR-1.
//!
//! Wave R10-C split this single-file crate into per-concern sub-modules to
//! keep every file ≤ 500 LOC.

use lazuli_ir::{Defaults, Feature, Policies, SpanRef, TranslationKeyRef};

mod back_compat;
mod feature_errors_shape;
mod full_round_trip;
mod key_and_defaults;

pub(crate) fn key_ref(name: &str, span_offset: usize) -> TranslationKeyRef {
    TranslationKeyRef {
        key: name.to_owned(),
        span_ref: Some(SpanRef {
            start: span_offset,
            end: span_offset + 16,
        }),
    }
}

pub(crate) fn empty_feature() -> Feature {
    Feature {
        name: "account".to_owned(),
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
