//! The populated capability registry — `ALL`.
//!
//! Every keyword/construct the Lazuli parser recognizes, ported from the
//! three pre-existing hand-maintained copies:
//!
//! * **parser literals** (`lazuli_syntax/src/parser/**`) — the
//!   authoritative set of what EXISTS; the proven-complete test asserts
//!   every parser keyword literal appears here.
//! * **LSP catalogs** — `lazuli_lsp/src/keywords.rs` `KEYWORDS` /
//!   `DESIGN_KEYWORDS` (hover text), the `*_BODY_KINDS` /
//!   `*_STATEMENT_KINDS` (per-context membership), `feature.rs`
//!   `FEATURE_BODY_KINDS`.
//! * **tmLanguage scopes** — `editors/vscode/syntaxes/lazuli.tmLanguage.json`
//!   + `editors/vscode/SCOPES.md` (the TextMate scope each keyword gets).
//!
//! A keyword valid in N contexts with different scopes is N rows
//! (context-as-data). Wave C1 backfilled the `produces` diagnostic-code
//! facets: each producing capability row is wrapped with `produces(row,
//! P_<CAP>)`, where `P_<CAP>` mirrors the live `lazuli_doctor` rule `CODE`
//! consts that capability guards. Cross-cutting codes not bound to a single
//! keyword live in [`crate::GLOBAL_DIAGNOSTICS`]. The
//! `lazuli_diagnostics_registry` bridge crate asserts every facet resolves
//! to a live rule and that every live code is claimed exactly once.

use crate::{CapabilitySpec, Context, DiagnosticFacet, SemanticToken, Sigil, Surface};

// ════════════════════════════════════════════════════════════════════
// Wave C1 — diagnostic-code facets (`produces`)
//
// Each `DiagnosticFacet` mirrors a live `lazuli_doctor` rule `CODE` const:
// `{code, base_severity, category}`. The categories below are the verbatim
// output of `RuleCategory::from_code_prefix(code)` (asserted equal by the
// `lazuli_diagnostics_registry` bridge crate's `resolvable` test). The
// `base_severity` is the rule's documented intrinsic posture (the
// `//! Severity:` line in each rule module), validated well-formed by the
// bridge. The bridge's `complete` test asserts every live CODE is claimed
// exactly once across these groups + `GLOBAL_DIAGNOSTICS`.
//
// `category` mirror strings MUST match `RuleCategory::as_str()`:
// "vocabulary" | "correctness" | "security" | "test_discipline" |
// "design" | "encryption" | "lifecycle" | "domain" | "cross_feature" |
// "error_vocab" | "poller" | "report" | "internal_hygiene" |
// "error_handling" | "lzi_hygiene".
// ════════════════════════════════════════════════════════════════════

/// Construct one diagnostic facet. `code`/`base`/`cat` are string mirrors;
/// the bridge crate (над `lazuli_doctor`) asserts coherence.
const fn df(code: &'static str, base: &'static str, cat: &'static str) -> DiagnosticFacet {
    DiagnosticFacet {
        code,
        base_severity: base,
        category: cat,
    }
}

/// Override the (H1-empty) `produces` facet of a capability row with the
/// diagnostic codes that capability guards. Const so the registry stays a
/// zero-cost `const` table.
const fn produces(mut spec: CapabilitySpec, facets: &'static [DiagnosticFacet]) -> CapabilitySpec {
    spec.produces = facets;
    spec
}

// ── per-capability facet groups (one `const` per producing capability) ──

const P_AGGREGATE: &[DiagnosticFacet] = &[
    df("AGGREGATE-CONTAINS-UNKNOWN", "error", "vocabulary"),
    df("AGGREGATE-ROOT-UNKNOWN", "error", "vocabulary"),
];

const P_AUDIT: &[DiagnosticFacet] = &[
    df("AUDIT-MATERIALIZE-TARGET-001", "error", "vocabulary"),
    df("VOCAB-AUDIT-001", "warning", "vocabulary"),
    df("VOCAB-AUDIT-002", "warning", "vocabulary"),
];

const P_CHANNEL: &[DiagnosticFacet] = &[df("CHANNEL-PAYLOAD-001", "error", "vocabulary")];

const P_COMMAND: &[DiagnosticFacet] = &[
    df("COMMAND-INPUT-SHADOWS-FIELD-001", "error", "vocabulary"),
    df("MUTATION-WITHOUT-READBACK-001", "warning", "correctness"),
    df("HOOK-TARGET-001", "error", "correctness"),
];

const P_COMPOSITE_KEY: &[DiagnosticFacet] =
    &[df("COMPOSITE-KEY-CONTRACT-001", "error", "vocabulary")];

const P_COMPUTED_DATE: &[DiagnosticFacet] = &[df("COMPUTED-DATE-EXPR-001", "error", "vocabulary")];

const P_UNIQUE: &[DiagnosticFacet] = &[
    df("CONSTRAINT-UNIQUE-WHEN-001", "error", "vocabulary"),
    df("SLUG-UNIQUENESS-IMPLICIT", "warning", "vocabulary"),
];

