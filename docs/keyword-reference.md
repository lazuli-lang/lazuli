<!-- GENERATED FILE — DO NOT EDIT BY HAND.
     Source of truth: crates/lazuli_keywords (the `ALL` capability registry).
     Regenerate with: cargo run -p xtask -- gen-keyword-reference
     Freshness is gated by tools/xtask/tests/keyword_reference_fresh.rs and by
     the `keyword_surface_parity` doc-coverage test. -->

# Lazuli keyword reference

This is the **complete** catalog of every keyword/construct the Lazuli parser
recognizes, rendered directly from the `lazuli_keywords` capability registry.
Every row is one `CapabilitySpec`; a literal valid in N contexts with distinct
scopes appears as N rows (context-as-data). Because this document is generated
from the registry, it is exhaustive by construction — if the parser knows a
keyword, it is here.

Prose explanations, EBNF, and worked examples live in the hand-written
`docs/grammar.*.md` and `docs/quickref.md`. This file is the flat, exhaustive
index those documents are checked against.

Columns:

- **keyword / literal** — the exact literal the parser recognizes.
- **scope** — the TextMate scope leaf the highlighter assigns (see
  `editors/vscode/SCOPES.md`).
- **semantic token** — the LSP semantic-token type.
- **sigil** — `@` for decorators, `.` for dotted kinds, `—` for bare keywords.
- **hover** — the one-line description surfaced on hover/completion.
- **produces** — diagnostic codes (`lazuli doctor` rules) this construct gates,
  if any.

<!-- BEGIN GENERATED BODY -->

_Generated from 691 capability rows across the `lazuli_keywords` registry._

## `.lzi` — feature source

### TopLevel

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `apps` | `keyword.control.section.lazuli` | `keyword` | — | Workspace `apps` listing block. | — |
| `boundaries` | `keyword.control.section.lazuli` | `keyword` | — | Workspace service-boundary declarations. | — |
| `contract` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a service contract. | — |
| `design` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares the project-root design token catalog. | `design-custom-duplicate`<br>`design-custom-invalid-value`<br>`design-custom-reserved-name`<br>`design-token-duplicate-value`<br>`design-token-fontfamily-leak`<br>`design-token-hex-leak`<br>`design-token-missing-dark`<br>`design-token-px-leak`<br>`design-token-shadow-leak`<br>`design-token-undefined`<br>`design-token-unused` |
| `error_page` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares an app-level error page. | — |
| `escape_route` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Documents a deliberate framework escape hatch. | — |
| `experience` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a shared surface experience. | — |
| `feature` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a feature: the unit of business capability. | — |
| `gate` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Top-level feature/limit gating directive. | — |
| `gateway` | `keyword.control.section.lazuli` | `keyword` | — | Workspace API gateway block. | — |
| `grants` | `keyword.control.statement.lazuli` | `keyword` | — | RBAC grant statement. | — |
| `grants_all` | `keyword.control.statement.lazuli` | `keyword` | — | RBAC grant-all statement. | — |
| `permission` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | RBAC permission catalog entry. | — |
| `plan` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a billing/entitlement plan. | — |
| `revoke_session_family` | `keyword.control.statement.lazuli` | `keyword` | — | RBAC revoke-session-family action. | — |
| `revoke_user` | `keyword.control.statement.lazuli` | `keyword` | — | RBAC revoke-user action. | — |
| `role` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | RBAC role catalog entry. | — |
| `route` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Top-level route declaration. | `ROUTE-GUARD-FIELD-MISSING-SERVER-PAIR-001`<br>`ROUTE-GUARD-FIELD-TYPE-MISMATCH-006`<br>`ROUTE-GUARD-FIELD-UNKNOWN-FEATURE-004`<br>`ROUTE-GUARD-FIELD-UNKNOWN-FIELD-005`<br>`ROUTE-GUARD-FORBID-ONLY-WHEN-RESOURCE-MISMATCH-007`<br>`ROUTE-GUARD-LIFECYCLE-EXCLUSIVE-001`<br>`ROUTE-GUARD-LIFECYCLE-IN-EMPTY-002`<br>`ROUTE-GUARD-LIFECYCLE-IN-UNKNOWN-003`<br>`ROUTE-ID-UNUSED-IN-EFFECT-001`<br>`ROUTE-LIFECYCLE-CANONICAL-FORM-001` |
| `shared_registry` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Workspace-level shared registry reference. | — |
| `skeleton` | `keyword.control.section.lazuli` | `keyword` | — | Package skeleton block. | — |

### App

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `actor_query` | `keyword.control.statement.lazuli` | `keyword` | — | App-level actor resolution query. | — |
| `architecture` | `keyword.control.section.lazuli` | `keyword` | — | App architecture / service-boundary block. | — |
| `auth_failed_redirect` | `keyword.control.statement.lazuli` | `keyword` | — | Redirect target on auth failure. | — |
| `bindings` | `keyword.control.section.lazuli` | `keyword` | — | Registry binding overrides for this app. | — |
| `capabilities` | `keyword.control.section.lazuli` | `keyword` | — | App capability declarations. | — |
| `communication` | `keyword.control.section.lazuli` | `keyword` | — | Inter-service communication block. | — |
| `cookie` | `keyword.control.section.lazuli` | `keyword` | — | App cookie defaults block. | — |
| `cors` | `keyword.control.section.lazuli` | `keyword` | — | CORS configuration block. | — |
| `default_locale` | `keyword.control.statement.lazuli` | `keyword` | — | Default locale for the app. | — |
| `default_timezone` | `keyword.control.statement.lazuli` | `keyword` | — | Default timezone for the app. | — |
| `deploy` | `keyword.control.section.lazuli` | `keyword` | — | Deployment topology + migration policy block. | — |
| `encryption` | `keyword.control.section.lazuli` | `keyword` | — | Field-encryption configuration block. | `ENC-E2EE-EVENT-001`<br>`ENC-KEY-MISSING-001`<br>`ENC-ROTATION-001`<br>`ENC-SOURCE-ENV-001`<br>`ENC-TEMPLATE-AXIS-001`<br>`ENC-TENANCY-001` |
| `enforce_service_boundaries` | `entity.name.function.statement.app-meta.lazuli` | `keyword` | — | Enforce declared service boundaries. | — |
| `env` | `keyword.control.section.lazuli` | `keyword` | — | Environment-variable declarations block. | — |
| `environment` | `entity.name.function.statement.app-meta.lazuli` | `keyword` | — | Environment selector. | — |
| `environments` | `keyword.control.section.lazuli` | `keyword` | — | Named environment declarations. | — |
| `headers` | `keyword.control.section.lazuli` | `keyword` | — | Security/response header defaults block. | — |
| `integrations` | `keyword.control.section.lazuli` | `keyword` | — | Third-party integration declarations. | — |
| `lazuli_version` | `keyword.control.statement.lazuli` | `keyword` | — | Pinned Lazuli framework version. | — |
| `limits` | `keyword.control.section.lazuli` | `keyword` | — | Request/body size + timeout limits block. | — |
| `locale` | `keyword.control.section.lazuli` | `keyword` | — | Locale negotiation block. | — |
| `logging` | `keyword.control.section.lazuli` | `keyword` | — | Structured logging configuration block. | — |
| `mode` | `entity.name.function.statement.app-meta.lazuli` | `keyword` | — | Service mode (monolith/service). | — |
| `not_found` | `keyword.control.statement.lazuli` | `keyword` | — | 404 / not-found handler reference. | — |
| `packs` | `keyword.control.section.lazuli` | `keyword` | — | Feature-pack inclusion block. | — |
| `proxy` | `keyword.control.section.lazuli` | `keyword` | — | Trusted-proxy / forwarded-header block. | — |
| `route_guard` | `keyword.control.statement.lazuli` | `keyword` | — | App-level route guard. | — |
| `runtime` | `keyword.control.section.lazuli` | `keyword` | — | Runtime unit / process topology block. | — |
| `service_ready` | `entity.name.function.statement.app-meta.lazuli` | `keyword` | — | Marks the app service-ready. | — |
| `services` | `keyword.control.section.lazuli` | `keyword` | — | Service decomposition block. | — |
| `targets` | `keyword.control.statement.lazuli` | `keyword` | — | Generation targets (go/ts/...). | — |
| `title` | `keyword.control.statement.lazuli` | `keyword` | — | App display title. | — |
| `tracing` | `keyword.control.section.lazuli` | `keyword` | — | Distributed-tracing configuration block. | — |
| `urls` | `keyword.control.section.lazuli` | `keyword` | — | Named URL declarations block. | — |
| `uses` | `keyword.control.statement.lazuli` | `keyword` | — | Declares a registry/experience the app uses. | — |
| `version` | `keyword.control.statement.lazuli` | `keyword` | — | App version string. | — |

### Registry

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `secret_rotation` | `keyword.control.section.lazuli` | `keyword` | — | Secret-rotation policy block. | — |
| `tools` | `keyword.control.section.lazuli` | `keyword` | — | Registry tool declarations. | — |
| `webhook_event` | `keyword.control.section.lazuli` | `keyword` | — | Registry webhook-event envelope. | — |
| `webhook_events` | `keyword.control.section.lazuli` | `keyword` | — | Registry webhook-events catalog. | — |

