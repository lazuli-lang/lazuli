//! Keyword-level hover content for the LSP.
//!
//! Two layers, in order of preference:
//!
//! 1. **`rich_keyword_hover`** — Markdown-rendered hover for the closed
//!    catalog of "first-class" kinds (`command`, `query.list`, `policy`,
//!    `agent`, `errors`, `conventions`, ...). Each entry packages a
//!    one-line summary, required-children bullets, optional-children
//!    bullets, a worked example, and a doc anchor. This is what the IDE
//!    surfaces on hover when the word matches one of these kinds.
//!
//! 2. **`keyword_description`** — plain `&'static str` one-liner for every
//!    other DSL keyword. Used as the **fallback** when the rich layer
//!    returns `None`, and as the `detail:` field on completion items so
//!    the LLM/human sees the contract inline in the completion popup.
//!
//! ## ABI guarantee
//!
//! Both functions are re-exported from the crate root via `pub use
//! hover::*;` so external consumers (Hostpoint VSCode extension,
//! `lazuli_cli::doctor`) keep importing them from the same path
//! (`lazuli_lsp::keyword_description`, `lazuli_lsp::rich_keyword_hover`).
//!
//! ## Cross-module references
//!
//! `keyword_description` aliases 12 error-code descriptions to
//! `crate::error_vocab_code_detail` (lives in `catalogs.rs` — the
//! resolved-text catalog) so the closed-catalog error codes share the
//! same hover phrasing whether the cursor is on the code itself or on a
//! `when_denied` arm referencing it. Three `crate::LIFECYCLE_*_HOVER`
//! constants (declared `pub(crate)` in lib.rs and also consumed by
//! `lifecycle_gate_hover`) carry the routed lifecycle-state copy.

