# Lazuli Error Contract

Generated targets should expose consistent errors across Go, React, CLI, LSP, MCP, and tests.

## Runtime Error Kinds

| Kind | Typical HTTP | Source |
|------|--------------|--------|
| `validation_failed` | 422 | input/field validation |
| `policy_denied` | 403 | `policy` check |
| `rule_denied` | 409 | `rule deny ... when ...` |
| `not_found` | 404 | `key` lookup inside effective scope |
| `conflict` | 409 | unique constraint, workflow invalid transition |
| `sql_query_failed` | 500 | `query.sql` execution |
| `extension_failed` | 500 | custom extension error |
| `webhook_rejected` | 401/400 | signature/idempotency failure |
| `job_failed` | 500 | background job failure |

## Error Shape

Generated APIs should return structured errors:

```json
{
  "kind": "rule_denied",
  "code": "customer.reassign.archived",
  "message": "Cannot reassign an archived customer",
  "feature": "customer",
  "path": "domain.rule.archived_customers_cannot_be_reassigned",
  "source": "features/customer/customer.lzi:42"
}
```

## Source Mapping

Every generated error should point back to `.lzi` source where possible:

```txt
feature customer
path domain.rule."archived customers cannot be reassigned"
generated backend/customer/commands.go:118
```

Generated code errors should not force agents to debug generated files first. The source map points back to the capsule.

## Surface Behavior

React surfaces should receive the same error kind/code as the backend. UI adapters can map:

- `validation_failed` to field errors
- `policy_denied` to forbidden state
- `rule_denied` to business message
- `not_found` to empty/not-found panel
- `conflict` to retry or refresh prompt

The message in a `rule` is user-facing unless an adapter overrides localization.

## CLI inspect symbol-mode (`lazuli inspect <qualified-symbol>`)

Per the `lsp-symbol-origin` proposal (operational archive) §5.4. When the inspect CLI is invoked in symbol-mode (per §5.3 lexical disambiguation), lookup failures are emitted as a soft error envelope on stdout with `exit 0`. Hard errors (parse failure on the module, IO error reading `.lzi` files) exit non-zero with stderr context.

Symbol-mode error codes:

| Code | Trigger | Resolution |
|---|---|---|
| `SYMBOL_NOT_FOUND` | The qualified symbol does not exist in the project's `SymbolOriginIndex`. | Confirm the spelling; if the symbol lives in another feature, qualify the lookup via `<feature>.<name>`. |
| `AMBIGUOUS_SYMBOL` | A bare-name lookup matches symbols in 2+ features. | Qualify the lookup. The `candidates` array in the envelope lists every match. |

Envelope shape:

```json
{
  "error": {
    "code": "SYMBOL_NOT_FOUND",
    "message": "no declaration named `Banana` in any feature of this project"
  }
}
```

```json
{
  "error": {
    "code": "AMBIGUOUS_SYMBOL",
    "message": "`Gender` is declared in multiple features; qualify the lookup as `<feature>.Gender`",
    "candidates": ["account.Gender", "host.Gender"]
  }
}
```

Soft-error exit policy matches `lazuli doctor` — findings are data, not failures. Scripts consuming the CLI parse the envelope and decide.

## Cross-feature contract diagnostics

Per the `cross-feature-contracts` proposal (operational archive) §7. All three rules are gated on `architecture mode microservices`; capsules under `monolith` / `modular_monolith` see no findings from these rules. Module placement: `crates/lazuli_doctor/src/cross_feature/`.

| Code | Severity | Trigger | Resolution |
|---|---|---|---|
| `CROSS-FEATURE-CONTRACT-MISSING-001` | error | A cross-feature reference (type/enum/record in field decl, query return, command input, event payload, or identity reference) resolves to a symbol in the origin feature that lacks `public contract`. | Add `public contract <Symbol> as v1` adjacent to the symbol's declaration in the origin feature. |
| `CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001` | error | Consumer feature pins `uses <origin> version v<N>` and references `<origin>.<Symbol>` whose origin currently publishes `public contract <Symbol> as v<M>` with `M ≠ N`. | Migrate the consumer to v\<M\> (and adjust call sites if the bump is breaking) or republish the origin at v\<N\> for compatibility. |
| `CROSS-FEATURE-WORKFLOW-SPAN-001` | warning | Workflow transitions touch resources owned by 2+ features; under `microservices` this implies distributed-aggregate / saga semantics. | Declare which feature hosts the saga coordinator and treat cross-feature steps as inter-service calls (no new keyword needed; the analyzer flags the span as a refactoring hint). |

## query.compose diagnostics

