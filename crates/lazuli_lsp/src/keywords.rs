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

/// Every reserved word valid inside canonical (`.lzi`) sources. Drives
/// the global completion list authors see when no narrower
/// context-aware list fires, plus the hover one-liner table consumed
/// by `keyword_description`.
///
/// The list is intentionally hand-curated — adding a keyword requires
/// a deliberate cut so the LSP completion experience keeps pace with
/// new sugar. See module docs for the `*.design.lzi`-only companion
/// list [`DESIGN_KEYWORDS`].
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
    // BUG-1 — `app.cors` block child keys. Surfaced so the
    // `keyword_surface_parity` LSP-catalog gate is satisfied for the new
    // `Context::Cors` registry rows; hover one-liners fall out of the
    // registry via `keyword_description`. (`max_age` is already listed for
    // the `cookie` / `headers` blocks.)
    "allow_origins",
    "allow_credentials",
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
    // Audit-block connectors (registry-backfilled via H2; surfaced for LSP completion).
    "data_subject",
    "retain_for",
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
    // `cookie-sessions-child` — the `auth.sessions.cookie` transport
    // sub-block + its attribute vocabulary. `same_site` / `secure` /
    // `http_only` are shared with the app-level `app.cookie` profiles
    // (`Context::Cookie`); `domain` / `path` already appear in this
    // catalog as generic keywords; `name` is the generic `modifier`.
    "cookie",
    "same_site",
    "secure",
    "http_only",
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
    "requires_lifecycle_in",
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
    // Surface-sync WT-2 — parser primitives the hand-curated catalog
    // was missing. Grouped by concern. `target` / `@full_text` /
    // `targets` already live above; not duplicated here.
    //
    // Domain / resource decorators + relations (GAP-07/13, GAP-AUDIT-02,
    // W3 GAP-03, W4 GAP-08).
    "append_only",
    "many_through",
    "polymorphic_ref",
    "computed_date",
    "schedule_rule",
    "@slug",
    "@owner_axis",
    // Command-side verbs + approval-chain vocab (W4 GAP-REORDER-01 /
    // GAP-06, GAP-AUDIT-01).
    "reorder",
    "materialize",
    "approval",
    "chain",
    "sequential",
    // Feature kinds + iron-hand context vocabulary.
    "report",
    "poller",
    "attach_ctx",
    "purpose",
    "non_goals",
    // `knowledge <sector>` — bareword sector slug naming the
    // `knowledge/<sector>/` vault the feature draws from. Sibling
    // of purpose / non_goals / attach_ctx. See
    // `docs/proposals/knowledge-sector-field.md`.
    "knowledge",
    // Surface (`.lzx`) view / create / UX primitives. `view list`
    // projections, the `view create` submit + post-submit contract, and
    // the Wave-W6 / GAP-UX UX primitives.
    "source",
    "submit",
    "on_success",
    "drawer",
    "sort",
    "selection",
    "bulk_actions",
    "settings",
    "persist",
    "flash",
    "replace",
    "back",
    "redirect",
    "tabs",
    "tab",
    "tab_group",
    "wizard",
    "wizard_steps",
    "view_mode",
    "inline_table",
    "date_range",
    "board",
    "lanes",
    "repeatable",
    "step",
    // Wave H3 — registry-parity backfill. These are `SemanticToken::Keyword`
    // literals carried in `lazuli_keywords::ALL` (proven complete against
    // the parser by H1) that the hand-curated catalog had drifted past.
    // The `keyword_surface_parity` LSP-coverage assertion now enforces
    // that every registry keyword-token is offered here; hover one-liners
    // come from the registry via `hover::keyword_description`'s H3 fallback.
    //
    // Feature-body / domain.
    "constraints",          // `non_goals` child (alongside delegated_to / out_of_scope)
    "discriminator",        // agent structured-output discriminator
    "on_delete",            // resource referential on-delete action
    "outbox",               // job outbox-pattern marker
    "payload_axis",         // channel partition axis
    "materialize_strategy", // tenant_migration materialization strategy
    "no_timestamps",        // defaults-block timestamp opt-out
    // tests / evals.
    "golden",    // golden-output assertion
    "min_score", // minimum eval score
    // Surface (`.lzx`) view primitives.
    "loader",        // data-loader reference
    "opens",         // drawer/modal open trigger
    "hint",          // field hint text
    "error_view",    // error-state view
    "error_redact",  // error redaction
    "pending_view",  // pending/loading view
    "role_mismatch", // route-guard per-role mismatch redirect
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
