//! Surface / extension / cap / webhook / RBAC / error-vocab
//! vocabulary one-liners.
//!
//! Covers the tail ~125 keywords of the original
//! `keyword_description`: cookie / proxy / limits scalars
//! (`signed`, `secure`, `http_only`, `same_site`, `max_age`,
//! `trusted`, `real_ip_header`, `forwarded_proto_header`,
//! `forwarded_host_header`, `body_size`, `header_size`,
//! `upload_size`), `reason` / `requires` / `integration` /
//! `password` / `hash` / `algorithm` / `secret` / `header` /
//! `modifier`, the surface anchor verbs (`emits`, `anchor`,
//! `extensible_by`), the testing block (`tests`, `permits`,
//! `forbids`, `allows`, `deny`, `denies`, `accepted`,
//! `rejected`), surface projections (`search`, `filter`, `list`,
//! `form`, `detail`, `columns`, `fields`, `validate`, `validates`),
//! extensible-by extension kinds (`client`, `fn`, `hook`,
//! `validator`, `adapter`, `query_modifier`, `escape_route`),
//! `group` / `required` / `unique` / `default`, the `@cap.File`
//! family (`max_size`, `accept`, `visibility`, `signed_ttl`),
//! the `report` family (`report`, `columns`, `formats`,
//! `filename`), the observability family (`logging`, `tracing`,
//! `level`, `format`, `redact`, `sample_rate`, `propagate`,
//! `exporter`, `emit_to`), the webhook expanded vocab
//! (`webhook_events`, `webhook_event`, `previous_version`,
//! `payload_from`, `replay`, `allow`, `within`, `dedupe`, `dlq`,
//! `emit`, `drop`, `from`), the RBAC catalog (`permission`,
//! `role`, `inherits`, `grants`, `grants_all`, `has_role`,
//! `has_permission`), the IR error-vocab (`when_denied`,
//! `message_key`, the 12 closed-catalog error codes that alias to
//! `crate::error_vocab_code_detail`), and the resource
//! `conventions` / `@owner_axis` opt-ins.