Per `docs/proposals/ir-composite-read-primitive-2026-05-29.md` §7 (cell W3). These rules read the resolved `ir::ComposeQuery` (and, for `COMPOSE-JOIN-PATH-001`, the Module graph) off the lowered feature — never the SQL text — so they are exact, not heuristic. Module placement: `crates/lazuli_doctor/src/compose/` (the eight `COMPOSE-*` codes); the complementary `VOCAB-SQL-COMPOSABLE-001` is homed under `crates/lazuli_doctor/src/vocab/` because it nudges from the `query.sql` side.

| Code | Severity | Trigger | Resolution |
|---|---|---|---|
| `COMPOSE-JOIN-PATH-001` | error | A `join <fk.path>` segment is not a declared FK relation on the prior resource, resolved against the Module graph (the cross-feature hops W2 trusts/defers). | Fix the FK-path segment to a declared relation; a `query.compose` JOIN is an FK path, never an authored ON-clause. |
| `COMPOSE-PROJECTION-SOURCE-001` | error | A projection `self.<col>` / `<alias>.<col>` names a column that does not exist on its resolved root/joined resource. | Project a real source column so the generated record cannot drift from the read. |
| `COMPOSE-SUBSELECT-RELATION-001` | error | A subselect `related_by <fk.path>` does not correlate its child resource back to root (wrong FK segment, or lands on the wrong resource). | Correct the correlation FK so it walks the child resource back to the compose root. |
| `COMPOSE-SUBSELECT-PREDICATE-FIELD-001` | error | A subselect `where`/`filter` references a field absent on the subselect resource, OR uses a forbidden ordered operator (`<`/`>`). Also covers the field-typing of `in [...]` literal-set members' LHS. | Reference a real field with the closed predicate language; move ordered comparisons to `query.sql`. |
| `COMPOSE-SCOPE-UNGROUNDED-001` | error | A tenant-bearing root has its inherited tenant scope overridden (`scope_origin == Overridden`) with no accompanying `policy` — a cross-tenant leak. | Remove `scope override` to inherit tenancy, or gate the override with `policy @policy.<name>` + a `reason`. |
| `COMPOSE-SUBSELECT-CATALOG-001` | error | A `count`/`exists`/`aggregate` sub-select declares `order` — the grouped/ordered-sub-list fingerprint (`order` is valid only on `latest`). | Express the grouped/ordered sub-list with `query.sql`; keep sub-selects scalar-per-row. |
| `COMPOSE-NULLABILITY-MISMATCH-001` | warning | A projection from an `optional` (LEFT) join or a `latest` sub-select maps to a non-optional return-record field. | Mark the record field `optional` so the generated type is honest, instead of relying on COALESCE. |
| `COMPOSE-DEMOTABLE-TO-LIST-001` | warning | A `query.compose` declares zero `join` and zero `subselect` — a single-resource read in compose costume. | Express it as `query.list` (the one canonical single-resource read), or `query.lookup` when keyed. |
| `VOCAB-SQL-COMPOSABLE-001` | warning | A `query.sql` body is a pure FK-JOIN + scalar sub-select read (no `GROUP BY`, window fn, ordered comparison, or geo fn) — i.e. it could be a checked `query.compose`. Heuristic; never an error (the framework does not fully parse SQL). | Consider migrating to `query.compose` so the tenant predicate, JOIN target, and projection become doctor-checked. |

## `.lzx` surface diagnostics

Per the `mobile-target` proposal (operational archive) §9. The catalog of `lzx-*` rules is target-aware: each surface's `target` field (`"web"` or `"mobile"`) drives the directory the rule expects to find. Module placement: `crates/lazuli_cli/src/doctor/lzx/`.

| Code | Severity | Trigger | Resolution |
|---|---|---|---|
| `lzx-cell-missing-impl` | error | `cells … @client.<slot>` references a slot whose `features/<feat>/<target>/cells/<slot>.tsx` impl is absent from disk (target derived from the enclosing surface). | Author the impl under the correct platform subdirectory, OR remove the cell binding. When the sibling target's impl exists, the diagnostic points there as a hint. |
| `lzx-route-collision` | error | Two views in the same `(audience, target)` tuple translate to the same router file path. Under Expo Router, distinct dynamic placeholder names at the same depth (`/users/:id` vs `/users/:user_id`) both collapse to a single `[name].tsx` file; under TanStack the collision is rarer (placeholders preserved as `$id` vs `$user_id`). | Disambiguate by making one route deeper (e.g., `/users/by-id/:id` and `/users/by-slug/:slug`) so the routers see distinct file paths. |

