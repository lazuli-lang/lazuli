//! Diagnostic-facet groups (`P_*`) + the `df`/`produces` const helpers.
//!
//! Split out of the former monolithic `registry.rs` (SPEC-19, <=500 LOC/file).
#![allow(dead_code)]

use crate::{CapabilitySpec, Context, DiagnosticFacet, SemanticToken, Sigil, Surface};

pub(crate) const fn df(
    code: &'static str,
    base: &'static str,
    cat: &'static str,
) -> DiagnosticFacet {
    DiagnosticFacet {
        code,
        base_severity: base,
        category: cat,
    }
}

/// Override the (H1-empty) `produces` facet of a capability row with the
/// diagnostic codes that capability guards. Const so the registry stays a
/// zero-cost `const` table.
pub(crate) const fn produces(
    mut spec: CapabilitySpec,
    facets: &'static [DiagnosticFacet],
) -> CapabilitySpec {
    spec.produces = facets;
    spec
}

// ── per-capability facet groups (one `const` per producing capability) ──

pub(crate) const P_AGGREGATE: &[DiagnosticFacet] = &[
    df("AGGREGATE-CONTAINS-UNKNOWN", "error", "vocabulary"),
    df("AGGREGATE-ROOT-UNKNOWN", "error", "vocabulary"),
];

pub(crate) const P_AUDIT: &[DiagnosticFacet] = &[
    df("AUDIT-MATERIALIZE-TARGET-001", "error", "vocabulary"),
    df("VOCAB-AUDIT-001", "warning", "vocabulary"),
    df("VOCAB-AUDIT-002", "warning", "vocabulary"),
];

pub(crate) const P_CHANNEL: &[DiagnosticFacet] =
    &[df("CHANNEL-PAYLOAD-001", "error", "vocabulary")];

pub(crate) const P_COMMAND: &[DiagnosticFacet] = &[
    df("COMMAND-INPUT-SHADOWS-FIELD-001", "error", "vocabulary"),
    df("MUTATION-WITHOUT-READBACK-001", "warning", "correctness"),
    df("HOOK-TARGET-001", "error", "correctness"),
    df(
        "CODEGEN-UNRESOLVED-BINDING-SOURCE-001",
        "error",
        "correctness",
    ),
];

pub(crate) const P_COMPOSITE_KEY: &[DiagnosticFacet] =
    &[df("COMPOSITE-KEY-CONTRACT-001", "error", "vocabulary")];

pub(crate) const P_COMPUTED_DATE: &[DiagnosticFacet] =
    &[df("COMPUTED-DATE-EXPR-001", "error", "vocabulary")];

pub(crate) const P_UNIQUE: &[DiagnosticFacet] = &[
    df("CONSTRAINT-UNIQUE-WHEN-001", "error", "vocabulary"),
    df("SLUG-UNIQUENESS-IMPLICIT", "warning", "vocabulary"),
];

pub(crate) const P_CROSS_FEATURE: &[DiagnosticFacet] = &[
    df(
        "CROSS-FEATURE-CONTRACT-MISSING-001",
        "error",
        "cross_feature",
    ),
    df(
        "CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001",
        "error",
        "cross_feature",
    ),
    df(
        "CROSS-FEATURE-WORKFLOW-SPAN-001",
        "warning",
        "cross_feature",
    ),
];

pub(crate) const P_QUERY: &[DiagnosticFacet] = &[
    df("DUPLICATE-QUERY-NAME-001", "error", "correctness"),
    df("MISSING-POLICY-ON-QUERY-001", "error", "correctness"),
    // A filter-predicate RHS (or field-default) enum-variant typo that would
    // silently lower to a `FromConst("<typo>")` literal that never matches.
    df("ENUM-VARIANT-UNDECLARED-001", "error", "correctness"),
];

pub(crate) const P_ENCRYPTION: &[DiagnosticFacet] = &[
    df("ENC-E2EE-EVENT-001", "error", "vocabulary"),
    df("ENC-KEY-MISSING-001", "error", "vocabulary"),
    df("ENC-ROTATION-001", "warning", "vocabulary"),
    df("ENC-SOURCE-ENV-001", "error", "vocabulary"),
    df("ENC-TEMPLATE-AXIS-001", "error", "vocabulary"),
    df("ENC-TENANCY-001", "error", "vocabulary"),
];