pub fn keyword_description(keyword: &str) -> Option<&'static str> {
    match keyword {
        "workspace" => Some(
            "Declares an optional distributed-system contract across local apps and external services.",
        ),
        "app" => Some("Declares the `.lzi` application entrypoint and operational contract."),
        "error_page" => Some(
            "Declares one custom app-level error response page. Header is `error_page <status>`; children are `template \"./...\"` and optional `audience <name>`.",
        ),
        "registry" => Some(
            "Declares the package-level catalog for env, capabilities, integrations, and packs.",
        ),
        "profile" => Some(
            "Declares environment-specific app overrides such as public URLs, sandbox integrations, binding overrides, and deploy topology.",
        ),
        "apps" => Some("Groups apps that participate in a workspace graph."),
        "shared_registry" => {
            Some("Declares the package registry shared by apps in a workspace contract.")
        }
        "boundaries" => Some("Groups workspace event publication and consumption edges."),
        "gateway" => {
            Some("Declares provider-neutral public ingress routes for a distributed workspace.")
        }
        "contract" => {
            Some("References a versioned external service contract, not an implementation.")
        }
        "compatibility" => Some("Declares the external contract compatibility policy."),
        "import" => Some(
            "Imports an external contract schema such as OpenAPI, AsyncAPI, Proto, JSON Schema, or Avro.",
        ),
        "operation" => Some("Declares one provider-neutral external service operation."),
        "environments" => {
            Some("Declares deployment/runtime environments such as local, staging, and production.")
        }
        "urls" => {
            Some("Declares public app URLs used by clients, CORS, emails, callbacks, and webhooks.")
        }
        "bindings" => Some(
            "Two shapes by context. (1) Under `app` / `profile`: binds abstract feature requirements to concrete integration entries (`<feature>.<slot> = integrations.<name>`). (2) Under `registry`: sugar alias for `integrations`, with the simplified `endpoint env.<NAME>` / `auth keys env.<ID> env.<SECRET>` child surface for adapter credential wiring (B1 — see W3-blockers + hostpoint-complete-roadmap §3.5).",
        ),
        "packs" => Some(
            "Declares Lazuli pack catalog entries in `registry.lzi` or enabled pack references in `app.lzi`.",
        ),
        "provides" => {
            Some("Declares what a registry pack provides, such as `provides feature payments`.")
        }
        "from" => Some(
            "Declares a source relationship, such as pack enablement or create-from-input sugar.",
        ),
        "capabilities" => Some(
            "Declares required runtime capabilities without choosing concrete infrastructure providers.",
        ),
        "integrations" => Some(
            "Declares external integration registry entries without provider-specific operation details.",
        ),
        "credentials" => {
            Some("Declares credential scope and bindings for an external integration.")
        }
        "data_classification" => Some("Declares the broad PII class returned by an integration."),
        "architecture" => Some(
            "Declares provider-neutral architecture mode and service-boundary enforcement intent.",
        ),
        "services" => Some("Declares logical service ownership boundaries for the app."),
        "service" => Some("Declares one logical app service boundary under `services`."),
        "owns" => Some("Declares which Lazuli features a logical service owns."),
        "exposes" => Some("Groups commands, queries, APIs, or workflows exposed by a service."),
        "publishes" => Some("Declares event patterns a logical service publishes."),
        "consumes" => Some("Declares external or cross-service events a logical service consumes."),
        "communication" => {
            Some("Declares sync/async intent and context propagation across service boundaries.")
        }
        "internal" => Some("Declares the internal sync communication contract."),
        "external" => Some("Declares the external communication contract."),
        "async" => Some("Declares the asynchronous communication contract."),
        "propagate" => Some("Declares context values propagated across service boundaries."),
        "timeout" => Some("Declares a default service-boundary timeout."),
        "runtime" => {
            Some("Declares generated runtime units such as api, web, worker, and scheduler.")
        }
        "unit" => Some("Declares one app runtime unit under the app manifest `runtime` block."),
        "mode" => Some("Declares the app architecture mode."),
        "service_ready" => Some(
            "Marks whether the app keeps service boundaries visible for future split deployments.",
        ),
        "enforce_service_boundaries" => {
            Some("Marks whether cross-service ownership boundaries should be enforced by tooling.")
        }
        "serves" => Some("Declares which contracts a runtime unit serves."),
        "runs" => Some("Declares which jobs or schedules a runtime unit runs."),
        "healthcheck" => Some("Declares a runtime healthcheck path for deploy safety."),
        "readiness" => Some("Declares a runtime readiness path for deploy safety."),
        "deploy" => {
            Some("Declares provider-neutral deploy gates such as migrations and rollback behavior.")
        }
        "migrations" => Some("Declares when deploy applies database migrations."),
        "migration_lock" => Some("Declares whether deploy must hold a migration lock."),
        "destructive_migrations" => Some("Declares how deploy handles destructive schema changes."),
        "rollback" => Some("Declares rollback behavior for failed deploy health checks."),
        "topology" => Some("Declares an environment deploy topology override in a profile."),
        "environment" => Some("Selects a provider environment such as sandbox or production."),
        "env" => Some("Declares typed environment variables and client/server exposure."),
        "aggregate" | "entity" => Some(
            "Declares a domain resource with fields and behavior. Inspect with `lazuli inspect <file> --expand=resources` to project the typed slice.",
        ),
        "record" => Some(
            "Declares a non-persisted typed result/DTO shape. Inspect with `lazuli inspect <file> --expand=records` to project the typed slice.",
        ),
        "agent" => Some(
            "Declares an LLM-powered capability with typed input, output, prompt template, model reference, policy, and rate limits. the runtime wires the LLM transport; Lazuli owns the contract.",
        ),
        "notification" => Some(
            "Declares a multi-channel outbound notification with `channel`, `recipient`, `trigger`, `template`, and `policy`. the runtime generates dispatch wiring; adapters (Sendgrid/SES/Twilio/APNs/FCM) handle transport.",
        ),
        "channel" => Some(
            "Two distinct uses, disambiguated by indent level:\n\n\
             • Feature-level kind: `channel <name>` declares a typed, tenant-scoped, \
               policy-gated push stream (realtime bucket cycle MVP). Required children: \
               `tenant_from <axis>`, `policy @policy.<name>`, `payload <RecordType>`. \
               Transport (WebSocket / SSE) is adapter-resolved at runtime; the language \
               declares the contract. Doctor: `CHANNEL-PAYLOAD-001`.\n\n\
             • On a `notification`, declares one or more delivery channels: \
               `email`, `push`, `sms`, `in_app`.",
        ),
        "recipient" => Some(
            "On a `notification`, declares the recipient expression (e.g., `target.email`, `payload.user_id`).",
        ),
        "template" => Some(
            "Points to a relative template file. Used by `notification` delivery templates and app-level `error_page` responses.",
        ),
        "digest" => Some(
            "On a `notification`, declares window-based aggregation. Children: `every \"<duration>\"` (required), `group_by <payload-path>`, `max_size <N>` (1..=10000), `template_strategy merge|append`. Distinct from scalar `rate_limit`. Doctor: `NOTIF-DIGEST-001/002/003`.",
        ),
        "throttle" => Some(
            "On a `notification`, declares structured per-recipient / per-channel rate-limit with optional burst. Children: `max_per \"<duration>\"` (required), `per_recipient`, `per_channel`, `burst <N>`. Distinct from scalar `rate_limit` (which is per-call). Doctor: `NOTIF-THROTTLE-001/002/003`.",
        ),
        "every" => Some(
            "On `notification.digest`, sets the aggregation window. Closed shape: `<N> (seconds|minutes|hours|days)`. Example: `every \"15 minutes\"`.",
        ),
        "group_by" => {
            Some("On `notification.digest`, keys the aggregation bucket on a payload path.")
        }
        "max_size" => Some(
            "On `notification.digest`, caps items per digest window. Range: 1..=10000. Above the ceiling buffers unbounded payloads.",
        ),
        "template_strategy" => Some(
            "On `notification.digest`, declares how the adapter combines per-trigger payloads when rendering the digest template. Closed catalog: `merge` (last-write-wins per key), `append` (emits a list).",
        ),
        "max_per" => Some(
            "On `notification.throttle`, sets the refill window for the rate-limit bucket. Closed shape: `<N> (seconds|minutes|hours|days)`.",
        ),
        "per_recipient" => Some(
            "On `notification.throttle`, keys the throttle bucket on the notification's `recipient <path>`. At least one of `per_recipient` or `per_channel` is required.",
        ),
        "per_channel" => Some(
            "On `notification.throttle`, gives each channel of a multi-channel notification its own bucket (e.g., email and `in_app` throttled independently).",
        ),
        "burst" => Some(
            "On `notification.throttle`, number of immediate dispatches the bucket allows before throttling starts. Useful for OTP / login flows.",
        ),
        "model" => {
            Some("On an `agent`, references the LLM model under the `@llm.<name>` namespace.")
        }
        "prompt" => Some("On an `agent`, points to the prompt template file at `./path`."),
        "tools" => Some(
            "On an `agent`, declares the closed list of `@tool.<name>` references the agent may invoke.",
        ),
        "safety" => Some(
            "On an `agent`, declares safety classifiers or policy checks applied to inputs/outputs.",
        ),
        "stream" => Some(
            "On an `agent` `output`, marks the response as a streamed value: `output stream <Type>`.",
        ),
        "command" => Some("Declares a write operation for an aggregate."),
        "query.list" => Some("Declares a generated collection query."),
        "query.lookup" => Some("Declares a generated single-record lookup query."),
        "query.sql" => Some("Declares a query backed by an external SQL file."),
        "query.view" => Some("Declares a typed SQL-backed screen-read query."),
        "defaults" => Some(
            "Declares repeated feature defaults such as tenancy and timestamps. Inspect with `lazuli inspect <file> --expand=defaults` to project the IR-driven defaults block.",
        ),
        "domain" => Some("Groups resources, records, queries, rules, and events."),
        "policies" => Some("Declares feature-local policy categories and field policies."),
        "auth" => Some(
            "Authentication block: groups identity, password, sessions, MFA, and OAuth subcontracts for a feature.",
        ),
        "identity" => Some(
            "`identity <Resource>.<field>` — names the resource field used as the canonical login identifier.",
        ),
        "oauth" => Some(
            "OAuth subcontract: `oauth <provider>` with `adapter @adapter.<x>`. v0 providers: `google`, `github`, `microsoft`, `apple`.",
        ),
        "mfa" => {
            Some("MFA subcontract: `mfa <method>` with `enroll` + `verify`. v0 method: `totp`.")
        }
        "sessions" => Some(
            "Sessions subcontract: backing resource, legacy `ttl`, optional short-lived `access_ttl`, and optional nested `rotation` refresh-token discipline.",
        ),
        "access_ttl" => Some(
            "Short-lived access token TTL. Used on every API request. Framework default under `rotation` is `15 minutes`. Shorter reduces stolen-access-token blast radius; longer reduces refresh round trips. Doctor warns above 1 hour (AUTH-REFRESH-005).",
        ),
        "refresh_ttl" => Some(
            "Long-lived refresh token TTL inside `auth.sessions.rotation`. Used only by the refresh endpoint. Framework default is `30 days`; it must exceed `access_ttl` so access tokens stay short-lived (AUTH-REFRESH-001).",
        ),
        "grace" => Some(
            "Refresh rotation grace window. A recently revoked refresh can still succeed briefly to absorb legitimate two-tab races. Framework default is `30 seconds`; longer windows weaken theft detection and doctor warns above 5 minutes (AUTH-REFRESH-004).",
        ),
        "theft_detection_action" => Some(
            "Closed auth-rotation catalog for revoked-refresh reuse past grace. `revoke_session_family` revokes this device chain; `revoke_user` revokes every session for the user. Framework default is `revoke_session_family`.",
        ),
        "refresh" => Some("Whether the session adapter issues refresh tokens. Default `false`."),
        "enroll" => {
            Some("Enrolment function reference (`@fn.*`) returning method-specific enrolment data.")
        }
        "verify" => {
            Some("Verification reference (`@fn.*` or `@validator.*`) returning success/failure.")
        }
        "errors" => Some("Declares public/private client error exposure defaults or cases."),
        "api" => {
            Some("Declares a custom typed HTTP endpoint outside command/query/webhook semantics.")
        }
        "event_group" => Some("Declares a shared same-feature event payload template."),
        "event.trace" => {
            Some("Declares an observability-only event that is outside the feature reaction graph.")
        }
        // CL.C.4 — domain-model vocabulary (roadmap §1.7).
        "aggregate" => Some(
            "Declares a DDD consistency boundary: `aggregate <Name>` with `root <Resource>` (required), optional `contains <Resource>, ...`, and an optional `invariants` block. The root is the consistency anchor; `contains` lists the closed cluster. Predicate language stays closed-catalog (no escape hatch).",
        ),
        "root" => Some(
            "Inside `aggregate`: declares the consistency-boundary root. Exactly one resource name (`root <Resource>`). Doctor: `AGGREGATE-ROOT-UNKNOWN` rejects roots that do not resolve to a same-feature resource.",
        ),
        "contains" => Some(
            "Inside `aggregate`: declares the closed cluster (`contains <Resource>, <Resource>, ...`). Comma-separated bare resource names. Doctor: `AGGREGATE-CONTAINS-UNKNOWN` rejects unknown members.",
        ),
        "invariant" => Some(
            "Declares a closed-catalog predicate: `invariant <name>` with `when <predicate>` (required) and optional `message \"<text>\"`. Standalone form lives inside `resource` or `aggregate.invariants`. Doctor: `INVARIANT-PREDICATE-INVALID` rejects predicates referencing unknown fields. NOT a mechanism escape hatch — predicate sublanguage is the same closed catalog used by agent `evals`.",
        ),
        "invariants" => Some(
            "Inside `aggregate`: open block of `invariant <name>` declarations whose predicates may span the cluster (root + contains members). Same shape as resource-scoped invariants.",
        ),
        "when" => Some(
            "Inside `invariant`: the closed-catalog predicate. Comparisons (`=`, `!=`, `<`, `<=`, `>`, `>=`), `has <element>`, `and`/`or`. Identifiers must resolve to fields on the scoping resource (or the aggregate's root + `contains` members).",
        ),
        "@slug" => Some(
            "Field decorator: marks the field as the resource's URL slug column. Codegen emits a unique index + case-insensitive lookup. Doctor: `SLUG-UNIQUENESS-IMPLICIT` warns when `@slug` is missing an explicit `unique` modifier.",
        ),
        "tenancy" => Some("Declares the tenant axis for generated scope and indexes."),
        "timestamps" => Some("Adds generated created/updated timestamp fields."),
        "soft_delete" => Some("Adds generated soft-delete scope and delete semantics."),
        "retention" => Some("Declares data retention for a resource or feature default."),
        "lock" => Some(
            "On a `resource`, declares the concurrency-control strategy for writes. Closed catalog: `optimistic version_field: <field>` (compare-and-set on a monotonic counter), `pessimistic` (transaction-wide row lock), `row_level` (`SELECT ... FOR UPDATE` per row). Doctor: `RESOURCE-LOCK-CONTRACT-001` cross-checks `version_field` against `Resource.fields` and rejects non-Integer types.",
        ),
        "composite_key" => Some(
            "On a `resource`, replaces or augments the implicit `id BIGSERIAL PRIMARY KEY`. Children: `fields <a>, <b>, ...` (required, >=1 name), `primary true|false` (default `false`). With `primary true` emits `PRIMARY KEY (<fields>)`; with `primary false` emits `UNIQUE (<fields>)`. Doctor: `COMPOSITE-KEY-CONTRACT-001` rejects unknown / empty `fields`.",
        ),
        "@full_text" => Some(
            "On a text field, marks the column for a Postgres `GIN (to_tsvector('english', col))` index so the runtime can execute full-text search. Applies to `Text` and the semantic string variants (`@semantic.Email`, `@semantic.Phone`, `@semantic.Url`, `@semantic.Uuid`, `@semantic.Currency`). Doctor: `FULL-TEXT-TYPE-001` rejects non-text types.",
        ),
        "version_field" => Some(
            "On `lock optimistic`, names the integer column carrying the row's monotonic version. Doctor: `RESOURCE-LOCK-CONTRACT-001` rejects unknown / non-Integer fields.",
        ),
        "paginate" => Some("Declares the positive default page size for a `query.list`."),
        "surface" => Some("Declares UI projections for list, form, and detail views."),
        "input" => Some("Lists fields accepted by a command."),
        "route" => Some(
            "Declares route or context values accepted by a command/view, or a top-level typed app route in `.lzx`.",
        ),
        "previously" => Some(
            "Declares identity continuity with an explicit `migrated` or `alias` mode. Doctor: `PREVIOUSLY-FWD-001` rejects stale rename targets; `PREVIOUSLY-CYCLE-001` rejects A→B→A cycles; `PREVIOUSLY-DUP-001` rejects two current names claiming the same previous identity.",
        ),
        "migrated" => Some(
            "Marks a previous name as migration-only history, not a generated compatibility alias.",
        ),
        "alias" => Some(
            "Marks a previous name as a temporary compatibility alias still accepted by generated surfaces.",
        ),
        "path" => Some("Declares a concrete URL path for app routes, APIs, or webhooks."),
        "params" => Some("Declares typed query or API parameters."),
        "to" => Some("Binds a top-level `.lzx` route to an abstract experience view."),
        "let" => Some("Binds a derived value for later command, job, or event expressions."),
        "derived" => Some(
            "Marks a resource field as computed at read time: `<name>: <Type> derived from <expression>`. Not persisted; cannot have `default`, `required`, or `optional`.",
        ),
        "audit" => Some(
            "Declares an operation as audited. Use `audit` for default fields, `audit <field>, <field>` for explicit entries, or `audit none` to opt out.",
        ),
        // Wave B — `effect` closed-catalog verbs on commands. Cursor
        // hover on the verb shows the one-liner; richer Markdown
        // template lives in `rich_keyword_hover("effect")`.
        "creates" => Some(
            "Command write effect — creates a new row of `<Resource>`. Body assigns input/derived values to fields. One mutating effect per command.",
        ),
        "updates" => Some(
            "Command write effect — mutates the loaded `target` of `<Resource>`. Body assigns the changed fields. One mutating effect per command.",
        ),
        "deletes" => Some(
            "Command write effect — removes the loaded `target` of `<Resource>`. Soft-delete is automatic when the resource declares `soft_delete`.",
        ),
        "returns" => Some(
            "Command non-mutating effect — returns a `<Record>` shape without writing to a resource. Also valid on `query.sql` as `returns <Type>`.",
        ),
        // PG.B — Plan & Gate vocabulary hovers.
        "plan" => Some(
            "Subscription tier declaration. Declares a feature set and a limit set, optionally with a `trial` revert policy. The catalog is package-wide; the union of every plan's `features`/`limits` forms the closed set for `gate` directives.",
        ),
        "features" => Some(
            "Comma-separated identifier list of features in this plan. Cross-plan reuse: `features <other_plan>.features`. References at call sites: `gate behind plan.feature: <name>`.",
        ),
        "limits" => Some(
            "Comma-separated `<name> <value>` pairs. Value is a positive integer or the literal `unlimited`. Cross-plan reuse: `limits <other_plan>.limits`. References at call sites: `gate quota plan.limit: <name>`.",
        ),
        "trial" => Some(
            "Trial revert policy on a plan: `trial duration <integer><s|m|h|d>, then <plan>`. Runtime watches the subscription's expires_at and reverts to `<plan>` after the duration.",
        ),
        "unlimited" => Some(
            "Limit value meaning the runtime emits no quota check at this tier. Use to opt a plan out of a quota that other plans declare.",
        ),
        "subscription" => Some(
            "App-level directive: `subscription resource <feature>.<field>` names the resource that holds the active subscription. Required when any callable uses `gate behind plan.*` or `gate quota plan.*`. Exactly one per app.",
        ),
        "gate" => Some(
            "Subscription gate on a callable. Two forms: `gate behind plan.feature: <name>` (boolean, 402 plan.feature_forbidden on refusal) or `gate quota plan.limit: <name>` (counter, 402 plan.quota_exceeded; increments after success).",
        ),
        "behind" => Some(
            "Boolean gate: `gate behind plan.feature: <name>`. Refuses dispatch when the caller's active plan does not list `<name>` in its `features` set. Evaluates before `policy`.",
        ),
        "quota" => Some(
            "Counter gate: `gate quota plan.limit: <name>`. Refuses dispatch when period usage has reached the plan's value for `<name>`; increments after successful dispatch.",
        ),
        "has_many" => Some(
            "Declares a collection on a resource: `has_many <name>: <Type> [inverse <field>]`. the runtime generates the inverse lookup query and foreign-key contract.",
        ),
        "inverse" => Some(
            "Declares the field on the target resource that owns the inverse foreign key for a `has_many` collection.",
        ),
        "policy" => Some(
            "Associates a command, query, API, webhook, job, or view route guard with an authorization policy capability.",
        ),
        "policy_for" => Some("Declares a feature default policy for specific construct families."),
        "route_guard" => Some(
            "App-level route guard defaults. Children: `default_policy`, `default_unauthenticated_redirect`, and `default_unauthorized_redirect`.",
        ),
        "requires_lifecycle" => Some(crate::LIFECYCLE_REQUIRES_HOVER),
        "on_lifecycle_pending" => Some(crate::LIFECYCLE_PENDING_HOVER),
        "resume" => Some(crate::LIFECYCLE_RESUME_HOVER),
        "actor_query" => Some(
            "App-level query reference that resolves the active actor for route guards. Format: `<feature>.query.<name>`.",
        ),
        "default_policy" => Some(
            "Inside `app.route_guard`, the fallback policy for routes without a view- or audience-level guard.",
        ),
        "default_unauthenticated_redirect" => {
            Some("Inside `app.route_guard`, the fallback redirect path when no actor is signed in.")
        }
        "default_unauthorized_redirect" => Some(
            "Inside `app.route_guard`, the fallback redirect path when a signed-in actor fails the guard policy.",
        ),
        "on_unauthenticated" => {
            Some("Inside a view route guard, redirect target for users who are not signed in.")
        }
        "on_unauthorized" => Some(
            "Inside a view route guard, redirect target for signed-in users who fail the guard policy.",
        ),
        "rate_limit" => Some(
            "`rate_limit \"N per UNIT per scope\" [in <env>, <env>...]`. Default rate limit applies in any env not matched by an `in`-qualified line. Closed env catalog: `production`, `staging`, `test`, `dev`, `local`. Multiple `rate_limit` lines per command are allowed when at most one is unqualified (the default)."
        ),
        "calls" => Some(
            "Declares that a command or job calls an abstract integration/service operation; the runtime wires this to Go transport bindings.",
        ),
        "method" => Some("Declares the HTTP method for a custom API endpoint."),
        "output" => Some("Declares the response shape for a custom API endpoint."),
        "locale" => Some(
            "App locale contract: `default <tag>`, `supported <tags>` (comma-separated), optional `fallback <src> -> <dst>` edges. Supersedes the bare `default_locale` scalar when present. BCP-47 tags (e.g. `pt-BR`, `en-US`).",
        ),
        "supported" => Some(
            "List of BCP-47 tags the app accepts. The locale-negotiation middleware matches `Accept-Language` against this list.",
        ),
        "fallback" => Some(
            "Locale fallback edge: `fallback <src> -> <dst>`. When a translation is missing in the source tag, the runtime walks fallbacks before defaulting to `app.locale.default`.",
        ),
        "cache" => Some(
            "Cache contract. Two shapes: (1) feature-level `cache <name>` profile (`key`/`ttl` required; optional `namespace`, `tags`, `stale_while_revalidate`, `coalesce`, `sliding`) — queries opt in via `cache <profile>`. (2) Inline `cache` block under a query for one-off `key`/`ttl` pairs. Requires a `cache <name>` capability in `registry.lzi`.",
        ),
        "key" => Some("Declares a cache key, lookup key, or dedupe key depending on context."),
        "ttl" => Some(
            "Cache time-to-live. Closed unit catalog: `s`, `m`, `h`, `d` (e.g. `5m`, `7d`). Quoted prose (`\"5 minutes\"`) also accepted; adapters parse it.",
        ),
        "tags" => Some(
            "Cache tags: comma-separated labels used by `invalidates tag:<label>` for fan-out invalidation across queries. Labels are author-defined lowercase identifiers.",
        ),
        "namespace" => Some(
            "Cache namespace label. Scopes the cache key beyond the default `<feature>.query.<name>` to avoid collisions in workspace / pack deployments. One namespace per query.",
        ),
        "stale_while_revalidate" => Some(
            "Cache stale-while-revalidate window (closed unit catalog: `s`, `m`, `h`, `d`). After `ttl` expires, the runtime may serve the stale value for up to this duration while a background refresh runs. Must be <= `ttl`.",
        ),
        "coalesce" => Some(
            "Cache request coalescing. `coalesce true` makes concurrent misses on the same key wait on a single populate (stampede protection); `coalesce false` lets each miss populate independently. Defaults to the runtime's policy (today: `false`).",
        ),
        "sliding" => Some(
            "Cache sliding TTL. `sliding true` extends the TTL window on every read (access-recency cache); `sliding false` keeps a fixed expiry. Requires a typed `ttl` literal (`<int>s|m|h|d`) so the runtime can slide deterministically.",
        ),
        "invalidates" => Some(
            "On a `command`, declares queries that become stale after the command succeeds. Each line is `query.<name>` or `query.<name>(arg: route.<slot>)`. The runtime evicts matching cache entries after a successful commit. Doctor: `cache_invalidates_target_unresolved`.",
        ),
        "approval" => Some(
            "On a `command`, declares a conditional human sign-off block (Cut A.9). Required children: `required_when <predicate>`, `by @role.<name>`, `timeout \"<duration>\"`, `then deny|allow|escalate`. Doctor: `approval_contract_diagnostics`, `approval_timeout_invalid_diagnostics`. IR field: `Command.approval: Option<ApprovalSpec>`.",
        ),
        "error" => Some("Declares a named public error case with status and exposure fields."),
        "expose" => Some("Declares which error fields are visible to generated clients."),
        "write_window" => Some("Declares the temporal write window checked before a command runs."),
        "idempotency" => Some(
            "Declares a dedupe key. Jobs, webhooks, and notifications use `idempotency by <path>`; tenant migrations use `idempotency <path>`. Re-fires sharing the same key are no-ops.",
        ),
        "job" => Some(
            "Declares a unit of asynchronous or scheduled work. `trigger event ...` runs as a reactor; `trigger schedule \"<cron>\"` runs as scheduled. Add `queue <lane>` to enqueue rather than run inline. Body is either `handler \"./...\"` or a declarative target / let / updates / emits chain.",
        ),
        "webhook" => Some(
            "Declares a verified inbound HTTP integration boundary. Requires `path \"...\"` and `verify hmac <alg>` with nested `secret env.X` + `header \"X-...\"`. Multi-tenant apps must declare `tenant_from payload.<axis>_id`.",
        ),
        // Migrations bucket cycle Route C — `tenant_migration` kind +
        // deploy block expansion. See `docs/proposals/bucket-migrations-cycle.md`.
        "tenant_migration" => Some(
            "Per-tenant idempotent data migration. Closed body: `target query.<name>|command.<name>`, `axis <tenant_axis>`, `idempotency <path>`, `retry`, `timeout`, and `handler \"./...\"`.",
        ),
        "axis" => Some(
            "Names the tenant axis for a `tenant_migration`; doctor checks it against feature `defaults.tenancy`.",
        ),
        "strategy" => Some(
            "Migration deployment strategy. Closed catalog: `rolling` (zero-downtime), `blue_green` (parallel cutover), `canary` (incremental traffic shift). Doctor: `DEPLOY-STRATEGY-001`.",
        ),
        "lock_timeout" => Some(
            "Max time to wait for the migration advisory lock before aborting. Adapter-parsed duration literal (`\"30s\"`, `\"5m\"`).",
        ),
        "pre_migration_hook" => Some(
            "Shell script the runtime executes before applying migrations. Path is relative to `app.lzi`.",
        ),
        "post_migration_hook" => Some(
            "Shell script the runtime executes after applying migrations. Path is relative to `app.lzi`.",
        ),
        "checkpoint" => Some(
            "Pinned IR snapshot for migration planning. `checkpoint <name> \"<path>\"` records a baseline; `lazuli plan --check <name>` validates the snapshot's integrity. Doctor: `DEPLOY-CHECKPOINT-001` (path missing) + `DEPLOY-CHECKPOINT-002` (version drift).",
        ),
        // OpenAPI bucket cycle — `deprecated` decorator + sub-fields.
        "deprecated" => Some(
            "Marks a command, api, or outbound `webhook_event` schema as deprecated. Commands/apis use inline metadata: `deprecated [since \"<version>\"] [replacement <ref>] [sunset \"<YYYY-MM-DD>\"]` or the block form with `since`/`replacement`/`sunset` children. `webhook_event` uses `deprecated <bool>` plus its version trail. Doctor: `deprecated-replacement-unknown`, `deprecated-sunset-past`, `deprecated-no-replacement`.",
        ),
        "since" => Some(
            "Version string when the deprecation was declared. Free-form (semver, calendar, git-sha); emitted verbatim as OpenAPI `x-lazuli-deprecated-since`.",
        ),
        "replacement" => Some(
            "Replacement reference for a deprecated command or api. Resolves to `command.<name>`, `api.<name>`, `<feature>.command.<name>`, `<feature>.api.<name>`, or an `https://` URL.",
        ),
        "sunset" => Some(
            "ISO-8601 date (`YYYY-MM-DD`) when consumers must stop using this endpoint. Emitted as OpenAPI `x-lazuli-sunset` and HTTP `Sunset` header.",
        ),
        // i18n bucket cycle — locale / translation / locale_negotiate.
        "translation" => Some(
            "Feature-scoped translation block. Declares a catalog path (`./i18n/<feature>.<locale>.json`) and typed keys. Each key declares one variant per `app.locale.supported` tag, plus optional CLDR plural arms (`zero/one/two/few/many/other`).",
        ),
        "catalog" => Some(
            "Translation catalog path. Carries `<locale>` placeholder; the runtime resolves it per request, e.g. `./i18n/customer.pt-BR.json`. Format (JSON/YAML/ICU MessageFormat) is an adapter contract on the Lazuli runtime side.",
        ),
        "locale_negotiate" => Some(
            "Per-runtime-unit (or per-api) middleware that resolves the request locale into `ctx.locale`. Closed catalog: `source` ∈ {accept_language|query_param|cookie|user_profile|subdomain}, `strategy` ∈ {best_match|prefix_match|exact_match}, optional `fallback <tag>`.",
        ),
        "source" => Some(
            "Inside `locale_negotiate`: the request axis the runtime reads to determine the locale. Closed catalog: `accept_language`, `query_param`, `cookie`, `user_profile`, `subdomain`.",
        ),
        "supported" => Some(
            "List of BCP-47 tags `app` accepts. The negotiation middleware matches `Accept-Language` against this list; `app.locale.default` must appear here.",
        ),
        "plural" => Some(
            "CLDR plural arm. Closed catalog: `zero`, `one`, `two`, `few`, `many`, `other`. The actual rule for which arm fires is locale data from CLDR, not language-declared.",
        ),
        "trigger" => Some(
            "Declares the event or schedule that starts a job or notification. `trigger event <feature>.<event>` for reactors; `trigger schedule \"<cron>\"` for scheduled work.",
        ),
        // L0 #8 — `poller` vocabulary (docs/proposals/poller-vocab.md).
        "poller" => Some(
            "Declares an async resolution loop over a persistent cursor table. `poller <name>` is a feature-level kind parallel to `job` / `webhook` / `notification`. Closed children: `source <Resource>`, `cursor`, `retry`, `states`, `resolve via @fn.<name>`, `terminal_status_field`, `terminal_result_field`, `tick every <duration> batch <int>`, `tenant_from row.<axis>_id`, `idempotency by row.<field>, ...`, `audit`, `emits`, `retry_quirk` (closed catalog).",
        ),
        "cursor" => Some(
            "Inside `poller`: names the three closed cursor fields on `source`. Body is exactly `eligible_when <next_at>, <resolved_at>` + `attempts <field>`. The runtime selects rows where `next_at <= NOW() AND resolved_at IS NULL`.",
        ),
        "eligible_when" => Some(
            "Inside `poller cursor`: the two field names that gate row eligibility. Positional pair: `eligible_when <next_check_at>, <resolved_at>` — first is `DateTime required`, second is nullable `DateTime`.",
        ),
        "tick" => Some(
            "Inside `poller`: wall-clock cadence. `tick every <duration> [batch <int>]`. Defaults: `every 30s`, `batch 100`. Duration unit catalog is closed (`s`/`m`/`h`/`d`); doctor warns when `every < 5s` (POLLER-TICK-TOO-FAST-001).",
        ),
        "retry_quirk" => Some(
            "Inside `poller`: closed-catalog retry transformation. v0.1 catalog: `gender_flip_once`. Body: `when <predicate>`, `counter <field>`, `mutate <field> = <transform>`. No predicate sublanguage; for arbitrary mutation, drop to a `command` chained off `emits`.",
        ),
        "backoff" => Some(
            "On `retry`: closed-catalog backoff strategy. Catalog: `fixed`, `linear`, `exponential`. `linear` and `exponential` require `base <duration>`; `exponential` strongly recommends `cap <duration>` (POLLER-EXPONENTIAL-NO-CAP-001).",
        ),
        "resolve" => Some(
            "Inside `poller`: names the per-row handler. `resolve via @fn.<name>` — handler signature is derived from the poller's row + state + terminal types (`poller.ResolveResult[State, Terminal, Result]`). Doctor enforces the `@fn.<name>` is declared in feature `extensions` (POLLER-HANDLER-ORPHAN-001).",
        ),
        "retry" => Some(
            "Declares retry attempts and backoff strategy. For jobs: `retry <count> backoff <fixed|exponential>`. For pollers: `retry` block with `max_attempts <int>` + `backoff <strategy> [base <d>] [cap <d>]`. v0 catalog is closed.",
        ),
        "queue" => Some(
            "Declares an async queue lane for event-triggered jobs. Without `queue`, event jobs run inline as reactors; with `queue <lane>`, the runtime adapter dispatches via the queue (River, Asynq).",
        ),
        "tenant_from" => Some(
            "Pins an event/job/webhook/notification's tenant context from a payload path. `tenant_from payload.<axis>_id` — doctor cross-checks the axis against the feature's tenancy.",
        ),
        "fanout" => Some(
            "Declares per-tenant expansion for scheduled jobs. `fanout tenants <axis>` runs one execution per tenant per fire. Requires `idempotency by ...` to avoid double-execution on re-fires (warning `JOB-FANOUT-002`).",
        ),
        "external_calls" => Some(
            "Inspect projection of every `calls <slot>.<op>` inside a command/job body. Doctor uses it to enforce timeout, retry, and idempotency on each external call (`INT-CALL-*`, `JOB-TIMEOUT-001`). IR field: `Command.external_calls` / `Job.external_calls` carry typed `ExternalCallRef { slot, operation, args, span_ref }`.",
        ),
        "payload_group" => Some(
            "On a notification template binding, references a shared `event_group` payload schema. The runtime hydrates the template with the named group's payload shape.",
        ),
        "payload" => Some(
            "On an `event_group`, declares the shared event payload schema for every concrete event under the group. Field-binding lines (`customer_id = id`) compile into the group's typed payload.",
        ),
        "encryption" => Some(
            "App-level encryption key binding catalog. One `key @key.<scope>` child per `@cap.Encrypted` / `@cap.E2ee` scope used in the capsule. Closed catalog: `@key.app`, `@key.tenant`, `@key.user`, `@key.record`. Per `docs/proposals/encryption-vocab.md`.",
        ),
        "rotation" => Some(
            "In `auth.sessions`, enables refresh-token rotation: every refresh issues a new access token and refresh token, then revokes the old refresh so replay becomes a theft signal through the parent_session_id chain. Framework default is absent/disabled for back-compat; production apps should declare `rotation`. In `encryption.key`, `rotation` remains the key rotation strategy; v0 catalog includes `manual`.",
        ),
        "rotation_profile" => Some(
            "Binds an `encryption.key @key.<scope>` to a `registry.secret_rotation <name>` profile. Cadence/overlap/auto_rollback live on the profile; the binding is a name reference. Doctor's `secret-rotation-binding-unknown` flags references to undeclared profiles.",
        ),
        // Roadmap §1.10 — `app.headers` keywords. Production-grade
        // HTTP security defaults lifted into the closed catalog.
        "headers" => Some(
            "Production-grade HTTP security headers (`app.headers`). Closed-catalog children: `csp`, `hsts`, `x_frame_options`, `x_content_type_options`, `referrer_policy`, `permissions_policy`. Doctor's `headers-contract` flags omissions under the `production` security profile.",
        ),
        "csp" => Some(
            "Content-Security-Policy header value (verbatim policy string). Authored as a quoted W3C CSP directive list, e.g. `csp \"default-src 'self'; script-src 'self' 'unsafe-inline'\"`. The runtime hands the value to the adapter unchanged.",
        ),
        "hsts" => Some(
            "Strict-Transport-Security sub-block on `app.headers`. Inline form `hsts max_age 31536000 include_subdomains preload`; equivalent six-space body. `max_age` (seconds) is required; `include_subdomains` and `preload` are boolean flags.",
        ),
        "include_subdomains" => Some(
            "Boolean flag on `app.headers hsts`. Sets the `includeSubDomains` directive on the Strict-Transport-Security header — opts every subdomain into the HSTS contract.",
        ),
        "preload" => Some(
            "Boolean flag on `app.headers hsts`. Sets the `preload` directive — required for inclusion in the HSTS preload list browsers ship with.",
        ),
        "x_frame_options" => Some(
            "X-Frame-Options header value on `app.headers`. Closed catalog: `DENY` (disallow framing), `SAMEORIGIN` (same-origin only), or `ALLOW-FROM <uri>` (legacy — prefer CSP `frame-ancestors`).",
        ),
        "x_content_type_options" => Some(
            "X-Content-Type-Options header value on `app.headers`. Closed catalog: `nosniff` is the only legal token per the WHATWG fetch spec.",
        ),
        "referrer_policy" => Some(
            "Referrer-Policy header value on `app.headers`. Closed catalog: `no-referrer`, `no-referrer-when-downgrade`, `origin`, `origin-when-cross-origin`, `same-origin`, `strict-origin`, `strict-origin-when-cross-origin`, `unsafe-url`.",
        ),
        "permissions_policy" => Some(
            "Permissions-Policy header value (verbatim policy string). Authored as a quoted W3C Permissions Policy directive list, e.g. `permissions_policy \"geolocation=(), camera=()\"`. Runtime hands the value to the adapter unchanged.",
        ),
        // Roadmap §1.10 — `registry.secret_rotation` profile kind.
        "secret_rotation" => Some(
            "Declares a secret-rotation policy profile in `registry.lzi`. Body: `cadence <duration>` (how often the secret rolls), `overlap <duration>` (grace window during which both old + new are accepted), `auto_rollback <bool>`. Bind a profile via `app.encryption.key @key.<scope> rotation_profile <name>`.",
        ),
        "cadence" => Some(
            "On `secret_rotation <name>`, declares how often the secret rolls. Closed unit catalog: `s`, `m`, `h`, `d`. Doctor's `secret-rotation-overlap-contract` enforces `overlap < cadence`.",
        ),
        "overlap" => Some(
            "On `secret_rotation <name>`, declares the grace window during which the previous secret is still accepted alongside the new one. Closed unit catalog: `s`, `m`, `h`, `d`. Must be strictly shorter than `cadence`.",
        ),
        "auto_rollback" => Some(
            "On `secret_rotation <name>`, a boolean flag. When `true`, the runtime reverts to the previous secret if a fresh rollover fails its smoke check.",
        ),
        // Roadmap §1.2 — HTTP hygiene blocks at the app level. Each
        // hover mirrors the doctor diagnostic catalog so the LLM /
        // human reading the source sees the contract without docs.
        "cookie" => Some(
            "App cookie contract (`app.cookie`). Named profiles (`default`, `session`, `csrf`, ...) declare per-cookie hygiene. Closed catalogs: `same_site ∈ {lax, strict, none}`. Optional bools: `signed`, `secure`, `http_only`. Duration: `max_age \"7d\"`. Default profile applies unless overridden.",
        ),
        "proxy" => Some(
            "App proxy contract (`app.proxy`). Declares trusted upstreams + real-IP header overrides. `trusted` accepts a comma-separated CIDR list (`10.0.0.0/8, 172.16.0.0/12`). Optional headers: `real_ip_header`, `forwarded_proto_header`, `forwarded_host_header`. The runtime trusts these headers only from the CIDRs in `trusted`.",
        ),
        // Note: `limits` and `timeout` reuse existing hovers (plan
        // limits / service-boundary timeout); doctor's
        // `app_limits_contract_diagnostics` disambiguates by structure.
        "signed" => Some(
            "On a cookie profile: append HMAC tag so the runtime detects tampering. Boolean. Default `false`.",
        ),
        "secure" => Some(
            "On a cookie profile: mark the cookie as TLS-only (`Secure` attribute). Boolean. Default `false`; flip to `true` for production. Required when `same_site none`.",
        ),
        "http_only" => Some(
            "On a cookie profile: hide from JavaScript (`document.cookie`). Boolean. Default `false`; flip to `true` for session cookies to mitigate XSS leaks.",
        ),
        "same_site" => Some(
            "On a cookie profile: CSRF policy. Closed catalog: `lax` (default), `strict` (never cross-site), `none` (cross-site OK; requires `secure true` per RFC 6265bis).",
        ),
        "max_age" => Some(
            "On a cookie profile: cookie lifetime. Duration literal (`\"7d\"`, `\"12h\"`, `\"30m\"`, `\"45s\"`). Doctor rejects unparseable values. Absent means session cookie (expires when browser closes).",
        ),
        "trusted" => Some(
            "On `app.proxy`: comma-separated CIDR list of trusted upstream proxies. Real-IP and forwarded headers are honored only when the immediate peer is in this list. Doctor validates each entry as a parseable CIDR.",
        ),
        "real_ip_header" => Some(
            "On `app.proxy`: header carrying the real client IP. Common values: `X-Forwarded-For`, `X-Real-IP`, `True-Client-IP`. Trusted only when peer is in `trusted` CIDRs.",
        ),
        "forwarded_proto_header" => Some(
            "On `app.proxy`: header carrying the originating protocol (`http` / `https`). Common value: `X-Forwarded-Proto`. Used to detect TLS-terminated origin requests behind L7 proxies.",
        ),
        "forwarded_host_header" => Some(
            "On `app.proxy`: header carrying the originating host. Common value: `X-Forwarded-Host`. The runtime substitutes the request's `Host` with this value when peer is trusted.",
        ),
        "body_size" => Some(
            "On `app.limits`: max in-memory request body size. Size literal (`\"512b\"`, `\"16kb\"`, `\"10mb\"`). Larger bodies are rejected with `413 Payload Too Large`.",
        ),
        "header_size" => Some(
            "On `app.limits`: max combined header size. Size literal (`\"4kb\"`, `\"16kb\"`). Larger header sets are rejected with `431 Request Header Fields Too Large`.",
        ),
        "upload_size" => Some(
            "On `app.limits`: max multipart upload size (streamed). Size literal (`\"100mb\"`, `\"2gb\"`). Distinct from `body_size`; uploads stream to disk after the in-memory ceiling.",
        ),
        "reason" => Some("Documents why a dangerous declarative override is intentional."),
        "requires" => Some(
            "Declares a feature requirement or an additional authority requirement for a workflow transition.",
        ),
        "integration" => {
            Some("Declares an abstract external integration requirement or registry capability.")
        }
        "password" => Some("Password subcontract: hash + verify + algorithm (+ rate_limit)."),
        "hash" => Some(
            "Hashing function reference (`@fn.*`) returning a `@cap.Hashed(algorithm:<X>)` value.",
        ),
        "algorithm" => Some(
            "Password hash algorithm. v0: `argon2id` (recommended) | `bcrypt` (legacy migration).",
        ),
        "secret" => Some("Declares the secret source for declarative webhook verification."),
        "header" => Some("Declares the signature header for declarative webhook verification."),
        "modifier" => Some("Attaches a query modifier extension to a generated query."),
        "emits" => Some("Declares a domain event emitted by a command."),
        "anchor" => Some("Declares the extension anchor for a routed abstract view."),
        "extensible_by" => Some("Whitelists features allowed to extend a view anchor."),
        "tests" => Some(
            "Declares inline IR assertions for a command, transition, rule, or view extension.",
        ),
        "permits" => Some(
            "Generated command authorization assertion; authored command policy matrices are redundant with `policy @policy.*`.",
        ),
        "forbids" => Some(
            "Generated command authorization assertion; authored command policy matrices are redundant with `policy @policy.*`.",
        ),
        "allows" => Some("Declares a positive predicate or transition test assertion."),
        "deny" => Some("Declares a rule precondition that rejects an operation."),
        "denies" => Some("Declares a negative predicate or transition test assertion."),
        "accepted" => {
            Some("Declares that a view extension should be accepted by an anchor whitelist.")
        }
        "rejected" => {
            Some("Declares that a view extension should be rejected by an anchor whitelist.")
        }
        "search" => Some("Lists fields used by a query search index."),
        "filter" => Some("Lists fields available as query filters."),
        "list" => Some("Declares table/list fields for a surface."),
        "form" => Some("Declares editable form fields for a surface."),
        "detail" => Some("Declares read-only detail fields for a surface."),
        "columns" => Some("Introduces list columns."),
        "fields" => Some("Introduces form or detail fields."),
        "validate" => Some(
            "Runs a blocking command validator with `validate @validator.*`; legacy whole-resource validators should use `validates resource`.",
        ),
        "validates" => Some(
            "Attaches a scoped validator implementation: `validates resource` or `validates field <name>`.",
        ),
        "client" => Some("Declares a reusable client-side extension contract."),
        "fn" => Some("Declares a reusable server-side pure function extension contract."),
        "hook" => Some("Declares a reusable lifecycle hook extension contract."),
        "validator" => Some("Declares a reusable validator extension contract."),
        "adapter" => Some(
            "Adapter slot: `@runtime/...`, `@lazuli/plugin-publisher/name`, `@adapter.<local>`, or a local path. Inside `auth`, resolved against `extensions adapter <name>` or `registry.integrations`.",
        ),
        "query_modifier" => Some("Declares a reusable query modifier extension contract."),
        "escape_route" => Some("Declares a custom route outside generated UI ownership."),
        "group" => Some("Groups related app env declarations without creating a namespace."),
        "required" => Some("Marks a field as required."),
        "unique" => Some("Marks a field as unique."),
        "default" => Some("Declares a default field value."),
        // Row 30 — `@cap.File(...)` argument keywords. The wider
        // `@cap.File` decorator itself surfaces via the word-at-position
        // hover lookup; the parser swallows the `@` so the word is
        // typically `cap.File` — both are listed here for symmetry.
        "@cap.File" | "cap.File" => Some(
            "File capability: `max_size:<size>` + `accept:<mime>` (required) + `visibility:<mode>` (required on api outputs) + `signed_ttl:<duration>` (required when `visibility:signed`). Authored on resource fields and api outputs. Requires the package to declare an `object_storage` or `storage` capability.",
        ),
        "max_size" => Some(
            "Maximum upload size for a `@cap.File`. Closed unit catalog: `kb`, `mb`, `gb` (binary prefixes — `25mb` = 25 * 1024 * 1024 bytes).",
        ),
        "accept" => Some(
            "Accepted MIME types for a `@cap.File`; pipe-separated for alternatives, e.g. `text/csv|application/vnd.ms-excel`. Known families: `text`, `image`, `application`, `audio`, `video`, `font`, `*`. Subtype `*` is also valid.",
        ),
        "visibility" => Some(
            "Visibility of the file URL produced by `@cap.File`. Closed catalog: `public` (unguessable but un-gated, suits CDN-served static assets), `private` (policy-gated download handler), `signed` (time-limited signed URL — requires `signed_ttl`).",
        ),
        "signed_ttl" => Some(
            "Signed-URL TTL for `@cap.File(visibility:signed)`. Closed unit catalog: `s`, `m`, `h`, `d`. Forbidden when `visibility` is `public` or `private`.",
        ),
        // Report vocab — `report <name>` kind keywords. See
        // `docs/proposals/report-vocab.md` v0.2.
        "report" => Some(
            "Declares a tabular export contract (CSV / XLSX) on a feature. Replaces the `api + opaque handler` pattern for static-column exports. Body: `source <query_ref>`, `columns`, `formats csv|xlsx`, optional `storage`, `visibility`, `signed_ttl`, `filename`, `policy`, `rate_limit`, `audit`.",
        ),
        "columns" => Some(
            "On a `report`, declares the column list at compile time. Each row: `<name> from row.<field> | @fn.<name>(args) [label \"...\"] [format \"...\"]`. Doctor cross-checks `row.<field>` against the source query's projection via `REPORT-COLUMN-MISMATCH-001`.",
        ),
        "formats" => Some(
            "On a `report`, declares the export formats. Closed catalog: `csv`, `xlsx`. Each entry auto-mounts `GET /api/reports/<name>.<format>`. Unknown formats raise `REPORT-FORMAT-UNKNOWN-001`.",
        ),
        "filename" => Some(
            "On a `report`, declares the download filename template. Closed token catalog: `{format}`, `{ctx.now:<strftime>}` (strftime tokens `yyyy`, `mm`, `dd`, `HH`, `MM`, `ss`), `{ctx.user.id}`, `{ctx.tenant.id}`. Unknown tokens raise `REPORT-FILENAME-TOKEN-UNKNOWN-001`.",
        ),
        // Observability bucket cycle row 36 — `app.logging` /
        // `app.tracing` keywords. Each closed catalog matches the
        // doctor diagnostic.
        "logging" => Some(
            "App logging contract (`app.logging`). Closed catalogs: `level ∈ {debug, info, warn, error}`, `format ∈ {json, text}`, `redact ∈ {pii, none}`. Optional `sample_rate ∈ [0.0, 1.0]`. Profile-aware overrides.",
        ),
        "tracing" => Some(
            "App tracing contract (`app.tracing`). `propagate <bool>` toggles trace-context propagation. `sample_rate ∈ [0.0, 1.0]` for head sampling. `exporter <name>` resolves to a `registry.capabilities <name>: tracing` entry; runtime picks default when absent.",
        ),
        "level" => Some(
            "Severity level. Closed catalog: `debug`, `info`, `warn`, `error`. Shared by `app.logging.level` and `event.trace <name> level`.",
        ),
        "format" => Some(
            "Log encoding. Closed catalog: `json` (machine-parseable, production-friendly) or `text` (human-readable, dev-friendly).",
        ),
        "redact" => Some(
            "PII redaction policy. Closed catalog: `pii` (auto-strip fields tagged `@pii.*`) or `none` (no auto-redaction; adapter may still redact).",
        ),
        "sample_rate" => Some(
            "Sampling rate, float in `[0.0, 1.0]`. `1.0` captures everything; `0.0` disables capture (tracing still propagates context). Out-of-range values are rejected by doctor.",
        ),
        "propagate" => Some(
            "Trace-context propagation toggle. `true` (default) threads `trace_id` / `request_id` through downstream calls; `false` disables propagation but keeps span capture.",
        ),
        "exporter" => Some(
            "Tracing exporter slot. Must resolve to a `registry.capabilities <name>: tracing` entry. `None` lets the runtime pick a default (no-op or stdout).",
        ),
        // Observability bucket cycle row 37.
        "emit_to" => Some(
            "Audit destination. Resolves to one of the reserved streams (`audit_log`, `audit_stream`) or to an `event_group <name>` declared in the same feature. Without `emit_to`, the runtime falls back to `audit_log`.",
        ),
        // Webhooks expanded cycle — payload/replay/dlq hover catalog.
        "webhook_events" => Some(
            "Registry-side catalog of expected inbound envelope shapes. Each entry under `registry.webhook_events.<name>` is a typed external envelope referenced by `webhook ... payload from webhook_events.<name>`. Treated as external-origin: Lazuli does not assume the source is trustworthy, only that the contract matches what the provider documents.",
        ),
        "webhook_event" => Some(
            "Declares a canonical outbound webhook event schema emitted to consumers. Use `payload` for typed fields, `version <n>`, optional `previous_version <n>`, and `deprecated <bool>`. Distinct from inbound `webhook` blocks.",
        ),
        "previous_version" => Some(
            "Records the prior version in a versioned outbound `webhook_event` schema migration trail.",
        ),
        "payload_from" => Some(
            "Typed reference to a `registry.webhook_events.<name>` envelope. Surface form: `payload from webhook_events.<name>`. Doctor cross-checks the envelope name and validates `tenant_from payload.<axis>` / `idempotency by payload.<axis>` against the declared fields.",
        ),
        "replay" => Some(
            "Declarative replay contract on an inbound webhook. Short form: `replay allow within \"<duration>\"`. Long form: `replay` header + nested `allow|deny` + optional `within \"...\"` + optional `dedupe by <path>`. `dedupe_by` defaults to the webhook's `idempotency by ...` path.",
        ),
        "allow" => Some(
            "On `replay`: re-deliveries within the window are accepted; the runtime returns 200 without re-running the handler. Requires `within \"<duration>\"`.",
        ),
        "within" => Some(
            "Replay window for `replay allow`. Quoted duration verbatim (e.g. `\"24h\"`, `\"7d\"`). The adapter parses; the language keeps the literal.",
        ),
        "dedupe" => Some(
            "On `replay`: `dedupe by <path>` overrides the dedupe key used to detect re-deliveries. Without `dedupe by`, replay reuses the webhook's `idempotency by ...` path.",
        ),
        "dlq" => Some(
            "Dead-letter routing after retry exhaustion. Three closed variants (mutually exclusive): `dlq emit <event>` publishes a tombstone event; `dlq handler \"./...\"` runs an adapter-side handler; `dlq drop` + `reason \"...\"` is an explicit waiver.",
        ),
        "emit" => Some(
            "On `dlq`: `dlq emit <event>` publishes a tombstone event onto the bus after retry exhaustion. The event must be declared in the same feature (via `emits`, `event_group`, or `event.trace`).",
        ),
        "drop" => Some(
            "On `dlq`: `dlq drop` discards re-delivery attempts after retry exhaustion. Must carry an explicit `reason \"...\"` waiver — silent drops on dead-letter are rejected by `WEBHOOK-DLQ-002`.",
        ),
        "from" => Some(
            "Catalog hop. In `payload from webhook_events.<name>`, points at the registry-side envelope shape. The `webhook_events.` prefix is mandatory at the surface so the catalog is obvious to a cold-reading author.",
        ),
        // RBAC catalog vocab — `permission` / `role` top-level kinds +
        // `inherits` / `grants` / `grants_all` children and the
        // `has_role` / `has_permission` policy predicates. See
        // `docs/proposals/rbac-catalog-vocab.md`.
        "permission" => Some(
            "Declares one closed-catalog permission at top level. Identifier is colon-separated, 2-4 segments (`<resource>:<action>` ... `<resource>:<action>:<scope>:<qualifier>`). Catalog is package-scoped; placement convention is `features/auth/auth.lzi`.",
        ),
        "role" => Some(
            "Declares one closed-catalog role at top level. Body accepts optional `inherits <role>` (single-parent) and exactly one of `grants` (indented list of permission refs) or `grants_all` (shorthand for every declared permission), or neither (inherits-only).",
        ),
        "inherits" => Some(
            "On `role`: single-parent inheritance (`inherits <role>`). Multi-parent (`inherits A, B`) is rejected in v0.1 — declare a chain instead. Closure is computed at compile time.",
        ),
        "grants" => Some(
            "On `role`: block listing the permissions granted by this role (one per line, indent 4). Each entry is a bare permission identifier resolved against the catalog. Mutually exclusive with `grants_all`.",
        ),
        "grants_all" => Some(
            "On `role`: shorthand granting every declared permission in the catalog. Mutually exclusive with `grants`. Newly added permissions are automatically included (useful for `admin`-style roles; LSP hover surfaces the resolved closure).",
        ),
        "has_role" => Some(
            "Closed predicate inside a `policy` expression: `has_role <name>` evaluates to true when the actor's current role is `<name>` or transitively inherits from it. Use `@role.<name>` inside `policies` dictionary entries instead.",
        ),
        "has_permission" => Some(
            "Closed predicate inside a `policy` expression: `has_permission <resource>:<action>` evaluates to true when the actor's current role grants the permission via the catalog closure. Reference must resolve against a declared `permission`.",
        ),
        // IR Error-Vocab — see `docs/proposals/ir-error-messages-vocab.md`
        // §7. `when_denied` attaches to a `command.policy` line (per-command
        // override) or under a `policies.<category>:` line (per-policy
        // default); `message_key` is the new opt-in wire-envelope field.
        // The 8 closed-catalog error codes get one-liners here too so they
        // surface in completion lists; the **resolved-text** hover is in
        // `rich_keyword_hover`.
        "when_denied" => Some(
            "Per-command or per-policy override for the `policy_denied` error message. References `@translation.<key>` in the surrounding feature's `translation` block. Highest-precedence layer in the resolution chain (proposal §2.E).",
        ),
        "message_key" => Some(
            "Exposable 4xx wire-envelope field carrying the resolved `@translation.<key>` token. Lets clients with offline catalogs (mobile apps, kiosks) localize independently. Opt-in via `expose client 4xx message_key`.",
        ),
        "policy_denied" => crate::error_vocab_code_detail("policy_denied"),
        "validation_failed" => crate::error_vocab_code_detail("validation_failed"),
        "tenant_mismatch" => crate::error_vocab_code_detail("tenant_mismatch"),
        "not_found" => crate::error_vocab_code_detail("not_found"),
        "rate_limited" => crate::error_vocab_code_detail("rate_limited"),
        "bad_request" => crate::error_vocab_code_detail("bad_request"),
        "method_not_allowed" => crate::error_vocab_code_detail("method_not_allowed"),
        "integration_error" => crate::error_vocab_code_detail("integration_error"),
        "unique_violation" => crate::error_vocab_code_detail("unique_violation"),
        "foreign_key_violation" => crate::error_vocab_code_detail("foreign_key_violation"),
        "not_null_violation" => crate::error_vocab_code_detail("not_null_violation"),
        "check_violation" => crate::error_vocab_code_detail("check_violation"),
        // `docs/proposals/ir-resource-conventions-crud.md` §4.4 + the
        // `docs/proposals/ir-resource-conventions-me.md` §4.4 — the
        // resource-level `conventions [<name>, ...]` opt-in for closed-
        // catalog convention bundles. One-liner fallback; rich hover
        // lives in `rich_keyword_hover`.
        "conventions" => Some(
            "Resource-level conventions opt-in: `conventions [<name1>, <name2>, ...]`. Each entry references a closed-catalog convention bundle that auto-synthesizes commands/queries during lowering. Today's catalog: `crud`, `me`. See `docs/proposals/ir-resource-conventions-crud.md` and `docs/proposals/ir-resource-conventions-me.md`.",
        ),
        // `docs/proposals/ir-resource-conventions-owner-scope.md` §7.5 +
        // §11.3 — `@owner_axis(through: <column>)` field-level annotation
        // marks an FK field as the ownership-chain anchor. The crud and
        // me synth passes consume it to emit owner-restricted WHERE
        // clauses. Both `@owner_axis` and `owner_axis` arms are listed
        // because the parser's word-at-position scan swallows the `@`
        // (mirrors the `@cap.File` / `cap.File` precedent above).
        "@owner_axis" | "owner_axis" => Some(
            "Field-level annotation: `@owner_axis(through: <column>)`. Marks the field as the FK that anchors the resource's ownership chain. The crud / me synth passes use this to emit ownership-restricted WHERE clauses (the row's owner is resolved through the chain to `ctx.User.ID` rather than just the tenant). See `docs/proposals/ir-resource-conventions-owner-scope.md` §7.",
        ),
        _ => None,
    }
}
/// Rich Markdown hover for the closed-catalog DSL kinds the LSP knows
/// best. Each entry renders a one-line summary, required-children
/// bullets, optional-children bullets, a worked example, and a doc
/// anchor link. Markdown intentionally uses only the conservative
/// subset (headings via `**bold**`, bullet lists, fenced code blocks,
/// inline `[label](path)` links) so VS Code and Helix both render it
/// the same way; we don't use VS Code-only renderer features.
///
/// Falls back to `keyword_description` (one-liner) when no rich
/// template exists, so adding a kind here is strictly additive and
/// cannot regress unrelated hover output.
///
/// The seven canonical kinds covered today: `command`, `query.list`,
/// `query.lookup`, `query.sql`, `api`, `policy`, `effect`, `audit`,
/// `rate_limit`. `agent` keeps its existing one-line description plus
/// the enriched markdown here so the canonical hover pattern from the
/// agent cycle remains the reference shape.
pub fn rich_keyword_hover(keyword: &str) -> Option<String> {
    match keyword {
        "command" => Some(
            [
                "**`command`** — write operation on an aggregate. Lazuli owns the contract; the runtime emits a typed handler that runs effects, emits events, and invalidates queries.",
                "",
                "**Required children**",
                "- `policy @policy.<name>` — feature-local authorization category.",
                "- An effect line — exactly one of `creates`/`updates`/`deletes`, or a non-mutating `returns <Record>` shape.",
                "",
                "**Optional children**",
                "- `input` / short-form `input name, email` — submitted fields.",
                "- `route <name>: <Type>` — URL/context slots.",
                "- `rate_limit \"<N> per <window> per <axis>\"` — required when the policy includes `@scope.public` or the command mutates state.",
                "- `audit` / `audit <field>+` / `audit none` — audit-log contract.",
                "- `emits <event> [from creates|updates|deletes]` — domain event publication.",
                "- `invalidates query.<name>` — cache fan-out.",
                "- `approval` — conditional human sign-off block.",
                "- `validate @validator.<name>` — blocking validator.",
                "",
                "**Example**",
                "```lazuli",
                "command create",
                "  input",
                "    name: Text required",
                "  policy @policy.create",
                "  rate_limit \"30 per hour per ip\"",
                "  creates Customer",
                "    name = input.name",
                "  emits customer_created from creates",
                "```",
                "",
                "See [quickref.md §Minimal Feature](docs/quickref.md) and [invariants.md §Security And Crypto](docs/invariants.md).",
                "",
                "**Inspect**: `lazuli inspect <file> --expand=commands` projects this declaration's typed IR slice (route + input + policy + audit + approval + invalidates + external_calls + rate_limit + timeout/retry/idempotency).",
            ]
            .join("\n"),
        ),
        "query.list" => Some(
            [
                "**`query.list`** — generated collection query. Defaults to `order created_at desc`; simple equality filters derive language-managed indexes.",
                "",
                "**Required children**",
                "- None at the syntax level — a bare `query.list <name>` is valid.",
                "",
                "**Optional children**",
                "- `params` — typed read arguments.",
                "- `filters` — equality / `when params.*` filter rows; derives indexes.",
                "- `search params.<name> over <field>...` with `mode contains|prefix|exact` — text matching (does not derive indexes).",
                "- `order <field> asc|desc` — override the `created_at desc` default.",
                "- `paginate <positive-int>` — generated default page size, not a hard maximum.",
                "- `cache key <expr> ttl <duration>` (+ optional `tags`, `namespace`).",
                "- `scope override` (+ `reason`) — cross-tenant / admin queries.",
                "- `policy @policy.<name>` — explicit category (required under `scope override`).",
                "- `modifier @query_modifier.<name>` — query-modifier extension.",
                "",
                "**Example**",
                "```lazuli",
                "query.list list",
                "  params",
                "    status: CustomerStatus optional",
                "  filters",
                "    status when params.status",
                "  paginate 50",
                "```",
                "",
                "See [quickref.md §Queries](docs/quickref.md) and [invariants.md §Queries And Relations](docs/invariants.md).",
                "",
                "**Inspect**: `lazuli inspect <file> --expand=queries` projects every lifted query (List / Lookup / Sql) with its full v0 child coverage.",
            ]
            .join("\n"),
        ),
        "query.lookup" => Some(
            [
                "**`query.lookup`** — generated single-record query. Single-key form sugars to `query.lookup <name> by <field>: <Type>`; composite/reshaped lookups use a `params`/`key` body.",
                "",
                "**Required slots**",
                "- A key spec — either `by <field>: <Type>` (single-key sugar) or a `params`/`key` body for composite lookups.",
                "",
                "**Optional children**",
                "- `params` — composite-key arguments (when `by` shorthand is not used).",
                "- `key` — explicit key composition for composite lookups.",
                "- `policy @policy.<name>` — explicit category.",
                "- `cache key <expr> ttl <duration>`.",
                "- `scope override` (+ `reason`) — cross-tenant lookups.",
                "",
                "**Example**",
                "```lazuli",
                "query.lookup by_id by id: ID",
                "",
                "query.lookup by_email by email: @semantic.Email",
                "```",
                "",
                "See [quickref.md §Queries](docs/quickref.md) and [invariants.md §Queries And Relations](docs/invariants.md).",
                "",
                "**Inspect**: `lazuli inspect <file> --expand=queries` includes Lookup queries alongside List/Sql.",
            ]
            .join("\n"),
        ),
        "query.sql" => Some(
            [
                "**`query.sql`** — SQL-backed query wrapper. The result type must resolve to a `record`, resource, or registered contract before codegen; Lazuli does not infer result shape from SQL text.",
                "",
                "**Required children**",
                "- `returns <Type>` or `returns <Type>[]` — must resolve to a `record`, resource, or contract.",
                "- `sql \"./queries/<name>.sql\"` — relative path to the SQL file.",
                "",
                "**Optional children**",
                "- `params` — typed query arguments referenced inside the SQL file.",
                "- `scope` — tenancy or filter scope applied at codegen.",
                "- `policy @policy.<name>` — explicit category.",
                "- `cache key <expr> ttl <duration>`.",
                "",
                "**Example**",
                "```lazuli",
                "query.sql lifetime_value",
                "  returns CustomerLtv[]",
                "  scope",
                "    org = ctx.user.org",
                "  sql \"./queries/customer_lifetime_value.sql\"",
                "```",
                "",
                "See [quickref.md §Queries](docs/quickref.md) and [invariants.md §Queries And Relations](docs/invariants.md).",
                "",
                "**Inspect**: `lazuli inspect <file> --expand=queries` includes Sql queries alongside List/Lookup.",
            ]
            .join("\n"),
        ),
        "query.view" => Some(
            [
                "**`query.view`** — typed SQL-backed screen-read projection. Use it for denormalized reads whose row shape is declared as a local `record`.",
                "",
                "**Required children**",
                "- `returns list of <Record>` or `returns <Record>` — typed row shape for the generated SDK.",
                "- `source @file.<name>.sql` — SQL file under the feature `queries/` directory.",
                "",
                "**Optional children**",
                "- `policy @policy.<name>` — explicit category.",
                "- `params` — typed query arguments bound to SQL placeholders in source order.",
                "- `scope` — tenancy or filter scope applied at codegen.",
                "",
                "**Example**",
                "```lazuli",
                "query.view host_home_view",
                "  policy @policy.host_only",
                "  returns list of HostHomeRow",
                "  source @file.host_home_view.sql",
                "  params",
                "    user_id: ID required",
                "```",
                "",
                "See [quickref.md §Queries](docs/quickref.md) and [invariants.md §Queries And Relations](docs/invariants.md).",
            ]
            .join("\n"),
        ),
        "api" => Some(
            [
                "**`api`** — custom typed HTTP endpoint outside `command`/`query`/`webhook` semantics. Use it when the handler does meaningful work beyond translating HTTP to a single dispatch; otherwise prefer `expose http` on an `agent` or a generated command/query.",
                "",
                "**Required children**",
                "- `method <GET|POST|PUT|PATCH|DELETE>` — HTTP verb.",
                "- `path \"<url>\"` — concrete URL path; `:slot` placeholders bind via `route`.",
                "- `output <Type>` — response shape (record, resource, or `@cap.File(...)`).",
                "- `policy @policy.<name>` — authorization category.",
                "- `handler @fn.<name>` or `handler \"./path.go\"` — handler reference.",
                "",
                "**Optional children**",
                "- `input <Type>` — request body shape.",
                "- `route <name>: <Type>` — one per `:slot` placeholder.",
                "- `rate_limit \"<N> per <window> per <axis>\"` — per-call throttle (required when policy includes `@scope.public`).",
                "- `audit` / `audit <field>+` / `audit none`.",
                "",
                "**Example**",
                "```lazuli",
                "api me",
                "  method GET",
                "  path \"/me\"",
                "  output User",
                "  policy @policy.authenticated",
                "  handler @fn.me",
                "```",
                "",
                "See [quickref.md §Security Checklist](docs/quickref.md) and [invariants.md §Security And Crypto](docs/invariants.md).",
                "",
                "**Inspect**: `lazuli inspect <file> --expand=apis` (or `--expand=api`) projects every lifted Api with method + path + output + policy + handler + locale_negotiate.",
            ]
            .join("\n"),
        ),
        "policy" => Some(
            [
                "**`policy`** — feature-local authorization category reference on a `command`/`query`/`api`/`webhook`/`job`. The category resolves against the same feature's `policies` block unless feature-qualified.",
                "",
                "**Forms**",
                "- `policy @policy.<name>` — single category from the feature `policies` dictionary.",
                "- `policy @policy.<feature>.<name>` — cross-feature category (rarely needed).",
                "- On `policies` entry lines (atom decomposition): `<category>: <atom>[, <atom>]+` where each atom is `@role.*`, `@scope.*`, or `@actor.*`.",
                "- Predicate combinators inside categories: comma = OR, `and` = AND, parentheses for grouping (canonical closed predicate language).",
                "",
                "**Rules**",
                "- Commands declare `policy` explicitly — there is no implicit `creates -> @policy.create`.",
                "- Direct atoms (`@role.*`, `@scope.*`, `@actor.*`) belong in `policies` entries, not on individual command lines. Jobs / webhooks / escape routes may use atoms directly where appropriate.",
                "",
                "**Example**",
                "```lazuli",
                "policies",
                "  create: @role.admin, @role.sales",
                "  read: @scope.same_org",
                "",
                "command reassign",
                "  policy @policy.update",
                "```",
                "",
                "See [quickref.md §Policy Vocabulary](docs/quickref.md) and [invariants.md §Policies](docs/invariants.md).",
            ]
            .join("\n"),
        ),
        "effect" => Some(
            [
                "**`effect`** — write effect on a `command`. The closed catalog is `creates` / `updates` / `deletes` / `returns`. Exactly one mutating effect per command; `returns` is non-mutating.",
                "",
                "**Closed catalog**",
                "- `creates <Resource>` — new row; body assigns input/derived values to fields.",
                "- `updates <Resource>` — mutates the loaded `target`; body assigns changed fields.",
                "- `deletes <Resource>` — removes the loaded `target`. Soft-delete is automatic when the resource declares `soft_delete`.",
                "- `returns <Record>` — non-mutating command (no row write); the handler returns a typed record.",
                "",
                "**Rules**",
                "- One mutating effect per command. Multi-effect commands are rejected.",
                "- `target` is loaded before `updates`/`deletes` (explicit `target query.by_id(...)` or sugar when route/lookup match).",
                "- Event derivation works with effects: `emits <event> from creates|updates|deletes` maps the effect's bindings into the event payload by name.",
                "",
                "**Example**",
                "```lazuli",
                "command create",
                "  policy @policy.create",
                "  creates Customer",
                "    name = input.name",
                "    email = input.email",
                "  emits customer_created from creates",
                "```",
                "",
                "See [quickref.md §Canonical Sugar Table](docs/quickref.md) and [invariants.md §Source And Derived Views](docs/invariants.md).",
            ]
            .join("\n"),
        ),
        "audit" => Some(
            [
                "**`audit`** — declares an operation as audited so generated audit-log codegen has a typed contract instead of relying on event-name conventions. Surfaces in `lazuli inspect --expand=security`.",
                "",
                "**Forms**",
                "- `audit` — emit the default audit fields (`actor`, `tenant`, `target.id`, `ctx.now`).",
                "- `audit <field>, <field>, ...` — explicit field list. Each entry resolves against the command's binding namespaces (`input.*`, `route.*`, `target.*`, `ctx.*`, `payload.*`, etc.).",
                "- `audit none` — opt out of audit-log generation. Doctor records the opt-out so security review can see it.",
                "",
                "**Optional child**",
                "- `emit_to <stream>` — direct audit emission to a specific stream (e.g. `audit_log`).",
                "",
                "**Rules**",
                "- Valid on `command`, `query.*`, `job`, `webhook`, and `report` (and `api` via `policy` linkage).",
                "- Audit declarations do not replace `emits`; events and audits are different contracts.",
                "",
                "**Example**",
                "```lazuli",
                "command reassign",
                "  policy @policy.update",
                "  audit actor, target.id, input.owner_id",
                "    emit_to audit_log",
                "  updates Customer",
                "    owner = resolved_owner",
                "```",
                "",
                "See [invariants.md §Source And Derived Views](docs/invariants.md) (audit fields paragraph) and [quickref.md §Security Checklist](docs/quickref.md).",
            ]
            .join("\n"),
        ),
        "rate_limit" => Some(
            [
                "**`rate_limit`** — per-call throttle on a `command`, `api`, `agent.expose http`, or `auth password`. Distinct from `notification.throttle` (which keys on recipient/channel axes).",
                "",
                "**Grammar**",
                "- `rate_limit \"<N> per <window> per <axis>\"`",
                "- `<N>` — positive integer.",
                "- `<window>` — duration string (`second`, `minute`, `hour`, `day`, or `<N> <unit>` like `\"5 10 minutes\"` for explicit count).",
                "- `<axis>` — closed catalog: `ip`, `user`, `org`, `tenant`.",
                "- `rate_limit none` (with `reason \"...\"`) — explicit opt-out; required when the strict security profile demands a decision.",
                "",
                "**When required**",
                "- Commands that mutate state.",
                "- Commands / APIs whose effective policy includes `@scope.public`.",
                "- `auth password` flows.",
                "",
                "**Example**",
                "```lazuli",
                "command create",
                "  policy @policy.create",
                "  rate_limit \"30 per hour per ip\"",
                "  creates Customer",
                "```",
                "",
                "See [quickref.md §Security Checklist](docs/quickref.md) and [invariants.md §Security And Crypto](docs/invariants.md).",
            ]
            .join("\n"),
        ),
        "error_page" => Some(
            [
                "**`error_page`** — declares a custom app-level HTTP error page once so every generated surface can share the same status response contract.",
                "",
                "**Header**",
                "- `error_page <status>` — closed catalog: `400`, `401`, `403`, `404`, `405`, `410`, `422`, `429`, `500`, `502`, `503`, `504`.",
                "",
                "**Required children**",
                "- `template \"./views/<status>.tmpl\"` — relative template path.",
                "",
                "**Optional children**",
                "- `audience <name>` — route audience that can see this page, commonly `public`.",
                "",
                "**Example**",
                "```lazuli",
                "app MyApp",
                "  error_page 404",
                "    template \"./views/404.tmpl\"",
                "    audience public",
                "```",
                "",
                "Doctor: `error-page-contract`, `error-page-template-missing`, `error-page-duplicate`.",
            ]
            .join("\n"),
        ),
        // IR Error-Vocab — see `docs/proposals/ir-error-messages-vocab.md`
        // §7.2. Rich-Markdown hovers for `errors` (feature-level block
        // keyword), `when_denied` (per-command + per-policy override),
        // `message_key` (wire-envelope opt-in field). The 8 closed-catalog
        // error codes get a separate **resolved-text** hover wired in the
        // dispatch above this function (see `hover` handler) because the
        // resolved text depends on the active document content, not just
        // the keyword name.
        "errors" => Some(
            [
                "**`errors`** — feature-level error contract. Combines (1) wire exposure rules and (2) typed per-code message overrides.",
                "",
                "**Children**",
                "- `default hide` | `default expose` — wire-envelope default for `message`/`data`/`message_key`. `hide` is the secure default.",
                "- `expose client 4xx <fields>` — comma-separated closed catalog: `message`, `code`, `data`, `message_key`.",
                "- `expose client 5xx <fields>` — comma-separated closed catalog: `code`, `data`. (`message` deliberately excluded — 5xx text is framework-internal.)",
                "- `<code> message @translation.<key>` — per-code typed override. Closed catalog of 12 codes: `policy_denied`, `validation_failed`, `tenant_mismatch`, `not_found`, `rate_limited`, `bad_request`, `method_not_allowed`, `integration_error`, `unique_violation`, `foreign_key_violation`, `not_null_violation`, `check_violation`.",
                "",
                "**Resolution chain** (proposal §2.E)",
                "1. `command.policy_when_denied` — per-command override.",
                "2. `policies.<category>.when_denied` — per-policy default.",
                "3. `feature.errors.<code> message` — per-feature catch-all (this block).",
                "4. Runtime built-in PT-BR / en-US catalog — framework floor.",
                "",
                "**Example**",
                "```lazuli",
                "feature account",
                "  errors",
                "    default hide",
                "    expose client 4xx message, code, message_key",
                "    expose client 5xx code",
                "    policy_denied      message @translation.account_signin_required",
                "    validation_failed  message @translation.account_invalid_input",
                "```",
                "",
                "Doctor: `ERR-VOCAB-001`/`002`/`003`, `ERR-VOCAB-CODE-UNKNOWN`, `ERR-VOCAB-EXPOSE-UNKNOWN`, `ERR-VOCAB-EXPOSE-5XX-MESSAGE`.",
            ]
            .join("\n"),
        ),
        "when_denied" => Some(
            [
                "**`when_denied`** — typed override for the `policy_denied` error message rendered to the wire. Two attachment sites:",
                "",
                "- Under a `command.policy` line — per-command override (resolution-chain step 1).",
                "- Under a `policies.<category>:` line — per-policy default (resolution-chain step 2).",
                "",
                "**Shape**",
                "- `when_denied @translation.<key>` — references a key declared in the surrounding feature's `translation` block.",
                "",
                "**Resolution chain** (proposal §2.E)",
                "1. `command.policy_when_denied` (this site, on a `command.policy` line).",
                "2. `policies.<category>.when_denied` (this site, on a `policies` entry).",
                "3. `feature.errors.policy_denied message`.",
                "4. Runtime built-in localized message.",
                "",
                "**Example**",
                "```lazuli",
                "feature account",
                "  policies",
                "    authenticated: @scope.authenticated",
                "      when_denied @translation.must_be_signed_in",
                "",
                "  command choose_role",
                "    policy @policy.authenticated",
                "      when_denied @translation.choose_role_signin_required",
                "```",
                "",
                "Doctor: `ERR-VOCAB-001` (warn, no override anywhere), `ERR-VOCAB-002` (error, key unresolved), `ERR-VOCAB-WHEN-DENIED-NO-POLICY` (error, attached to a command without a `policy`).",
            ]
            .join("\n"),
        ),
        "message_key" => Some(
            [
                "**`message_key`** — opt-in 4xx wire-envelope field exposing the resolved `@translation.<key>` token alongside `message`. Lets clients with offline catalogs (native mobile apps, kiosks) localize independently from the server-rendered string.",
                "",
                "**Shape**",
                "- Listed inside `expose client 4xx <fields>` in a feature's `errors` block.",
                "- Always namespaced on the wire: `<feature>.<key>` (e.g. `account.choose_role_signin_required`).",
                "",
                "**Example**",
                "```lazuli",
                "feature account",
                "  errors",
                "    expose client 4xx message, code, message_key",
                "    policy_denied message @translation.account_signin_required",
                "```",
                "",
                "Wire payload:",
                "```json",
                "{",
                "  \"code\": \"policy_denied\",",
                "  \"message\": \"Para escolher seu papel, entre na sua conta primeiro.\",",
                "  \"message_key\": \"account.account_signin_required\"",
                "}",
                "```",
                "",
                "Closed-catalog field — alternatives (`message`, `code`, `data`) live under the same parent. Doctor: `ERR-VOCAB-EXPOSE-UNKNOWN` rejects unknown fields.",
            ]
            .join("\n"),
        ),
        // `docs/proposals/ir-resource-conventions-crud.md` §4.4 + the
        // `docs/proposals/ir-resource-conventions-me.md` §4.4 — rich
        // hover for the resource-level `conventions [..]` opt-in. Body
        // begins with the verbatim one-liner from the proposal so the
        // hover surface, the docstring on `Resource.conventions`, and
        // the doctor diagnostic share phrasing. Cells C4 + M3.
        "conventions" => Some(
            [
                "**`conventions`** — Resource-level conventions opt-in: `conventions [<name1>, <name2>, ...]`. Each entry references a closed-catalog convention bundle that auto-synthesizes commands/queries during lowering. Today's catalog: `crud`, `me`. See `docs/proposals/ir-resource-conventions-crud.md` and `docs/proposals/ir-resource-conventions-me.md`.",
                "",
                "**Closed catalog**",
                "- `crud` — auto-synthesizes the 5 canonical CRUD shapes (`create_<r>`, `update_<r>`, `delete_<r>`, `lookup_<r>`, `list_<r>s`).",
                "- `me` — auto-synthesizes one `lookup_my_<r>` query keyed by the active actor (`ctx.User.ID` / `ctx.User.OrgID`), per `ir-resource-conventions-me.md` §5.",
                "",
                "**Example**",
                "```lazuli",
                "resource Customer",
                "  email: @semantic.Email required unique",
                "  name: Text required",
                "  conventions [crud, me]",
                "```",
                "",
                "**Authoring rules**",
                "- Empty list (`conventions []`) is a parse error — omit the slot instead.",
                "- An author-written `command <name>` overrides the synth for that name; the remaining synth entries still emit (per §6 RULE-VOCAB-02).",
                "- Unknown identifiers fail at parse time with doctor code `conventions_unknown`.",
                "",
                "**Inspect**: `lazuli inspect features` annotates each opted-in resource with `(conventions: <bundle>)` and each synthesized command/query with `[conv:<bundle>]`.",
            ]
            .join("\n"),
        ),
        // `docs/proposals/ir-resource-conventions-owner-scope.md` §7.5 +
        // §11.3 — rich Markdown hover for the `@owner_axis(through: <col>)`
        // FK annotation. Body opens with the verbatim one-liner from
        // §11.3 (also surfaced as the `keyword_description` fallback) so
        // the hover surface, the doctor diagnostic phrasing, and the
        // docstring on `Field.owner_axis` all agree. Cell O3.
        "@owner_axis" | "owner_axis" => Some(
            [
                "**`@owner_axis`** — Field-level annotation: `@owner_axis(through: <column>)`. Marks the field as the FK that anchors the resource's ownership chain. The crud / me synth passes use this to emit ownership-restricted WHERE clauses (the row's owner is resolved through the chain to `ctx.User.ID` rather than just the tenant). See `docs/proposals/ir-resource-conventions-owner-scope.md` §7.",
                "",
                "**Required parameter**",
                "- `through: <column>` — column on the FK target resource that holds the actor key. Typically `user` (the User-typed column on the target).",
                "",
                "**Example**",
                "```lazuli",
                "resource Property",
                "  org: Org required",
                "  host: Host required @owner_axis(through: user)",
                "  name: Text required",
                "  conventions [crud]",
                "```",
                "",
                "**Lowered SQL** (for `delete_property` under `conventions [crud]`):",
                "```sql",
                "DELETE FROM \"property\"",
                "WHERE id = $1",
                "  AND org_id = $2",
                "  AND host IN (SELECT id FROM \"host\" WHERE \"user\" = $3)",
                "```",
                "",
                "**Authoring rules**",
                "- Only valid on FK fields (the type must reference another resource). Doctor code `owner_axis_on_non_fk` fires otherwise.",
                "- The `through:` column must exist on the FK target resource. Doctor code `owner_axis_unknown_through` fires otherwise.",
                "- Redundant with `user: User required unique` on the same resource. Doctor code `owner_axis_collides_with_unique_user` warns when both are present.",
                "",
                "**Inspect**: `lazuli inspect features` adds `, owner-scope` to the resource's `(conventions: ...)` annotation and `, owner-scope` to each synth-origin command/query's `[conv:<bundle>]` tag.",
            ]
            .join("\n"),
        ),
        _ => None,
    }
}
