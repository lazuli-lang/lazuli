//! Closed keyword catalogs surfaced through LSP completion and hover.
//!
//! Two arrays:
//!
//! * `KEYWORDS` — every reserved word valid inside canonical (.lzi)
//!   sources. Drives `lazuli_keyword_completion_items` (the global
//!   completion list authors see when no narrower context-aware list
//!   fires) and `keyword_description` (hover one-liners).
//! * `DESIGN_KEYWORDS` — vocabulary specific to `*.design.lzi` sources.
//!   Drives `design_keyword_completion_items` (Backend's completion
//!   path branches on `is_design_lzi_uri`).
//!
//! New keywords land here as comment-tagged blocks so the audit trail
//! survives future rebases. The lists are intentionally hand-curated
//! rather than auto-generated from the grammar — adding a keyword
//! requires a deliberate cut so the LSP completion experience can
//! keep pace with new sugar.
//!
//! ## See also
//! * `lib.rs::lazuli_keyword_completion_items` — wraps `KEYWORDS` into
//!   `CompletionItem`s with hover detail.
//! * `lib.rs::design_keyword_completion_items` — same shape, driven by
//!   `DESIGN_KEYWORDS`.
//! * `crate::hover::keyword_description` — hover one-liner table.
//! * `crate::catalogs` — closed-value catalogs (auth, deploy strategy,
//!   etc.); keep distinct from keyword catalogs.

pub const DESIGN_KEYWORDS: &[&str] = &[
    "design",
    "extends",
    "color",
    "typography",
    "space",
    "radius",
    "shadow",
    "motion",
    "breakpoint",
    "z",
    "family",
    "scale",
    "weight",
    "tracking",
    "duration",
    "easing",
    "size",
    "line_height",
    "base",
    "hover",
    "active",
    "foreground",
    "dark",
];

pub const KEYWORDS: &[&str] = &[
    "workspace",
    "app",
    "error_page",
    "registry",
    "profile",
    "apps",
    "shared_registry",
    "boundaries",
    "gateway",
    "contract",
    "compatibility",
    "import",
    "operation",
    "env",
    "aggregate",
    // CL.C.4 — domain-model vocabulary (roadmap §1.7). `aggregate`
    // was reserved in the catalog before this cut; the four new
    // entries below complete the closed catalog.
    "root",
    "contains",
    "invariant",
    "invariants",
    "when",
    "message",
    "entity",
    "record",
    "command",
    "query.list",
    "query.lookup",
    "query.sql",
    "query.view",
    "agent",
    "model",
    "prompt",
    "tools",
    "safety",
    "stream",
    "temperature",
    "max_tokens",
    "top_p",
    "seed",
    "notification",
    "channel",
    "recipient",
    "template",
    // Notifications expanded bucket cycle — `digest` / `throttle`
    // sub-blocks + their child keywords. Closed-catalog completion
    // for `template_strategy` lives below in
    // `NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES`.
    "digest",
    "throttle",
    "every",
    "group_by",
    "template_strategy",
    "max_per",
    "per_recipient",
    "per_channel",
    "burst",
    // Row 30 — storage bucket cycle `@cap.File(...)` argument keywords.
    "max_size",
    "accept",
    "visibility",
    "signed_ttl",
    // Observability bucket cycle row 36 — `app.logging` /
    // `app.tracing` slot keywords. Closed catalogs surface through
    // `keyword_hover` above and the closed-catalog completion below.
    "logging",
    "tracing",
    "level",
    "format",
    "redact",
    "sample_rate",
    "propagate",
    "exporter",
    // Observability bucket cycle row 37 — `audit emit_to` slot.
    "emit_to",
    // Migrations bucket cycle Route C — `tenant_migration` kind +
    // `deploy.{strategy, lock_timeout, pre_migration_hook,
    // post_migration_hook, checkpoint}` keywords. See
    // `docs/proposals/bucket-migrations-cycle.md`.
    "tenant_migration",
    "strategy",
    "lock_timeout",
    "pre_migration_hook",
    "post_migration_hook",
    "checkpoint",
    "defaults",
    "domain",
    "policies",
    "errors",
    "auth",
    "identity",
    "password",
    "oauth",
    "mfa",
    "sessions",
    "access_ttl",
    "rotation",
    "refresh_ttl",
    "grace",
    "theft_detection_action",
    "refresh",
    "enroll",
    "verify",
    "hash",
    "api",
    "event_group",
    "event.trace",
    "tenancy",
    "timestamps",
    "soft_delete",
    "retention",
    // `docs/proposals/ir-resource-conventions-crud.md` §4 — resource-level
    // `conventions [..]` opt-in. Sibling slot of tenancy/timestamps/etc.
    "conventions",
    "paginate",
    "experience",
    "surface",
    "requires_lifecycle",
    "on_lifecycle_pending",
    "resume",
    "imports",
    "uses",
    "bindings",
    "targets",
    "environments",
    "urls",
    "group",
    "in",
    "integrations",
    "integration",
    "credentials",
    "data_classification",
    "capabilities",
    "architecture",
    "services",
    "service",
    "mode",
    "service_ready",
    "enforce_service_boundaries",
    "owns",
    "exposes",
    "publishes",
    "consumes",
    "communication",
    "internal",
    "external",
    "async",
    "propagate",
    "timeout",
    "runtime",
    "unit",
    "serves",
    "runs",
    "healthcheck",
    "readiness",
    "deploy",
    "migrations",
    "migration_lock",
    "destructive_migrations",
    "rollback",
    "topology",
    "environment",
    "view",
    "audience",
    "extends",
    "input",
    "route",
    "route_guard",
    "previously",
    "migrated",
    "alias",
    "path",
    "params",
    "to",
    "actor_query",
    "default_policy",
    "default_unauthenticated_redirect",
    "default_unauthorized_redirect",
    "on_unauthenticated",
    "on_unauthorized",
    "let",
    "derived",
    "audit",
    "has_many",
    "inverse",
    "policy",
    "policy_for",
    "rate_limit",
    "calls",
    "method",
    "output",
    "cache",
    "key",
    "ttl",
    // Cache bucket cycle (CL.C.3) — feature-level `cache <name>` profile
    // decorators. `tags` / `namespace` already covered above as keywords;
    // these three are the CL.C.3 additions.
    "stale_while_revalidate",
    "coalesce",
    "sliding",
    "invalidates",
    "error",
    "expose",
    "write_window",
    "idempotency",
    "job",
    "webhook",
    "trigger",
    "retry",
    "queue",
    "tenant_from",
    "fanout",
    "axis",
    "external_calls",
    "payload",
    "payload_group",
    "reason",
    "requires",
    "algorithm",
    "secret",
    "header",
    "modifier",
    "from",
    "emits",
    "anchor",
    "extensible_by",
    "slot",
    "platforms",
    "tests",
    "allows",
    "permits",
    "forbids",
    "deny",
    "denies",
    "accepted",
    "rejected",
    "search",
    "filter",
    "list",
    "form",
    "detail",
    "columns",
    "fields",
    "validate",
    "validates",
    "client",
    "fn",
    "hook",
    "validator",
    "adapter",
    "query_modifier",
    "escape_route",
    "required",
    "unique",
    "default",
    // Webhooks expanded cycle — payload/replay/dlq vocabulary.
    "webhook_event",
    "webhook_events",
    "payload_from",
    "version",
    "previous_version",
    "deprecated",
    "replay",
    "allow",
    "within",
    "dedupe",
    "dlq",
    "emit",
    "drop",
    // Roadmap §1.5 (CL.C.2) — DB resource-level decorators.
    "lock",
    "composite_key",
    "@full_text",
    "version_field",
    "primary",
];

