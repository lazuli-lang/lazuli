//! Domain-model + policy + command + i18n + secret-rotation
//! vocabulary one-liners.
//!
//! Covers ~125 keywords in the middle band of the original
//! `keyword_description`: `aggregate` / `root` / `contains` /
//! `invariant`, resource decorators (`@slug`, `tenancy`,
//! `timestamps`, `soft_delete`, `lock`, `composite_key`,
//! `@full_text`), query / surface verbs (`paginate`, `surface`,
//! `input`, `route`, `path`, `params`), command effect verbs
//! (`creates` / `updates` / `deletes` / `returns`), the Plan & Gate
//! family (`plan`, `features`, `limits`, `gate behind`,
//! `gate quota`), `has_many` / `inverse`, `policy` / `policy_for` /
//! `route_guard` defaults, `actor_query`, the cache contract
//! (`cache`, `key`, `ttl`, `tags`, `namespace`,
//! `stale_while_revalidate`, `coalesce`, `sliding`),
//! `invalidates` / `approval` / `error` / `expose` /
//! `write_window` / `idempotency`, `job` / `webhook` /
//! `tenant_migration` / `axis` / `strategy` / `lock_timeout` /
//! `pre_migration_hook` / `post_migration_hook` / `checkpoint`,
//! `deprecated` / `since` / `replacement` / `sunset`, the i18n
//! family (`translation`, `catalog`, `locale_negotiate`, `source`,
//! `supported`, `plural`, `trigger`), the `poller` family
//! (`poller`, `cursor`, `eligible_when`, `tick`, `retry_quirk`,
//! `backoff`, `resolve`, `retry`), `queue` / `tenant_from` /
//! `fanout` / `external_calls` / `payload_group` / `payload` /
//! `encryption` / `rotation` / `rotation_profile`, the
//! `app.headers` security family (`headers`, `csp`, `hsts`,
//! `include_subdomains`, `preload`, `x_frame_options`,
//! `x_content_type_options`, `referrer_policy`, `permissions_policy`),
//! and `secret_rotation` / `cadence` / `overlap` / `auto_rollback`,
//! plus `cookie` / `proxy` headers.