### FeatureHeader

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `agent` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares an LLM agent. | — |
| `aggregate` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a domain aggregate root. | `AGGREGATE-CONTAINS-UNKNOWN`<br>`AGGREGATE-ROOT-UNKNOWN` |
| `api` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a full-control HTTP endpoint. | `API-HANDLER-UNWIRED-001` |
| `auth` | `keyword.control.section.lazuli` | `keyword` | — | Declares the authentication block. | — |
| `cache` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a named cache profile. | — |
| `channel` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a realtime channel. | `CHANNEL-PAYLOAD-001` |
| `command` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a write command (mutation effect). | `CODEGEN-UNRESOLVED-BINDING-SOURCE-001`<br>`COMMAND-INPUT-SHADOWS-FIELD-001`<br>`CREATES-EMPTY-BINDINGS-001`<br>`CTX-PATH-UNRESOLVED-001`<br>`HOOK-TARGET-001`<br>`MUTATION-WITHOUT-READBACK-001` |
| `compatibility` | `keyword.control.statement.lazuli` | `keyword` | — | Declares contract compatibility. | — |
| `constraints` | `entity.name.function.statement.non-goals.lazuli` | `keyword` | — | Non-goal constraints. | — |
| `defaults` | `keyword.control.section.lazuli` | `keyword` | — | Declares resource-convention defaults. | — |
| `delegated_to` | `entity.name.function.statement.non-goals.lazuli` | `keyword` | — | Non-goal delegated to another feature. | — |
| `domain` | `keyword.control.section.lazuli` | `keyword` | — | Declares the domain-model block. | — |
| `entity` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a domain entity. | — |
| `enum` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares an enumeration. | `VOCAB-UNION-001`<br>`VOCAB-UNION-002` |
| `errors` | `keyword.control.section.lazuli` | `keyword` | — | Declares the error-vocabulary block. | `ERR-VOCAB-001`<br>`ERR-VOCAB-002`<br>`ERR-VOCAB-003`<br>`ERR-VOCAB-CODE-UNKNOWN`<br>`ERR-VOCAB-EXPOSE-5XX-MESSAGE`<br>`ERR-VOCAB-EXPOSE-UNKNOWN`<br>`ERR-VOCAB-WHEN-DENIED-NO-POLICY` |
| `event` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a domain event. | — |
| `event.trace` | `keyword.control.declaration.structural.lazuli` | `keyword` | `.` (dotted kind) | Declares an event trace. | — |
| `event_group` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares an event group. | `EVENT-GROUP-VARIANT-TYPE-001` |
| `events` | `keyword.control.section.lazuli` | `keyword` | — | Declares the events block. | — |
| `extensions` | `keyword.control.section.lazuli` | `keyword` | — | Declares typed extension points. | — |
| `import` | `keyword.control.statement.lazuli` | `keyword` | — | Imports a contract/operation. | — |
| `imports` | `keyword.control.statement.lazuli` | `keyword` | — | Declares feature imports. | — |
| `invariants` | `keyword.control.section.lazuli` | `keyword` | — | Declares the invariants block. | `INVARIANT-PREDICATE-INVALID` |
| `job` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a background job. | `JOB-DECLARATIVE-BODY-UNSUPPORTED-001` |
| `knowledge` | `keyword.control.statement.lazuli` | `keyword` | — | Feature knowledge sector (iron-hand context). | `VOCAB-KNOWLEDGE-DANGLING-CITE-001`<br>`VOCAB-KNOWLEDGE-DUP-TOPIC-001`<br>`VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001`<br>`VOCAB-KNOWLEDGE-SINGLE-FEATURE-001`<br>`VOCAB-KNOWLEDGE-STALE-001`<br>`VOCAB-KNOWLEDGE-UNGATED-WRITE-001` |
| `mcp_server` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares an MCP server surface. | — |
| `non_goals` | `keyword.control.section.lazuli` | `keyword` | — | Feature non-goals (iron-hand context). | `VOCAB-CONTEXT-NONGOALS-001` |
| `notification` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a notification. | — |
| `operation` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a contract operation. | — |
| `out_of_scope` | `entity.name.function.statement.non-goals.lazuli` | `keyword` | — | Explicitly out-of-scope concern. | — |
| `permission` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Feature-scoped RBAC permission. | — |
| `policies` | `keyword.control.section.lazuli` | `keyword` | — | Declares the policy block. | `POLICY-PREDICATE-001` |
| `poller` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a polling integration. | `POLLER-CURSOR-MISSING-001`<br>`POLLER-DUAL-SCHEDULER-001`<br>`POLLER-HANDLER-ORPHAN-001`<br>`POLLER-IDEMPOTENCY-ATTEMPTS-MISSING-001`<br>`POLLER-MAX-RETRIES-UNBOUNDED-001`<br>`POLLER-NO-TERMINAL-001`<br>`POLLER-QUIRK-CATALOG-MISMATCH-001`<br>`POLLER-TERMINAL-FIELD-ENUM-001`<br>`POLLER-TERMINAL-NO-EMIT-001`<br>`POLLER-TICK-TOO-FAST-001` |
| `purpose` | `keyword.control.statement.lazuli` | `keyword` | — | Feature purpose (iron-hand context). | `VOCAB-CONTEXT-PURPOSE-001` |
| `query.list` | `keyword.control.declaration.structural.lazuli` | `keyword` | `.` (dotted kind) | Declares a list query (collection projection). | `DUPLICATE-QUERY-NAME-001`<br>`ENUM-VARIANT-UNDECLARED-001`<br>`MISSING-POLICY-ON-QUERY-001` |
| `query.lookup` | `keyword.control.declaration.structural.lazuli` | `keyword` | `.` (dotted kind) | Declares a lookup query (single-record fetch). | — |
| `query.sql` | `keyword.control.declaration.structural.lazuli` | `keyword` | `.` (dotted kind) | Declares a raw-SQL query. | — |
| `query.view` | `keyword.control.declaration.structural.lazuli` | `keyword` | `.` (dotted kind) | Declares a database-view-backed query. | — |
| `record` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a value-object record. | `VOCAB-SHADOW-RECORD-001` |
| `refs` | `keyword.control.section.lazuli` | `keyword` | — | Declares cross-feature references. | `CROSS-FEATURE-CONTRACT-MISSING-001`<br>`CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001`<br>`CROSS-FEATURE-WORKFLOW-SPAN-001` |
| `report` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a report/export. | `REPORT-COLUMN-MISMATCH-001`<br>`REPORT-COLUMNS-EMPTY-001`<br>`REPORT-FILENAME-TOKEN-UNKNOWN-001`<br>`REPORT-FORMAT-UNKNOWN-001`<br>`REPORT-INPUT-UNBOUND-001`<br>`REPORT-PATH-COLLISION-001`<br>`REPORT-POLICY-PUBLIC-NO-RATE-LIMIT-001`<br>`REPORT-SIGNED-NO-STORAGE-001`<br>`REPORT-SIGNED-TTL-FORBIDDEN-001`<br>`REPORT-SIGNED-TTL-MISSING-001`<br>`REPORT-SOURCE-KIND-001`<br>`REPORT-STORAGE-AMBIGUOUS-001` |
| `requires` | `entity.name.function.statement.feature-meta.lazuli` | `keyword` | — | Feature dependency / requirement. | — |
| `resource` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a domain resource. | — |
| `role` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Feature-scoped RBAC role. | — |
| `subscription` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares an event subscription. | — |
| `surface` | `keyword.control.section.lazuli` | `keyword` | — | Declares a feature surface. | — |
| `tenant_migration` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a tenant-data migration. | — |
| `tests` | `keyword.control.section.lazuli` | `keyword` | — | Declares the policy/behavior tests block. | `TEST-COMMAND-ASSERTION-DRIFT-001`<br>`TEST-EVAL-VERB-RETIRED-001`<br>`TEST-FAILURE-ONLY-COVERAGE-001`<br>`TEST-FIXTURE-LITERAL-001`<br>`TEST-HANDLER-MISSING-001`<br>`TEST-MATRIX-VERB-MISPLACED-001`<br>`TEST-MISSING-AUTHORED-001`<br>`TEST-PINS-STUB-VOCAB-001`<br>`TEST-PREDICATE-UNCOVERED-001`<br>`TEST-RESTATES-EFFECT-001`<br>`TEST-RESTATES-POLICY-001`<br>`TEST-STUB-001`<br>`TEST-VIEW-DRIFT-001`<br>`TEST-VIEW-E2E-MISSING-001`<br>`TEST-VIEW-EXTENSIBILITY-001`<br>`TEST-VIEW-EXTENSION-VERB-RETIRED-001`<br>`VOCAB-TESTS-MISSING-001` |
| `translation` | `keyword.control.section.lazuli` | `keyword` | — | Declares a translation catalog. | — |
| `view` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a surface view. | — |
| `webhook` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares an inbound webhook handler. | `WEBHOOK-EMIT-PREDICATE-FIELD-001` |