const P_CROSS_FEATURE: &[DiagnosticFacet] = &[
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

const P_QUERY: &[DiagnosticFacet] = &[
    df("DUPLICATE-QUERY-NAME-001", "error", "correctness"),
    df("MISSING-POLICY-ON-QUERY-001", "error", "correctness"),
];

const P_ENCRYPTION: &[DiagnosticFacet] = &[
    df("ENC-E2EE-EVENT-001", "error", "vocabulary"),
    df("ENC-KEY-MISSING-001", "error", "vocabulary"),
    df("ENC-ROTATION-001", "warning", "vocabulary"),
    df("ENC-SOURCE-ENV-001", "error", "vocabulary"),
    df("ENC-TEMPLATE-AXIS-001", "error", "vocabulary"),
    df("ENC-TENANCY-001", "error", "vocabulary"),
];

const P_ERRORS: &[DiagnosticFacet] = &[
    df("ERR-VOCAB-001", "warning", "error_vocab"),
    df("ERR-VOCAB-002", "warning", "error_vocab"),
    df("ERR-VOCAB-003", "warning", "error_vocab"),
    df("ERR-VOCAB-CODE-UNKNOWN", "error", "error_vocab"),
    df("ERR-VOCAB-EXPOSE-5XX-MESSAGE", "warning", "error_vocab"),
    df("ERR-VOCAB-EXPOSE-UNKNOWN", "error", "error_vocab"),
    df("ERR-VOCAB-WHEN-DENIED-NO-POLICY", "warning", "error_vocab"),
];

const P_EVENT_GROUP: &[DiagnosticFacet] =
    &[df("EVENT-GROUP-VARIANT-TYPE-001", "error", "vocabulary")];

const P_EMITS: &[DiagnosticFacet] = &[
    df("EVENT-OUTBOX-001", "warning", "vocabulary"),
    df("VOCAB-EVENT-ORPHAN-001", "warning", "vocabulary"),
    df("VOCAB-EVENT-PAYLOAD-001", "warning", "vocabulary"),
    df("VOCAB-EVENT-PRODUCER-001", "warning", "vocabulary"),
];

const P_FULL_TEXT: &[DiagnosticFacet] = &[df("FULL-TEXT-TYPE-001", "error", "vocabulary")];

const P_HOOK: &[DiagnosticFacet] = &[
    df("HANDLER-ERROR-WRAP-001", "warning", "error_handling"),
    df("HANDLER-NO-PANIC-001", "warning", "error_handling"),
    df("HANDLER-NO-STRING-ERROR-001", "warning", "error_handling"),
];

const P_FN: &[DiagnosticFacet] = &[
    df("HANDLER-MISSING-001", "error", "error_handling"),
    df("HANDLER-SIGNATURE-MISMATCH-001", "error", "error_handling"),
    df("HANDLER-SQL-COLUMN-DRIFT-001", "error", "error_handling"),
    df("VOCAB-HANDLER-HEAVY-001", "warning", "vocabulary"),
];

const P_INVARIANTS: &[DiagnosticFacet] =
    &[df("INVARIANT-PREDICATE-INVALID", "error", "vocabulary")];

const P_JOB: &[DiagnosticFacet] =
    &[df("JOB-DECLARATIVE-BODY-UNSUPPORTED-001", "warning", "error_handling")];

const P_LIFECYCLE: &[DiagnosticFacet] = &[
    df("LIFECYCLE-ENUM-DUPLICATE", "error", "lifecycle"),
    df("LIFECYCLE-FIELD-DOUBLE-DECLARED", "error", "lifecycle"),
    df("LIFECYCLE-INITIAL-AMBIGUOUS", "error", "lifecycle"),
    df("LIFECYCLE-INVARIANT-CATALOG-MISMATCH", "error", "lifecycle"),
    df("LIFECYCLE-INVARIANT-PARAM-UNRESOLVED", "error", "lifecycle"),
    df("LIFECYCLE-NO-INITIAL-STATE", "error", "lifecycle"),
    df("LIFECYCLE-NO-JUMP-NEEDS-LINEAR", "warning", "lifecycle"),
    df("LIFECYCLE-POLICY-REQUIRED", "warning", "lifecycle"),
    df("LIFECYCLE-STATE-DUPLICATE", "error", "lifecycle"),
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

const P_MANY_THROUGH: &[DiagnosticFacet] =
    &[df("MANY-THROUGH-ENDPOINT-001", "error", "vocabulary")];

// `@semantic` — semantic-scalar typing discipline (Money arithmetic +
// typed-JSON). Both guard "use a typed scalar, not a raw primitive".
const P_SEMANTIC: &[DiagnosticFacet] = &[
    df("MONEY-ARITHMETIC-001", "error", "vocabulary"),
    df("MONEY-COMPARE-001", "error", "vocabulary"),
    df("VOCAB-MONEY-MULTI-CURRENCY-001", "warning", "vocabulary"),
    df("VOCAB-JSON-TYPED-001", "warning", "vocabulary"),
];

const P_POLICY: &[DiagnosticFacet] = &[df("POLICY-PREDICATE-001", "error", "vocabulary")];

const P_POLLER: &[DiagnosticFacet] = &[
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

const P_REF: &[DiagnosticFacet] = &[
    df("REF-CROSS-FEATURE-UNKNOWN-001", "error", "vocabulary"),
    df("REF-POLYMORPHIC-TARGET-001", "error", "vocabulary"),
];

const P_REORDER: &[DiagnosticFacet] = &[df("REORDER-POSITION-FIELD-001", "error", "vocabulary")];

const P_REPORT: &[DiagnosticFacet] = &[
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

const P_APPEND_ONLY: &[DiagnosticFacet] = &[df("RESOURCE-APPEND-ONLY-001", "error", "vocabulary")];

const P_LOCK: &[DiagnosticFacet] = &[df("RESOURCE-LOCK-CONTRACT-001", "error", "vocabulary")];

const P_ROUTE: &[DiagnosticFacet] = &[
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

const P_SCHEDULE: &[DiagnosticFacet] = &[df("SCHEDULE-RULE-001", "error", "vocabulary")];

const P_WEBHOOK: &[DiagnosticFacet] =
    &[df("WEBHOOK-EMIT-PREDICATE-FIELD-001", "error", "security")];

const P_TESTS: &[DiagnosticFacet] = &[
    df(
        "TEST-COMMAND-ASSERTION-DRIFT-001",
        "warning",
        "test_discipline",
    ),
    df(
        "TEST-FAILURE-ONLY-COVERAGE-001",
        "warning",
        "test_discipline",
    ),
    df("TEST-FIXTURE-LITERAL-001", "warning", "test_discipline"),
    df("TEST-HANDLER-MISSING-001", "error", "test_discipline"),
    df("TEST-MISSING-AUTHORED-001", "warning", "test_discipline"),
    df("TEST-PINS-STUB-VOCAB-001", "warning", "test_discipline"),
    df("TEST-PREDICATE-UNCOVERED-001", "warning", "test_discipline"),
    df("TEST-RESTATES-EFFECT-001", "warning", "test_discipline"),
    df("TEST-RESTATES-POLICY-001", "warning", "test_discipline"),
    df("TEST-STUB-001", "warning", "test_discipline"),
    df("TEST-VIEW-DRIFT-001", "warning", "test_discipline"),
    df("TEST-VIEW-E2E-MISSING-001", "warning", "test_discipline"),
    df("TEST-VIEW-EXTENSIBILITY-001", "warning", "test_discipline"),
    df("VOCAB-TESTS-MISSING-001", "warning", "vocabulary"),
];

const P_CAP: &[DiagnosticFacet] = &[df("VOCAB-CAP-MISSING-001", "warning", "vocabulary")];

const P_ATTACH_CTX: &[DiagnosticFacet] = &[
    df("VOCAB-CONTEXT-CTXMD-001", "warning", "vocabulary"),
    df("VOCAB-CONTEXT-NONGOALS-001", "warning", "vocabulary"),
    df("VOCAB-CONTEXT-PURPOSE-001", "warning", "vocabulary"),
];

// Knowledge-sector vocabulary — the five `VOCAB-KNOWLEDGE-*` rules
// (`crates/lazuli_doctor/src/vocab/vocab_knowledge_*`) cross-check
// `knowledge <sector>` against the `knowledge/<sector>/` document
// vault. Same `Vocabulary` category + `warning` posture as the sibling
// `VOCAB-CONTEXT-*` family above. See
// `docs/proposals/knowledge-sector-field.md` §Doctor.
const P_KNOWLEDGE: &[DiagnosticFacet] = &[
    df("VOCAB-KNOWLEDGE-DANGLING-CITE-001", "warning", "vocabulary"),
    df("VOCAB-KNOWLEDGE-DUP-TOPIC-001", "warning", "vocabulary"),
    df("VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001", "warning", "vocabulary"),
    df("VOCAB-KNOWLEDGE-STALE-001", "warning", "vocabulary"),
    df("VOCAB-KNOWLEDGE-UNGATED-WRITE-001", "warning", "vocabulary"),
];

const P_DERIVED: &[DiagnosticFacet] = &[df("VOCAB-DERIVED-READ-001", "warning", "vocabulary")];

const P_RESOURCE: &[DiagnosticFacet] = &[
    df("VOCAB-RESOURCE-WIDE-CLUSTER-001", "warning", "vocabulary"),
    df("VOCAB-GRAMMAR-FORM-001", "warning", "vocabulary"),
    df("UPDATES-MISSING-UPDATED-AT-001", "warning", "correctness"),
];

const P_RECORD: &[DiagnosticFacet] = &[df("VOCAB-SHADOW-RECORD-001", "warning", "vocabulary")];

const P_UNION: &[DiagnosticFacet] = &[
    df("VOCAB-UNION-001", "warning", "vocabulary"),
    df("VOCAB-UNION-002", "warning", "vocabulary"),
];

const P_OWNER_AXIS: &[DiagnosticFacet] = &[
    df(
        "owner_axis_collides_with_unique_user",
        "error",
        "vocabulary",
    ),
    df("owner_axis_on_non_fk", "error", "vocabulary"),
    df("owner_axis_through_not_user_keyed", "error", "vocabulary"),
    df("owner_axis_unknown_through", "error", "vocabulary"),
];

const P_RATE_LIMIT: &[DiagnosticFacet] = &[
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

const P_CONVENTIONS: &[DiagnosticFacet] = &[df("conventions_unknown", "warning", "vocabulary")];

const P_DESIGN: &[DiagnosticFacet] = &[
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

// ── tmLanguage scope-leaf constants (SCOPES.md taxonomy) ──
const DECL: &str = "keyword.control.declaration.structural.lazuli";
const SECTION: &str = "keyword.control.section.lazuli";
const STMT: &str = "keyword.control.statement.lazuli";
const MODIFIER: &str = "storage.modifier.lazuli";
const DECORATOR: &str = "entity.name.tag.decorator.lazuli";
const OP_LOGICAL: &str = "keyword.operator.logical.lazuli";
const OP_PREDICATE: &str = "keyword.operator.predicate.lazuli";
const TYPE_CTOR: &str = "support.function.type-constructor.lazuli";

// ── per-block statement-scope leaves (used by the H2 tmLanguage backfill) ──
const TESTS: &str = "entity.name.function.statement.tests.lazuli";
const TRANSLATION: &str = "entity.name.function.statement.translation.lazuli";
const HEADERS: &str = "entity.name.function.statement.headers.lazuli";
const LIMITS: &str = "entity.name.function.statement.limits.lazuli";
const ENCRYPTION: &str = "entity.name.function.statement.encryption.lazuli";
const TRACING: &str = "entity.name.function.statement.tracing.lazuli";
const DEPLOY: &str = "entity.name.function.statement.deploy.lazuli";
const COMMUNICATION: &str = "entity.name.function.statement.communication.lazuli";
const ENV: &str = "entity.name.function.statement.env.lazuli";
const INTEGRATION: &str = "entity.name.function.statement.integration.lazuli";
const PACKS: &str = "entity.name.function.statement.packs.lazuli";
const DEFAULTS: &str = "entity.name.function.statement.defaults.lazuli";
const AUDIT: &str = "entity.name.function.statement.audit.lazuli";
const APPROVAL: &str = "entity.name.function.statement.approval.lazuli";
const REPLAY: &str = "entity.name.function.statement.replay.lazuli";

/// Build a bare-keyword `.lzi` capability row with no `produces` facet.
const fn kw(
    literal: &'static str,
    context: Context,
    scope: &'static str,
    hover: &'static str,
) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context,
        scope,
        token: SemanticToken::Keyword,
        surface: Surface::Lzi,
        sigil: None,
        hover,
        produces: &[],
    }
}

/// Per-block statement-keyword scope leaf (`entity.name.function.statement.<block>.lazuli`).
const fn stmt(
    literal: &'static str,
    context: Context,
    scope: &'static str,
    hover: &'static str,
) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context,
        scope,
        token: SemanticToken::Keyword,
        surface: Surface::Lzi,
        sigil: None,
        hover,
        produces: &[],
    }
}

/// A closed-catalog VALUE row (`constant.language.<catalog>.lazuli`).
const fn value(literal: &'static str, scope: &'static str) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context: Context::Value,
        scope,
        token: SemanticToken::EnumMember,
        surface: Surface::Lzi,
        sigil: None,
        hover: "",
        produces: &[],
    }
}

/// A cross-cutting `storage.modifier` word.
const fn modifier(literal: &'static str, hover: &'static str) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context: Context::Modifier,
        scope: MODIFIER,
        token: SemanticToken::Modifier,
        surface: Surface::Lzi,
        sigil: None,
        hover,
        produces: &[],
    }
}

/// A filter/policy/test expression operator or predicate.
const fn op(literal: &'static str, scope: &'static str) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context: Context::Expression,
        scope,
        token: SemanticToken::Operator,
        surface: Surface::Lzi,
        sigil: None,
        hover: "",
        produces: &[],
    }
}

/// An `@`-decorator namespace row.
const fn decorator(literal: &'static str, hover: &'static str) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context: Context::ResourceBody,
        scope: DECORATOR,
        token: SemanticToken::Decorator,
        surface: Surface::Lzi,
        sigil: Some(Sigil::At),
        hover,
        produces: &[],
    }
}

/// A dotted-kind feature-body declaration (`query.list`, `event.trace`).
const fn dotted(literal: &'static str, context: Context, hover: &'static str) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context,
        scope: DECL,
        token: SemanticToken::Keyword,
        surface: Surface::Lzi,
        sigil: Some(Sigil::DottedKind),
        hover,
        produces: &[],
    }
}

/// A `*.design.lzi`-surface keyword row.
const fn design(literal: &'static str, hover: &'static str) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context: Context::Design,
        scope: SECTION,
        token: SemanticToken::Keyword,
        surface: Surface::Design,
        sigil: None,
        hover,
        produces: &[],
    }
}

/// An `.lzx` surface-view keyword row.
const fn lzx(
    literal: &'static str,
    context: Context,
    scope: &'static str,
    hover: &'static str,
) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context,
        scope,
        token: SemanticToken::Keyword,
        surface: Surface::Lzx,
        sigil: None,
        hover,
        produces: &[],
    }
}