pub(crate) const P_ERRORS: &[DiagnosticFacet] = &[
    df("ERR-VOCAB-001", "warning", "error_vocab"),
    df("ERR-VOCAB-002", "warning", "error_vocab"),
    df("ERR-VOCAB-003", "warning", "error_vocab"),
    df("ERR-VOCAB-CODE-UNKNOWN", "error", "error_vocab"),
    df("ERR-VOCAB-EXPOSE-5XX-MESSAGE", "warning", "error_vocab"),
    df("ERR-VOCAB-EXPOSE-UNKNOWN", "error", "error_vocab"),
    df("ERR-VOCAB-WHEN-DENIED-NO-POLICY", "warning", "error_vocab"),
];

pub(crate) const P_EVENT_GROUP: &[DiagnosticFacet] =
    &[df("EVENT-GROUP-VARIANT-TYPE-001", "error", "vocabulary")];

pub(crate) const P_EMITS: &[DiagnosticFacet] = &[
    df("EVENT-OUTBOX-001", "warning", "vocabulary"),
    df("VOCAB-EVENT-ORPHAN-001", "warning", "vocabulary"),
    df("VOCAB-EVENT-PAYLOAD-001", "warning", "vocabulary"),
    df("VOCAB-EVENT-PRODUCER-001", "warning", "vocabulary"),
];

pub(crate) const P_FULL_TEXT: &[DiagnosticFacet] =
    &[df("FULL-TEXT-TYPE-001", "error", "vocabulary")];

pub(crate) const P_HOOK: &[DiagnosticFacet] = &[
    df("HANDLER-ERROR-WRAP-001", "warning", "error_handling"),
    df("HANDLER-NO-PANIC-001", "warning", "error_handling"),
    df("HANDLER-NO-STRING-ERROR-001", "warning", "error_handling"),
];

pub(crate) const P_FN: &[DiagnosticFacet] = &[
    df("HANDLER-MISSING-001", "error", "error_handling"),
    df("HANDLER-SIGNATURE-MISMATCH-001", "error", "error_handling"),
    df("HANDLER-SQL-COLUMN-DRIFT-001", "error", "error_handling"),
    df("VOCAB-HANDLER-HEAVY-001", "warning", "vocabulary"),
    // Spec 0024 — the reinvention audit oracle. Fires on a `@fn` handler that
    // reinvents a runtime/language primitive (argon2, token mint/hash,
    // lifecycle-transition shape, hex/soft-delete). `warning`/`vocabulary`
    // ⇒ advisory, non-gating.
    df("VOCAB-RUNTIME-REINVENTED-001", "warning", "vocabulary"),
];

pub(crate) const P_INVARIANTS: &[DiagnosticFacet] =
    &[df("INVARIANT-PREDICATE-INVALID", "error", "vocabulary")];

pub(crate) const P_JOB: &[DiagnosticFacet] = &[df(
    "JOB-DECLARATIVE-BODY-UNSUPPORTED-001",
    "warning",
    "error_handling",
)];

pub(crate) const P_LIFECYCLE: &[DiagnosticFacet] = &[
    df("LIFECYCLE-ENUM-DUPLICATE", "error", "lifecycle"),
    df("LIFECYCLE-FIELD-DOUBLE-DECLARED", "error", "lifecycle"),
    df("LIFECYCLE-INITIAL-AMBIGUOUS", "error", "lifecycle"),
    df("LIFECYCLE-INVARIANT-CATALOG-MISMATCH", "error", "lifecycle"),
    df("LIFECYCLE-INVARIANT-PARAM-UNRESOLVED", "error", "lifecycle"),
    df("LIFECYCLE-NO-INITIAL-STATE", "error", "lifecycle"),
    df("LIFECYCLE-NO-JUMP-NEEDS-LINEAR", "warning", "lifecycle"),
    df("LIFECYCLE-POLICY-REQUIRED", "warning", "lifecycle"),
    df("LIFECYCLE-STATE-DUPLICATE", "error", "lifecycle"),
    df("LIFECYCLE-STATE-SET-UNDECLARED-001", "error", "lifecycle"),
    df(
        "LIFECYCLE-TERMINAL-HAS-OUTGOING-TRANSITION",
        "error",
        "lifecycle",
    ),
    df("LIFECYCLE-TIMESTAMP-TYPE", "error", "lifecycle"),
    df("LIFECYCLE-TRANSITION-FROM-UNDECLARED", "error", "lifecycle"),
    df("LIFECYCLE-TRANSITION-TO-UNDECLARED", "error", "lifecycle"),
    df("LIFECYCLE-UNREACHABLE-STATE", "warning", "lifecycle"),
    df("VOCAB-LIFECYCLE-001", "warning", "vocabulary"),
];