### ResourceBody

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `@actor` | `entity.name.tag.catalog-atom.lazuli` | `decorator` | `@` | Identity-axis catalog atom (`@actor.<name>`, e.g. `@actor.system` → an app-level actor principal). | — |
| `@adapter` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Adapter decorator. | — |
| `@anchor` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Anchor reference decorator. | — |
| `@audience` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Audience reference decorator. | — |
| `@cap` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Capability decorator (`@cap.File`). | `VOCAB-CAP-MISSING-001` |
| `@client` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Client extension decorator. | — |
| `@doctor.allow` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Doctor waiver annotation (`@doctor.allow(CODE, reason: "...")`) — suppresses a doctor finding on the following construct or the file. | — |
| `@feature` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Feature reference decorator. | — |
| `@file` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | File reference decorator. | — |
| `@fn` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Custom-function reference decorator. | `HANDLER-MISSING-001`<br>`HANDLER-SIGNATURE-MISMATCH-001`<br>`HANDLER-SQL-COLUMN-DRIFT-001`<br>`VOCAB-HANDLER-HEAVY-001`<br>`VOCAB-RUNTIME-REINVENTED-001` |
| `@full_text` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Full-text-index decorator. | `FULL-TEXT-TYPE-001` |
| `@hook` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Hook reference decorator. | `HANDLER-ERROR-WRAP-001`<br>`HANDLER-NO-PANIC-001`<br>`HANDLER-NO-STRING-ERROR-001` |
| `@key` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Encryption-key decorator. | — |
| `@llm` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | LLM decorator. | — |
| `@owner_axis` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Ownership-axis decorator. | `owner_axis_collides_with_unique_user`<br>`owner_axis_on_non_fk`<br>`owner_axis_through_not_user_keyed`<br>`owner_axis_unknown_through` |
| `@pii` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | PII-classification decorator. | — |
| `@policy` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Feature-local policy named reference (`@policy.<category>` → a `policies` block category in this feature). | — |
| `@query_modifier` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Query-modifier reference decorator. | — |
| `@resume` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Resume reference decorator. | — |
| `@role` | `entity.name.tag.catalog-atom.lazuli` | `decorator` | `@` | Identity-axis catalog atom (`@role.<name>` → an app-level role from the registry identity catalog). | — |
| `@scope` | `entity.name.tag.catalog-atom.lazuli` | `decorator` | `@` | Identity-axis catalog atom (`@scope.<name>` → an app-level OAuth/permission scope). | — |
| `@semantic` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Semantic-scalar decorator (`@semantic.HexColor`). | `MONEY-ARITHMETIC-001`<br>`MONEY-COMPARE-001`<br>`VOCAB-JSON-TYPED-001`<br>`VOCAB-MONEY-MULTI-CURRENCY-001`<br>`VOCAB-MONEY-SHAPE-001` |
| `@slug` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Slug field decorator. | — |
| `@tool` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Tool decorator. | — |
| `@translation` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Translation reference decorator. | — |
| `@validator` | `entity.name.tag.decorator.lazuli` | `decorator` | `@` | Validator reference decorator. | — |
| `alias` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Field alias. | — |
| `append_only` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Append-only (event-log) resource. | `RESOURCE-APPEND-ONLY-001` |
| `assign` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Default-literal / field-rename assignment merged into a synthesized `crud` command (overlay). | — |
| `composite_key` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Composite primary key. | `COMPOSITE-KEY-CONTRACT-001` |
| `computed_date` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Computed date field. | `COMPUTED-DATE-EXPR-001` |
| `contains` | `keyword.control.statement.lazuli` | `keyword` | — | Aggregate containment. | — |
| `conventions` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Opt into resource conventions. | `VOCAB-CRUD-SYNTH-AVAILABLE-001`<br>`VOCAB-SOFT-DELETE-ACTOR-001`<br>`conventions_unknown` |
| `derived` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Derived/computed field. | `VOCAB-DERIVED-READ-001` |
| `field` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Explicit field declaration. | — |
| `fields` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Field group. | — |
| `has_many` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | One-to-many relation. | — |
| `index` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Index declaration. | — |
| `invariant` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Resource invariant. | — |
| `inverse` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Inverse relation side. | — |
| `lifecycle` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a resource lifecycle. | `LIFECYCLE-ENUM-DUPLICATE`<br>`LIFECYCLE-FIELD-DOUBLE-DECLARED`<br>`LIFECYCLE-INITIAL-AMBIGUOUS`<br>`LIFECYCLE-INVARIANT-CATALOG-MISMATCH`<br>`LIFECYCLE-INVARIANT-PARAM-UNRESOLVED`<br>`LIFECYCLE-NO-INITIAL-STATE`<br>`LIFECYCLE-NO-JUMP-NEEDS-LINEAR`<br>`LIFECYCLE-POLICY-REQUIRED`<br>`LIFECYCLE-STATE-DUPLICATE`<br>`LIFECYCLE-STATE-SET-UNDECLARED-001`<br>`LIFECYCLE-TERMINAL-HAS-OUTGOING-TRANSITION`<br>`LIFECYCLE-TIMESTAMP-TYPE`<br>`LIFECYCLE-TRANSITION-FROM-UNDECLARED`<br>`LIFECYCLE-TRANSITION-TO-UNDECLARED`<br>`LIFECYCLE-UNREACHABLE-STATE`<br>`VOCAB-LIFECYCLE-001` |
| `lock` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Concurrency lock strategy. | `RESOURCE-LOCK-CONTRACT-001` |
| `many_through` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Many-to-many through a join. | `MANY-THROUGH-ENDPOINT-001` |
| `migrated` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Migration marker. | — |
| `on_delete` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Referential on-delete action. | — |
| `paginate` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Default pagination. | — |
| `polymorphic_ref` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Polymorphic reference field. | `REF-CROSS-FEATURE-UNKNOWN-001`<br>`REF-POLYMORPHIC-TARGET-001` |
| `previously` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Previous field name (rename). | — |
| `primary` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Primary-key marker. | — |
| `resource` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a persisted resource. | `UPDATES-MISSING-UPDATED-AT-001`<br>`VOCAB-GRAMMAR-FORM-001`<br>`VOCAB-RESOURCE-WIDE-CLUSTER-001` |
| `retention` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Data-retention policy. | — |
| `root` | `keyword.control.statement.lazuli` | `keyword` | — | Aggregate root marker. | — |
| `schedule_rule` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Recurrence/schedule rule field. | `SCHEDULE-RULE-001` |
| `soft_delete` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Enable soft-delete (`deleted_at`). Add `by` (`soft_delete by`) to also project a `deleted_by` actor column populated from `ctx.actor`. | — |
| `storage` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Storage hint. | — |
| `tenancy` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Tenancy axis for the resource. | — |
| `timestamps` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Enable created/updated timestamps. | — |
| `unique` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Unique constraint. | `CONSTRAINT-UNIQUE-WHEN-001`<br>`SLUG-UNIQUENESS-IMPLICIT` |
| `validate` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Field validation rule. | — |
| `validates` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Field validation rule. | — |
| `version_field` | `entity.name.function.statement.resource.lazuli` | `keyword` | — | Optimistic-lock version field. | — |

### CommandBody

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `approval` | `keyword.control.section.lazuli` | `keyword` | — | Approval-chain block. | — |
| `audit` | `keyword.control.section.lazuli` | `keyword` | — | Audit-logging block. | `AUDIT-MATERIALIZE-TARGET-001`<br>`VOCAB-AUDIT-001`<br>`VOCAB-AUDIT-002` |
| `calls` | `keyword.control.statement.lazuli` | `keyword` | — | External call. | — |
| `creates` | `keyword.control.statement.lazuli` | `keyword` | — | Effect: creates a resource. | — |
| `deletes` | `keyword.control.statement.lazuli` | `keyword` | — | Effect: deletes a resource. | — |
| `deprecated` | `keyword.control.section.lazuli` | `keyword` | — | Deprecation metadata block. | — |
| `emits` | `keyword.control.statement.lazuli` | `keyword` | — | Event emission. | `EVENT-OUTBOX-001`<br>`VOCAB-EVENT-ORPHAN-001`<br>`VOCAB-EVENT-PAYLOAD-001`<br>`VOCAB-EVENT-PRODUCER-001` |
| `gate` | `keyword.control.statement.lazuli` | `keyword` | — | Entitlement gate. | — |
| `handler` | `keyword.control.statement.lazuli` | `keyword` | — | Custom Go handler reference (`@fn.X`). | — |
| `idempotency` | `keyword.control.statement.lazuli` | `keyword` | — | Idempotency key declaration. | — |
| `input` | `keyword.control.section.lazuli` | `keyword` | — | Input field block. | — |
| `invalidates` | `keyword.control.section.lazuli` | `keyword` | — | Cache invalidation block. | — |
| `let` | `keyword.control.statement.lazuli` | `keyword` | — | Local binding. | — |
| `policy` | `keyword.control.statement.lazuli` | `keyword` | — | Authorization policy expression. | — |
| `rate_limit` | `keyword.control.statement.lazuli` | `keyword` | — | Rate-limit declaration. | `rate_limit_duplicate_default`<br>`rate_limit_duplicate_env`<br>`rate_limit_invalid_spec`<br>`rate_limit_no_default_with_qualifications`<br>`rate_limit_unknown_env` |
| `reorder` | `keyword.control.statement.lazuli` | `keyword` | — | Reorder effect. | `REORDER-POSITION-FIELD-001` |
| `retry` | `keyword.control.statement.lazuli` | `keyword` | — | Retry policy. | — |
| `returns` | `keyword.control.statement.lazuli` | `keyword` | — | Declares the command return type. | — |
| `route` | `keyword.control.statement.lazuli` | `keyword` | — | HTTP route for the command. | — |
| `target` | `keyword.control.statement.lazuli` | `keyword` | — | Effect target resource. | — |
| `timeout` | `keyword.control.statement.lazuli` | `keyword` | — | Command timeout. | — |
| `triggers` | `keyword.control.statement.lazuli` | `keyword` | — | Triggers a lifecycle transition (`triggers transition <t>`). | — |
| `updates` | `keyword.control.statement.lazuli` | `keyword` | — | Effect: updates a resource. | — |
| `validate` | `keyword.control.statement.lazuli` | `keyword` | — | Inline validation. | — |
| `where` | `keyword.control.statement.lazuli` | `keyword` | — | Row-scoping clause inside an `updates`/`deletes` effect (`where <col> = <expr>`). Becomes the WHERE binding, not a SET. | — |
| `write_window` | `keyword.control.statement.lazuli` | `keyword` | — | Write-window constraint. | — |

### Query

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `filters` | `keyword.control.section.lazuli` | `keyword` | — | Filter predicate block. | — |
| `modifier` | `keyword.control.statement.lazuli` | `keyword` | — | Query modifier reference. | — |
| `order` | `keyword.control.statement.lazuli` | `keyword` | — | Default ordering. | — |
| `params` | `keyword.control.section.lazuli` | `keyword` | — | Query parameter block. | — |
| `scope` | `keyword.control.section.lazuli` | `keyword` | — | Query scope block. | — |
| `search` | `keyword.control.statement.lazuli` | `keyword` | — | Full-text search declaration. | — |
| `source` | `keyword.control.statement.lazuli` | `keyword` | — | Query source resource. | — |
| `sql` | `keyword.control.statement.lazuli` | `keyword` | — | Raw SQL body. | — |

### Job

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `axis` | `keyword.control.statement.lazuli` | `keyword` | — | Fan-out axis. | — |
| `fanout` | `keyword.control.statement.lazuli` | `keyword` | — | Fan-out declaration. | — |
| `outbox` | `keyword.control.statement.lazuli` | `keyword` | — | Outbox-pattern marker. | — |
| `queue` | `keyword.control.statement.lazuli` | `keyword` | — | Job queue. | — |
| `schedule` | `keyword.control.statement.lazuli` | `keyword` | — | Job schedule. | — |
| `tenant_from` | `keyword.control.statement.lazuli` | `keyword` | — | Tenant resolution source. | — |
| `trigger` | `keyword.control.statement.lazuli` | `keyword` | — | Job trigger. | — |