## Route guard codes

These diagnostics are emitted by `crates/lazuli_doctor/src/route_guard/`
during `lazuli doctor` walks of `.lzx` files. They cover the
escape-hatch surface added by
`docs/proposals/ir-route-guard-escape-hatch-2026-05-28.md` §4.3 —
`requires_lifecycle_in`, the composed `forbid_when ... only_when
lifecycle ...`, and the `requires <feature>.lookup_my.<field> =
<literal> on_unmet redirect "..."` row-field predicate. All codes
surface via the CLI, LSP, and IDE under the
`@translation.doctor.route_guard.<code>` namespace.

### ROUTE-GUARD-LIFECYCLE-EXCLUSIVE-001

| Field | Value |
|---|---|
| `message_key` | `@translation.doctor.route_guard.lifecycle_exclusive_001` |
| `message` (en, default) | `<owner> declares both 'requires_lifecycle <R> = ...' AND 'requires_lifecycle_in <R> [...]' — pick exactly one form.` |
| `hint` (en, default) | `The allow-list form ('_in') is canonical for any list shape; the exact-match form is the shorthand for single states only.` |
| `status_code` | n/a (doctor diagnostic, not runtime wire error) |
| `retriable` | n/a |
| `severity` | `error` (all profiles) |
| `locale_anchor` | `crates/lazuli_doctor/i18n/{en,pt-BR}.toml#doctor.route_guard.lifecycle_exclusive_001` |
| `producer` | `crates/lazuli_doctor/src/route_guard/lifecycle_exclusive_001.rs` |

### ROUTE-GUARD-LIFECYCLE-IN-EMPTY-002

| Field | Value |
|---|---|
| `message_key` | `@translation.doctor.route_guard.lifecycle_in_empty_002` |
| `message` (en, default) | `<owner> declares 'requires_lifecycle_in <R> []' with an empty allow-list — the view is unreachable.` |
| `hint` (en, default) | `Add at least one lifecycle state to the allow-list, or drop the slot entirely.` |
| `status_code` | n/a |
| `retriable` | n/a |
| `severity` | `error` (all profiles) |
| `locale_anchor` | `crates/lazuli_doctor/i18n/{en,pt-BR}.toml#doctor.route_guard.lifecycle_in_empty_002` |
| `producer` | `crates/lazuli_doctor/src/route_guard/lifecycle_in_empty_002.rs` |

### ROUTE-GUARD-LIFECYCLE-IN-UNKNOWN-003

| Field | Value |
|---|---|
| `message_key` | `@translation.doctor.route_guard.lifecycle_in_unknown_003` |
| `message` (en, default) | `<owner> declares 'requires_lifecycle_in <R> [..., {state}, ...]' but '{state}' is not a declared lifecycle state on resource '<R>'.` |
| `hint` (en, default) | `Use a state declared on the resource's Lifecycle block. Run 'lazuli inspect <feature>.<R>.lifecycle' to list valid states.` |
| `status_code` | n/a |
| `retriable` | n/a |
| `severity` | `error` (all profiles) |
| `locale_anchor` | `crates/lazuli_doctor/i18n/{en,pt-BR}.toml#doctor.route_guard.lifecycle_in_unknown_003` |
| `producer` | `crates/lazuli_doctor/src/route_guard/lifecycle_in_unknown_003.rs` |

### ROUTE-GUARD-FIELD-UNKNOWN-FEATURE-004

| Field | Value |
|---|---|
| `message_key` | `@translation.doctor.route_guard.field_unknown_feature_004` |
| `message` (en, default) | `<owner> declares 'requires {feature}.lookup_my.{field}' but feature '{feature}' ships no 'lookup_my_*' query.` |
| `hint` (en, default) | `Add 'query.lookup my_<resource>' to feature '{feature}', or correct the qualified path on the 'requires' slot.` |
| `status_code` | n/a |
| `retriable` | n/a |
| `severity` | `error` (all profiles) |
| `locale_anchor` | `crates/lazuli_doctor/i18n/{en,pt-BR}.toml#doctor.route_guard.field_unknown_feature_004` |
| `producer` | `crates/lazuli_doctor/src/route_guard/field_unknown_feature_004.rs` |

### ROUTE-GUARD-FIELD-UNKNOWN-FIELD-005