pub(crate) const P_MANY_THROUGH: &[DiagnosticFacet] =
    &[df("MANY-THROUGH-ENDPOINT-001", "error", "vocabulary")];

// `@semantic` — semantic-scalar typing discipline (Money arithmetic +
// typed-JSON). Both guard "use a typed scalar, not a raw primitive".
pub(crate) const P_SEMANTIC: &[DiagnosticFacet] = &[
    df("MONEY-ARITHMETIC-001", "error", "vocabulary"),
    df("MONEY-COMPARE-001", "error", "vocabulary"),
    df("VOCAB-MONEY-MULTI-CURRENCY-001", "warning", "vocabulary"),
    df("VOCAB-MONEY-SHAPE-001", "warning", "vocabulary"),
    df("VOCAB-JSON-TYPED-001", "warning", "vocabulary"),
];

pub(crate) const P_POLICY: &[DiagnosticFacet] =
    &[df("POLICY-PREDICATE-001", "error", "vocabulary")];

// `adapter` (integrations block) — spec 0022's adapter wiring-contract
// check. `PLUGIN-CONTRACT-001` fires when a plugin's declared `implements`
// / `[binds]` interface is not a known framework bucket interface, or when
// its capability is bound to a different plugin. The shared classifier is
// `lazuli_manifest::plugin_contract::classify_adapter_contract`.
pub(crate) const P_PLUGIN_CONTRACT: &[DiagnosticFacet] =
    &[df("PLUGIN-CONTRACT-001", "error", "correctness")];

pub(crate) const P_POLLER: &[DiagnosticFacet] = &[
    df("POLLER-CURSOR-MISSING-001", "error", "poller"),
    df("POLLER-DUAL-SCHEDULER-001", "error", "poller"),
    df("POLLER-HANDLER-ORPHAN-001", "error", "poller"),
    df(
        "POLLER-IDEMPOTENCY-ATTEMPTS-MISSING-001",
        "warning",
        "poller",
    ),
    df("POLLER-MAX-RETRIES-UNBOUNDED-001", "warning", "poller"),
    df("POLLER-NO-TERMINAL-001", "error", "poller"),
    df("POLLER-QUIRK-CATALOG-MISMATCH-001", "error", "poller"),
    df("POLLER-TERMINAL-FIELD-ENUM-001", "error", "poller"),
    df("POLLER-TERMINAL-NO-EMIT-001", "warning", "poller"),
    df("POLLER-TICK-TOO-FAST-001", "warning", "poller"),
];

pub(crate) const P_REF: &[DiagnosticFacet] = &[
    df("REF-CROSS-FEATURE-UNKNOWN-001", "error", "vocabulary"),
    df("REF-POLYMORPHIC-TARGET-001", "error", "vocabulary"),
];

pub(crate) const P_REORDER: &[DiagnosticFacet] =
    &[df("REORDER-POSITION-FIELD-001", "error", "vocabulary")];

pub(crate) const P_REPORT: &[DiagnosticFacet] = &[
    df("REPORT-COLUMN-MISMATCH-001", "error", "report"),
    df("REPORT-COLUMNS-EMPTY-001", "error", "report"),
    df("REPORT-FILENAME-TOKEN-UNKNOWN-001", "error", "report"),
    df("REPORT-FORMAT-UNKNOWN-001", "error", "report"),
    df("REPORT-INPUT-UNBOUND-001", "error", "report"),
    df("REPORT-PATH-COLLISION-001", "error", "report"),
    df(
        "REPORT-POLICY-PUBLIC-NO-RATE-LIMIT-001",
        "warning",
        "report",
    ),
    df("REPORT-SIGNED-NO-STORAGE-001", "error", "report"),
    df("REPORT-SIGNED-TTL-FORBIDDEN-001", "error", "report"),
    df("REPORT-SIGNED-TTL-MISSING-001", "error", "report"),
    df("REPORT-SOURCE-KIND-001", "error", "report"),
    df("REPORT-STORAGE-AMBIGUOUS-001", "error", "report"),
];

pub(crate) const P_APPEND_ONLY: &[DiagnosticFacet] =
    &[df("RESOURCE-APPEND-ONLY-001", "error", "vocabulary")];

pub(crate) const P_LOCK: &[DiagnosticFacet] =
    &[df("RESOURCE-LOCK-CONTRACT-001", "error", "vocabulary")];