### Webhook

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `allow` | `entity.name.function.statement.replay.lazuli` | `keyword` | — | Replay allow window. | — |
| `by` | `entity.name.function.statement.replay.lazuli` | `keyword` | — | Replay dedupe binding. | — |
| `dedupe` | `entity.name.function.statement.replay.lazuli` | `keyword` | — | Replay deduplication key. | — |
| `deny` | `entity.name.function.statement.replay.lazuli` | `keyword` | — | Replay deny rule. | — |
| `dlq` | `keyword.control.section.lazuli` | `keyword` | — | Dead-letter-queue block. | — |
| `header` | `keyword.control.statement.lazuli` | `keyword` | — | Signature header name. | — |
| `payload` | `keyword.control.statement.lazuli` | `keyword` | — | Webhook payload type. | — |
| `payload_from` | `keyword.control.statement.lazuli` | `keyword` | — | Payload source reference. | — |
| `payload_group` | `keyword.control.statement.lazuli` | `keyword` | — | Webhook payload group. | — |
| `previous_version` | `keyword.control.statement.lazuli` | `keyword` | — | Previous payload version. | — |
| `replay` | `keyword.control.section.lazuli` | `keyword` | — | Replay-protection block. | — |
| `secret` | `keyword.control.statement.lazuli` | `keyword` | — | Verification secret. | — |
| `verify` | `keyword.control.section.lazuli` | `keyword` | — | Signature-verification block. | — |
| `within` | `entity.name.function.statement.replay.lazuli` | `keyword` | — | Replay window. | — |

### Agent

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `discriminator` | `keyword.control.statement.lazuli` | `keyword` | — | Output discriminator. | — |
| `expose` | `keyword.control.statement.lazuli` | `keyword` | — | Expose the agent over HTTP/MCP. | — |
| `max_tokens` | `keyword.control.statement.lazuli` | `keyword` | — | Max output tokens. | — |
| `model` | `keyword.control.statement.lazuli` | `keyword` | — | LLM model. | — |
| `output` | `keyword.control.statement.lazuli` | `keyword` | — | Agent output shape (bare / `stream` / `discriminator`). | — |
| `prompt` | `keyword.control.statement.lazuli` | `keyword` | — | Agent prompt. | — |
| `safety` | `keyword.control.statement.lazuli` | `keyword` | — | Safety constraints. | — |
| `seed` | `keyword.control.statement.lazuli` | `keyword` | — | Deterministic seed. | — |
| `stream` | `keyword.control.statement.lazuli` | `keyword` | — | Streaming mode. | — |
| `temperature` | `keyword.control.statement.lazuli` | `keyword` | — | Sampling temperature. | — |
| `tool` | `keyword.control.statement.lazuli` | `keyword` | — | Declares a single tool / MCP tool. | — |
| `tools` | `keyword.control.section.lazuli` | `keyword` | — | Agent tool-binding block. | — |
| `top_p` | `keyword.control.statement.lazuli` | `keyword` | — | Top-p sampling. | — |

### Notification

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `burst` | `entity.name.function.statement.throttle.lazuli` | `keyword` | — | Throttle burst allowance. | — |
| `digest` | `keyword.control.section.lazuli` | `keyword` | — | Digest-batching block. | — |
| `every` | `entity.name.function.statement.digest.lazuli` | `keyword` | — | Digest interval. | — |
| `group_by` | `entity.name.function.statement.digest.lazuli` | `keyword` | — | Digest grouping key. | — |
| `max_per` | `entity.name.function.statement.throttle.lazuli` | `keyword` | — | Throttle max-per-window. | — |
| `max_size` | `entity.name.function.statement.digest.lazuli` | `keyword` | — | Digest max batch size. | — |
| `per_channel` | `entity.name.function.statement.throttle.lazuli` | `keyword` | — | Per-channel throttle. | — |
| `per_recipient` | `entity.name.function.statement.throttle.lazuli` | `keyword` | — | Per-recipient throttle. | — |
| `recipient` | `keyword.control.statement.lazuli` | `keyword` | — | Notification recipient. | — |
| `template` | `keyword.control.statement.lazuli` | `keyword` | — | Notification template. | — |
| `template_strategy` | `entity.name.function.statement.digest.lazuli` | `keyword` | — | Digest template strategy. | — |
| `throttle` | `keyword.control.section.lazuli` | `keyword` | — | Throttling block. | — |

### Poller

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `backoff` | `keyword.control.statement.lazuli` | `keyword` | — | Backoff policy. | — |
| `counter` | `keyword.control.statement.lazuli` | `keyword` | — | Poll counter. | — |
| `cursor` | `keyword.control.statement.lazuli` | `keyword` | — | Poll cursor field. | — |
| `max_attempts` | `keyword.control.statement.lazuli` | `keyword` | — | Max poll retry attempts. | — |
| `retry_quirk` | `keyword.control.statement.lazuli` | `keyword` | — | Poller retry-quirk catalog entry. | — |
| `tick` | `keyword.control.statement.lazuli` | `keyword` | — | Poll interval. | — |

### Report

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `columns` | `keyword.control.section.lazuli` | `keyword` | — | Report column block. | — |
| `formats` | `keyword.control.statement.lazuli` | `keyword` | — | Export formats. | — |
| `label` | `entity.name.function.statement.columns.lazuli` | `keyword` | — | Column label. | — |
| `visibility` | `keyword.control.statement.lazuli` | `keyword` | — | Report visibility. | — |

### Channel

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `payload_axis` | `keyword.control.statement.lazuli` | `keyword` | — | Channel partition axis. | — |

### TenantMigration

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `materialize_strategy` | `keyword.control.statement.lazuli` | `keyword` | — | Materialization strategy. | — |

### Api

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `method` | `keyword.control.statement.lazuli` | `keyword` | — | HTTP method. | — |
| `output` | `keyword.control.statement.lazuli` | `keyword` | — | Typed response shape for the endpoint. | — |
| `transport` | `keyword.control.statement.lazuli` | `keyword` | — | Transport (http). | — |

### Audit

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `after` | `entity.name.function.statement.audit.lazuli` | `keyword` | — | Audit after-image clause. | — |
| `before` | `entity.name.function.statement.audit.lazuli` | `keyword` | — | Audit before-image clause. | — |
| `data_subject` | `entity.name.function.statement.audit.lazuli` | `keyword` | — | GDPR data-subject binding. | — |
| `emit_to` | `entity.name.function.statement.audit.lazuli` | `keyword` | — | Audit-event sink. | — |
| `materialize` | `entity.name.function.statement.audit.lazuli` | `keyword` | — | Materialize the audit projection. | — |
| `retain_for` | `entity.name.function.statement.audit.lazuli` | `keyword` | — | Audit retention window. | — |

### Approval

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `allow` | `entity.name.function.statement.approval.lazuli` | `keyword` | — | Approval allow branch. | — |
| `by` | `entity.name.function.statement.approval.lazuli` | `keyword` | — | Approver binding. | — |
| `chain` | `entity.name.function.statement.approval.lazuli` | `keyword` | — | Approval chain. | — |
| `deny` | `entity.name.function.statement.approval.lazuli` | `keyword` | — | Approval deny branch. | — |
| `escalate` | `entity.name.function.statement.approval.lazuli` | `keyword` | — | Approval escalation action. | — |
| `required_when` | `entity.name.function.statement.approval.lazuli` | `keyword` | — | Condition requiring approval. | — |
| `sequential` | `entity.name.function.statement.approval.lazuli` | `keyword` | — | Sequential approval mode. | — |
| `then` | `entity.name.function.statement.approval.lazuli` | `keyword` | — | Approval next-step connector. | — |
| `timeout` | `entity.name.function.statement.approval.lazuli` | `keyword` | — | Approval timeout. | — |

### Policy

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `allow` | `entity.name.function.statement.policy.lazuli` | `keyword` | — | Policy: allow. | — |
| `default_policy` | `keyword.control.statement.lazuli` | `keyword` | — | Default policy. | — |
| `denies` | `entity.name.function.statement.policy.lazuli` | `keyword` | — | Policy: denies. | — |
| `deny` | `entity.name.function.statement.policy.lazuli` | `keyword` | — | Policy: deny. | — |
| `forbids` | `entity.name.function.statement.policy.lazuli` | `keyword` | — | Policy: forbids. | — |
| `permits` | `entity.name.function.statement.policy.lazuli` | `keyword` | — | Policy: permits. | — |

### Errors

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `code` | `entity.name.function.statement.errors.lazuli` | `keyword` | — | Error code. | — |
| `error` | `entity.name.function.statement.errors.lazuli` | `keyword` | — | Error declaration. | — |
| `expose` | `entity.name.function.statement.errors.lazuli` | `keyword` | — | Expose error to client. | — |
| `hide` | `entity.name.function.statement.errors.lazuli` | `keyword` | — | Hide error from client. | — |
| `message` | `entity.name.function.statement.errors.lazuli` | `keyword` | — | Error message. | — |
| `reason` | `entity.name.function.statement.errors.lazuli` | `keyword` | — | Error reason. | — |
| `status` | `entity.name.function.statement.errors.lazuli` | `keyword` | — | HTTP status. | — |

### Defaults

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `audit` | `entity.name.function.statement.defaults.lazuli` | `keyword` | — | Default audit mode hoisted to every command (`audit default`; per-command `audit`/`audit none` wins). | — |
| `no_timestamps` | `entity.name.function.statement.defaults.lazuli` | `keyword` | — | Disable timestamps by default. | — |
| `policy_for` | `entity.name.function.statement.defaults.lazuli` | `keyword` | — | Default policy for an action. | — |
| `rate_limit` | `entity.name.function.statement.defaults.lazuli` | `keyword` | — | Default rate-limit spec hoisted to every command (per-command `rate_limit` wins). | — |
| `retention` | `entity.name.function.statement.defaults.lazuli` | `keyword` | — | Default retention policy. | — |
| `soft_delete` | `entity.name.function.statement.defaults.lazuli` | `keyword` | — | Default soft-delete convention (`deleted_at`). `soft_delete by` also projects a `deleted_by` actor column populated from `ctx.actor`. | — |
| `tenancy` | `entity.name.function.statement.defaults.lazuli` | `keyword` | — | Default tenancy mode. | — |
| `timestamps` | `entity.name.function.statement.defaults.lazuli` | `keyword` | — | Default timestamp convention. | — |