pub(crate) fn keyword_description(keyword: &str) -> Option<&'static str> {
    match keyword {
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
        "allows" => Some(
            "Declares a positive authored test assertion. The subject names the dimension: `allows when <pred>` (predicate), `allows from <state>` (transition), `allows extension <feature>` (view extensibility), `allows <pred>` (agent eval).",
        ),
        "deny" => Some("Declares a rule precondition that rejects an operation."),
        "denies" => Some(
            "Declares a negative authored test assertion. The subject names the dimension: `denies when <pred>`, `denies from <state>`, `denies extension <feature>`, `denies <pred>` (agent eval).",
        ),
        "extension" => Some(
            "View-test subject: `allows extension <feature>` / `denies extension <feature>` whitelists which features may extend a view via its anchor (SPEC-08 folded the retired `accepted by`/`rejected by` verbs into this authored allows/denies form).",
        ),
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
        // Surface-sync WT-2 — `.lzx` view / create / UX primitives.
        "submit" => Some(
            "On `view create`: `submit <feature>.command.<name>` binds the form to the command it dispatches. Required for a create view (the dual of `source <query>` on list/detail views).",
        ),
        "on_success" => Some(
            "On `view create`: post-submit navigation/feedback block. Children (each at most once): `back`, `redirect \"<path>\"`, `flash <success|error|info> @translation.<key>`, `invalidates query.<name>`, `replace`. Valid only in submit-backed create bodies.",
        ),
        "back" => Some(
            "Inside `on_success`: navigate to the previous route after a successful submit. Mutually composable with `flash` / `invalidates`; declared at most once.",
        ),
        "redirect" => Some(
            "Inside `on_success`: `redirect \"<path>\"` navigates to an explicit quoted path after a successful submit. Declared at most once.",
        ),
        "flash" => Some(
            "Inside `on_success`: `flash <success|error|info> @translation.<key>` raises a toast/notice after submit. Kind is a closed catalog; the message references a `@translation.<key>`.",
        ),
        "replace" => Some(
            "Inside `on_success`: switches the post-submit navigation transition from push to replace (the new route replaces the current history entry). Flag, declared at most once.",
        ),
        "drawer" => Some(
            "On `view list`: `drawer <name> on select` mounts a side panel opened when a row is selected. List-only; declared at most once.",
        ),
        "sort" => Some(
            "On `view list`: sort contract block. `by <field>, ...` lists sortable columns; `default <field> asc|desc` sets the initial order. List-only; declared at most once.",
        ),
        "selection" => Some(
            "On `view list`: row-selection mode. Closed catalog: `single` / `multi` / `none`. Pairs with `bulk_actions` to enable batch operations over the selected set. List-only.",
        ),
        "bulk_actions" => Some(
            "On `view list`: comma-separated action names enabled over the current row selection (`bulk_actions archive, delete`). Implies a selection set even without an explicit `selection` line. List-only.",
        ),
        "settings" => Some(
            "On `view list`: per-view user-adjustable preferences block (e.g. `density: Enum [comfortable, compact] default comfortable`). Each entry may carry `persist` to retain the choice across sessions. List-only; declared at most once.",
        ),
        "persist" => Some(
            "Inside a `settings` declaration: marks a view setting as persisted across sessions (stored per-user) rather than reset on reload. Valid only as a `settings` child.",
        ),
        // Wave-W6 / GAP-UX view-level + audience-level UX primitives.
        "wizard_steps" => Some(
            "View-level UX primitive (list/detail): `wizard_steps <total> current <field>`. Renders a multi-step progress affordance; `<total>` is a positive integer and `current` names the step-tracking field. Declared at most once (GAP-UX-01).",
        ),
        "tab_group" => Some(
            "View-level UX primitive (list/detail): `tab_group derived_from <field>` with `case <V1, V2> -> tab \"<label>\"` arms. Groups the view into tabs chosen by the discriminant field's value (GAP-UX-02).",
        ),
        "view_mode" => Some(
            "View-level UX primitive (`view list` only): block of bare render-mode keywords (e.g. `table`, `kanban`) the list can toggle between. Declared at most once (GAP-UX-04).",
        ),
        "inline_table" => Some(
            "View-level UX primitive (`view list` only): `view.inline_table on_change @command.<name>` enables inline row editing that dispatches the named command on change. Declared at most once (GAP-UX-04).",
        ),
        "board" => Some(
            "View-level UX primitive (`view list` only): `view.board [<name>]` renders a kanban board. Requires a `lanes derived_from <field>` body line (GAP-UX-05).",
        ),
        "lanes" => Some(
            "Inside `view.board`: `lanes derived_from <field>` names the field whose values become the board's columns/lanes. Declared exactly once per board (GAP-UX-05).",
        ),
        "repeatable" => Some(
            "View-level UX primitive: `repeatable input <name> group <f>: <T>, ... [validates sum(<f>) = <n>]` declares a repeatable input group with an optional aggregate validation constraint (GAP-UX-05).",
        ),
        "tabs" => Some(
            "Audience-level UX primitive (sibling to `view`): `tabs` block of `tab \"<label>\" -> view <name> [audience <a>]` entries. Groups several views behind a tabbed navigation (GAP-UX-03).",
        ),
        "tab" => Some(
            "Inside `tabs` or a `tab_group` case: `tab \"<label>\" -> view <name> [audience <a>]` (audience tabs) or `case ... -> tab \"<label>\"` (view tab groups). Names one tab and its target view/label.",
        ),
        "wizard" => Some(
            "Audience-level UX primitive (sibling to `view`): `wizard <name> steps` with `step <n>: <view>` children. Sequences several views into an ordered multi-step flow (GAP-UX-03).",
        ),
        "step" => Some(
            "Inside an audience-level `wizard`: `step <n>: <view>` binds one ordinal step to a view. Steps are ordered by `<n>` (GAP-UX-03).",
        ),
        "date_range" => Some(
            "Filter cardinality on a `view list` `filters` entry: `<name>: date_range [<Date|DateTime>] [from query]`. Surfaces a paired from/to date picker bound to two query params (`<name>_from` / `<name>_to`) over a single Date/DateTime field (GAP-UX-07).",
        ),
        _ => None,
    }
}