/// Rich hover detail strings for every token in [`DESIGN_KEYWORDS`].
/// Returns `None` for keywords without a curated description.
///
/// Consumed by `handlers::hover` (for `*.design.lzi` documents) and by
/// `completion_items::design_keyword_completion_items` so authors get
/// the same explanation in tooltips and completion details.
pub(crate) fn design_keyword_description(keyword: &str) -> Option<&'static str> {
    match keyword {
        "design" => Some(
            "Declares the project-root design token catalog. See `docs/proposals/design-tokens.md`.",
        ),
        "extends" => Some(
            "Declares a base design catalog that this design overrides. See `docs/proposals/design-tokens.md`.",
        ),
        "color" => Some(
            "Closed design token group for brand, semantic, and surface colors. See `docs/proposals/design-tokens.md`.",
        ),
        "typography" => Some(
            "Closed design token group for type families, scales, weights, and tracking. See `docs/proposals/design-tokens.md`.",
        ),
        "space" => Some(
            "Closed design token group for the spacing scale. See `docs/proposals/design-tokens.md`.",
        ),
        "radius" => Some(
            "Closed design token group for border radius values. See `docs/proposals/design-tokens.md`.",
        ),
        "shadow" => Some(
            "Closed design token group for CSS box-shadow elevation values. See `docs/proposals/design-tokens.md`.",
        ),
        "motion" => Some(
            "Closed design token group for transition and animation primitives. See `docs/proposals/design-tokens.md`.",
        ),
        "breakpoint" => Some(
            "Closed design token group for responsive viewport cutoffs. See `docs/proposals/design-tokens.md`.",
        ),
        "z" => Some(
            "Closed design token group for stacking order values. See `docs/proposals/design-tokens.md`.",
        ),
        "family" => Some(
            "Typography sub-group for named font stacks. See `docs/proposals/design-tokens.md`.",
        ),
        "scale" => Some(
            "Typography sub-group for named text sizes and line heights. See `docs/proposals/design-tokens.md`.",
        ),
        "weight" => Some(
            "Typography sub-group for named font weights. See `docs/proposals/design-tokens.md`.",
        ),
        "tracking" => Some(
            "Typography sub-group for named letter-spacing values. See `docs/proposals/design-tokens.md`.",
        ),
        "duration" => Some(
            "Motion sub-group for named transition durations. See `docs/proposals/design-tokens.md`.",
        ),
        "easing" => {
            Some("Motion sub-group for named easing curves. See `docs/proposals/design-tokens.md`.")
        }
        "size" => Some(
            "Typography scale field for a text token's font size. See `docs/proposals/design-tokens.md`.",
        ),
        "line_height" => Some(
            "Typography scale field for a text token's line height. See `docs/proposals/design-tokens.md`.",
        ),
        "base" => Some(
            "Required default color state; also commonly used as a token name. See `docs/proposals/design-tokens.md`.",
        ),
        "hover" => Some(
            "Optional color state for mouse hover or touch press start. See `docs/proposals/design-tokens.md`.",
        ),
        "active" => Some(
            "Optional color state for mouse down or touch press end. See `docs/proposals/design-tokens.md`.",
        ),
        "foreground" => Some(
            "Optional text/icon color when the token is used as a background. See `docs/proposals/design-tokens.md`.",
        ),
        "dark" => Some(
            "Optional dark-theme suffix for a color value. See `docs/proposals/design-tokens.md`.",
        ),
        _ => None,
    }
}