pub(crate) fn keyword_description(keyword: &str) -> Option<&'static str> {
    match keyword {
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
        // Surface-sync WT-2 — resource decorators + relations.
        "append_only" => Some(
            "Bare resource modifier (sibling of `soft_delete` / `timestamps`). Makes the resource insert-only — doctor `RESOURCE-APPEND-ONLY-001` rejects any `update`/`delete` command targeting it. The canonical sink for `audit materialize` (an immutable operation log).",
        ),
        "many_through" => Some(
            "Declares an M:N relation carrying payload: `many_through <Junction> to <Partner>` with at least one payload field at grandchild indent. Desugars into a synthesized junction resource holding the two endpoint FKs plus the payload columns. A payload-free junction is a plain `has_many` pair, not a `many_through` (GAP-07).",
        ),
        "polymorphic_ref" => Some(
            "Declares a polymorphic foreign key: `polymorphic_ref <type_field> <id_field> targets [A, B, ...]`. The `<type_field>` discriminates the row's referent among the bracketed PascalCase resource list; `<id_field>` carries the target row id (GAP-13).",
        ),
        "computed_date" => Some(
            "Read-time computed Date field: `<name>: Date computed_date from <base_field> offset <field|integer>`. The base is another field on the resource; the offset is a sibling Integer field or an integer-day literal. Not persisted — the clause is stripped from the column type (W3 GAP-03).",
        ),
        "schedule_rule" => Some(
            "Rule-driven computed Date field: `<name>: Date schedule_rule from @fn.<name>(<arg>) offset <field|integer>`. Like `computed_date` but the base is a `@fn.<name>(<arg>)` rule call rather than a bare field. The `@fn` base is mandatory; a bare-field base is a parse error (W4 GAP-08).",
        ),
        // Surface-sync WT-2 — command-side verbs + approval-chain vocab.
        "reorder" => Some(
            "Command mutating verb: `reorder <Resource> by <position_field>`. Batch position update — the runtime rewrites the position column of the target resource in one transaction. The `by <field>` clause is required (W4 GAP-REORDER-01).",
        ),
        "materialize" => Some(
            "Inside `audit`: `materialize @feature.<feature>.<Resource>` routes the audit record to an `append_only` resource (an immutable operation log) in the named feature. Cross-feature reachability and the `append_only` invariant are enforced by doctor `AUDIT-MATERIALIZE-TARGET-001` (GAP-AUDIT-01).",
        ),
        "chain" => Some(
            "Inside `approval`: declares a multi-approver chain, `chain [@role.a, @role.b, ...]` optionally followed by `sequential`. Mutually exclusive with the single-approver `by @role.<name>` form (which lifts to a one-element chain). The first chain element becomes `approval.by` (W4 GAP-06).",
        ),
        "sequential" => Some(
            "Inside `approval`: marks a `chain [...]` as ordered — approvers must sign off in declared order rather than in parallel. Without `sequential`, any-order approval is accepted (W4 GAP-06).",
        ),
        // Surface-sync WT-2 — iron-hand feature context vocabulary.
        "purpose" => Some(
            "Feature-context directive (at most once per feature): `purpose \"<one sentence>\"`. A single-sentence statement of intent for cold readers (humans and LLMs). Doctor `VOCAB-CONTEXT-PURPOSE-001` fires when missing or empty; escalated to error under the `tdd-iron-hand` coverage preset.",
        ),
        "non_goals" => Some(
            "Feature-context block (at most once per feature): boundary statements naming what the feature explicitly does NOT do. Simple form is a list of quoted strings; the partitioned form nests `delegated_to` / `out_of_scope`. Doctor `VOCAB-CONTEXT-NONGOALS-001` fires when empty.",
        ),
        "knowledge" => Some(
            "Feature-context directive (at most once per feature): `knowledge <sector>` names the bareword sector slug whose `knowledge/<sector>/` vault the feature draws authoring knowledge from. The planned `VOCAB-KNOWLEDGE-*` doctor lints cross-check the sector against its on-disk vault.",
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
        "requires_lifecycle_in" => Some(
            "Allow-list lifecycle route guard: `requires_lifecycle_in <Resource> [s1, s2]` paints the view only when the resolved lifecycle state is any of the listed states. Canonical, grep-friendly form; mutually exclusive with `requires_lifecycle <R> = <state>` on the same guard. Empty list = unreachable view (ROUTE-GUARD-LIFECYCLE-IN-EMPTY-002).",
        ),
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
            "`rate_limit \"N per UNIT per scope\" [in <env>, <env>...]`. Default rate limit applies in any env not matched by an `in`-qualified line. Closed env catalog: `production`, `staging`, `test`, `dev`, `local`. Multiple `rate_limit` lines per command are allowed when at most one is unqualified (the default).",
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
        // (`supported` is described by the `locale`-block arm above; this is the
        // same BCP-47 tag list, so no duplicate arm here.)
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
            "Cookie hygiene block. As `app.cookie`: named profiles (`default`, `session`, `csrf`, ...) declaring per-cookie hygiene (`signed`, `secure`, `http_only`, `same_site ∈ {lax, strict, none}`, `max_age \"7d\"`). As the `auth.sessions.cookie` child: the session-cookie transport envelope with six optional attributes (`name`, `same_site`, `secure`, `http_only`, `domain`, `path`) — each omitted attribute keeps the runtime default for that axis.",
        ),
        "proxy" => Some(
            "App proxy contract (`app.proxy`). Declares trusted upstreams + real-IP header overrides. `trusted` accepts a comma-separated CIDR list (`10.0.0.0/8, 172.16.0.0/12`). Optional headers: `real_ip_header`, `forwarded_proto_header`, `forwarded_host_header`. The runtime trusts these headers only from the CIDRs in `trusted`.",
        ),
        // Note: `limits` and `timeout` reuse existing hovers (plan
        // limits / service-boundary timeout); doctor's
        _ => None,
    }
}
