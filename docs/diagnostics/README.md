# Diagnostics catalog

Index of every rule shipped by `crates/lazuli_doctor`. The doctor is the surface that turns IR invariants into authoring-time feedback; the LSP mounts the same checks for live diagnostics in editors.

This catalog mines each rule's module-header docstring (the canonical source of truth) and pairs it with the on-disk module path so cold readers can jump straight to the rule logic + its `#[cfg(test)] mod tests`. When a rule lacks a docstring the row carries a `documentation gap` tag instead of a one-liner.

- **Anchor crate**: [`crates/lazuli_doctor/src/`](../../crates/lazuli_doctor/src/)
- **CLI entry**: `lazuli doctor` (`crates/lazuli_cli/src/doctor/mod.rs`)
- **Severity rendering**: `crates/lazuli_cli/src/doctor/mod.rs:DoctorSeverity` (`error` / `warning` / `info`).
- **Profile gating**: many rules carry a `strict` / `production` profile pair; see each module header.
- **Inline escape hatch**: `// lazuli-allow: <code> — <reason>` (see `design/helpers.rs::is_allowed_by_escape_comment`).

## Categories

| Category | Module | Rule count | Theme |
|---|---|---|---|
| [Correctness](#correctness) | `correctness/` | 17 | Dangling references, shape mismatches, codegen contract violations |
| [Cross-feature contracts](#cross-feature-contracts) | `cross_feature/` | 3 | Microservices-mode contract gating |
| [Lifecycle](#lifecycle) | `lifecycle/` | 10 | State-machine well-formedness |
| [Poller](#poller) | `poller/` | 12 | Background poller invariants |
| [Encryption](#encryption) | `encryption/` | 6 | `@cap.Encrypted` / `@cap.E2ee` axis enforcement |
| [Domain](#domain) | `domain/` | 4 | `aggregate` / `invariant` / `@slug` |
| [Design tokens](#design-tokens) | `design/` | 11 | Tailwind / inline-style token enforcement |
| [Error vocabulary](#error-vocabulary) | `error_vocab/` | 7 | Typed error-message resolution chain |
| [Vocabulary (Rule Zero)](#vocabulary-rule-zero) | `vocab/` | 36 | Vocabulary fitness — `VOCAB-*`, `MONEY-*`, `@owner_axis`, `conventions`, `rate_limit` |
| [Security](#security) | `security/` | 1 | HTTP-edge config the runtime refuses (`CORS-*`) |

**Total: 106 rules.**

---

## Correctness

Source: [`crates/lazuli_doctor/src/correctness/`](../../crates/lazuli_doctor/src/correctness/). Severity is typically `error` in both strict and production profiles — these rules surface concrete bugs (typo, shape mismatch), not style drift.

| Code | Severity | Anchor | Summary |
|---|---|---|---|
| `API-HANDLER-UNWIRED-001` | error (prototype/strict: warning) | `correctness/api_handler_unwired_001.rs` | A declared `api` whose runtime `Handler` stays nil: codegen emits the `lazuli.Api[I,O]{...}` value + `RegisterApi(&var)` but never sets `Handler` and never bridges the declared `handler @fn.<name>` to it. The runtime mount loop skips registrations with a nil Handler (`HandlerChecker()` false), so the endpoint is never added to the mux → 404 for the whole api surface as-shipped. Fires once per declared `api`; the fix is to wire `<var>.Handler = ...` in app code (e.g. main.go) and call `lazuli.ValidateApiHandlers()` at startup. |
| `CHANNEL-PAYLOAD-001` | error | `correctness/channel_payload_unresolved_001.rs` | Realtime channel `payload <Type>` doesn't resolve to a same-feature `record` or `resource`. |
| `CODEGEN-UNRESOLVED-BINDING-SOURCE-001` | error | `correctness/codegen_unresolved_binding_source_001.rs` | A `creates`/`updates`/`deletes` binding RHS (SET or authored `where`) is a path resolving to none of `{input, ctx, target, route, @fn(), literal, let}`; codegen would silently lower it to a `FromConst("<raw>")` garbage string. |
| `COMMAND-INPUT-SHADOWS-FIELD-001` | error | `correctness/command_input_shadows_field_001.rs` | Typed `command.input` slot shares a name with a `creates`/`updates` resource field but a different `TypeRef`. |
| `COMPOSITE-KEY-CONTRACT-001` | error | `correctness/composite_key_contract_001.rs` | `composite_key { fields ... }` references a name not declared on the resource (or an empty list). |
| `CREATES-EMPTY-BINDINGS-001` | error | `correctness/creates_empty_bindings_001.rs` | A `creates`/`updates` effect binds zero author columns (excluding framework-synthesized id/timestamps/soft-delete) — a degenerate INSERT (all-default row) / no-op UPDATE. Handler-backed and lifecycle-transition commands are skipped. |
| `ENUM-VARIANT-UNDECLARED-001` | error | `correctness/enum_variant_undeclared_001.rs` | A query filter-predicate RHS (`status == publishedd`) or resource field-default enum literal names a variant the target enum never declared; it would silently lower to a `FromConst("<typo>")` literal that never matches. |
| `EVENT-GROUP-VARIANT-TYPE-001` | error | `correctness/event_group_variant_type_001.rs` | `event_group` variant payload field's `TypeRef::UserDefined` doesn't resolve to a scalar / capability / record / enum. |
| `EVENT-OUTBOX-001` | warning | `correctness/event_outbox_001.rs` | Payments-class event (feature name contains `payment` or `billing`) lacks `outbox guaranteed`. |
| `FULL-TEXT-TYPE-001` | error | `correctness/full_text_type_001.rs` | `@full_text` decorator on a non-text field — Postgres `to_tsvector` only tokenizes text columns. |
| `HOOK-TARGET-001` | error | `correctness/hook_target_001.rs` | `hook x: Hook[Foo]` references `Foo` with no matching command/query/job/record/event/resource in the same feature. |
| `MISSING-POLICY-ON-QUERY-001` | warning | `correctness/missing_policy_on_query_001.rs` | Query has neither an explicit policy nor inherits one via `defaults.policy` — runtime default is implicit public. |
| `MUTATION-WITHOUT-READBACK-001` (`@correctness.mutation_without_readback`) | warning | `correctness/mutation_without_readback.rs` | Mutating command with no reachable `query.lookup` / `query.list` over the same resource (front-end cache has nothing to invalidate). |
| `@info.record_column_jsonb` | info (strict-only) | `correctness/record_column_storage.rs` | A **same-feature** record-typed field → JSONB column (the canonical value-object storage). Surfaces codegen intent without reading templates; benign, so info. |
| `@correctness.record_column_cross_feature` | error | `correctness/record_column_storage.rs` | A resource field typed as a `record` **owned by another feature** (qualified `b.Address`, or a bare name resolving only to another feature's record). Codegen embeds a JSONB copy of B's struct instead of a FK relation to feature B — the two copies drift independently. Resolved by bare name across the full feature set (mirrors codegen `name_is_record`), closing the rule's old per-feature blind spot. |
| `RESOURCE-LOCK-CONTRACT-001` | error | `correctness/resource_lock_contract_001.rs` | `lock optimistic` references a missing `version_field` or a non-`Integer` field. |
| `ROUTE-ID-UNUSED-IN-EFFECT-001` (`@correctness.route_id_unused_in_effect`) | error | `correctness/route_id_effect_consistency.rs` | `command.route <name>: <Type>` (no `from ctx.<expr>`) isn't reachable from `CommandInput`; codegen would read zero-valued `id`. |
| `RUNTIME-REACHABLE-STUB-001` | error | `correctness/runtime_reachable_stub_001.rs` | A DSL construct lowers to a runtime path that is a known not-implemented 501 stub: a `target.<field>` source (→ `lazuli.FromTarget` → `sourceTarget` 501) in a command binding / query filter, or a resource `retention ... then archive` (→ `ErrRetentionArchiveNotImplemented`). Compiles + `go build`s but is dead on arrival at the first request. |
| `@correctness.migration_out_of_sync` | warning | `correctness/schema_migration_present.rs` | IR resource columns drift from the highest-numbered emitted `dist/go/migrations/NNN_<feature>_<resource>*.sql`. |
| `RUNTIME-EMITTED-TABLE-MIGRATION-001` | error (prototype: info) | `correctness/runtime_emitted_table_migration_001.rs` | A framework-synthesized table the runtime WRITES (`audit_log`, `lazuli_audit`, `lazuli_outbox`) — gated on the activating construct (`audit` / `outbox guaranteed`) — has no `CREATE TABLE` migration under `dist/go/migrations/`; the first request that hits the path 500s with `relation "<table>" does not exist`. Generalizes the `audit_log` point-fix to the whole synthesized-table set. |
| `WEBHOOK-EMIT-PREDICATE-FIELD-001` | error | `correctness/webhook_emit_predicate_field_001.rs` | Webhook `emits ... when <path> = ...` path doesn't resolve against the webhook's payload contract. |

## Cross-feature contracts

Source: [`crates/lazuli_doctor/src/cross_feature/`](../../crates/lazuli_doctor/src/cross_feature/). All rules gated on `architecture mode microservices` — `monolith` / `modular_monolith` capsules see no findings.

| Code | Severity | Anchor | Summary |
|---|---|---|---|
| `CROSS-FEATURE-CONTRACT-MISSING-001` | error | `cross_feature/contract_missing_001.rs` | Cross-feature reference resolves to an origin symbol without `public contract <Symbol> as v<N>`. |
| `CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001` | error | `cross_feature/version_drift_001.rs` | Consumer `uses <feature> version v<N>` pinned to a different version than the origin currently publishes. |
| `CROSS-FEATURE-WORKFLOW-SPAN-001` | warning | `cross_feature/workflow_span_001.rs` | Workflow transitions touch resources owned by multiple features. |

## Lifecycle

Source: [`crates/lazuli_doctor/src/lifecycle/`](../../crates/lazuli_doctor/src/lifecycle/). Per `docs/proposals/lifecycle-vocab.md` §5.

| Code | Severity | Anchor | Summary |
|---|---|---|---|
| `LIFECYCLE-ENUM-DUPLICATE` | error | `lifecycle/enum_duplicate.rs` | `lifecycle.generated_enum` collides with an authored `enum <Name>` in the same feature. |
| `LIFECYCLE-FIELD-DOUBLE-DECLARED` | error | `lifecycle/field_double_declared.rs` | `lifecycle <field>` names a discriminator that's also declared in the resource's `fields` list. |
| `LIFECYCLE-INVARIANT-PARAM-UNRESOLVED` | error | `lifecycle/invariant_param_unresolved.rs` | `invariant single <state> per <scope_field>` references an unknown state or scope field. |
| `LIFECYCLE-NO-INITIAL-STATE` | error | `lifecycle/no_initial_state.rs` | Lifecycle declares states but none are marked `initial`. |
| `LIFECYCLE-STATE-DUPLICATE` | error | `lifecycle/state_duplicate.rs` | Two or more `state` entries with the same name in one lifecycle. |
| `LIFECYCLE-STATE-SET-UNDECLARED-001` | error | `lifecycle/state_set_undeclared_001.rs` | Lifecycle/transition machine carries transitions but declares no closed `state` set (the "enum-by-command" shape). |
| `LIFECYCLE-TERMINAL-HAS-OUTGOING-TRANSITION` | error | `lifecycle/terminal_has_outgoing.rs` | `terminal` state used as a transition source. |
| `LIFECYCLE-TIMESTAMP-TYPE` | error | `lifecycle/timestamp_type.rs` | `transition.timestamps <field>` names an existing field whose type is not `DateTime`. |
| `LIFECYCLE-TRANSITION-FROM-UNDECLARED` | error | `lifecycle/transition_from_undeclared.rs` | Transition `from` references a state not declared in the lifecycle. |
| `LIFECYCLE-TRANSITION-TO-UNDECLARED` | error | `lifecycle/transition_to_undeclared.rs` | Transition `to` references a state not declared in the lifecycle. |
| `LIFECYCLE-UNREACHABLE-STATE` | warning (strict) / error (production) | `lifecycle/unreachable_state.rs` | Non-initial state with no incoming transition. |

### Command-triggered transitions

These six codes validate a command's `triggers transition` binding. They live in
the **analyzer** (not the doctor), at
[`crates/lazuli_analyzer/src/checks/lifecycle_transition/mod.rs`](../../crates/lazuli_analyzer/src/checks/lifecycle_transition/mod.rs),
so they fire during `lazuli check` rather than as `lazuli_doctor/src/lifecycle/`
rules. See [lifecycle-transitions.md](../lifecycle-transitions.md).

| Code | Summary |
|---|---|
| `LIFECYCLE-TRANSITION-001` | Command references a transition name that does not exist on the target resource lifecycle. The message lists the declared transitions to pick from. |
| `LIFECYCLE-TRANSITION-002` | Command has no single lifecycle-bearing target resource for the transition binding. |
| `LIFECYCLE-TRANSITION-003` | Command binds a transition from a lifecycle on a different resource than the command updates. |
| `LIFECYCLE-TRANSITION-004` | Transition chain is not contiguous; one transition's `to` does not match the next transition's `from`. |
| `LIFECYCLE-TRANSITION-005` | Command body writes the lifecycle discriminator column directly while also using `triggers transition`. |
| `LIFECYCLE-TRANSITION-006` | Transition binding crosses an unsupported feature boundary. |

## Poller

Source: [`crates/lazuli_doctor/src/poller/`](../../crates/lazuli_doctor/src/poller/). Per `docs/proposals/poller-vocab.md` §5.

| Code | Severity | Anchor | Summary |
|---|---|---|---|
| `POLLER-CURSOR-FIELD-TYPE-001` | error | `poller/cursor_field_type_001.rs` | `cursor.eligible_when` field is not `DateTime` — pollers compare against `ctx.now()`. |
| `POLLER-CURSOR-MISSING-001` | error | `poller/cursor_missing_001.rs` | Poller source resource lacks one or more cursor fields declared on the cursor block. |
| `POLLER-DUAL-SCHEDULER-001` | error | `poller/dual_scheduler_001.rs` | Feature declares both a `poller` and a `job trigger schedule` whose handler walks the same source resource. |
| `POLLER-EXPONENTIAL-NO-CAP-001` | warning | `poller/exponential_no_cap_001.rs` | `backoff exponential base <d>` declared without a paired `cap <d>` — schedule grows forever. |
| `POLLER-HANDLER-ORPHAN-001` | error | `poller/handler_orphan_001.rs` | `poller resolve via @fn.<name>` references a handler not declared under feature `extensions`. |
| `POLLER-IDEMPOTENCY-ATTEMPTS-MISSING-001` | error | `poller/idempotency_attempts_001.rs` | `poller idempotency by` omits `row.attempts`. |
| `POLLER-MAX-RETRIES-UNBOUNDED-001` | error | `poller/max_retries_unbounded_001.rs` | `poller retry max_attempts` is 0 or above the 1000-sanity cap. |
| `POLLER-NO-TERMINAL-001` | error | `poller/no_terminal_001.rs` | `poller states` list has no `terminal` entry. |
| `POLLER-QUIRK-CATALOG-MISMATCH-001` | error | `poller/quirk_catalog_001.rs` | `poller retry_quirk` kind is outside the v0.1 closed catalog. |
| `POLLER-TERMINAL-FIELD-ENUM-001` | error | `poller/terminal_field_enum_001.rs` | `terminal_status_field` must reference an enum-typed source resource field. |
| `POLLER-TERMINAL-NO-EMIT-001` | warning | `poller/terminal_no_emit_001.rs` | Poller has terminal states but emits no events on resolution. |
| `POLLER-TICK-TOO-FAST-001` | warning | `poller/tick_too_fast_001.rs` | `poller tick every <duration>` below 5 seconds — risks hammering the database. |

## Encryption

Source: [`crates/lazuli_doctor/src/encryption/`](../../crates/lazuli_doctor/src/encryption/). Per `docs/proposals/encryption-vocab.md` §Doctor diagnostics.

| Code | Severity | Anchor | Summary |
|---|---|---|---|
| `ENC-E2EE-EVENT-001` | error | `encryption/e2ee_event.rs` | Event payload field carries `@cap.E2ee(...)` — consumers would see opaque ciphertext. |
| `ENC-KEY-MISSING-001` | error | `encryption/key_missing.rs` | Field declares `@cap.Encrypted(key:@key.<scope>)` / `@cap.E2ee(...)` but the app has no matching `encryption.key @key.<scope>` binding. |
| `ENC-ROTATION-001` | warning | `encryption/rotation.rs` | `encryption.key` for `@key.tenant` / `@key.user` / `@key.record` lacks an explicit `rotation` strategy. (Placeholder — see module doc for the parser-distinction caveat.) |
| `ENC-SOURCE-ENV-001` | error | `encryption/source_env.rs` | `encryption.key` references `env.<NAME>` but the env-var schema has no matching entry. |
| `ENC-TEMPLATE-AXIS-001` | error | `encryption/template_axis.rs` | `encryption.key` source template missing the required axis brace or carries an axis the scope forbids (e.g. `@key.app` with `{tenant_id}`). |
| `ENC-TENANCY-001` | warning | `encryption/tenancy.rs` | Feature uses `@cap.Encrypted(key:@key.tenant)` / `@cap.E2ee(...)` but `defaults.tenancy` is not `Org` or another tenant-bearing axis. |

## Domain

Source: [`crates/lazuli_doctor/src/domain/`](../../crates/lazuli_doctor/src/domain/). Roadmap §1.7 — ships alongside the `aggregate` / `invariant` / `@slug` primitives.

| Code | Severity | Anchor | Summary |
|---|---|---|---|
| `aggregate-contains-unknown` | error | `domain/aggregate_contains_unknown.rs` | `aggregate <Name> contains` lists a resource not declared in the same feature. |
| `aggregate-root-unknown` | error | `domain/aggregate_root_unknown.rs` | `aggregate <Name> root <Resource>` resolves to an unknown resource. |
| `invariant-predicate-invalid` | error | `domain/invariant_predicate_invalid.rs` | Invariant `when <pred>` lhs/rhs path references a field not present on the scoping resource (or aggregate root). |
| `slug-uniqueness-implicit` | warning | `domain/slug_uniqueness_implicit.rs` | `@slug` field did not also declare `unique` — codegen still emits the unique index, but the contract should be explicit. |

## Design tokens

Source: [`crates/lazuli_doctor/src/design/`](../../crates/lazuli_doctor/src/design/). Per `docs/proposals/design-tokens.md` §6 + `design-tokens-custom.md` §4. All rules suppress when `dist/ts-web/design/allowlist.json` is missing (no `design.lzi` authored yet). Inline escape hatch: `// lazuli-allow: <code> — <reason>`.

| Code | Severity | Anchor | Summary |
|---|---|---|---|
| `design-token-undefined` | warning (strict) / error (production) | `design/token_undefined.rs` | Tailwind class (e.g. `bg-purple-500`) not in the `design.lzi` color group via `allowlist.json`. |
| `design-token-hex-leak` | warning (strict) / error (production) | `design/hex_leak.rs` | Hex literal in `.tsx` style prop (`"#7c3aed"`) or arbitrary Tailwind value (`bg-[#fff]`). |
| `design-token-px-leak` | warning | `design/px_leak.rs` | Non-zero `px` / `rem` / `em` literal inside `style={{ ... }}`. |
| `design-token-fontfamily-leak` | warning (strict) / error (production) | `design/fontfamily_leak.rs` | Inline `fontFamily: "..."` whose value isn't a declared `typography.family` token. |
| `design-token-shadow-leak` | warning | `design/shadow_leak.rs` | Inline `boxShadow: "..."` literal — author should declare a `shadow` token instead. |
| `design-token-unused` | info | `design/hygiene.rs` | Catalog hygiene (§6.2) — token declared but never referenced. Placeholder until `catalog.json` ships. |
| `design-token-duplicate-value` | info | `design/hygiene.rs` | Two color tokens with the same hex (§6.2). Placeholder. |
| `design-token-missing-dark` | info | `design/hygiene.rs` | Color token without a dark-mode override (§6.2). Placeholder. |
| `DESIGN-CUSTOM-DUPLICATE` | error | `design/custom.rs` | `custom` 9th-group token collides with an existing `color` token name. |
| `DESIGN-CUSTOM-INVALID-VALUE` | error | `design/custom.rs` | `custom` token value isn't a valid hex literal (defensive re-check beside the analyzer). |
| `DESIGN-CUSTOM-RESERVED-NAME` | error | `design/custom.rs` | `custom` name collides with the closed Shadcn-semantic vocabulary (13 reserved names; §4 of `design-tokens-custom.md`). |

## Error vocabulary

Source: [`crates/lazuli_doctor/src/error_vocab/`](../../crates/lazuli_doctor/src/error_vocab/). Per `docs/proposals/ir-error-messages-vocab.md` §6 §11. Closed catalogs live verbatim in `error_vocab.rs` (`FRAMEWORK_ERROR_CODES` — 12 codes; `EXPOSE_4XX_FIELDS` — 4; `EXPOSE_5XX_FIELDS` — 2).

| Code | Severity | Anchor | Summary |
|---|---|---|---|
| `ERR-VOCAB-001` | warning | `error_vocab/error_vocab.rs` (`check_policies_no_when_denied`) | Feature has `policies` block but no `when_denied` anywhere and no `errors policy_denied` catch-all. |
| `ERR-VOCAB-002` | error | `error_vocab/error_vocab.rs` (`check_translation_resolves`) | `@translation.<key>` doesn't resolve in the feature (including cross-feature `uses`). |
| `ERR-VOCAB-003` | warning | `error_vocab/error_vocab.rs` (`check_per_command_policy_override`) | Per-command policy without override and no feature catch-all. |
| `ERR-VOCAB-CODE-UNKNOWN` | error | `error_vocab/error_vocab.rs` (`check_error_code_in_catalog`) | `errors <code>` outside the closed catalog of 12 framework codes. |
| `ERR-VOCAB-EXPOSE-UNKNOWN` | error | `error_vocab/error_vocab.rs` (`check_expose_field_in_catalog`) | Unknown field in `expose client 4xx \| 5xx`. |
| `ERR-VOCAB-WHEN-DENIED-NO-POLICY` | error | `error_vocab/error_vocab.rs` (`check_when_denied_has_policy`) | `when_denied` on command with no `policy`. |
| `ERR-VOCAB-EXPOSE-5XX-MESSAGE` | error | `error_vocab/error_vocab.rs` (`check_expose_5xx_no_message`) | `message` listed in `expose client 5xx` (proposal rejects this — see §2.C). |

## Vocabulary (Rule Zero)

Source: [`crates/lazuli_doctor/src/vocab/`](../../crates/lazuli_doctor/src/vocab/). Vocabulary-fitness lints — Rule Zero ("Vocabulary Over Mechanism"). Reference docs: `docs/proposals/doctor-vocabulary-lints.md`, the operational next-checklist (lazuli-ops), plus per-rule proposal links in each module header.

### Vocabulary fitness — core `VOCAB-*` catalog

| Code | Severity | Anchor | Summary |
|---|---|---|---|
| `VOCAB-AUDIT-001` | warning (strict) / error (production) | `vocab/vocab_audit_001.rs` | Mutating command without an `audit default` / `audit <fields>` / `audit none` child. |
| `VOCAB-AUDIT-002` | warning | `vocab/vocab_audit_002.rs` | Handler-only command on capability-tagged fields lacks `audit`. |
| `VOCAB-CAP-MISSING-001` | error (strict) / warning (prototype) | `vocab/vocab_cap_missing_001.rs` | `@pii.*` field without a `@cap.Hashed` / `@cap.Encrypted` / `@cap.E2ee` / `@cap.Token` capability. |
| `VOCAB-DERIVED-READ-001` | warning | `vocab/vocab_derived_read_001.rs` | Optional field with no `default` / `@cap.*` / `derived from` and no write site — likely computed every read. |
| `VOCAB-EVENT-ORPHAN-001` | warning | `vocab/vocab_event_orphan_001.rs` | Feature-level event declaration that no command ever `emits`. |
| `VOCAB-EVENT-PAYLOAD-001` | warning | `vocab/vocab_event_payload_001.rs` | `emits <event>` where the event has neither `payload <Type>` nor `payload none`. |
| `VOCAB-EVENT-PRODUCER-001` | warning | `vocab/vocab_event_producer_001.rs` | Mutating command without IR-visible `emits` (handler-side emission would drift from audit + projection tooling). |
| `VOCAB-GRAMMAR-FORM-001` | warning (strict) / error (production) | `vocab/vocab_grammar_form_001.rs` | Deprecated `.lzi` forms: `validates resource`, `validates field`, inline `previously`, `validate "./path.go"`. |
| `VOCAB-HANDLER-HEAVY-001` | warning | `vocab/vocab_handler_heavy_001.rs` | Feature ≥3 commands and ≥70% route through `@fn.<name>` handlers instead of declarative `creates`/`updates`. |
| `VOCAB-JSON-TYPED-001` | warning | `vocab/vocab_json_typed_001.rs` | Resource carries a `JSON` field while the same feature declares a sibling enum no typed slot references. |
| `VOCAB-LIFECYCLE-001` | warning (strict) / error (production) | `vocab/vocab_lifecycle_001.rs` | Status-like enum + ≥3 commands advancing it through named states — refactor to a `lifecycle` block. |
| `VOCAB-MONEY-MULTI-CURRENCY-001` | warning | `vocab/vocab_money_multi_currency_001.rs` | Resource with 2+ `Money` fields and no per-field `<money>_currency: Currency` opt-out. |
| `VOCAB-MONEY-SHAPE-001` | warning | `vocab/money_field_shape_001.rs` | Money modelled the hand-rolled way (`_cents:Integer`+`_currency:Text` pair, bare money-named `Decimal`, or string-tagged money with no `<field>_currency` sibling) instead of the first-class `Money` type. |
| `VOCAB-RESOURCE-WIDE-CLUSTER-001` | warning (strict) / info (production) | `vocab/vocab_resource_wide_cluster_001.rs` | Resource >K post-filter fields and ≥M sharing a leading/trailing snake-case token — candidate for record extraction. |
| `VOCAB-RUNTIME-REINVENTED-001` | warning | `vocab/runtime_reinvented_001.rs` | `@fn` handler reinvents a runtime/language primitive (argon2 hashing, opaque-token mint/hash, `UPDATE ... status IN (...) RowsAffected==0` lifecycle shape, `^#?[0-9A-Fa-f]{6}$` HexColor) the runtime already owns — names the equivalent. Precision-guarded against vendor `crypto/hmac`. The reinvention audit oracle (spec 0024). |
| `VOCAB-SHADOW-RECORD-001` | warning | `vocab/vocab_shadow_record_001.rs` | Two declaration sites in one feature share ≥N `(name, type_ref)` pairs and ≥50% intersection — candidate for shared record. |
| `VOCAB-TESTS-MISSING-001` | warning | `vocab/vocab_tests_missing_001.rs` | Feature with resources / commands but zero inline `test` blocks anywhere. |
| `VOCAB-UNION-001` | warning | `vocab/vocab_union_001.rs` | Enum-typed "kind" axis + optional fields prefixed by variant names — refactor to discriminated `union`. |
| `VOCAB-UNION-002` | warning (strict) / error (production) | `vocab/vocab_union_002.rs` | Polymorphic FK shape (`target: <Enum> + target_id: ID`) — refactor to typed FKs per variant. |
| `VOCAB-LIFECYCLE-001` (legacy alias `vocab_lifecycle_001`) | see above | `vocab/vocab_lifecycle_001.rs` | (See above.) |

### Money (`MONEY-*`)

| Code | Severity | Anchor | Summary |
|---|---|---|---|
| `MONEY-ARITHMETIC-001` | error | `vocab/money_arithmetic_001.rs` | `Money(X) + Decimal` or `Money(X) * Money(Y)` in a `derived_from` string — adds bare numbers to a currency-tagged amount or breaks dimensional analysis. |
| `MONEY-COMPARE-001` | error | `vocab/money_compare_001.rs` | Comparison spans two `Money` fields with different ISO 4217 currencies. |

### Conventions (`conventions [crud]` / `conventions [me]`)

| Code | Severity | Anchor | Summary |
|---|---|---|---|
| `conventions_unknown` | error | `vocab/conventions.rs` | `conventions [<name>]` references an identifier outside the closed catalog. |
| `crud_synth_signature_mismatch` | error | `vocab/conventions.rs` | Author override of a `crud` synth name diverges from the canonical signature. |
| `crud_synth_policy_not_found` | error | `vocab/conventions.rs` | Resource opts into `conventions [crud]` but the feature has no `authenticated` policy. |
| `crud_synth_no_required_fields` | error | `vocab/conventions.rs` | `crud` synth's `create_<r>` would have an empty input (every required field is auto). |
| `me_synth_no_actor_resolution` | error | `vocab/conventions.rs` | `conventions [me]` resource has no owner axis to filter on. |
| `me_synth_signature_mismatch` | error | `vocab/conventions.rs` | Author override of a `me` synth name diverges from the canonical signature. |

### `@owner_axis`

| Code | Severity | Anchor | Summary |
|---|---|---|---|
| `owner_axis_on_non_fk` | error | `vocab/owner_axis.rs` | `@owner_axis` applied to a primitive field (non-FK). |
| `owner_axis_unknown_through` | error | `vocab/owner_axis.rs` | `@owner_axis(through: <col>)` references a column that doesn't exist on the FK target. |
| `owner_axis_through_not_user_keyed` | warning | `vocab/owner_axis.rs` | `@owner_axis(through: <col>)` target lacks `user: User required unique` — ownership chain can't bottom out at an actor. |
| `owner_axis_collides_with_unique_user` | warning | `vocab/owner_axis.rs` | Resource has both `user: User required unique` AND `@owner_axis(through: <col>)`. |

### Rate-limit (env-qualified)

| Code | Severity | Anchor | Summary |
|---|---|---|---|
| `rate_limit_unknown_env` | warning | `vocab/rate_limit.rs` | `in <env>` where `<env>` is not in the closed env catalog. |
| `rate_limit_invalid_spec` | error | `vocab/rate_limit.rs` | Malformed spec literal that doesn't parse as `N per UNIT per scope`. |
| `rate_limit_duplicate_env` | error | `vocab/rate_limit.rs` | Same env appears in two qualified entries for one command. |
| `rate_limit_duplicate_default` | error | `vocab/rate_limit.rs` | Two unqualified `rate_limit` lines on one command. |
| `rate_limit_no_default_with_qualifications` | warning | `vocab/rate_limit.rs` | Only env-qualified lines, no unqualified default. |

### Shared helpers (not user-facing diagnostics)

| File | Purpose |
|---|---|
| `vocab/conventions.rs` shared constants | Closed catalog of `conventions [<name>]` identifiers. |
| `vocab/universal_columns.rs` | Universal-column filter + view-projection helpers shared by `VOCAB-SHADOW-RECORD-001` and `VOCAB-RESOURCE-WIDE-CLUSTER-001`. |
| `design/helpers.rs` | `Allowlist` reader + `walk_tsx_files` + `iter_class_strings` / `iter_style_block_segments` + escape-comment matcher. |
| `encryption/test_support.rs` | `#[cfg(test)]` fixture helpers for the 6 encryption rules. |

## Security

Source: [`crates/lazuli_doctor/src/security/`](../../crates/lazuli_doctor/src/security/). HTTP-edge configuration that ships a footgun the runtime refuses (or that the CORS / cookie / CSRF spec forbids). The `CORS-*` / `SECURITY-*` / `AUTH-*` / `SESSION-*` prefix family routes to `RuleCategory::Security`.

| Code | Severity | Anchor | Summary |
|---|---|---|---|
| `CORS-WILDCARD-PROD-001` | error | `security/cors_wildcard_prod_001.rs` | A wildcard (`"*"`) `cors allow_origins` in a production-targeted environment (env name not in `{dev, local}`). Compile-time companion to the runtime `Mux()` boot refusal (`ErrCSRFWildcardProd`): the runtime panics at boot when `LAZULI_ENV` is production AND the active CORS allowlist contains `"*"`. Error for production envs; downgraded to a warning for `dev`/`local` (mirrors the runtime's dev `slog.Warn`). No-op when no `cors` contract is declared. |

## Fixtures

The canonical `.lzi` fixtures live under [`examples/full-capsule/`](../../examples/full-capsule/) and adjacent directories. Each rule's `#[cfg(test)] mod tests` block carries its own minimal positive/negative fixture inline — see the rule's `*.rs` file for the literal IR construction. The kitchen-sink capsule [`examples/full-capsule/full-capsule.lzi`](../../examples/full-capsule/full-capsule.lzi) is the integration anchor for "does this rule fire on a realistic capsule?" coverage.

## Documentation gaps

Cycle 3 DOC1 closed the five thin-coverage rows identified by DC2. These rules now carry dedicated per-rule module headers with rule statement, severity profile, fixture example, proposal/history anchor, and diagnostic code:

- `vocab/vocab_grammar_form_001.rs`
- `vocab/vocab_event_payload_001.rs`
- `vocab/vocab_json_typed_001.rs`
- `correctness/missing_policy_on_query_001.rs`
- `correctness/composite_key_contract_001.rs`

No currently cataloged documentation gaps remain. When mining for an authoring helper or LSP hover surface, prefer the per-rule `.rs` file's `//!` block — those are the canonical source of truth, updated alongside the logic.

## Maintenance

- **New rule lands**: add a row in the appropriate section, anchor it to the `.rs` file, surface the severity + one-liner from the module header.
- **Rule retired**: strike-through the row and link the retiring proposal; don't delete (the row remains a discoverable breadcrumb for cold readers).
- **Catalog rendering**: this file is the human-facing index; the machine-readable surface stays in [`docs/error-contract.md`](../error-contract.md) (codes + severities for codegen + CLI rendering).
