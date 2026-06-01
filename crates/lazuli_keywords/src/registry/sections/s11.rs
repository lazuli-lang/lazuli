//! Registry `ALL` section 11/11 (SPEC-19 split; concatenated in `registry::ALL`).
#![allow(clippy::all, unused_imports)]

use super::super::builders::*;
use super::super::facets::*;
use crate::{CapabilitySpec, Context, DiagnosticFacet, SemanticToken, Sigil, Surface};

pub(crate) const ROWS: &[CapabilitySpec] = &[
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
    stmt(
        "rate_limit",
        Context::Defaults,
        DEFAULTS,
        "Default rate-limit spec hoisted to every command (per-command `rate_limit` wins).",
    ),
    stmt(
        "audit",
        Context::Defaults,
        DEFAULTS,
        "Default audit mode hoisted to every command (`audit default`; per-command `audit`/`audit none` wins).",
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
