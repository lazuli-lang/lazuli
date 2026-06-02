//! Registry `ALL` section 4/11 (SPEC-19 split; concatenated in `registry::ALL`).
#![allow(clippy::all, unused_imports)]

use super::super::builders::*;
use super::super::facets::*;
use crate::{CapabilitySpec, Context, DiagnosticFacet, SemanticToken, Sigil, Surface};

pub(crate) const ROWS: &[CapabilitySpec] = &[
    stmt(
        "operation",
        Context::Integrations,
        "entity.name.function.statement.integration.lazuli",
        "Integration operation.",
    ),
    // ── registry body ──
    kw(
        "secret_rotation",
        Context::Registry,
        SECTION,
        "Secret-rotation policy block.",
    ),
    kw(
        "tools",
        Context::Registry,
        SECTION,
        "Registry tool declarations.",
    ),
    kw(
        "webhook_event",
        Context::Registry,
        SECTION,
        "Registry webhook-event envelope.",
    ),
    kw(
        "webhook_events",
        Context::Registry,
        SECTION,
        "Registry webhook-events catalog.",
    ),
    // ════════════════════════════════════════════════════════════════
    // Feature body (indent-2 kinds) — FEATURE_BODY_KINDS
    // ════════════════════════════════════════════════════════════════
    produces(
        kw(
            "command",
            Context::FeatureHeader,
            DECL,
            "Declares a write command (mutation effect).",
        ),
        P_COMMAND,
    ),
    produces(
        kw(
            "api",
            Context::FeatureHeader,
            DECL,
            "Declares a full-control HTTP endpoint.",
        ),
        P_API,
    ),
    kw(
        "view",
        Context::FeatureHeader,
        DECL,
        "Declares a surface view.",
    ),
    produces(
        kw(
            "webhook",
            Context::FeatureHeader,
            DECL,
            "Declares an inbound webhook handler.",
        ),
        P_WEBHOOK,
    ),
    produces(
        kw(
            "job",
            Context::FeatureHeader,
            DECL,
            "Declares a background job.",
        ),
        P_JOB,
    ),
    kw(
        "agent",
        Context::FeatureHeader,
        DECL,
        "Declares an LLM agent.",
    ),
    kw(
        "notification",
        Context::FeatureHeader,
        DECL,
        "Declares a notification.",
    ),
    produces(
        kw(
            "poller",
            Context::FeatureHeader,
            DECL,
            "Declares a polling integration.",
        ),
        P_POLLER,
    ),
    produces(
        kw(
            "report",
            Context::FeatureHeader,
            DECL,
            "Declares a report/export.",
        ),
        P_REPORT,
    ),
    produces(
        kw(
            "channel",
            Context::FeatureHeader,
            DECL,
            "Declares a realtime channel.",
        ),
        P_CHANNEL,
    ),
    kw(
        "cache",
        Context::FeatureHeader,
        DECL,
        "Declares a named cache profile.",
    ),
    produces(
        kw(
            "aggregate",
            Context::FeatureHeader,
            DECL,
            "Declares a domain aggregate root.",
        ),
        P_AGGREGATE,
    ),
    produces(
        kw(
            "record",
            Context::FeatureHeader,
            DECL,
            "Declares a value-object record.",
        ),
        P_RECORD,
    ),
    kw(
        "entity",
        Context::FeatureHeader,
        DECL,
        "Declares a domain entity.",
    ),
    kw(
        "resource",
        Context::FeatureHeader,
        DECL,
        "Declares a domain resource.",
    ),
    produces(
        kw(
            "enum",
            Context::FeatureHeader,
            DECL,
            "Declares an enumeration.",
        ),
        P_UNION,
    ),
    kw(
        "events",
        Context::FeatureHeader,
        SECTION,
        "Declares the events block.",
    ),
    kw(
        "event",
        Context::FeatureHeader,
        DECL,
        "Declares a domain event.",
    ),
    produces(
        kw(
            "event_group",
            Context::FeatureHeader,
            DECL,
            "Declares an event group.",
        ),
        P_EVENT_GROUP,
    ),
    kw(
        "surface",
        Context::FeatureHeader,
        SECTION,
        "Declares a feature surface.",
    ),
    kw(
        "extensions",
        Context::FeatureHeader,
        SECTION,
        "Declares typed extension points.",
    ),
    produces(
        kw(
            "tests",
            Context::FeatureHeader,
            SECTION,
            "Declares the policy/behavior tests block.",
        ),
        P_TESTS,
    ),
    kw(
        "auth",
        Context::FeatureHeader,
        SECTION,
        "Declares the authentication block.",
    ),
    produces(
        kw(
            "errors",
            Context::FeatureHeader,
            SECTION,
            "Declares the error-vocabulary block.",
        ),
        P_ERRORS,
    ),
    produces(
        kw(
            "policies",
            Context::FeatureHeader,
            SECTION,
            "Declares the policy block.",
        ),
        P_POLICY,
    ),
    kw(
        "domain",
        Context::FeatureHeader,
        SECTION,
        "Declares the domain-model block.",
    ),
    kw(
        "defaults",
        Context::FeatureHeader,
        SECTION,
        "Declares resource-convention defaults.",
    ),
    produces(
        kw(
            "purpose",
            Context::FeatureHeader,
            STMT,
            "Feature purpose (iron-hand context).",
        ),
        P_PURPOSE,
    ),
    produces(
        kw(
            "non_goals",
            Context::FeatureHeader,
            SECTION,
            "Feature non-goals (iron-hand context).",
        ),
        P_NONGOALS,
    ),
    // Iron-hand context vocabulary — `knowledge <sector>` names the
    // `knowledge/<sector>/` vault the feature draws from. The
    // sector is a bareword slug. Produces the five `VOCAB-KNOWLEDGE-*`
    // doctor rules that cross-check the sector against its on-disk vault
    // (see `docs/proposals/knowledge-sector-field.md` §Doctor).
    produces(
        kw(
            "knowledge",
            Context::FeatureHeader,
            STMT,
            "Feature knowledge sector (iron-hand context).",
        ),
        P_KNOWLEDGE,
    ),
    stmt(
        "delegated_to",
        Context::FeatureHeader,
        "entity.name.function.statement.non-goals.lazuli",
        "Non-goal delegated to another feature.",
    ),
    stmt(
        "out_of_scope",
        Context::FeatureHeader,
        "entity.name.function.statement.non-goals.lazuli",
        "Explicitly out-of-scope concern.",
    ),
    stmt(
        "constraints",
        Context::FeatureHeader,
        "entity.name.function.statement.non-goals.lazuli",
        "Non-goal constraints.",
    ),
];