| Field | Value |
|---|---|
| `message_key` | `@translation.doctor.route_guard.field_unknown_field_005` |
| `message` (en, default) | `<owner> declares 'requires {feature}.lookup_my.{field}' but field '{field}' is not declared on any resource of feature '{feature}'.` |
| `hint` (en, default) | `Confirm the field spelling, or add the field to the resource backing 'lookup_my_<resource>'.` |
| `status_code` | n/a |
| `retriable` | n/a |
| `severity` | `error` (all profiles) |
| `locale_anchor` | `crates/lazuli_doctor/i18n/{en,pt-BR}.toml#doctor.route_guard.field_unknown_field_005` |
| `producer` | `crates/lazuli_doctor/src/route_guard/field_unknown_field_005.rs` |

### ROUTE-GUARD-FIELD-TYPE-MISMATCH-006

| Field | Value |
|---|---|
| `message_key` | `@translation.doctor.route_guard.field_type_mismatch_006` |
| `message` (en, default) | `<owner> declares 'requires {feature}.lookup_my.{field} = <{literal_kind}>' but field '{field}' is of type '{field_type}' — the literal type does not match.` |
| `hint` (en, default) | `Match the literal type to the field's IR type (Boolean ↔ true/false, Integer ↔ <n>, Text ↔ "<string>", any nullable ↔ null).` |
| `status_code` | n/a |
| `retriable` | n/a |
| `severity` | `error` (all profiles) |
| `locale_anchor` | `crates/lazuli_doctor/i18n/{en,pt-BR}.toml#doctor.route_guard.field_type_mismatch_006` |
| `producer` | `crates/lazuli_doctor/src/route_guard/field_type_mismatch_006.rs` |

### ROUTE-GUARD-FORBID-ONLY-WHEN-RESOURCE-MISMATCH-007

| Field | Value |
|---|---|
| `message_key` | `@translation.doctor.route_guard.forbid_only_when_resource_mismatch_007` |
| `message` (en, default) | `<owner> declares 'forbid_when {atom} ... only_when lifecycle {only_when_resource} = ...' but the rest of the guard targets '{guard_primary_resource}'.` |
| `hint` (en, default) | `Confirm the cross-resource composition is intentional, or align the 'only_when' resource with the guard's primary lifecycle resource.` |
| `status_code` | n/a |
| `retriable` | n/a |
| `severity` | `warning` (all profiles) |
| `locale_anchor` | `crates/lazuli_doctor/i18n/{en,pt-BR}.toml#doctor.route_guard.forbid_only_when_resource_mismatch_007` |
| `producer` | `crates/lazuli_doctor/src/route_guard/forbid_only_when_resource_mismatch_007.rs` |

### ROUTE-LIFECYCLE-CANONICAL-FORM-001

| Field | Value |
|---|---|
| `message_key` | `@translation.doctor.route_guard.lifecycle_canonical_form_001` |
| `message` (en, default) | `Project mixes 'requires_lifecycle <R> = ...' AND 'requires_lifecycle_in <R> [...]' for the same resource, OR uses the shorthand at scale (2+ sites for one resource).` |
| `hint` (en, default) | `Pick the canonical allow-list form ('_in') for both shapes per §6.5, or collapse to the shorthand when the project's convention is single-state. The shorthand is admissible for single states; the allow-list form is canonical for any list shape.` |
| `status_code` | n/a |
| `retriable` | n/a |
| `severity` | `warning` at strict, `error` at production |
| `locale_anchor` | `crates/lazuli_doctor/i18n/{en,pt-BR}.toml#doctor.route_guard.lifecycle_canonical_form_001` |
| `producer` | `crates/lazuli_doctor/src/route_guard/lifecycle_canonical_form_001.rs` |

### ROUTE-GUARD-FIELD-MISSING-SERVER-PAIR-001

| Field | Value |
|---|---|
| `message_key` | `@translation.doctor.route_guard.field_missing_server_pair_001` |
| `message` (en, default) | `<owner> declares 'requires {feature}.lookup_my.{field}' (a UX-only client gate) but no paired server-side enforcement was found — no SQL trigger 'enforce_*_{field}' in migrations/ AND no command-level 'requires_field "{field}"' policy companion.` |
| `hint` (en, default) | `Add the server-side gate (SQL trigger or policy companion), OR suppress with 'doctor:allow ROUTE-GUARD-FIELD-MISSING-SERVER-PAIR-001 -- reason "UX-cosmetic only, ..."' citing the downstream-operation audit.` |
| `status_code` | n/a |
| `retriable` | n/a |
| `severity` | `warning` at strict, `error` at production |
| `locale_anchor` | `crates/lazuli_doctor/i18n/{en,pt-BR}.toml#doctor.route_guard.field_missing_server_pair_001` |
| `producer` | `crates/lazuli_doctor/src/route_guard/field_missing_server_pair_001.rs` |