### Cache

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `coalesce` | `entity.name.function.statement.cache.lazuli` | `keyword` | — | Request coalescing. | — |
| `key` | `entity.name.function.statement.cache.lazuli` | `keyword` | — | Cache key. | — |
| `namespace` | `entity.name.function.statement.cache.lazuli` | `keyword` | — | Cache namespace. | — |
| `sliding` | `entity.name.function.statement.cache.lazuli` | `keyword` | — | Sliding-expiration flag. | — |
| `stale_while_revalidate` | `entity.name.function.statement.cache.lazuli` | `keyword` | — | Stale-while-revalidate window. | — |
| `tags` | `entity.name.function.statement.cache.lazuli` | `keyword` | — | Cache tags. | — |
| `ttl` | `entity.name.function.statement.cache.lazuli` | `keyword` | — | Cache TTL. | — |

### Tests

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `allows` | `entity.name.function.statement.tests.lazuli` | `keyword` | — | Test: action allowed. | — |
| `as` | `entity.name.function.statement.tests.lazuli` | `keyword` | — | Test actor/role alias. | — |
| `by` | `entity.name.function.statement.tests.lazuli` | `keyword` | — | Test actor binding. | — |
| `case` | `entity.name.function.statement.tests.lazuli` | `keyword` | — | Eval case. | — |
| `denies` | `entity.name.function.statement.tests.lazuli` | `keyword` | — | Test: action denied. | — |
| `evals` | `keyword.control.section.lazuli` | `keyword` | — | Agent evaluation block. | — |
| `extension` | `entity.name.function.statement.tests.lazuli` | `keyword` | — | View-test subject: `allows extension <feature>` / `denies extension <feature>` whitelists which features may extend a view via its anchor. | — |
| `forbids` | `entity.name.function.statement.tests.lazuli` | `keyword` | — | Test: permission forbidden. | — |
| `from` | `entity.name.function.statement.tests.lazuli` | `keyword` | — | Test source binding. | — |
| `golden` | `entity.name.function.statement.tests.lazuli` | `keyword` | — | Golden-output assertion. | — |
| `min_score` | `entity.name.function.statement.tests.lazuli` | `keyword` | — | Minimum eval score. | — |
| `permits` | `entity.name.function.statement.tests.lazuli` | `keyword` | — | Test: permission granted. | — |
| `requires` | `entity.name.function.statement.tests.lazuli` | `keyword` | — | Test precondition clause. | — |
| `to` | `entity.name.function.statement.tests.lazuli` | `keyword` | — | Test transition target. | — |
| `when` | `entity.name.function.statement.tests.lazuli` | `keyword` | — | Test guard clause. | — |

### Extensions

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `block` | `entity.name.function.statement.extension.lazuli` | `keyword` | — | UI block extension point. | — |
| `client` | `entity.name.function.statement.extension.lazuli` | `keyword` | — | Client-side extension point. | — |
| `escape_route` | `keyword.control.statement.lazuli` | `keyword` | — | Escape-route extension. | — |
| `fn` | `entity.name.function.statement.extension.lazuli` | `keyword` | — | Custom function extension point. | — |
| `hook` | `entity.name.function.statement.extension.lazuli` | `keyword` | — | Lifecycle hook extension point. | — |
| `query_modifier` | `entity.name.function.statement.extension.lazuli` | `keyword` | — | Query-modifier extension point. | — |
| `validator` | `entity.name.function.statement.extension.lazuli` | `keyword` | — | Validator extension point. | — |

### Translation

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `catalog` | `entity.name.function.statement.translation.lazuli` | `keyword` | — | Translation catalog. | — |
| `key` | `entity.name.function.statement.translation.lazuli` | `keyword` | — | Translation key. | — |
| `plural` | `entity.name.function.statement.translation.lazuli` | `keyword` | — | Plural form. | — |

### Auth

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `access_ttl` | `keyword.control.statement.lazuli` | `keyword` | — | Access-token TTL. | — |
| `cookie` | `keyword.control.section.lazuli` | `keyword` | — | Session-cookie transport attributes block (name/same_site/secure/http_only/domain/path). | — |
| `enroll` | `keyword.control.statement.lazuli` | `keyword` | — | MFA enrollment. | — |
| `grace` | `keyword.control.statement.lazuli` | `keyword` | — | Rotation grace window. | — |
| `hash` | `keyword.control.statement.lazuli` | `keyword` | — | Password hash algorithm. | — |
| `identity` | `keyword.control.statement.lazuli` | `keyword` | — | Identity strategy. | — |
| `mfa` | `keyword.control.statement.lazuli` | `keyword` | — | Multi-factor auth. | — |
| `oauth` | `keyword.control.statement.lazuli` | `keyword` | — | OAuth provider. | — |
| `password` | `keyword.control.statement.lazuli` | `keyword` | — | Password strategy. | — |
| `refresh` | `keyword.control.statement.lazuli` | `keyword` | — | Refresh operation. | — |
| `refresh_ttl` | `keyword.control.statement.lazuli` | `keyword` | — | Refresh-token TTL. | — |
| `rotation` | `keyword.control.statement.lazuli` | `keyword` | — | Token-rotation policy. | — |
| `sessions` | `keyword.control.statement.lazuli` | `keyword` | — | Session settings. | — |
| `theft_detection_action` | `keyword.control.statement.lazuli` | `keyword` | — | Token-theft action. | — |

### Cookie

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `csrf` | `entity.name.function.statement.cookie.lazuli` | `keyword` | — | CSRF cookie settings. | — |
| `default` | `entity.name.function.statement.cookie.lazuli` | `keyword` | — | Default cookie profile. | — |
| `domain` | `entity.name.function.statement.cookie.lazuli` | `keyword` | — | Cookie domain. | — |
| `http_only` | `entity.name.function.statement.cookie.lazuli` | `keyword` | — | HttpOnly flag. | — |
| `max_age` | `entity.name.function.statement.cookie.lazuli` | `keyword` | — | Cookie max-age. | — |
| `path` | `entity.name.function.statement.cookie.lazuli` | `keyword` | — | Cookie path. | — |
| `same_site` | `entity.name.function.statement.cookie.lazuli` | `keyword` | — | SameSite policy. | — |
| `secure` | `entity.name.function.statement.cookie.lazuli` | `keyword` | — | Secure (HTTPS-only) flag. | — |
| `session` | `entity.name.function.statement.cookie.lazuli` | `keyword` | — | Session cookie settings. | — |
| `signed` | `entity.name.function.statement.cookie.lazuli` | `keyword` | — | Signed-cookie flag. | — |

### Headers

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `csp` | `entity.name.function.statement.headers.lazuli` | `keyword` | — | Content-Security-Policy header. | — |
| `hsts` | `entity.name.function.statement.headers.lazuli` | `keyword` | — | Strict-Transport-Security header. | — |
| `include_subdomains` | `entity.name.function.statement.headers.lazuli` | `keyword` | — | HSTS includeSubDomains flag. | — |
| `max_age` | `entity.name.function.statement.headers.lazuli` | `keyword` | — | HSTS max-age directive. | — |
| `permissions_policy` | `entity.name.function.statement.headers.lazuli` | `keyword` | — | Permissions-Policy header. | — |
| `preload` | `entity.name.function.statement.headers.lazuli` | `keyword` | — | HSTS preload flag. | — |
| `referrer_policy` | `entity.name.function.statement.headers.lazuli` | `keyword` | — | Referrer-Policy header. | — |
| `x_content_type_options` | `entity.name.function.statement.headers.lazuli` | `keyword` | — | X-Content-Type-Options header. | — |
| `x_frame_options` | `entity.name.function.statement.headers.lazuli` | `keyword` | — | X-Frame-Options header. | — |

### Limits

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `body_size` | `entity.name.function.statement.limits.lazuli` | `keyword` | — | Max request body size. | — |
| `header_size` | `entity.name.function.statement.limits.lazuli` | `keyword` | — | Max header size. | — |
| `timeout` | `entity.name.function.statement.limits.lazuli` | `keyword` | — | Request timeout limit. | — |
| `upload_size` | `entity.name.function.statement.limits.lazuli` | `keyword` | — | Max upload size. | — |

### Proxy

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `forwarded_host_header` | `entity.name.function.statement.proxy.lazuli` | `keyword` | — | Forwarded-Host header name. | — |
| `forwarded_proto_header` | `entity.name.function.statement.proxy.lazuli` | `keyword` | — | Forwarded-Proto header name. | — |
| `real_ip_header` | `entity.name.function.statement.proxy.lazuli` | `keyword` | — | Real-IP header name. | — |
| `trusted` | `entity.name.function.statement.proxy.lazuli` | `keyword` | — | Trusted proxy CIDRs. | — |

### Encryption

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `algorithm` | `entity.name.function.statement.encryption.lazuli` | `keyword` | — | Encryption algorithm. | — |
| `key` | `entity.name.function.statement.encryption.lazuli` | `keyword` | — | Encryption key reference. | — |
| `rotation` | `entity.name.function.statement.encryption.lazuli` | `keyword` | — | Key-rotation policy. | — |
| `rotation_profile` | `entity.name.function.statement.encryption.lazuli` | `keyword` | — | Key-rotation profile. | — |
| `source` | `entity.name.function.statement.encryption.lazuli` | `keyword` | — | Key-source declaration. | — |

### Locale

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `default` | `entity.name.function.statement.locale.lazuli` | `keyword` | — | Primary BCP-47 locale tag. | — |
| `fallback` | `entity.name.function.statement.locale.lazuli` | `keyword` | — | Fallback locale. | — |
| `supported` | `entity.name.function.statement.locale.lazuli` | `keyword` | — | Supported locales. | — |