pub(crate) const P_ROUTE: &[DiagnosticFacet] = &[
    df(
        "ROUTE-GUARD-FIELD-MISSING-SERVER-PAIR-001",
        "error",
        "correctness",
    ),
    df(
        "ROUTE-GUARD-FIELD-TYPE-MISMATCH-006",
        "error",
        "correctness",
    ),
    df(
        "ROUTE-GUARD-FIELD-UNKNOWN-FEATURE-004",
        "error",
        "correctness",
    ),
    df(
        "ROUTE-GUARD-FIELD-UNKNOWN-FIELD-005",
        "error",
        "correctness",
    ),
    df(
        "ROUTE-GUARD-FORBID-ONLY-WHEN-RESOURCE-MISMATCH-007",
        "error",
        "correctness",
    ),
    df(
        "ROUTE-GUARD-LIFECYCLE-EXCLUSIVE-001",
        "error",
        "correctness",
    ),
    df("ROUTE-GUARD-LIFECYCLE-IN-EMPTY-002", "error", "correctness"),
    df(
        "ROUTE-GUARD-LIFECYCLE-IN-UNKNOWN-003",
        "error",
        "correctness",
    ),
    df("ROUTE-ID-UNUSED-IN-EFFECT-001", "warning", "correctness"),
    df(
        "ROUTE-LIFECYCLE-CANONICAL-FORM-001",
        "warning",
        "correctness",
    ),
];

pub(crate) const P_SCHEDULE: &[DiagnosticFacet] = &[df("SCHEDULE-RULE-001", "error", "vocabulary")];

pub(crate) const P_WEBHOOK: &[DiagnosticFacet] =
    &[df("WEBHOOK-EMIT-PREDICATE-FIELD-001", "error", "security")];

pub(crate) const P_TESTS: &[DiagnosticFacet] = &[
    df(
        "TEST-COMMAND-ASSERTION-DRIFT-001",
        "warning",
        "test_discipline",
    ),
    df("TEST-EVAL-VERB-RETIRED-001", "error", "test_discipline"),
    df(
        "TEST-FAILURE-ONLY-COVERAGE-001",
        "warning",
        "test_discipline",
    ),
    df("TEST-FIXTURE-LITERAL-001", "warning", "test_discipline"),
    df("TEST-HANDLER-MISSING-001", "error", "test_discipline"),
    df(
        "TEST-MATRIX-VERB-MISPLACED-001",
        "warning",
        "test_discipline",
    ),
    df("TEST-MISSING-AUTHORED-001", "warning", "test_discipline"),
    df("TEST-PINS-STUB-VOCAB-001", "warning", "test_discipline"),
    df("TEST-PREDICATE-UNCOVERED-001", "warning", "test_discipline"),
    df("TEST-RESTATES-EFFECT-001", "warning", "test_discipline"),
    df("TEST-RESTATES-POLICY-001", "warning", "test_discipline"),
    df("TEST-STUB-001", "warning", "test_discipline"),
    df("TEST-VIEW-DRIFT-001", "warning", "test_discipline"),
    df("TEST-VIEW-E2E-MISSING-001", "warning", "test_discipline"),
    df("TEST-VIEW-EXTENSIBILITY-001", "warning", "test_discipline"),
    df(
        "TEST-VIEW-EXTENSION-VERB-RETIRED-001",
        "error",
        "test_discipline",
    ),
    df("VOCAB-TESTS-MISSING-001", "warning", "vocabulary"),
];

pub(crate) const P_CAP: &[DiagnosticFacet] =
    &[df("VOCAB-CAP-MISSING-001", "warning", "vocabulary")];

// Iron-hand context vocabulary — `purpose` / `non_goals` keyword facets.
// (`attach_ctx` was retired → its row + the `P_ATTACH_CTX` const are
// gone; the orphaned `VOCAB-CONTEXT-PURPOSE-001` / `-NONGOALS-001` codes
// are re-homed onto the surviving bare keywords below, and
// `VOCAB-CONTEXT-CTXMD-001` — now a convention-derived rule with no
// keyword owner — moves to `GLOBAL_DIAGNOSTICS`.)
pub(crate) const P_PURPOSE: &[DiagnosticFacet] =
    &[df("VOCAB-CONTEXT-PURPOSE-001", "warning", "vocabulary")];

pub(crate) const P_NONGOALS: &[DiagnosticFacet] =
    &[df("VOCAB-CONTEXT-NONGOALS-001", "warning", "vocabulary")];

