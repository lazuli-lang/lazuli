//! Registry `ALL` section 6/11 (SPEC-19 split; concatenated in `registry::ALL`).
#![allow(clippy::all, unused_imports)]

use super::super::builders::*;
use super::super::facets::*;
use crate::{CapabilitySpec, Context, DiagnosticFacet, SemanticToken, Sigil, Surface};

pub(crate) const ROWS: &[CapabilitySpec] = &[
    stmt(
        "contains",
        Context::ResourceBody,
        "keyword.control.statement.lazuli",
        "Aggregate containment.",
    ),
    produces(
        stmt(
            "append_only",
            Context::ResourceBody,
            "entity.name.function.statement.resource.lazuli",
            "Append-only (event-log) resource.",
        ),
        P_APPEND_ONLY,
    ),
    produces(
        stmt(
            "many_through",
            Context::ResourceBody,
            "entity.name.function.statement.resource.lazuli",
            "Many-to-many through a join.",
        ),
        P_MANY_THROUGH,
    ),
    produces(
        stmt(
            "polymorphic_ref",
            Context::ResourceBody,
            "entity.name.function.statement.resource.lazuli",
            "Polymorphic reference field.",
        ),
        P_REF,
    ),
    produces(
        stmt(
            "computed_date",
            Context::ResourceBody,
            "entity.name.function.statement.resource.lazuli",
            "Computed date field.",
        ),
        P_COMPUTED_DATE,
    ),
    produces(
        stmt(
            "schedule_rule",
            Context::ResourceBody,
            "entity.name.function.statement.resource.lazuli",
            "Recurrence/schedule rule field.",
        ),
        P_SCHEDULE,
    ),
    stmt(
        "storage",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Storage hint.",
    ),
    stmt(
        "version_field",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Optimistic-lock version field.",
    ),
    produces(
        stmt(
            "lifecycle",
            Context::ResourceBody,
            DECL,
            "Declares a resource lifecycle.",
        ),
        P_LIFECYCLE,
    ),
    // ── enum body ──
    // member identifiers are user-named; the catalog keyword is `enum`
    // (declared above in FeatureHeader). No fixed member keywords.

    // ════════════════════════════════════════════════════════════════
    // Command body (indent-4) — COMMAND_STATEMENT_KINDS
    // ════════════════════════════════════════════════════════════════
    stmt(
        "creates",
        Context::CommandBody,
        STMT,
        "Effect: creates a resource.",
    ),
    stmt(
        "updates",
        Context::CommandBody,
        STMT,
        "Effect: updates a resource.",
    ),
    stmt(
        "deletes",
        Context::CommandBody,
        STMT,
        "Effect: deletes a resource.",
    ),
    stmt(
        "returns",
        Context::CommandBody,
        STMT,
        "Declares the command return type.",
    ),
    stmt(
        "route",
        Context::CommandBody,
        STMT,
        "HTTP route for the command.",
    ),
    stmt("input", Context::CommandBody, SECTION, "Input field block."),
    stmt(
        "policy",
        Context::CommandBody,
        STMT,
        "Authorization policy expression.",
    ),
    produces(
        stmt(
            "rate_limit",
            Context::CommandBody,
            STMT,
            "Rate-limit declaration.",
        ),
        P_RATE_LIMIT,
    ),
    produces(
        stmt(
            "audit",
            Context::CommandBody,
            SECTION,
            "Audit-logging block.",
        ),
        P_AUDIT,
    ),
    stmt(
        "approval",
        Context::CommandBody,
        SECTION,
        "Approval-chain block.",
    ),
    stmt(
        "target",
        Context::CommandBody,
        STMT,
        "Effect target resource.",
    ),
    stmt("let", Context::CommandBody, STMT, "Local binding."),
    stmt("validate", Context::CommandBody, STMT, "Inline validation."),
    produces(
        stmt("reorder", Context::CommandBody, STMT, "Reorder effect."),
        P_REORDER,
    ),
    stmt(
        "handler",
        Context::CommandBody,
        STMT,
        "Custom Go handler reference (`@fn.X`).",
    ),
    produces(
        stmt("emits", Context::CommandBody, STMT, "Event emission."),
        P_EMITS,
    ),
    stmt(
        "triggers",
        Context::CommandBody,
        STMT,
        "Triggers a lifecycle transition (`triggers transition <t>`).",
    ),
    stmt(
        "invalidates",
        Context::CommandBody,
        SECTION,
        "Cache invalidation block.",
    ),
    stmt("calls", Context::CommandBody, STMT, "External call."),
    stmt("timeout", Context::CommandBody, STMT, "Command timeout."),
    stmt("retry", Context::CommandBody, STMT, "Retry policy."),
    stmt(
        "idempotency",
        Context::CommandBody,
        STMT,
        "Idempotency key declaration.",
    ),
    stmt(
        "write_window",
        Context::CommandBody,
        STMT,
        "Write-window constraint.",
    ),
    stmt(
        "deprecated",
        Context::CommandBody,
        SECTION,
        "Deprecation metadata block.",
    ),
    stmt("gate", Context::CommandBody, STMT, "Entitlement gate."),
    // ── command: audit sub-block ──
    stmt(
        "emit_to",
        Context::Audit,
        "entity.name.function.statement.audit.lazuli",
        "Audit-event sink.",
    ),
    // ── command: approval sub-block ──
    stmt(
        "required_when",
        Context::Approval,
        "entity.name.function.statement.approval.lazuli",
        "Condition requiring approval.",
    ),
    // H2 reconcile: `chain`/`sequential` are approval-block statements — the
    // hand-written grammar colors them `entity.name.function.statement.approval`
    // (alongside `required_when`), not the generic statement leaf. Promote off
    // STMT so the generated `#kw-approval` alternation is faithful.
    stmt("chain", Context::Approval, APPROVAL, "Approval chain."),
    stmt(
        "sequential",
        Context::Approval,
        APPROVAL,
        "Sequential approval mode.",
    ),
    // ── command/api: deprecated sub-block (indent-6 children) ──
    stmt(
        "since",
        Context::Deprecated,
        "entity.name.function.statement.deprecated.lazuli",
        "Deprecation since-version.",
    ),
    stmt(
        "replacement",
        Context::Deprecated,
        "entity.name.function.statement.deprecated.lazuli",
        "Replacement reference.",
    ),
    stmt(
        "sunset",
        Context::Deprecated,
        "entity.name.function.statement.deprecated.lazuli",
        "Sunset date.",
    ),
    // ════════════════════════════════════════════════════════════════
    // Query body (indent-4) — QUERY_STATEMENT_KINDS
    // ════════════════════════════════════════════════════════════════
    stmt(
        "filters",
        Context::Query,
        SECTION,
        "Filter predicate block.",
    ),
    stmt("scope", Context::Query, SECTION, "Query scope block."),
    stmt(
        "modifier",
        Context::Query,
        STMT,
        "Query modifier reference.",
    ),
    stmt(
        "search",
        Context::Query,
        STMT,
        "Full-text search declaration.",
    ),
    stmt("order", Context::Query, STMT, "Default ordering."),
    stmt("sql", Context::Query, STMT, "Raw SQL body."),
    stmt("source", Context::Query, STMT, "Query source resource."),
    stmt("params", Context::Query, SECTION, "Query parameter block."),
    // ════════════════════════════════════════════════════════════════
    // Job / webhook / agent / notification / poller / report / channel
    // ════════════════════════════════════════════════════════════════
    stmt("queue", Context::Job, STMT, "Job queue."),
    stmt("trigger", Context::Job, STMT, "Job trigger."),
    stmt("schedule", Context::Job, STMT, "Job schedule."),
    stmt(
        "tenant_from",
        Context::Job,
        STMT,
        "Tenant resolution source.",
    ),
    stmt("fanout", Context::Job, STMT, "Fan-out declaration."),
    stmt("axis", Context::Job, STMT, "Fan-out axis."),
];