### Logging

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `format` | `entity.name.function.statement.logging.lazuli` | `keyword` | — | Log format (json/text). | — |
| `level` | `entity.name.function.statement.logging.lazuli` | `keyword` | — | Log level. | — |
| `redact` | `entity.name.function.statement.logging.lazuli` | `keyword` | — | Redacted fields. | — |
| `sample_rate` | `entity.name.function.statement.logging.lazuli` | `keyword` | — | Log sample rate. | — |

### Tracing

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `exporter` | `entity.name.function.statement.tracing.lazuli` | `keyword` | — | Trace exporter. | — |
| `propagate` | `entity.name.function.statement.tracing.lazuli` | `keyword` | — | Trace-context propagation. | — |
| `sample_rate` | `entity.name.function.statement.tracing.lazuli` | `keyword` | — | Trace sampling rate. | — |

### Runtime

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `healthcheck` | `entity.name.function.statement.runtime.lazuli` | `keyword` | — | Healthcheck endpoint. | — |
| `readiness` | `entity.name.function.statement.runtime.lazuli` | `keyword` | — | Readiness probe. | — |
| `runs` | `entity.name.function.statement.runtime.lazuli` | `keyword` | — | What the unit runs. | — |
| `serves` | `entity.name.function.statement.runtime.lazuli` | `keyword` | — | What the unit serves. | — |
| `unit` | `entity.name.function.statement.runtime.lazuli` | `keyword` | — | Runtime unit (process). | — |

### Deploy

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `checkpoint` | `entity.name.function.statement.deploy.lazuli` | `keyword` | — | Migration checkpoint. | — |
| `destructive_migrations` | `entity.name.function.statement.deploy.lazuli` | `keyword` | — | Destructive-migration policy. | — |
| `environment` | `entity.name.function.statement.deploy.lazuli` | `keyword` | — | Deploy target environment. | — |
| `lock_timeout` | `entity.name.function.statement.deploy.lazuli` | `keyword` | — | Migration lock timeout. | — |
| `migration_lock` | `entity.name.function.statement.deploy.lazuli` | `keyword` | — | Migration advisory lock. | — |
| `migrations` | `entity.name.function.statement.deploy.lazuli` | `keyword` | — | Migration policy. | — |
| `post_migration_hook` | `entity.name.function.statement.deploy.lazuli` | `keyword` | — | Post-migration hook. | — |
| `pre_migration_hook` | `entity.name.function.statement.deploy.lazuli` | `keyword` | — | Pre-migration hook. | — |
| `rollback` | `entity.name.function.statement.deploy.lazuli` | `keyword` | — | Rollback policy. | — |
| `strategy` | `entity.name.function.statement.deploy.lazuli` | `keyword` | — | Deploy strategy (rolling/blue_green/canary). | — |
| `topology` | `entity.name.function.statement.deploy.lazuli` | `keyword` | — | Deployment topology. | — |

### Services

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `consumes` | `entity.name.function.statement.services.lazuli` | `keyword` | — | Events a service consumes. | — |
| `exposes` | `entity.name.function.statement.services.lazuli` | `keyword` | — | Operations a service exposes. | — |
| `owns` | `entity.name.function.statement.services.lazuli` | `keyword` | — | Resources a service owns. | — |
| `publishes` | `entity.name.function.statement.services.lazuli` | `keyword` | — | Events a service publishes. | — |
| `service` | `entity.name.function.statement.services.lazuli` | `keyword` | — | Declares a service. | — |

### Communication

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `async` | `entity.name.function.statement.communication.lazuli` | `keyword` | — | Async communication. | — |
| `external` | `entity.name.function.statement.communication.lazuli` | `keyword` | — | External communication mode. | — |
| `internal` | `entity.name.function.statement.communication.lazuli` | `keyword` | — | Internal communication mode. | — |
| `propagate` | `entity.name.function.statement.communication.lazuli` | `keyword` | — | Context propagation toggle. | — |
| `sync` | `entity.name.function.statement.communication.lazuli` | `keyword` | — | Synchronous channel. | — |
| `timeout` | `entity.name.function.statement.communication.lazuli` | `keyword` | — | Call timeout. | — |

### Env

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `client` | `entity.name.function.statement.env.lazuli` | `keyword` | — | Client-exposed env var. | — |
| `default` | `entity.name.function.statement.env.lazuli` | `keyword` | — | Environment-variable default value. | — |
| `group` | `entity.name.function.statement.env.lazuli` | `keyword` | — | Env-var group. | — |
| `optional` | `entity.name.function.statement.env.lazuli` | `keyword` | — | Optional environment variable. | — |
| `required` | `entity.name.function.statement.env.lazuli` | `keyword` | — | Required environment variable. | — |
| `server` | `entity.name.function.statement.env.lazuli` | `keyword` | — | Server-only env var. | — |

### Integrations

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `adapter` | `entity.name.function.statement.integration.lazuli` | `keyword` | — | Integration adapter. | `PLUGIN-CONTRACT-001` |
| `contract` | `entity.name.function.statement.integration.lazuli` | `keyword` | — | Integration contract reference. | — |
| `credentials` | `entity.name.function.statement.integration.lazuli` | `keyword` | — | Integration credentials. | — |
| `data_classification` | `entity.name.function.statement.integration.lazuli` | `keyword` | — | Data-classification tag. | — |
| `environment` | `entity.name.function.statement.integration.lazuli` | `keyword` | — | Integration environment selector. | — |
| `integration` | `entity.name.label.integration.lazuli` | `keyword` | — | Named integration. | — |
| `operation` | `entity.name.function.statement.integration.lazuli` | `keyword` | — | Integration operation. | — |

### Packs

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `feature` | `entity.name.function.statement.packs.lazuli` | `keyword` | — | Pack-included feature. | — |
| `from` | `entity.name.function.statement.packs.lazuli` | `keyword` | — | Pack source reference. | — |
| `provides` | `entity.name.function.statement.packs.lazuli` | `keyword` | — | Pack-provided capability. | — |

### SecretRotation

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `auto_rollback` | `keyword.control.statement.lazuli` | `keyword` | — | Auto-rollback on failure. | — |
| `cadence` | `keyword.control.statement.lazuli` | `keyword` | — | Rotation cadence. | — |
| `overlap` | `keyword.control.statement.lazuli` | `keyword` | — | Key-overlap window. | — |

### ErrorPage

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `audience` | `entity.name.function.statement.error-page.lazuli` | `keyword` | — | Error-page audience selector. | — |
| `template` | `entity.name.function.statement.error-page.lazuli` | `keyword` | — | Error-page template path. | — |

### Plan

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `features` | `keyword.control.section.lazuli` | `keyword` | — | Plan feature entitlements. | — |
| `limits` | `keyword.control.section.lazuli` | `keyword` | — | Plan limit-entitlement block. | — |
| `then` | `keyword.other.plan.lazuli` | `keyword` | — | Plan upgrade target. | — |
| `trial` | `keyword.control.section.lazuli` | `keyword` | — | Plan trial-window block. | — |
| `unlimited` | `keyword.other.plan.lazuli` | `keyword` | — | Unlimited plan quota. | — |

### Value

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `DELETE` | `constant.language.http-method.lazuli` | `enumMember` | — | — | — |
| `GET` | `constant.language.http-method.lazuli` | `enumMember` | — | — | — |
| `HEAD` | `constant.language.http-method.lazuli` | `enumMember` | — | — | — |
| `OPTIONS` | `constant.language.http-method.lazuli` | `enumMember` | — | — | — |
| `PATCH` | `constant.language.http-method.lazuli` | `enumMember` | — | — | — |
| `POST` | `constant.language.http-method.lazuli` | `enumMember` | — | — | — |
| `PUT` | `constant.language.http-method.lazuli` | `enumMember` | — | — | — |
| `anonymize` | `constant.language.retention-action.lazuli` | `enumMember` | — | — | — |
| `append` | `constant.language.template-strategy.lazuli` | `enumMember` | — | — | — |
| `archive` | `constant.language.retention-action.lazuli` | `enumMember` | — | — | — |
| `asc` | `constant.language.direction.lazuli` | `enumMember` | — | — | — |
| `authenticated` | `constant.language.lazuli` | `enumMember` | — | — | — |
| `blue_green` | `constant.language.deploy.lazuli` | `enumMember` | — | — | — |
| `btree` | `constant.language.lazuli` | `enumMember` | — | — | — |
| `canary` | `constant.language.deploy.lazuli` | `enumMember` | — | — | — |
| `create` | `entity.name.function.statement.policy.lazuli` | `enumMember` | — | — | — |
| `crud` | `constant.language.lazuli` | `enumMember` | — | — | — |
| `csv` | `constant.language.report-format.lazuli` | `enumMember` | — | — | — |
| `custom` | `constant.language.lazuli` | `enumMember` | — | — | — |
| `debug` | `constant.language.log-level.lazuli` | `enumMember` | — | — | — |
| `delete` | `entity.name.function.statement.policy.lazuli` | `enumMember` | — | — | — |
| `desc` | `constant.language.direction.lazuli` | `enumMember` | — | — | — |
| `drop` | `constant.language.dlq.lazuli` | `enumMember` | — | — | — |
| `email` | `constant.language.channel.lazuli` | `enumMember` | — | — | — |
| `emit` | `constant.language.dlq.lazuli` | `enumMember` | — | — | — |
| `false` | `constant.language.boolean.lazuli` | `enumMember` | — | — | — |
| `gin` | `constant.language.lazuli` | `enumMember` | — | — | — |
| `gist` | `constant.language.lazuli` | `enumMember` | — | — | — |
| `hmac` | `constant.language.verify.lazuli` | `enumMember` | — | — | — |
| `http` | `constant.language.transport.lazuli` | `enumMember` | — | — | — |
| `in_app` | `constant.language.channel.lazuli` | `enumMember` | — | — | — |
| `info` | `constant.language.log-level.lazuli` | `enumMember` | — | — | — |
| `json` | `constant.language.log-level.lazuli` | `enumMember` | — | — | — |
| `jwt` | `constant.language.verify.lazuli` | `enumMember` | — | — | — |
| `kms_managed` | `constant.language.rotation.lazuli` | `enumMember` | — | — | — |
| `lax` | `constant.language.cookie.lazuli` | `enumMember` | — | — | — |
| `local` | `constant.language.persistence.lazuli` | `enumMember` | — | — | — |
| `manual` | `constant.language.rotation.lazuli` | `enumMember` | — | — | — |
| `me` | `constant.language.lazuli` | `enumMember` | — | — | — |
| `merge` | `constant.language.template-strategy.lazuli` | `enumMember` | — | — | — |
| `mobile` | `constant.language.lazuli` | `enumMember` | — | — | — |
| `multi` | `constant.language.selection-mode.lazuli` | `enumMember` | — | — | — |
| `nil` | `constant.language.boolean.lazuli` | `enumMember` | — | — | — |
| `none` | `constant.language.cookie.lazuli` | `enumMember` | — | — | — |
| `null` | `constant.language.boolean.lazuli` | `enumMember` | — | — | — |
| `optimistic` | `constant.language.lock.lazuli` | `enumMember` | — | — | — |
| `org` | `constant.language.lazuli` | `enumMember` | — | — | — |
| `pessimistic` | `constant.language.lock.lazuli` | `enumMember` | — | — | — |
| `private` | `constant.language.visibility.lazuli` | `enumMember` | — | — | — |
| `public` | `constant.language.visibility.lazuli` | `enumMember` | — | — | — |
| `push` | `constant.language.channel.lazuli` | `enumMember` | — | — | — |
| `query` | `constant.language.binding-source.lazuli` | `enumMember` | — | — | — |
| `rolling` | `constant.language.deploy.lazuli` | `enumMember` | — | — | — |
| `row_level` | `constant.language.lock.lazuli` | `enumMember` | — | — | — |
| `segmented` | `constant.language.search-mode.lazuli` | `enumMember` | — | — | — |
| `single` | `constant.language.selection-mode.lazuli` | `enumMember` | — | — | — |
| `sms` | `constant.language.channel.lazuli` | `enumMember` | — | — | — |
| `strict` | `constant.language.cookie.lazuli` | `enumMember` | — | — | — |
| `team` | `constant.language.lazuli` | `enumMember` | — | — | — |
| `text` | `constant.language.log-level.lazuli` | `enumMember` | — | — | — |
| `true` | `constant.language.boolean.lazuli` | `enumMember` | — | — | — |
| `unauthenticated` | `constant.language.lazuli` | `enumMember` | — | — | — |
| `update` | `entity.name.function.statement.policy.lazuli` | `enumMember` | — | — | — |
| `warn` | `constant.language.log-level.lazuli` | `enumMember` | — | — | — |
| `web` | `constant.language.lazuli` | `enumMember` | — | — | — |
| `xlsx` | `constant.language.report-format.lazuli` | `enumMember` | — | — | — |