// Knowledge-sector vocabulary — the five `VOCAB-KNOWLEDGE-*` rules
// (`crates/lazuli_doctor/src/vocab/vocab_knowledge_*`) cross-check
// `knowledge <sector>` against the `knowledge/<sector>/` document
// vault. Same `Vocabulary` category + `warning` posture as the sibling
// `VOCAB-CONTEXT-*` family above. See
// `docs/proposals/knowledge-sector-field.md` §Doctor.
pub(crate) const P_KNOWLEDGE: &[DiagnosticFacet] = &[
    df("VOCAB-KNOWLEDGE-DANGLING-CITE-001", "warning", "vocabulary"),
    df("VOCAB-KNOWLEDGE-DUP-TOPIC-001", "warning", "vocabulary"),
    df(
        "VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001",
        "warning",
        "vocabulary",
    ),
    df(
        "VOCAB-KNOWLEDGE-SINGLE-FEATURE-001",
        "warning",
        "vocabulary",
    ),
    df("VOCAB-KNOWLEDGE-STALE-001", "warning", "vocabulary"),
    df("VOCAB-KNOWLEDGE-UNGATED-WRITE-001", "warning", "vocabulary"),
];

pub(crate) const P_DERIVED: &[DiagnosticFacet] =
    &[df("VOCAB-DERIVED-READ-001", "warning", "vocabulary")];

pub(crate) const P_RESOURCE: &[DiagnosticFacet] = &[
    df("VOCAB-RESOURCE-WIDE-CLUSTER-001", "warning", "vocabulary"),
    df("VOCAB-GRAMMAR-FORM-001", "warning", "vocabulary"),
    df("UPDATES-MISSING-UPDATED-AT-001", "warning", "correctness"),
];

pub(crate) const P_RECORD: &[DiagnosticFacet] =
    &[df("VOCAB-SHADOW-RECORD-001", "warning", "vocabulary")];

pub(crate) const P_UNION: &[DiagnosticFacet] = &[
    df("VOCAB-UNION-001", "warning", "vocabulary"),
    df("VOCAB-UNION-002", "warning", "vocabulary"),
];

pub(crate) const P_OWNER_AXIS: &[DiagnosticFacet] = &[
    df(
        "owner_axis_collides_with_unique_user",
        "error",
        "vocabulary",
    ),
    df("owner_axis_on_non_fk", "error", "vocabulary"),
    df("owner_axis_through_not_user_keyed", "error", "vocabulary"),
    df("owner_axis_unknown_through", "error", "vocabulary"),
];

pub(crate) const P_RATE_LIMIT: &[DiagnosticFacet] = &[
    df("rate_limit_duplicate_default", "error", "vocabulary"),
    df("rate_limit_duplicate_env", "error", "vocabulary"),
    df("rate_limit_invalid_spec", "error", "vocabulary"),
    df(
        "rate_limit_no_default_with_qualifications",
        "error",
        "vocabulary",
    ),
    df("rate_limit_unknown_env", "error", "vocabulary"),
];

pub(crate) const P_CONVENTIONS: &[DiagnosticFacet] = &[
    df("conventions_unknown", "warning", "vocabulary"),
    // Spec 0002 — inverse-synth adoption nudge. Advisory: base severity
    // `warning`; the `vocabulary` category keeps it out of the gating set so
    // `lazuli check`/`doctor` exit codes never change.
    df("VOCAB-CRUD-SYNTH-AVAILABLE-001", "warning", "vocabulary"),
    // Spec 0015 — soft_delete actor-projection adoption nudge. Advisory:
    // fires on a hand-rolled `deleted_at` + `deleted_by` field pair, points
    // at the `soft_delete by` trait. `warning`/`vocabulary` ⇒ non-gating.
    df("VOCAB-SOFT-DELETE-ACTOR-001", "warning", "vocabulary"),
];

pub(crate) const P_DESIGN: &[DiagnosticFacet] = &[
    df("design-custom-duplicate", "error", "vocabulary"),
    df("design-custom-invalid-value", "error", "vocabulary"),
    df("design-custom-reserved-name", "error", "vocabulary"),
    df("design-token-duplicate-value", "warning", "vocabulary"),
    df("design-token-fontfamily-leak", "warning", "vocabulary"),
    df("design-token-hex-leak", "warning", "vocabulary"),
    df("design-token-missing-dark", "warning", "vocabulary"),
    df("design-token-px-leak", "warning", "vocabulary"),
    df("design-token-shadow-leak", "warning", "vocabulary"),
    df("design-token-undefined", "error", "vocabulary"),
    df("design-token-unused", "warning", "vocabulary"),
];