/// The single iterable every downstream surface derives-from or is
/// asserted-against. One row per `(literal, context)` pair.
pub const ALL: &[CapabilitySpec] = &[
    // ════════════════════════════════════════════════════════════════
    // Top-level declarations (indent 0) — Surface::App / Lzi
    // ════════════════════════════════════════════════════════════════
    CapabilitySpec {
        literal: "workspace",
        context: Context::TopLevel,
        scope: DECL,
        token: SemanticToken::Keyword,
        surface: Surface::App,
        sigil: None,
        hover: "Declares a multi-app workspace root.",
        produces: &[],
    },
    CapabilitySpec {
        literal: "app",
        context: Context::TopLevel,
        scope: DECL,
        token: SemanticToken::Keyword,
        surface: Surface::App,
        sigil: None,
        hover: "Declares an application: targets, urls, runtime, deploy topology.",
        produces: &[],
    },
    CapabilitySpec {
        literal: "registry",
        context: Context::TopLevel,
        scope: DECL,
        token: SemanticToken::Keyword,
        surface: Surface::Registry,
        sigil: None,
        hover: "Declares the shared integration/capability registry.",
        produces: &[],
    },
    CapabilitySpec {
        literal: "profile",
        context: Context::TopLevel,
        scope: DECL,
        token: SemanticToken::Keyword,
        surface: Surface::App,
        sigil: None,
        hover: "Declares a deployment/runtime profile.",
        produces: &[],
    },
    kw(
        "feature",
        Context::TopLevel,
        DECL,
        "Declares a feature: the unit of business capability.",
    ),
    produces(
        kw(
            "design",
            Context::TopLevel,
            DECL,
            "Declares the project-root design token catalog.",
        ),
        P_DESIGN,
    ),
    kw(
        "plan",
        Context::TopLevel,
        DECL,
        "Declares a billing/entitlement plan.",
    ),
    kw(
        "gate",
        Context::TopLevel,
        DECL,
        "Top-level feature/limit gating directive.",
    ),
    produces(
        kw(
            "route",
            Context::TopLevel,
            DECL,
            "Top-level route declaration.",
        ),
        P_ROUTE,
    ),
    kw(
        "permission",
        Context::TopLevel,
        DECL,
        "RBAC permission catalog entry.",
    ),
    kw("role", Context::TopLevel, DECL, "RBAC role catalog entry."),
    kw(
        "experience",
        Context::TopLevel,
        DECL,
        "Declares a shared surface experience.",
    ),
    kw(
        "contract",
        Context::TopLevel,
        DECL,
        "Declares a service contract.",
    ),
    kw(
        "error_page",
        Context::TopLevel,
        DECL,
        "Declares an app-level error page.",
    ),
    kw(
        "escape_route",
        Context::TopLevel,
        DECL,
        "Documents a deliberate framework escape hatch.",
    ),
    kw(
        "shared_registry",
        Context::TopLevel,
        DECL,
        "Workspace-level shared registry reference.",
    ),
    kw(
        "apps",
        Context::TopLevel,
        SECTION,
        "Workspace `apps` listing block.",
    ),
    kw(
        "boundaries",
        Context::TopLevel,
        SECTION,
        "Workspace service-boundary declarations.",
    ),
    kw(
        "gateway",
        Context::TopLevel,
        SECTION,
        "Workspace API gateway block.",
    ),
    kw("grants", Context::TopLevel, STMT, "RBAC grant statement."),
    kw(
        "grants_all",
        Context::TopLevel,
        STMT,
        "RBAC grant-all statement.",
    ),
    kw(
        "revoke_user",
        Context::TopLevel,
        STMT,
        "RBAC revoke-user action.",
    ),
    kw(
        "revoke_session_family",
        Context::TopLevel,
        STMT,
        "RBAC revoke-session-family action.",
    ),
    kw(
        "skeleton",
        Context::TopLevel,
        SECTION,
        "Package skeleton block.",
    ),
    // ════════════════════════════════════════════════════════════════
    // App body (indent-2 kinds + app-meta) — APP_BODY_KINDS
    // ════════════════════════════════════════════════════════════════
    kw(
        "architecture",
        Context::App,
        SECTION,
        "App architecture / service-boundary block.",
    ),
    kw(
        "actor_query",
        Context::App,
        STMT,
        "App-level actor resolution query.",
    ),
    kw(
        "auth_failed_redirect",
        Context::App,
        STMT,
        "Redirect target on auth failure.",
    ),
    kw(
        "bindings",
        Context::App,
        SECTION,
        "Registry binding overrides for this app.",
    ),
    kw(
        "capabilities",
        Context::App,
        SECTION,
        "App capability declarations.",
    ),
    kw(
        "communication",
        Context::App,
        SECTION,
        "Inter-service communication block.",
    ),
    kw(
        "cookie",
        Context::App,
        SECTION,
        "App cookie defaults block.",
    ),
    kw("cors", Context::App, SECTION, "CORS configuration block."),
    kw(
        "default_locale",
        Context::App,
        STMT,
        "Default locale for the app.",
    ),
    kw(
        "default_timezone",
        Context::App,
        STMT,
        "Default timezone for the app.",
    ),
    kw(
        "deploy",
        Context::App,
        SECTION,
        "Deployment topology + migration policy block.",
    ),
    produces(
        kw(
            "encryption",
            Context::App,
            SECTION,
            "Field-encryption configuration block.",
        ),
        P_ENCRYPTION,
    ),
    kw(
        "environments",
        Context::App,
        SECTION,
        "Named environment declarations.",
    ),
    kw(
        "headers",
        Context::App,
        SECTION,
        "Security/response header defaults block.",
    ),
    kw(
        "integrations",
        Context::App,
        SECTION,
        "Third-party integration declarations.",
    ),
    kw(
        "lazuli_version",
        Context::App,
        STMT,
        "Pinned Lazuli framework version.",
    ),
    kw(
        "limits",
        Context::App,
        SECTION,
        "Request/body size + timeout limits block.",
    ),
    kw("locale", Context::App, SECTION, "Locale negotiation block."),
    kw(
        "logging",
        Context::App,
        SECTION,
        "Structured logging configuration block.",
    ),
    kw(
        "not_found",
        Context::App,
        STMT,
        "404 / not-found handler reference.",
    ),
    kw(
        "packs",
        Context::App,
        SECTION,
        "Feature-pack inclusion block.",
    ),
    kw(
        "proxy",
        Context::App,
        SECTION,
        "Trusted-proxy / forwarded-header block.",
    ),
    kw(
        "runtime",
        Context::App,
        SECTION,
        "Runtime unit / process topology block.",
    ),
    kw(
        "services",
        Context::App,
        SECTION,
        "Service decomposition block.",
    ),
    kw(
        "targets",
        Context::App,
        STMT,
        "Generation targets (go/ts/...).",
    ),
    kw("title", Context::App, STMT, "App display title."),
    kw(
        "tracing",
        Context::App,
        SECTION,
        "Distributed-tracing configuration block.",
    ),
    kw(
        "urls",
        Context::App,
        SECTION,
        "Named URL declarations block.",
    ),
    kw(
        "uses",
        Context::App,
        STMT,
        "Declares a registry/experience the app uses.",
    ),
    kw("version", Context::App, STMT, "App version string."),
    kw(
        "env",
        Context::App,
        SECTION,
        "Environment-variable declarations block.",
    ),
    kw("route_guard", Context::App, STMT, "App-level route guard."),
    // app-meta scalar lines (statements-app-meta scope leaf)
    stmt(
        "mode",
        Context::App,
        "entity.name.function.statement.app-meta.lazuli",
        "Service mode (monolith/service).",
    ),
    stmt(
        "service_ready",
        Context::App,
        "entity.name.function.statement.app-meta.lazuli",
        "Marks the app service-ready.",
    ),
    stmt(
        "enforce_service_boundaries",
        Context::App,
        "entity.name.function.statement.app-meta.lazuli",
        "Enforce declared service boundaries.",
    ),
    stmt(
        "environment",
        Context::App,
        "entity.name.function.statement.app-meta.lazuli",
        "Environment selector.",
    ),
    // ── app: cookie block ──
    stmt(
        "default",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "Default cookie profile.",
    ),
    stmt(
        "session",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "Session cookie settings.",
    ),
    stmt(
        "csrf",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "CSRF cookie settings.",
    ),
    stmt(
        "signed",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "Signed-cookie flag.",
    ),
    stmt(
        "secure",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "Secure (HTTPS-only) flag.",
    ),
    stmt(
        "http_only",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "HttpOnly flag.",
    ),
    stmt(
        "same_site",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "SameSite policy.",
    ),
    stmt(
        "max_age",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "Cookie max-age.",
    ),
    stmt(
        "domain",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "Cookie domain.",
    ),
    stmt(
        "path",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "Cookie path.",
    ),
    // ── app: headers block ──
    stmt(
        "csp",
        Context::Headers,
        "entity.name.function.statement.headers.lazuli",
        "Content-Security-Policy header.",
    ),
    stmt(
        "hsts",
        Context::Headers,
        "entity.name.function.statement.headers.lazuli",
        "Strict-Transport-Security header.",
    ),
    stmt(
        "x_frame_options",
        Context::Headers,
        "entity.name.function.statement.headers.lazuli",
        "X-Frame-Options header.",
    ),
    stmt(
        "x_content_type_options",
        Context::Headers,
        "entity.name.function.statement.headers.lazuli",
        "X-Content-Type-Options header.",
    ),
    stmt(
        "referrer_policy",
        Context::Headers,
        "entity.name.function.statement.headers.lazuli",
        "Referrer-Policy header.",
    ),
    stmt(
        "permissions_policy",
        Context::Headers,
        "entity.name.function.statement.headers.lazuli",
        "Permissions-Policy header.",
    ),
    stmt(
        "include_subdomains",
        Context::Headers,
        "entity.name.function.statement.headers.lazuli",
        "HSTS includeSubDomains flag.",
    ),
    stmt(
        "preload",
        Context::Headers,
        "entity.name.function.statement.headers.lazuli",
        "HSTS preload flag.",
    ),
    // ── app: limits block ──
    stmt(
        "body_size",
        Context::Limits,
        "entity.name.function.statement.limits.lazuli",
        "Max request body size.",
    ),
    stmt(
        "header_size",
        Context::Limits,
        "entity.name.function.statement.limits.lazuli",
        "Max header size.",
    ),
    stmt(
        "upload_size",
        Context::Limits,
        "entity.name.function.statement.limits.lazuli",
        "Max upload size.",
    ),
    // ── app: proxy block ──
    stmt(
        "trusted",
        Context::Proxy,
        "entity.name.function.statement.proxy.lazuli",
        "Trusted proxy CIDRs.",
    ),
    stmt(
        "real_ip_header",
        Context::Proxy,
        "entity.name.function.statement.proxy.lazuli",
        "Real-IP header name.",
    ),
    stmt(
        "forwarded_proto_header",
        Context::Proxy,
        "entity.name.function.statement.proxy.lazuli",
        "Forwarded-Proto header name.",
    ),
    stmt(
        "forwarded_host_header",
        Context::Proxy,
        "entity.name.function.statement.proxy.lazuli",
        "Forwarded-Host header name.",
    ),
    // ── app: encryption block ──
    stmt(
        "algorithm",
        Context::Encryption,
        "entity.name.function.statement.encryption.lazuli",
        "Encryption algorithm.",
    ),
    stmt(
        "rotation_profile",
        Context::Encryption,
        "entity.name.function.statement.encryption.lazuli",
        "Key-rotation profile.",
    ),
    // ── app: locale block ──
    stmt(
        "supported",
        Context::Locale,
        "entity.name.function.statement.locale.lazuli",
        "Supported locales.",
    ),
    stmt(
        "fallback",
        Context::Locale,
        "entity.name.function.statement.locale.lazuli",
        "Fallback locale.",
    ),
    // ── app: logging block ──
    stmt(
        "level",
        Context::Logging,
        "entity.name.function.statement.logging.lazuli",
        "Log level.",
    ),
    stmt(
        "format",
        Context::Logging,
        "entity.name.function.statement.logging.lazuli",
        "Log format (json/text).",
    ),
    stmt(
        "redact",
        Context::Logging,
        "entity.name.function.statement.logging.lazuli",
        "Redacted fields.",
    ),
    stmt(
        "sample_rate",
        Context::Logging,
        "entity.name.function.statement.logging.lazuli",
        "Log sample rate.",
    ),
    // ── app: tracing block ──
    stmt(
        "propagate",
        Context::Tracing,
        "entity.name.function.statement.tracing.lazuli",
        "Trace-context propagation.",
    ),
    stmt(
        "exporter",
        Context::Tracing,
        "entity.name.function.statement.tracing.lazuli",
        "Trace exporter.",
    ),
    // ── app: runtime block ──
    stmt(
        "unit",
        Context::Runtime,
        "entity.name.function.statement.runtime.lazuli",
        "Runtime unit (process).",
    ),
    stmt(
        "serves",
        Context::Runtime,
        "entity.name.function.statement.runtime.lazuli",
        "What the unit serves.",
    ),
    stmt(
        "runs",
        Context::Runtime,
        "entity.name.function.statement.runtime.lazuli",
        "What the unit runs.",
    ),
    stmt(
        "healthcheck",
        Context::Runtime,
        "entity.name.function.statement.runtime.lazuli",
        "Healthcheck endpoint.",
    ),
    stmt(
        "readiness",
        Context::Runtime,
        "entity.name.function.statement.runtime.lazuli",
        "Readiness probe.",
    ),
    // ── app: deploy block ──
    stmt(
        "migrations",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Migration policy.",
    ),
    stmt(
        "migration_lock",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Migration advisory lock.",
    ),
    stmt(
        "destructive_migrations",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Destructive-migration policy.",
    ),
    stmt(
        "rollback",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Rollback policy.",
    ),
    stmt(
        "topology",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Deployment topology.",
    ),
    stmt(
        "strategy",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Deploy strategy (rolling/blue_green/canary).",
    ),
    stmt(
        "lock_timeout",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Migration lock timeout.",
    ),
    stmt(
        "pre_migration_hook",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Pre-migration hook.",
    ),
    stmt(
        "post_migration_hook",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Post-migration hook.",
    ),
    stmt(
        "checkpoint",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Migration checkpoint.",
    ),
    // ── app: services + communication ──
    stmt(
        "service",
        Context::Services,
        "entity.name.function.statement.services.lazuli",
        "Declares a service.",
    ),
    stmt(
        "owns",
        Context::Services,
        "entity.name.function.statement.services.lazuli",
        "Resources a service owns.",
    ),
    stmt(
        "exposes",
        Context::Services,
        "entity.name.function.statement.services.lazuli",
        "Operations a service exposes.",
    ),
    stmt(
        "publishes",
        Context::Services,
        "entity.name.function.statement.services.lazuli",
        "Events a service publishes.",
    ),
    stmt(
        "consumes",
        Context::Services,
        "entity.name.function.statement.services.lazuli",
        "Events a service consumes.",
    ),
    stmt(
        "internal",
        Context::Communication,
        "entity.name.function.statement.communication.lazuli",
        "Internal communication mode.",
    ),
    stmt(
        "external",
        Context::Communication,
        "entity.name.function.statement.communication.lazuli",
        "External communication mode.",
    ),
    stmt(
        "async",
        Context::Communication,
        "entity.name.function.statement.communication.lazuli",
        "Async communication.",
    ),
    // ── app/registry: env block ──
    stmt(
        "group",
        Context::Env,
        "entity.name.function.statement.env.lazuli",
        "Env-var group.",
    ),
    stmt(
        "client",
        Context::Env,
        "entity.name.function.statement.env.lazuli",
        "Client-exposed env var.",
    ),
    stmt(
        "server",
        Context::Env,
        "entity.name.function.statement.env.lazuli",
        "Server-only env var.",
    ),
    // ── integrations block ──
    stmt(
        "adapter",
        Context::Integrations,
        "entity.name.function.statement.integration.lazuli",
        "Integration adapter.",
    ),
    stmt(
        "credentials",
        Context::Integrations,
        "entity.name.function.statement.integration.lazuli",
        "Integration credentials.",
    ),
    stmt(
        "data_classification",
        Context::Integrations,
        "entity.name.function.statement.integration.lazuli",
        "Data-classification tag.",
    ),
    stmt(
        "integration",
        Context::Integrations,
        "entity.name.label.integration.lazuli",
        "Named integration.",
    ),
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
    kw(
        "api",
        Context::FeatureHeader,
        DECL,
        "Declares a full-control HTTP endpoint.",
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
    kw(
        "purpose",
        Context::FeatureHeader,
        STMT,
        "Feature purpose (iron-hand context).",
    ),
    produces(
        kw(
            "attach_ctx",
            Context::FeatureHeader,
            STMT,
            "Attaches a context-provider to the feature.",
        ),
        P_ATTACH_CTX,
    ),
    kw(
        "non_goals",
        Context::FeatureHeader,
        SECTION,
        "Feature non-goals (iron-hand context).",
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
    kw(
        "role",
        Context::FeatureHeader,
        DECL,
        "Feature-scoped RBAC role.",
    ),
    kw(
        "permission",
        Context::FeatureHeader,
        DECL,
        "Feature-scoped RBAC permission.",
    ),
    produces(
        kw(
            "invariants",
            Context::FeatureHeader,
            SECTION,
            "Declares the invariants block.",
        ),
        P_INVARIANTS,
    ),
    kw(
        "compatibility",
        Context::FeatureHeader,
        STMT,
        "Declares contract compatibility.",
    ),
    kw(
        "import",
        Context::FeatureHeader,
        STMT,
        "Imports a contract/operation.",
    ),
    kw(
        "imports",
        Context::FeatureHeader,
        STMT,
        "Declares feature imports.",
    ),
    kw(
        "operation",
        Context::FeatureHeader,
        DECL,
        "Declares a contract operation.",
    ),
    kw(
        "mcp_server",
        Context::FeatureHeader,
        DECL,
        "Declares an MCP server surface.",
    ),
    kw(
        "subscription",
        Context::FeatureHeader,
        DECL,
        "Declares an event subscription.",
    ),
    kw(
        "translation",
        Context::FeatureHeader,
        SECTION,
        "Declares a translation catalog.",
    ),
    produces(
        kw(
            "refs",
            Context::FeatureHeader,
            SECTION,
            "Declares cross-feature references.",
        ),
        P_CROSS_FEATURE,
    ),
    kw(
        "tenant_migration",
        Context::FeatureHeader,
        DECL,
        "Declares a tenant-data migration.",
    ),
    produces(
        dotted(
            "query.list",
            Context::FeatureHeader,
            "Declares a list query (collection projection).",
        ),
        P_QUERY,
    ),
    dotted(
        "query.lookup",
        Context::FeatureHeader,
        "Declares a lookup query (single-record fetch).",
    ),
    dotted(
        "query.sql",
        Context::FeatureHeader,
        "Declares a raw-SQL query.",
    ),
    dotted(
        "query.view",
        Context::FeatureHeader,
        "Declares a database-view-backed query.",
    ),
    dotted(
        "event.trace",
        Context::FeatureHeader,
        "Declares an event trace.",
    ),
    // feature-meta scalar lines (statements-feature-meta scope leaf)
    stmt(
        "requires",
        Context::FeatureHeader,
        "entity.name.function.statement.feature-meta.lazuli",
        "Feature dependency / requirement.",
    ),
    // ════════════════════════════════════════════════════════════════
    // Resource / aggregate / record body — field modifiers + relations
    // ════════════════════════════════════════════════════════════════
    produces(
        kw(
            "resource",
            Context::ResourceBody,
            DECL,
            "Declares a persisted resource.",
        ),
        P_RESOURCE,
    ),
    stmt(
        "tenancy",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Tenancy axis for the resource.",
    ),
    stmt(
        "timestamps",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Enable created/updated timestamps.",
    ),
    stmt(
        "soft_delete",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Enable soft-delete.",
    ),
    stmt(
        "retention",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Data-retention policy.",
    ),
    produces(
        stmt(
            "conventions",
            Context::ResourceBody,
            "entity.name.function.statement.resource.lazuli",
            "Opt into resource conventions.",
        ),
        P_CONVENTIONS,
    ),
    stmt(
        "paginate",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Default pagination.",
    ),
    stmt(
        "validates",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Field validation rule.",
    ),
    stmt(
        "validate",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Field validation rule.",
    ),
    produces(
        stmt(
            "unique",
            Context::ResourceBody,
            "entity.name.function.statement.resource.lazuli",
            "Unique constraint.",
        ),
        P_UNIQUE,
    ),
    stmt(
        "index",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Index declaration.",
    ),
    stmt(
        "on_delete",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Referential on-delete action.",
    ),
    produces(
        stmt(
            "derived",
            Context::ResourceBody,
            "entity.name.function.statement.resource.lazuli",
            "Derived/computed field.",
        ),
        P_DERIVED,
    ),
    stmt(
        "has_many",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "One-to-many relation.",
    ),
    stmt(
        "inverse",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Inverse relation side.",
    ),
    stmt(
        "previously",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Previous field name (rename).",
    ),
    stmt(
        "migrated",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Migration marker.",
    ),
    stmt(
        "alias",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Field alias.",
    ),
    produces(
        stmt(
            "composite_key",
            Context::ResourceBody,
            "entity.name.function.statement.resource.lazuli",
            "Composite primary key.",
        ),
        P_COMPOSITE_KEY,
    ),
    produces(
        stmt(
            "lock",
            Context::ResourceBody,
            "entity.name.function.statement.resource.lazuli",
            "Concurrency lock strategy.",
        ),
        P_LOCK,
    ),
    stmt(
        "invariant",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Resource invariant.",
    ),
    stmt(
        "fields",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Field group.",
    ),
    stmt(
        "primary",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Primary-key marker.",
    ),
    stmt(
        "field",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Explicit field declaration.",
    ),
    stmt(
        "root",
        Context::ResourceBody,
        "keyword.control.statement.lazuli",
        "Aggregate root marker.",
    ),
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
        "output",
        Context::CommandBody,
        SECTION,
        "Output field block.",
    ),
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
        "materialize",
        Context::CommandBody,
        STMT,
        "Materialize a projection.",
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
    // ── command: deprecated sub-block ──
    stmt(
        "since",
        Context::CommandBody,
        "entity.name.function.statement.deprecated.lazuli",
        "Deprecation since-version.",
    ),
    stmt(
        "replacement",
        Context::CommandBody,
        "entity.name.function.statement.deprecated.lazuli",
        "Replacement reference.",
    ),
    stmt(
        "sunset",
        Context::CommandBody,
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
    stmt("outbox", Context::Job, STMT, "Outbox-pattern marker."),
    // ── webhook + verify/replay/dlq sub ──
    stmt("payload", Context::Webhook, STMT, "Webhook payload type."),
    stmt(
        "payload_group",
        Context::Webhook,
        STMT,
        "Webhook payload group.",
    ),
    stmt(
        "payload_from",
        Context::Webhook,
        STMT,
        "Payload source reference.",
    ),
    stmt(
        "verify",
        Context::Webhook,
        SECTION,
        "Signature-verification block.",
    ),
    stmt(
        "replay",
        Context::Webhook,
        SECTION,
        "Replay-protection block.",
    ),
    stmt("dlq", Context::Webhook, SECTION, "Dead-letter-queue block."),
    // H2 reconcile: `dedupe`/`within` belong to the webhook `replay` sub-block;
    // the grammar colors them `entity.name.function.statement.replay`. Promote
    // off the generic statement leaf so `#kw-replay` is faithful.
    stmt(
        "dedupe",
        Context::Webhook,
        REPLAY,
        "Replay deduplication key.",
    ),
    stmt("within", Context::Webhook, REPLAY, "Replay window."),
    stmt(
        "previous_version",
        Context::Webhook,
        STMT,
        "Previous payload version.",
    ),
    stmt("secret", Context::Webhook, STMT, "Verification secret."),
    stmt("header", Context::Webhook, STMT, "Signature header name."),
    // ── agent + tools/expose/io/evals ──
    stmt("model", Context::Agent, STMT, "LLM model."),
    stmt("prompt", Context::Agent, STMT, "Agent prompt."),
    stmt("safety", Context::Agent, STMT, "Safety constraints."),
    stmt("stream", Context::Agent, STMT, "Streaming mode."),
    stmt("temperature", Context::Agent, STMT, "Sampling temperature."),
    stmt("max_tokens", Context::Agent, STMT, "Max output tokens."),
    stmt("top_p", Context::Agent, STMT, "Top-p sampling."),
    stmt("seed", Context::Agent, STMT, "Deterministic seed."),
    stmt(
        "expose",
        Context::Agent,
        STMT,
        "Expose the agent over HTTP/MCP.",
    ),
    stmt(
        "discriminator",
        Context::Agent,
        STMT,
        "Output discriminator.",
    ),
    stmt(
        "tools",
        Context::Agent,
        SECTION,
        "Agent tool-binding block.",
    ),
    stmt(
        "tool",
        Context::Agent,
        STMT,
        "Declares a single tool / MCP tool.",
    ),
    // ── notification + digest/throttle ──
    stmt(
        "recipient",
        Context::Notification,
        STMT,
        "Notification recipient.",
    ),
    stmt(
        "template",
        Context::Notification,
        STMT,
        "Notification template.",
    ),
    stmt(
        "digest",
        Context::Notification,
        SECTION,
        "Digest-batching block.",
    ),
    stmt(
        "throttle",
        Context::Notification,
        SECTION,
        "Throttling block.",
    ),
    stmt(
        "every",
        Context::Notification,
        "entity.name.function.statement.digest.lazuli",
        "Digest interval.",
    ),
    stmt(
        "group_by",
        Context::Notification,
        "entity.name.function.statement.digest.lazuli",
        "Digest grouping key.",
    ),
    stmt(
        "template_strategy",
        Context::Notification,
        "entity.name.function.statement.digest.lazuli",
        "Digest template strategy.",
    ),
    stmt(
        "max_per",
        Context::Notification,
        "entity.name.function.statement.throttle.lazuli",
        "Throttle max-per-window.",
    ),
    stmt(
        "per_recipient",
        Context::Notification,
        "entity.name.function.statement.throttle.lazuli",
        "Per-recipient throttle.",
    ),
    stmt(
        "per_channel",
        Context::Notification,
        "entity.name.function.statement.throttle.lazuli",
        "Per-channel throttle.",
    ),
    stmt(
        "burst",
        Context::Notification,
        "entity.name.function.statement.throttle.lazuli",
        "Throttle burst allowance.",
    ),
    stmt(
        "max_size",
        Context::Notification,
        "entity.name.function.statement.digest.lazuli",
        "Digest max batch size.",
    ),
    // ── poller ──
    stmt("cursor", Context::Poller, STMT, "Poll cursor field."),
    stmt("tick", Context::Poller, STMT, "Poll interval."),
    stmt("backoff", Context::Poller, STMT, "Backoff policy."),
    stmt("counter", Context::Poller, STMT, "Poll counter."),
    stmt(
        "retry_quirk",
        Context::Poller,
        STMT,
        "Poller retry-quirk catalog entry.",
    ),
    stmt(
        "max_attempts",
        Context::Poller,
        STMT,
        "Max poll retry attempts.",
    ),
    // ── report + columns ──
    stmt("columns", Context::Report, SECTION, "Report column block."),
    stmt("formats", Context::Report, STMT, "Export formats."),
    stmt(
        "label",
        Context::Report,
        "entity.name.function.statement.columns.lazuli",
        "Column label.",
    ),
    stmt("visibility", Context::Report, STMT, "Report visibility."),
    // ── channel ──
    stmt(
        "payload_axis",
        Context::Channel,
        STMT,
        "Channel partition axis.",
    ),
    // ── tenant_migration ──
    stmt(
        "materialize_strategy",
        Context::TenantMigration,
        STMT,
        "Materialization strategy.",
    ),
    // ════════════════════════════════════════════════════════════════
    // api / operation body
    // ════════════════════════════════════════════════════════════════
    stmt("method", Context::Api, STMT, "HTTP method."),
    stmt("transport", Context::Api, STMT, "Transport (http)."),
    // ════════════════════════════════════════════════════════════════
    // auth block + sub
    // ════════════════════════════════════════════════════════════════
    stmt("identity", Context::Auth, STMT, "Identity strategy."),
    stmt("password", Context::Auth, STMT, "Password strategy."),
    stmt("oauth", Context::Auth, STMT, "OAuth provider."),
    stmt("mfa", Context::Auth, STMT, "Multi-factor auth."),
    stmt("sessions", Context::Auth, STMT, "Session settings."),
    stmt("access_ttl", Context::Auth, STMT, "Access-token TTL."),
    stmt("refresh_ttl", Context::Auth, STMT, "Refresh-token TTL."),
    stmt("rotation", Context::Auth, STMT, "Token-rotation policy."),
    stmt("grace", Context::Auth, STMT, "Rotation grace window."),
    stmt(
        "theft_detection_action",
        Context::Auth,
        STMT,
        "Token-theft action.",
    ),
    stmt("refresh", Context::Auth, STMT, "Refresh operation."),
    stmt("enroll", Context::Auth, STMT, "MFA enrollment."),
    stmt("hash", Context::Auth, STMT, "Password hash algorithm."),
    // `auth.sessions.cookie` — session-cookie transport block. Option (b)
    // from `docs/proposals/cookie-sessions-child.md`: the cookie attribute
    // vocabulary (`same_site`/`secure`/`http_only`/`domain`/`path` —
    // `Context::Cookie` rows under the app `cookie` SECTION above; `name`
    // is the generic `modifier`) is REUSED, not duplicated. This row is the
    // second anchor position: the `cookie` SECTION re-rooted under the
    // `sessions` parent (`Context::Auth`) instead of `Context::App`. The
    // parser dispatches the cookie children to the same closed catalog.
    kw(
        "cookie",
        Context::Auth,
        SECTION,
        "Session-cookie transport attributes block (name/same_site/secure/http_only/domain/path).",
    ),
    // ════════════════════════════════════════════════════════════════
    // errors block
    // ════════════════════════════════════════════════════════════════
    stmt(
        "error",
        Context::Errors,
        "entity.name.function.statement.errors.lazuli",
        "Error declaration.",
    ),
    stmt(
        "expose",
        Context::Errors,
        "entity.name.function.statement.errors.lazuli",
        "Expose error to client.",
    ),
    stmt(
        "hide",
        Context::Errors,
        "entity.name.function.statement.errors.lazuli",
        "Hide error from client.",
    ),
    stmt(
        "message",
        Context::Errors,
        "entity.name.function.statement.errors.lazuli",
        "Error message.",
    ),
    stmt(
        "status",
        Context::Errors,
        "entity.name.function.statement.errors.lazuli",
        "HTTP status.",
    ),
    stmt(
        "code",
        Context::Errors,
        "entity.name.function.statement.errors.lazuli",
        "Error code.",
    ),
    stmt(
        "reason",
        Context::Errors,
        "entity.name.function.statement.errors.lazuli",
        "Error reason.",
    ),
    // ════════════════════════════════════════════════════════════════
    // defaults block
    // ════════════════════════════════════════════════════════════════
    stmt(
        "no_timestamps",
        Context::Defaults,
        "entity.name.function.statement.defaults.lazuli",
        "Disable timestamps by default.",
    ),
    stmt(
        "policy_for",
        Context::Defaults,
        "entity.name.function.statement.defaults.lazuli",
        "Default policy for an action.",
    ),
    // ════════════════════════════════════════════════════════════════
    // invariants block
    // ════════════════════════════════════════════════════════════════
    op("when", OP_PREDICATE),
    // ════════════════════════════════════════════════════════════════
    // cache profile block
    // ════════════════════════════════════════════════════════════════
    stmt(
        "key",
        Context::Cache,
        "entity.name.function.statement.cache.lazuli",
        "Cache key.",
    ),
    stmt(
        "ttl",
        Context::Cache,
        "entity.name.function.statement.cache.lazuli",
        "Cache TTL.",
    ),
    stmt(
        "tags",
        Context::Cache,
        "entity.name.function.statement.cache.lazuli",
        "Cache tags.",
    ),
    stmt(
        "namespace",
        Context::Cache,
        "entity.name.function.statement.cache.lazuli",
        "Cache namespace.",
    ),
    stmt(
        "stale_while_revalidate",
        Context::Cache,
        "entity.name.function.statement.cache.lazuli",
        "Stale-while-revalidate window.",
    ),
    stmt(
        "coalesce",
        Context::Cache,
        "entity.name.function.statement.cache.lazuli",
        "Request coalescing.",
    ),
    stmt(
        "sliding",
        Context::Cache,
        "entity.name.function.statement.cache.lazuli",
        "Sliding-expiration flag.",
    ),
    // ════════════════════════════════════════════════════════════════
    // emits / event group
    // ════════════════════════════════════════════════════════════════
    op("level", OP_PREDICATE),
    // ════════════════════════════════════════════════════════════════
    // tests / evals block
    // ════════════════════════════════════════════════════════════════
    stmt(
        "allows",
        Context::Tests,
        "entity.name.function.statement.tests.lazuli",
        "Test: action allowed.",
    ),
    stmt(
        "denies",
        Context::Tests,
        "entity.name.function.statement.tests.lazuli",
        "Test: action denied.",
    ),
    stmt(
        "permits",
        Context::Tests,
        "entity.name.function.statement.tests.lazuli",
        "Test: permission granted.",
    ),
    stmt(
        "forbids",
        Context::Tests,
        "entity.name.function.statement.tests.lazuli",
        "Test: permission forbidden.",
    ),
    stmt(
        "accepted",
        Context::Tests,
        "entity.name.function.statement.tests.lazuli",
        "Test: input accepted.",
    ),
    stmt(
        "rejected",
        Context::Tests,
        "entity.name.function.statement.tests.lazuli",
        "Test: input rejected.",
    ),
    stmt(
        "case",
        Context::Tests,
        "entity.name.function.statement.tests.lazuli",
        "Eval case.",
    ),
    stmt(
        "golden",
        Context::Tests,
        "entity.name.function.statement.tests.lazuli",
        "Golden-output assertion.",
    ),
    stmt(
        "min_score",
        Context::Tests,
        "entity.name.function.statement.tests.lazuli",
        "Minimum eval score.",
    ),
    stmt("evals", Context::Tests, SECTION, "Agent evaluation block."),
    // ════════════════════════════════════════════════════════════════
    // policy block
    // ════════════════════════════════════════════════════════════════
    stmt(
        "allow",
        Context::Policy,
        "entity.name.function.statement.policy.lazuli",
        "Policy: allow.",
    ),
    stmt(
        "deny",
        Context::Policy,
        "entity.name.function.statement.policy.lazuli",
        "Policy: deny.",
    ),
    stmt(
        "denies",
        Context::Policy,
        "entity.name.function.statement.policy.lazuli",
        "Policy: denies.",
    ),
    stmt(
        "permits",
        Context::Policy,
        "entity.name.function.statement.policy.lazuli",
        "Policy: permits.",
    ),
    stmt(
        "forbids",
        Context::Policy,
        "entity.name.function.statement.policy.lazuli",
        "Policy: forbids.",
    ),
    stmt("default_policy", Context::Policy, STMT, "Default policy."),
    op("forbid_when", OP_PREDICATE),
    op("only_when", OP_PREDICATE),
    op("eligible_when", OP_PREDICATE),
    op("when_denied", OP_PREDICATE),
    op("required_when", OP_PREDICATE),
    // ════════════════════════════════════════════════════════════════
    // typed extensions
    // ════════════════════════════════════════════════════════════════
    stmt(
        "fn",
        Context::Extensions,
        "entity.name.function.statement.extension.lazuli",
        "Custom function extension point.",
    ),
    stmt(
        "hook",
        Context::Extensions,
        "entity.name.function.statement.extension.lazuli",
        "Lifecycle hook extension point.",
    ),
    stmt(
        "validator",
        Context::Extensions,
        "entity.name.function.statement.extension.lazuli",
        "Validator extension point.",
    ),
    stmt(
        "query_modifier",
        Context::Extensions,
        "entity.name.function.statement.extension.lazuli",
        "Query-modifier extension point.",
    ),
    stmt(
        "client",
        Context::Extensions,
        "entity.name.function.statement.extension.lazuli",
        "Client-side extension point.",
    ),
    stmt(
        "block",
        Context::Extensions,
        "entity.name.function.statement.extension.lazuli",
        "UI block extension point.",
    ),
    stmt(
        "escape_route",
        Context::Extensions,
        STMT,
        "Escape-route extension.",
    ),
    // ════════════════════════════════════════════════════════════════
    // translation block
    // ════════════════════════════════════════════════════════════════
    stmt(
        "catalog",
        Context::Translation,
        "entity.name.function.statement.translation.lazuli",
        "Translation catalog.",
    ),
    stmt(
        "plural",
        Context::Translation,
        "entity.name.function.statement.translation.lazuli",
        "Plural form.",
    ),
    // ════════════════════════════════════════════════════════════════
    // secret_rotation block
    // ════════════════════════════════════════════════════════════════
    stmt(
        "cadence",
        Context::SecretRotation,
        STMT,
        "Rotation cadence.",
    ),
    stmt(
        "overlap",
        Context::SecretRotation,
        STMT,
        "Key-overlap window.",
    ),
    stmt(
        "auto_rollback",
        Context::SecretRotation,
        STMT,
        "Auto-rollback on failure.",
    ),
    // ════════════════════════════════════════════════════════════════
    // SURFACE (.lzx) — view / audience / extends
    // ════════════════════════════════════════════════════════════════
    lzx(
        "audience",
        Context::Surface,
        DECL,
        "Audience grouping for surface views.",
    ),
    lzx(
        "requires",
        Context::SurfaceAudience,
        STMT,
        "Audience scope requirement (`requires @scope.X`).",
    ),
    lzx(
        "uses",
        Context::Surface,
        STMT,
        "`uses experience` declaration.",
    ),
    lzx("source", Context::SurfaceView, STMT, "View data source."),
    lzx("submit", Context::SurfaceView, STMT, "Form submit target."),
    lzx(
        "on_success",
        Context::SurfaceView,
        SECTION,
        "Post-submit success block.",
    ),
    lzx(
        "columns",
        Context::SurfaceView,
        SECTION,
        "List column projection.",
    ),
    lzx(
        "fields",
        Context::SurfaceView,
        SECTION,
        "Form/detail field block.",
    ),
    lzx(
        "sections",
        Context::SurfaceView,
        SECTION,
        "Detail section block.",
    ),
    lzx(
        "cells",
        Context::SurfaceView,
        SECTION,
        "Cell renderer block.",
    ),
    lzx("route", Context::SurfaceView, STMT, "View route."),
    lzx(
        "actions",
        Context::SurfaceView,
        SECTION,
        "View action block.",
    ),
    lzx("action", Context::SurfaceView, STMT, "View action."),
    lzx("filter", Context::SurfaceView, STMT, "List filter."),
    lzx(
        "filters",
        Context::SurfaceView,
        SECTION,
        "List filter block.",
    ),
    lzx("search", Context::SurfaceView, STMT, "List search."),
    lzx(
        "drawer",
        Context::SurfaceView,
        SECTION,
        "Detail drawer block.",
    ),
    lzx("sort", Context::SurfaceView, STMT, "List sort."),
    lzx(
        "selection",
        Context::SurfaceView,
        STMT,
        "Row selection mode.",
    ),
    lzx(
        "bulk_actions",
        Context::SurfaceView,
        SECTION,
        "Bulk-action block.",
    ),
    lzx(
        "settings",
        Context::SurfaceView,
        SECTION,
        "View settings block.",
    ),
    lzx(
        "persist",
        Context::SurfaceView,
        STMT,
        "Filter/sort persistence.",
    ),
    lzx(
        "flash",
        Context::SurfaceView,
        STMT,
        "Flash message on success.",
    ),
    lzx("replace", Context::SurfaceView, STMT, "Replace navigation."),
    lzx(
        "redirect",
        Context::SurfaceView,
        STMT,
        "Redirect on success.",
    ),
    lzx(
        "back",
        Context::SurfaceView,
        STMT,
        "Navigate back on success.",
    ),
    lzx("tabs", Context::SurfaceView, SECTION, "Tabbed view block."),
    lzx("tab", Context::SurfaceView, STMT, "A single tab."),
    lzx(
        "tab_group",
        Context::SurfaceView,
        SECTION,
        "Tab-group block.",
    ),
    lzx(
        "wizard",
        Context::SurfaceView,
        SECTION,
        "Wizard view block.",
    ),
    lzx(
        "wizard_steps",
        Context::SurfaceView,
        SECTION,
        "Wizard-steps block.",
    ),
    lzx("step", Context::SurfaceView, STMT, "A wizard step."),
    lzx(
        "view_mode",
        Context::SurfaceView,
        STMT,
        "View display mode.",
    ),
    lzx(
        "date_range",
        Context::SurfaceView,
        STMT,
        "Date-range filter primitive.",
    ),
    lzx(
        "repeatable",
        Context::SurfaceView,
        STMT,
        "Repeatable input group.",
    ),
    lzx("lazy", Context::SurfaceView, STMT, "Lazy-load marker."),
    lzx("prerender", Context::SurfaceView, STMT, "Prerender marker."),
    lzx(
        "loader",
        Context::SurfaceView,
        STMT,
        "Data loader reference.",
    ),
    lzx(
        "opens",
        Context::SurfaceView,
        STMT,
        "Drawer/modal open trigger.",
    ),
    lzx("lanes", Context::SurfaceView, STMT, "Board lanes."),
    lzx("icon", Context::SurfaceView, STMT, "Action/tab icon."),
    lzx("hint", Context::SurfaceView, STMT, "Field hint text."),
    lzx(
        "error_view",
        Context::SurfaceView,
        STMT,
        "Error-state view.",
    ),
    lzx(
        "error_redact",
        Context::SurfaceView,
        STMT,
        "Error redaction.",
    ),
    lzx(
        "pending_view",
        Context::SurfaceView,
        STMT,
        "Pending/loading view.",
    ),
    lzx(
        "on_change",
        Context::SurfaceView,
        STMT,
        "On-change handler.",
    ),
    lzx("mutate", Context::SurfaceView, STMT, "Optimistic mutation."),
    lzx(
        "requires_lifecycle",
        Context::SurfaceView,
        STMT,
        "Lifecycle-state gate for the view/route.",
    ),
    lzx(
        "requires_lifecycle_in",
        Context::SurfaceView,
        STMT,
        "Lifecycle-state-set gate.",
    ),
    lzx(
        "on_lifecycle_pending",
        Context::SurfaceView,
        STMT,
        "Pending-lifecycle handler.",
    ),
    lzx(
        "resume",
        Context::SurfaceView,
        STMT,
        "Resume-from-pending reference.",
    ),
    lzx(
        "view.inline_table",
        Context::SurfaceView,
        STMT,
        "Inline-table view primitive (W6).",
    ),
    lzx(
        "view.board",
        Context::SurfaceView,
        STMT,
        "Board view primitive (W6).",
    ),
    lzx(
        "detail",
        Context::SurfaceView,
        DECL,
        "`view detail` projection kind.",
    ),
    lzx(
        "create",
        Context::SurfaceView,
        DECL,
        "`view create` projection kind.",
    ),
    // route / view guards (.lzx app + feature route blocks)
    lzx(
        "on_unauthenticated",
        Context::SurfaceView,
        STMT,
        "Guard: action when unauthenticated.",
    ),
    lzx(
        "on_unauthorized",
        Context::SurfaceView,
        STMT,
        "Guard: action when unauthorized.",
    ),
    lzx(
        "role_mismatch",
        Context::SurfaceView,
        STMT,
        "Route guard: per-role mismatch redirect.",
    ),
    lzx(
        "default_policy",
        Context::SurfaceView,
        STMT,
        "Default route policy.",
    ),
    lzx(
        "default_unauthenticated_redirect",
        Context::SurfaceView,
        STMT,
        "Default unauthenticated redirect.",
    ),
    lzx(
        "default_unauthorized_redirect",
        Context::SurfaceView,
        STMT,
        "Default unauthorized redirect.",
    ),
    // extends / anchor / slot extensibility
    lzx(
        "extends",
        Context::Extends,
        DECL,
        "Extends a sibling feature's anchor.",
    ),
    lzx(
        "anchor",
        Context::Extends,
        STMT,
        "Declares an extensibility anchor.",
    ),
    lzx(
        "extensible_by",
        Context::Extends,
        STMT,
        "Declares this view extensible by siblings.",
    ),
    lzx("slot", Context::Extends, STMT, "Extension slot position."),
    lzx(
        "platforms",
        Context::Extends,
        STMT,
        "Target platforms for the extension.",
    ),
    // ════════════════════════════════════════════════════════════════
    // plan block
    // ════════════════════════════════════════════════════════════════
    kw(
        "features",
        Context::Plan,
        SECTION,
        "Plan feature entitlements.",
    ),
    // H2 reconcile: the grammar's plan-block alternation colors
    // `features | limits | trial` at the section leaf; promote `trial` off the
    // generic statement leaf so `#kw-plan-section` is faithful.
    kw("trial", Context::Plan, SECTION, "Plan trial-window block."),
    CapabilitySpec {
        literal: "then",
        context: Context::Plan,
        scope: "keyword.other.plan.lazuli",
        token: SemanticToken::Keyword,
        surface: Surface::Lzi,
        sigil: None,
        hover: "Plan upgrade target.",
        produces: &[],
    },
    CapabilitySpec {
        literal: "unlimited",
        context: Context::Plan,
        scope: "keyword.other.plan.lazuli",
        token: SemanticToken::Keyword,
        surface: Surface::Lzi,
        sigil: None,
        hover: "Unlimited plan quota.",
        produces: &[],
    },
    // ════════════════════════════════════════════════════════════════
    // Cross-cutting modifiers — storage.modifier.lazuli
    // ════════════════════════════════════════════════════════════════
    modifier("required", "Field is required."),
    modifier("optional", "Field is optional."),
    modifier("readonly", "Field is read-only."),
    modifier("raw", "Raw/unprocessed value."),
    modifier("override", "Override a base declaration."),
    modifier("per", "Rate/quota unit connector."),
    modifier("at", "Position/time connector."),
    modifier("from", "Source connector."),
    modifier("to", "Target connector."),
    modifier("by", "Agent/grouping connector."),
    modifier("on", "Relation/event connector."),
    modifier("provides", "Provides connector."),
    modifier("cascade", "On-delete cascade."),
    modifier("restrict", "On-delete restrict."),
    modifier("nullify", "On-delete set-null."),
    modifier("terminal", "Terminal lifecycle state."),
    modifier("initial", "Initial lifecycle state."),
    modifier("sync", "Synchronous mode."),
    modifier("after", "Slot-position after."),
    modifier("before", "Slot-position before."),
    modifier("using", "Using connector."),
    modifier("inherits", "Inherits connector."),
    modifier("parent", "Parent reference."),
    modifier("name", "Name connector."),
    modifier("description", "Description connector."),
    modifier("filename", "Filename connector."),
    modifier("mime", "MIME-type connector."),
    modifier("size", "Size connector."),
    modifier("attempts", "Attempts connector."),
    modifier("max_attempts", "Max-attempts connector."),
    modifier("max_recursion", "Max-recursion connector."),
    modifier("signed_ttl", "Signed-URL TTL."),
    modifier("accept", "Accepted MIME types."),
    modifier("uri_template", "URI template."),
    modifier("uses", "Uses connector."),
    modifier("data_subject", "GDPR data-subject."),
    modifier("retain", "Retention connector."),
    modifier("opaque", "Opaque-token flag."),
    modifier("terminal_result_field", "Terminal result field."),
    modifier("terminal_status_field", "Terminal status field."),
    modifier("invariant_handler", "Invariant handler reference."),
    modifier("derived_from", "Derived-from source."),
    modifier("resolve", "Resolve-via connector."),
    modifier("triggers", "Triggers connector."),
    modifier("states", "Lifecycle states block."),
    modifier("state", "Lifecycle state."),
    modifier("transition", "Lifecycle transition."),
    modifier("lifecycle_routes", "Lifecycle route bindings."),
    modifier("lifecycle_stage", "Lifecycle-stage marker."),
    modifier("when_denied_route", "Route when policy denied."),
    modifier("free", "`free text into` sugar."),
    modifier("list", "`list of` type-constructor head."),
    // ════════════════════════════════════════════════════════════════
    // Type constructors — support.function.type-constructor.lazuli
    // ════════════════════════════════════════════════════════════════
    CapabilitySpec {
        literal: "many",
        context: Context::Expression,
        scope: TYPE_CTOR,
        token: SemanticToken::Type,
        surface: Surface::Lzi,
        sigil: None,
        hover: "`many` relation type constructor.",
        produces: &[],
    },
    CapabilitySpec {
        literal: "list_of",
        context: Context::Expression,
        scope: TYPE_CTOR,
        token: SemanticToken::Type,
        surface: Surface::Lzi,
        sigil: None,
        hover: "`list_of` collection type constructor.",
        produces: &[],
    },
    CapabilitySpec {
        literal: "ref",
        context: Context::Expression,
        scope: TYPE_CTOR,
        token: SemanticToken::Type,
        surface: Surface::Lzi,
        sigil: None,
        hover: "`ref` reference type constructor.",
        produces: &[],
    },
    // ════════════════════════════════════════════════════════════════
    // Filter / policy / expression operators + predicates
    // ════════════════════════════════════════════════════════════════
    op("and", OP_LOGICAL),
    op("or", OP_LOGICAL),
    op("not", OP_LOGICAL),
    op("has", OP_PREDICATE),
    op("in", OP_PREDICATE),
    op("exists", OP_PREDICATE),
    op("matches", OP_PREDICATE),
    op("is", OP_PREDICATE),
    op("between", OP_PREDICATE),
    op("contains", OP_PREDICATE),
    op("length", OP_PREDICATE),
    op("pattern", OP_PREDICATE),
    op("min", OP_PREDICATE),
    op("max", OP_PREDICATE),
    op("excludes", OP_PREDICATE),
    op("includes", OP_PREDICATE),
    op("covers_pii", OP_PREDICATE),
    op("guaranteed", OP_PREDICATE),
    op("behind", "keyword.operator.plan-and-gate.lazuli"),
    op("quota", "keyword.operator.plan-and-gate.lazuli"),
    // ════════════════════════════════════════════════════════════════
    // Decorator namespaces — entity.name.tag.decorator.lazuli
    // ════════════════════════════════════════════════════════════════
    produces(
        decorator(
            "@semantic",
            "Semantic-scalar decorator (`@semantic.HexColor`).",
        ),
        P_SEMANTIC,
    ),
    produces(
        decorator("@cap", "Capability decorator (`@cap.File`)."),
        P_CAP,
    ),
    decorator("@pii", "PII-classification decorator."),
    decorator("@key", "Encryption-key decorator."),
    decorator("@slug", "Slug field decorator."),
    produces(
        decorator("@full_text", "Full-text-index decorator."),
        P_FULL_TEXT,
    ),
    produces(
        decorator("@owner_axis", "Ownership-axis decorator."),
        P_OWNER_AXIS,
    ),
    decorator("@llm", "LLM decorator."),
    decorator("@tool", "Tool decorator."),
    decorator("@adapter", "Adapter decorator."),
    decorator("@policy", "Policy reference decorator."),
    decorator("@scope", "Scope reference decorator."),
    decorator("@role", "Role reference decorator."),
    decorator("@actor", "Actor reference decorator."),
    decorator("@anchor", "Anchor reference decorator."),
    decorator("@client", "Client extension decorator."),
    produces(
        decorator("@fn", "Custom-function reference decorator."),
        P_FN,
    ),
    produces(decorator("@hook", "Hook reference decorator."), P_HOOK),
    decorator("@validator", "Validator reference decorator."),
    decorator("@query_modifier", "Query-modifier reference decorator."),
    decorator("@translation", "Translation reference decorator."),
    decorator("@command", "Command reference decorator."),
    decorator("@file", "File reference decorator."),
    decorator("@feature", "Feature reference decorator."),
    decorator("@resume", "Resume reference decorator."),
    decorator("@audience", "Audience reference decorator."),
    // ════════════════════════════════════════════════════════════════
    // design.lzi token catalog — DESIGN_KEYWORDS
    // ════════════════════════════════════════════════════════════════
    design("color", "Closed design token group for colors."),
    design("typography", "Closed design token group for type."),
    design("space", "Spacing scale token group."),
    design("radius", "Border-radius token group."),
    design("shadow", "Box-shadow elevation token group."),
    design("motion", "Transition/animation token group."),
    design("breakpoint", "Responsive breakpoint token group."),
    design("z", "Stacking-order token group."),
    design("family", "Font-family sub-group."),
    design("scale", "Type-scale sub-group."),
    design("weight", "Font-weight sub-group."),
    design("tracking", "Letter-spacing sub-group."),
    design("duration", "Motion duration sub-group."),
    design("easing", "Motion easing sub-group."),
    design("line_height", "Type line-height field."),
    CapabilitySpec {
        literal: "base",
        context: Context::Design,
        scope: SECTION,
        token: SemanticToken::Keyword,
        surface: Surface::Design,
        sigil: None,
        hover: "Required default color state.",
        produces: &[],
    },
    CapabilitySpec {
        literal: "foreground",
        context: Context::Design,
        scope: SECTION,
        token: SemanticToken::Keyword,
        surface: Surface::Design,
        sigil: None,
        hover: "Foreground color when used as background.",
        produces: &[],
    },
    CapabilitySpec {
        literal: "dark",
        context: Context::Design,
        scope: SECTION,
        token: SemanticToken::Keyword,
        surface: Surface::Design,
        sigil: None,
        hover: "Dark-theme color suffix.",
        produces: &[],
    },
    // ════════════════════════════════════════════════════════════════
    // Closed-catalog VALUES — constant.language.<catalog>.lazuli
    // (carried so the proven-complete scan resolves match-arm value
    //  literals; these are EnumMember tokens, not keywords.)
    // ════════════════════════════════════════════════════════════════
    // boolean
    value("true", "constant.language.boolean.lazuli"),
    value("false", "constant.language.boolean.lazuli"),
    value("nil", "constant.language.boolean.lazuli"),
    value("null", "constant.language.boolean.lazuli"),
    // notification channels
    value("email", "constant.language.channel.lazuli"),
    value("push", "constant.language.channel.lazuli"),
    value("sms", "constant.language.channel.lazuli"),
    value("in_app", "constant.language.channel.lazuli"),
    // cookie same_site
    value("lax", "constant.language.cookie.lazuli"),
    value("strict", "constant.language.cookie.lazuli"),
    value("none", "constant.language.cookie.lazuli"),
    // deploy strategy
    value("rolling", "constant.language.deploy.lazuli"),
    value("blue_green", "constant.language.deploy.lazuli"),
    value("canary", "constant.language.deploy.lazuli"),
    // dlq mode
    value("emit", "constant.language.dlq.lazuli"),
    value("drop", "constant.language.dlq.lazuli"),
    // http methods
    value("GET", "constant.language.http-method.lazuli"),
    value("POST", "constant.language.http-method.lazuli"),
    value("PUT", "constant.language.http-method.lazuli"),
    value("PATCH", "constant.language.http-method.lazuli"),
    value("DELETE", "constant.language.http-method.lazuli"),
    value("OPTIONS", "constant.language.http-method.lazuli"),
    value("HEAD", "constant.language.http-method.lazuli"),
    // lock strategy
    value("optimistic", "constant.language.lock.lazuli"),
    value("pessimistic", "constant.language.lock.lazuli"),
    value("row_level", "constant.language.lock.lazuli"),
    // log level / format
    value("debug", "constant.language.log-level.lazuli"),
    value("info", "constant.language.log-level.lazuli"),
    value("warn", "constant.language.log-level.lazuli"),
    value("text", "constant.language.log-level.lazuli"),
    value("json", "constant.language.log-level.lazuli"),
    // report format
    value("csv", "constant.language.report-format.lazuli"),
    value("xlsx", "constant.language.report-format.lazuli"),
    // rotation
    value("manual", "constant.language.rotation.lazuli"),
    value("kms_managed", "constant.language.rotation.lazuli"),
    // template strategy
    value("merge", "constant.language.template-strategy.lazuli"),
    value("append", "constant.language.template-strategy.lazuli"),
    // transport
    value("http", "constant.language.transport.lazuli"),
    // verify
    value("hmac", "constant.language.verify.lazuli"),
    value("jwt", "constant.language.verify.lazuli"),
    // visibility
    value("public", "constant.language.visibility.lazuli"),
    value("private", "constant.language.visibility.lazuli"),
    // sort direction
    value("asc", "constant.language.direction.lazuli"),
    value("desc", "constant.language.direction.lazuli"),
    // selection mode
    value("multi", "constant.language.selection-mode.lazuli"),
    value("single", "constant.language.selection-mode.lazuli"),
    // search mode
    value("segmented", "constant.language.search-mode.lazuli"),
    // binding source
    value("query", "constant.language.binding-source.lazuli"),
    // persistence
    value("local", "constant.language.persistence.lazuli"),
    // index methods
    value("btree", "constant.language.lazuli"),
    value("gin", "constant.language.lazuli"),
    value("gist", "constant.language.lazuli"),
    // policy effect actions (catalog values)
    value("create", "entity.name.function.statement.policy.lazuli"),
    value("update", "entity.name.function.statement.policy.lazuli"),
    value("delete", "entity.name.function.statement.policy.lazuli"),
    // semantic-value contexts / lifecycle / misc closed catalogs
    // Retention-action catalog — tmLanguage assigns the precise leaf
    // `constant.language.retention-action.lazuli` (lazuli.tmLanguage.json
    // `feature-retention-line`: `then (anonymize|delete|archive)`). H2
    // reconcile (step 4): promote these off the generic `constant.language`
    // leaf to the precise one so a future value-catalog generator is faithful.
    value("anonymize", "constant.language.retention-action.lazuli"),
    value("archive", "constant.language.retention-action.lazuli"),
    // `escalate` is NOT a closed-catalog value — it is an `approval` block
    // statement keyword. tmLanguage colors it
    // `entity.name.function.statement.approval.lazuli` (approval-block
    // alternation). H2 reconcile (step 4): the spurious `value()` row is
    // dropped here; the faithful Approval-context row is added in the H2
    // tmLanguage-faithfulness backfill block below.
    value("me", "constant.language.lazuli"),
    value("org", "constant.language.lazuli"),
    value("team", "constant.language.lazuli"),
    value("crud", "constant.language.lazuli"),
    value("web", "constant.language.lazuli"),
    value("mobile", "constant.language.lazuli"),
    value("authenticated", "constant.language.lazuli"),
    value("unauthenticated", "constant.language.lazuli"),
    value("custom", "constant.language.lazuli"),
    // ════════════════════════════════════════════════════════════════
    // H2 tmLanguage-faithfulness backfill (Wave H2)
    // ════════════════════════════════════════════════════════════════
    //
    // The tmLanguage keyword-alternation rules are GENERATED from this
    // registry (`cargo xtask gen-tmlanguage`), grouped by `(context, scope)`.
    // H1 enumerated each block's *novel* statement keywords but left some
    // per-block rows implicit — the cross-cutting words (`from`/`to`/`by`/
    // `when`/`required`/...) lived only in their `Modifier`/`Expression`
    // rows, and a handful of block-specific keywords were simply missing.
    //
    // For the generated `#kw-*` alternation of a block to reproduce the
    // hand-written grammar's coverage EXACTLY (zero snapshot drift), each
    // generatable `(context, scope)` group must hold every literal the old
    // inline alternation listed. These rows are that backfill: the literal +
    // its per-block context + the scope leaf the grammar already assigned.
    // Context-as-data (lib.rs §"Context-as-data") — a literal valid in N
    // blocks with different scopes is N rows; these are the per-block rows.
    //
    // Each row mirrors a literal in `editors/vscode/syntaxes/lazuli.tmLanguage.json`.
    // The generator's `GROUPS` allowlist names exactly which `(context, scope)`
    // groups below are wired into the grammar as `#kw-*` includes.

    // ── plan-block: `features | limits | trial` (section scope) ──
    // (`trial` is reconciled in place above, off STMT; only `limits` is new here.)
    stmt(
        "limits",
        Context::Plan,
        SECTION,
        "Plan limit-entitlement block.",
    ),
    // ── tests-block: filter/predicate connectors at the tests scope ──
    stmt(
        "requires",
        Context::Tests,
        TESTS,
        "Test precondition clause.",
    ),
    stmt("when", Context::Tests, TESTS, "Test guard clause."),
    stmt("by", Context::Tests, TESTS, "Test actor binding."),
    stmt("from", Context::Tests, TESTS, "Test source binding."),
    stmt("as", Context::Tests, TESTS, "Test actor/role alias."),
    stmt("to", Context::Tests, TESTS, "Test transition target."),
    // ── translation-block: `catalog | key | plural` ──
    stmt("key", Context::Translation, TRANSLATION, "Translation key."),
    // ── headers-block: `max_age` ──
    stmt(
        "max_age",
        Context::Headers,
        HEADERS,
        "HSTS max-age directive.",
    ),
    // ── limits-block: `timeout` ──
    stmt("timeout", Context::Limits, LIMITS, "Request timeout limit."),
    // ── encryption-block: `key | source | algorithm | rotation | rotation_profile` ──
    stmt(
        "key",
        Context::Encryption,
        ENCRYPTION,
        "Encryption key reference.",
    ),
    stmt(
        "source",
        Context::Encryption,
        ENCRYPTION,
        "Key-source declaration.",
    ),
    stmt(
        "rotation",
        Context::Encryption,
        ENCRYPTION,
        "Key-rotation policy.",
    ),
    // ── tracing-block: `sample_rate` ──
    stmt(
        "sample_rate",
        Context::Tracing,
        TRACING,
        "Trace sampling rate.",
    ),
    // ── deploy-block: `environment` ──
    stmt(
        "environment",
        Context::Deploy,
        DEPLOY,
        "Deploy target environment.",
    ),
    // ── communication-block: `sync | propagate | timeout` ──
    stmt(
        "sync",
        Context::Communication,
        COMMUNICATION,
        "Synchronous channel.",
    ),
    stmt(
        "propagate",
        Context::Communication,
        COMMUNICATION,
        "Context propagation toggle.",
    ),
    stmt(
        "timeout",
        Context::Communication,
        COMMUNICATION,
        "Call timeout.",
    ),
    // ── env-block: `required | optional | default` ──
    stmt(
        "required",
        Context::Env,
        ENV,
        "Required environment variable.",
    ),
    stmt(
        "optional",
        Context::Env,
        ENV,
        "Optional environment variable.",
    ),
    stmt(
        "default",
        Context::Env,
        ENV,
        "Environment-variable default value.",
    ),
    // ── integrations-block: `environment | contract` ──
    stmt(
        "environment",
        Context::Integrations,
        INTEGRATION,
        "Integration environment selector.",
    ),
    stmt(
        "contract",
        Context::Integrations,
        INTEGRATION,
        "Integration contract reference.",
    ),
    // ── packs-block: `provides | from | feature` ──
    stmt(
        "provides",
        Context::Packs,
        PACKS,
        "Pack-provided capability.",
    ),
    stmt("from", Context::Packs, PACKS, "Pack source reference."),
    stmt("feature", Context::Packs, PACKS, "Pack-included feature."),
    // ── defaults-block: project-default modifiers (resource conventions) ──
    stmt(
        "tenancy",
        Context::Defaults,
        DEFAULTS,
        "Default tenancy mode.",
    ),
    stmt(
        "timestamps",
        Context::Defaults,
        DEFAULTS,
        "Default timestamp convention.",
    ),
    stmt(
        "soft_delete",
        Context::Defaults,
        DEFAULTS,
        "Default soft-delete convention.",
    ),
    stmt(
        "retention",
        Context::Defaults,
        DEFAULTS,
        "Default retention policy.",
    ),
    // ── audit-block: lifecycle connectors at the audit scope ──
    stmt(
        "materialize",
        Context::Audit,
        AUDIT,
        "Materialize the audit projection.",
    ),
    stmt(
        "before",
        Context::Audit,
        AUDIT,
        "Audit before-image clause.",
    ),
    stmt("after", Context::Audit, AUDIT, "Audit after-image clause."),
    stmt(
        "data_subject",
        Context::Audit,
        AUDIT,
        "GDPR data-subject binding.",
    ),
    stmt(
        "retain_for",
        Context::Audit,
        AUDIT,
        "Audit retention window.",
    ),
    // ── approval-block: chain connectors at the approval scope ──
    stmt("by", Context::Approval, APPROVAL, "Approver binding."),
    stmt("timeout", Context::Approval, APPROVAL, "Approval timeout."),
    stmt(
        "then",
        Context::Approval,
        APPROVAL,
        "Approval next-step connector.",
    ),
    stmt("deny", Context::Approval, APPROVAL, "Approval deny branch."),
    stmt(
        "allow",
        Context::Approval,
        APPROVAL,
        "Approval allow branch.",
    ),
    stmt(
        "escalate",
        Context::Approval,
        APPROVAL,
        "Approval escalation action.",
    ),
    // (`chain`/`sequential` are reconciled in place above, off STMT.)
    // ── replay-block (webhook): `allow | deny | within | dedupe | by` ──
    // (`within`/`dedupe` are reconciled in place above, off STMT.)
    stmt("allow", Context::Webhook, REPLAY, "Replay allow window."),
    stmt("deny", Context::Webhook, REPLAY, "Replay deny rule."),
    stmt("by", Context::Webhook, REPLAY, "Replay dedupe binding."),
];