### Modifier

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `accept` | `storage.modifier.lazuli` | `modifier` | — | Accepted MIME types. | — |
| `after` | `storage.modifier.lazuli` | `modifier` | — | Slot-position after. | — |
| `at` | `storage.modifier.lazuli` | `modifier` | — | Position/time connector. | — |
| `attempts` | `storage.modifier.lazuli` | `modifier` | — | Attempts connector. | — |
| `before` | `storage.modifier.lazuli` | `modifier` | — | Slot-position before. | — |
| `by` | `storage.modifier.lazuli` | `modifier` | — | Agent/grouping connector. | — |
| `cascade` | `storage.modifier.lazuli` | `modifier` | — | On-delete cascade. | — |
| `data_subject` | `storage.modifier.lazuli` | `modifier` | — | GDPR data-subject. | — |
| `derived_from` | `storage.modifier.lazuli` | `modifier` | — | Derived-from source. | — |
| `description` | `storage.modifier.lazuli` | `modifier` | — | Description connector. | — |
| `filename` | `storage.modifier.lazuli` | `modifier` | — | Filename connector. | — |
| `free` | `storage.modifier.lazuli` | `modifier` | — | `free text into` sugar. | — |
| `from` | `storage.modifier.lazuli` | `modifier` | — | Source connector. | — |
| `inherits` | `storage.modifier.lazuli` | `modifier` | — | Inherits connector. | — |
| `initial` | `storage.modifier.lazuli` | `modifier` | — | Initial lifecycle state. | — |
| `invariant_handler` | `storage.modifier.lazuli` | `modifier` | — | Invariant handler reference. | — |
| `lifecycle_routes` | `storage.modifier.lazuli` | `modifier` | — | Lifecycle route bindings. | — |
| `lifecycle_stage` | `storage.modifier.lazuli` | `modifier` | — | Lifecycle-stage marker. | — |
| `list` | `storage.modifier.lazuli` | `modifier` | — | `list of` type-constructor head. | — |
| `max_attempts` | `storage.modifier.lazuli` | `modifier` | — | Max-attempts connector. | — |
| `max_recursion` | `storage.modifier.lazuli` | `modifier` | — | Max-recursion connector. | — |
| `mime` | `storage.modifier.lazuli` | `modifier` | — | MIME-type connector. | — |
| `name` | `storage.modifier.lazuli` | `modifier` | — | Name connector. | — |
| `nullify` | `storage.modifier.lazuli` | `modifier` | — | On-delete set-null. | — |
| `on` | `storage.modifier.lazuli` | `modifier` | — | Relation/event connector. | — |
| `opaque` | `storage.modifier.lazuli` | `modifier` | — | Opaque-token flag. | — |
| `optional` | `storage.modifier.lazuli` | `modifier` | — | Field is optional. | — |
| `override` | `storage.modifier.lazuli` | `modifier` | — | Override a base declaration. | — |
| `parent` | `storage.modifier.lazuli` | `modifier` | — | Parent reference. | — |
| `per` | `storage.modifier.lazuli` | `modifier` | — | Rate/quota unit connector. | — |
| `provides` | `storage.modifier.lazuli` | `modifier` | — | Provides connector. | — |
| `raw` | `storage.modifier.lazuli` | `modifier` | — | Raw/unprocessed value. | — |
| `readonly` | `storage.modifier.lazuli` | `modifier` | — | Field is read-only. | — |
| `references` | `storage.modifier.lazuli` | `modifier` | — | Referenced relation connector. | — |
| `required` | `storage.modifier.lazuli` | `modifier` | — | Field is required. | — |
| `resolve` | `storage.modifier.lazuli` | `modifier` | — | Resolve-via connector. | — |
| `restrict` | `storage.modifier.lazuli` | `modifier` | — | On-delete restrict. | — |
| `retain` | `storage.modifier.lazuli` | `modifier` | — | Retention connector. | — |
| `signed_ttl` | `storage.modifier.lazuli` | `modifier` | — | Signed-URL TTL. | — |
| `size` | `storage.modifier.lazuli` | `modifier` | — | Size connector. | — |
| `state` | `storage.modifier.lazuli` | `modifier` | — | Lifecycle state — a member of the lifecycle's closed, named state set (mark exactly one `initial`, zero+ `terminal`). | — |
| `states` | `storage.modifier.lazuli` | `modifier` | — | Lifecycle states block. | — |
| `sync` | `storage.modifier.lazuli` | `modifier` | — | Synchronous mode. | — |
| `terminal` | `storage.modifier.lazuli` | `modifier` | — | Terminal lifecycle state. | — |
| `terminal_result_field` | `storage.modifier.lazuli` | `modifier` | — | Terminal result field. | — |
| `terminal_status_field` | `storage.modifier.lazuli` | `modifier` | — | Terminal status field. | — |
| `to` | `storage.modifier.lazuli` | `modifier` | — | Target connector. | — |
| `transition` | `storage.modifier.lazuli` | `modifier` | — | Lifecycle transition — `from`/`to` bind to members of the closed `state` set. | — |
| `uri_template` | `storage.modifier.lazuli` | `modifier` | — | URI template. | — |
| `uses` | `storage.modifier.lazuli` | `modifier` | — | Uses connector. | — |
| `using` | `storage.modifier.lazuli` | `modifier` | — | Using connector. | — |
| `via` | `storage.modifier.lazuli` | `modifier` | — | Foreign-key column connector. | — |
| `when_denied_route` | `storage.modifier.lazuli` | `modifier` | — | Route when policy denied. | — |

### Expression

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `and` | `keyword.operator.logical.lazuli` | `operator` | — | — | — |
| `behind` | `keyword.operator.plan-and-gate.lazuli` | `operator` | — | — | — |
| `between` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `contains` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `covers_pii` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `eligible_when` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `excludes` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `exists` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `forbid_when` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `guaranteed` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `has` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `in` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `includes` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `is` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `length` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `level` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `list_of` | `support.function.type-constructor.lazuli` | `type` | — | `list_of` collection type constructor. | — |
| `many` | `support.function.type-constructor.lazuli` | `type` | — | `many` relation type constructor. | — |
| `matches` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `max` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `min` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `not` | `keyword.operator.logical.lazuli` | `operator` | — | — | — |
| `only_when` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `or` | `keyword.operator.logical.lazuli` | `operator` | — | — | — |
| `pattern` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `quota` | `keyword.operator.plan-and-gate.lazuli` | `operator` | — | — | — |
| `ref` | `support.function.type-constructor.lazuli` | `type` | — | `ref` reference type constructor. | — |
| `required_when` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `when` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |
| `when_denied` | `keyword.operator.predicate.lazuli` | `operator` | — | — | — |

### Cors

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `allow_credentials` | `entity.name.function.statement.cors.lazuli` | `keyword` | — | Allow credentialed CORS requests. | — |
| `allow_origins` | `entity.name.function.statement.cors.lazuli` | `keyword` | — | Allowed CORS origins. | — |
| `max_age` | `entity.name.function.statement.cors.lazuli` | `keyword` | — | CORS preflight max-age. | — |

