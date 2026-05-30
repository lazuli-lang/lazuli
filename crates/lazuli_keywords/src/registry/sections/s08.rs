//! Registry `ALL` section 8/11 (SPEC-19 split; concatenated in `registry::ALL`).
#![allow(clippy::all, unused_imports)]

use crate::{CapabilitySpec, Context, DiagnosticFacet, SemanticToken, Sigil, Surface};
use super::super::builders::*;
use super::super::facets::*;

pub(crate) const ROWS: &[CapabilitySpec] = &[
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
        "extension",
        Context::Tests,
        "entity.name.function.statement.tests.lazuli",
        "View-test subject: `allows extension <feature>` / `denies extension <feature>` whitelists which features may extend a view via its anchor.",
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
];