### RouteGuard

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `default_policy` | `entity.name.function.statement.route-guard.lazuli` | `keyword` | — | App-level default route policy. | — |
| `on_unauthenticated` | `entity.name.function.statement.route-guard.lazuli` | `keyword` | — | Default redirect when unauthenticated. | — |
| `on_unauthorized` | `entity.name.function.statement.route-guard.lazuli` | `keyword` | — | Default redirect when unauthorized. | — |
| `skeleton` | `entity.name.function.statement.route-guard.lazuli` | `keyword` | — | Default loading skeleton. | — |

### Deprecated

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `replacement` | `entity.name.function.statement.deprecated.lazuli` | `keyword` | — | Replacement reference. | — |
| `since` | `entity.name.function.statement.deprecated.lazuli` | `keyword` | — | Deprecation since-version. | — |
| `sunset` | `entity.name.function.statement.deprecated.lazuli` | `keyword` | — | Sunset date. | — |

## `.lzx` — surface source

### Surface

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `audience` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Audience grouping for surface views. | — |
| `uses` | `keyword.control.statement.lazuli` | `keyword` | — | `uses experience` declaration. | — |

### SurfaceAudience

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `requires` | `keyword.control.statement.lazuli` | `keyword` | — | Audience scope requirement (`requires @scope.X`). | — |

### SurfaceView

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `action` | `keyword.control.statement.lazuli` | `keyword` | — | View action. | — |
| `actions` | `keyword.control.section.lazuli` | `keyword` | — | View action block. | — |
| `back` | `keyword.control.statement.lazuli` | `keyword` | — | Navigate back on success. | — |
| `bulk_actions` | `keyword.control.section.lazuli` | `keyword` | — | Bulk-action block. | — |
| `cells` | `keyword.control.section.lazuli` | `keyword` | — | Cell renderer block. | — |
| `columns` | `keyword.control.section.lazuli` | `keyword` | — | List column projection. | — |
| `create` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | `view create` projection kind. | — |
| `date_range` | `keyword.control.statement.lazuli` | `keyword` | — | Date-range filter primitive. | — |
| `default_policy` | `keyword.control.statement.lazuli` | `keyword` | — | Default route policy. | — |
| `default_unauthenticated_redirect` | `keyword.control.statement.lazuli` | `keyword` | — | Default unauthenticated redirect. | — |
| `default_unauthorized_redirect` | `keyword.control.statement.lazuli` | `keyword` | — | Default unauthorized redirect. | — |
| `detail` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | `view detail` projection kind. | — |
| `drawer` | `keyword.control.section.lazuli` | `keyword` | — | Detail drawer block. | — |
| `error_redact` | `keyword.control.statement.lazuli` | `keyword` | — | Error redaction. | — |
| `error_view` | `keyword.control.statement.lazuli` | `keyword` | — | Error-state view. | — |
| `fields` | `keyword.control.section.lazuli` | `keyword` | — | Form/detail field block. | — |
| `filter` | `keyword.control.statement.lazuli` | `keyword` | — | List filter. | — |
| `filters` | `keyword.control.section.lazuli` | `keyword` | — | List filter block. | — |
| `flash` | `keyword.control.statement.lazuli` | `keyword` | — | Flash message on success. | — |
| `hint` | `keyword.control.statement.lazuli` | `keyword` | — | Field hint text. | — |
| `icon` | `keyword.control.statement.lazuli` | `keyword` | — | Action/tab icon. | — |
| `lanes` | `keyword.control.statement.lazuli` | `keyword` | — | Board lanes. | — |
| `lazy` | `keyword.control.statement.lazuli` | `keyword` | — | Lazy-load marker. | — |
| `loader` | `keyword.control.statement.lazuli` | `keyword` | — | Data loader reference. | — |
| `mutate` | `keyword.control.statement.lazuli` | `keyword` | — | Optimistic mutation. | — |
| `on_change` | `keyword.control.statement.lazuli` | `keyword` | — | On-change handler. | — |
| `on_lifecycle_pending` | `keyword.control.statement.lazuli` | `keyword` | — | Pending-lifecycle handler. | — |
| `on_success` | `keyword.control.section.lazuli` | `keyword` | — | Post-submit success block. | — |
| `on_unauthenticated` | `keyword.control.statement.lazuli` | `keyword` | — | Guard: action when unauthenticated. | — |
| `on_unauthorized` | `keyword.control.statement.lazuli` | `keyword` | — | Guard: action when unauthorized. | — |
| `opens` | `keyword.control.statement.lazuli` | `keyword` | — | Drawer/modal open trigger. | — |
| `pending_view` | `keyword.control.statement.lazuli` | `keyword` | — | Pending/loading view. | — |
| `persist` | `keyword.control.statement.lazuli` | `keyword` | — | Filter/sort persistence. | — |
| `prerender` | `keyword.control.statement.lazuli` | `keyword` | — | Prerender marker. | — |
| `redirect` | `keyword.control.statement.lazuli` | `keyword` | — | Redirect on success. | — |
| `repeatable` | `keyword.control.statement.lazuli` | `keyword` | — | Repeatable input group. | — |
| `replace` | `keyword.control.statement.lazuli` | `keyword` | — | Replace navigation. | — |
| `requires_lifecycle` | `keyword.control.statement.lazuli` | `keyword` | — | Lifecycle-state gate for the view/route. | — |
| `requires_lifecycle_in` | `keyword.control.statement.lazuli` | `keyword` | — | Lifecycle-state-set gate. | — |
| `resume` | `keyword.control.statement.lazuli` | `keyword` | — | Resume-from-pending reference. | — |
| `role_mismatch` | `keyword.control.statement.lazuli` | `keyword` | — | Route guard: per-role mismatch redirect. | — |
| `route` | `keyword.control.statement.lazuli` | `keyword` | — | View route. | — |
| `search` | `keyword.control.statement.lazuli` | `keyword` | — | List search. | — |
| `sections` | `keyword.control.section.lazuli` | `keyword` | — | Detail section block. | — |
| `selection` | `keyword.control.statement.lazuli` | `keyword` | — | Row selection mode. | — |
| `settings` | `keyword.control.section.lazuli` | `keyword` | — | View settings block. | — |
| `sort` | `keyword.control.statement.lazuli` | `keyword` | — | List sort. | — |
| `source` | `keyword.control.statement.lazuli` | `keyword` | — | View data source. | — |
| `step` | `keyword.control.statement.lazuli` | `keyword` | — | A wizard step. | — |
| `submit` | `keyword.control.statement.lazuli` | `keyword` | — | Form submit target. | — |
| `tab` | `keyword.control.statement.lazuli` | `keyword` | — | A single tab. | — |
| `tab_group` | `keyword.control.section.lazuli` | `keyword` | — | Tab-group block. | — |
| `tabs` | `keyword.control.section.lazuli` | `keyword` | — | Tabbed view block. | — |
| `view.board` | `keyword.control.statement.lazuli` | `keyword` | — | Board view primitive (W6). | — |
| `view.inline_table` | `keyword.control.statement.lazuli` | `keyword` | — | Inline-table view primitive (W6). | — |
| `view_mode` | `keyword.control.statement.lazuli` | `keyword` | — | View display mode. | — |
| `wizard` | `keyword.control.section.lazuli` | `keyword` | — | Wizard view block. | — |
| `wizard_steps` | `keyword.control.section.lazuli` | `keyword` | — | Wizard-steps block. | — |

### Extends

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `anchor` | `keyword.control.statement.lazuli` | `keyword` | — | Declares an extensibility anchor. | — |
| `extends` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Extends a sibling feature's anchor. | — |
| `extensible_by` | `keyword.control.statement.lazuli` | `keyword` | — | Declares this view extensible by siblings. | — |
| `platforms` | `keyword.control.statement.lazuli` | `keyword` | — | Target platforms for the extension. | — |
| `slot` | `keyword.control.statement.lazuli` | `keyword` | — | Extension slot position. | — |

## App manifest / orchestration

### TopLevel

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `app` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares an application: targets, urls, runtime, deploy topology. | — |
| `profile` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a deployment/runtime profile. | — |
| `workspace` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares a multi-app workspace root. | — |

## Registry source

### TopLevel

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `registry` | `keyword.control.declaration.structural.lazuli` | `keyword` | — | Declares the shared integration/capability registry. | — |

## `design.lzi` token catalog

### Design

| keyword / literal | scope | semantic token | sigil | hover | produces |
| --- | --- | --- | --- | --- | --- |
| `base` | `keyword.control.section.lazuli` | `keyword` | — | Required default color state. | — |
| `breakpoint` | `keyword.control.section.lazuli` | `keyword` | — | Responsive breakpoint token group. | — |
| `color` | `keyword.control.section.lazuli` | `keyword` | — | Closed design token group for colors. | — |
| `dark` | `keyword.control.section.lazuli` | `keyword` | — | Dark-theme color suffix. | — |
| `duration` | `keyword.control.section.lazuli` | `keyword` | — | Motion duration sub-group. | — |
| `easing` | `keyword.control.section.lazuli` | `keyword` | — | Motion easing sub-group. | — |
| `family` | `keyword.control.section.lazuli` | `keyword` | — | Font-family sub-group. | — |
| `foreground` | `keyword.control.section.lazuli` | `keyword` | — | Foreground color when used as background. | — |
| `line_height` | `keyword.control.section.lazuli` | `keyword` | — | Type line-height field. | — |
| `motion` | `keyword.control.section.lazuli` | `keyword` | — | Transition/animation token group. | — |
| `radius` | `keyword.control.section.lazuli` | `keyword` | — | Border-radius token group. | — |
| `scale` | `keyword.control.section.lazuli` | `keyword` | — | Type-scale sub-group. | — |
| `shadow` | `keyword.control.section.lazuli` | `keyword` | — | Box-shadow elevation token group. | — |
| `space` | `keyword.control.section.lazuli` | `keyword` | — | Spacing scale token group. | — |
| `tracking` | `keyword.control.section.lazuli` | `keyword` | — | Letter-spacing sub-group. | — |
| `typography` | `keyword.control.section.lazuli` | `keyword` | — | Closed design token group for type. | — |
| `weight` | `keyword.control.section.lazuli` | `keyword` | — | Font-weight sub-group. | — |
| `z` | `keyword.control.section.lazuli` | `keyword` | — | Stacking-order token group. | — |
