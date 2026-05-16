# Lazuli Style Guide — Idiomatic `.lzi` Conventions

**Status**: Normative. Prescriptive idioms for canonical `.lzi` and `.lzx` source.

**Audience**: Authors of `.lzi` source — human contributors, LLM agents, and the `lazuli-audit` skill. Compiler and runtime teams consult `docs/invariants.md` and `docs/canonical-semantics.md` first; this file is for the author. `.lzx` idioms ship in a follow-up; today's scope is `.lzi`.

**Purpose**: Codify recurring authoring choices that are grammatical under the closed catalog but vary in cognitive cost. Each idiom names the preferred form, cites a canonical example, and links the doctor lint that enforces it (where one exists).

## How to use

Consult this file when choosing between two grammatical forms. Invariants in `docs/invariants.md` decide what is legal; this file decides what is canonical when the grammar permits both. Where a doctor lint exists (cited per idiom), it carries the rule code; section-number cross-references in diagnostics are a future enhancement, not a current contract. When no lint exists, the idiom is convention-only and the reviewer enforces it during PR review or design grading.

The 6 idioms below are closed for v0. Adding a 7th idiom requires the same grade-then-fix discipline as a language proposal (see `docs/design-principles.md` Rule Zero). Polishing wording inside an existing idiom does not.

## Catalog

### 1. Enum identifiers in English; user-facing labels via i18n catalog

**Rule**: Author every `enum` identifier in English `snake_case` (variants) or `PascalCase` (type name). Localize user-visible labels through `translation` blocks and the per-feature catalog file.

**Why**: Rule Zero (`docs/design-principles.md`) — the IR is the single source of truth for product semantics. Enum identifiers travel through generated Go types, generated TS types, doctor diagnostics, inspect JSON, source maps, error codes, and audit logs. Mixing a localized identifier into any of those surfaces forces every downstream consumer (LSP, agents, error catalog, CI) to know the source locale. English is the canonical authoring locale of the framework; localized strings belong in `translation`. The two layers are disambiguated by the catalog vs identifier split.

**Idiomatic example** — `examples/full-capsule/full-capsule.lzi:28-43` declares enums in English and `examples/full-capsule/full-capsule.lzi:282-295` ships the matching translation catalog:

```lazuli
enum CustomerStatus
  lead = 10
  active = 20
  paused = 30
  archived = 90

enum CustomerTier
  free
  pro
  enterprise

translation
  catalog "./i18n/customer.<locale>.json"

  key archive_archived_blocked
    pt-BR "Não é possível reatribuir um cliente arquivado"
    en-US "Cannot reassign an archived customer"
```

**Anti-idiom**:

```lazuli
# AVOID — localized enum identifiers pollute IR/Go/TS/audit-log surfaces.
enum StatusCliente
  potencial
  ativo
  pausado
  arquivado
```

**Doctor enforcement**: No dedicated lint today; convention enforced at review. Tracked under `next-checklist` "VOCAB-* extensions" for a future `VOCAB-ENUM-LOCALE-001`.

---

### 2. Shared value-types live in a dedicated owning feature

**Rule**: When two or more features reference the same value-type (user identity, address, tax ID, money column, etc.), declare the type once inside the owning feature and reach it across the capsule through `uses <feature>`. Do not redeclare; do not copy fields.

**Why**: Self-Contained Declarations (`docs/design-principles.md`) and `uses` strictness (`docs/invariants.md` §Authored Shape — every `uses` entry must be referenced by a semantic edge). Duplicated declarations diverge silently under refactor; one canonical declaration plus a typed `uses` edge gives the analyzer a graph it can check, gives the agent a single home for changes, and removes the "which copy is authoritative?" question for cold readers.

**Idiomatic example** — `examples/full-capsule/full-capsule.lzi:918-940` declares the canonical `User` resource inside `feature account`; every other capsule reaches it through `uses`:

```lazuli
feature account
  purpose
    Identity owner for application users (canonical, cross-feature).

  domain
    resource User
      email: @semantic.Email @pii.contact required unique
      name: Text required
      role: Text optional

      retention forever then anonymize
```

Consuming features reference `User` after declaring the edge (`examples/full-capsule/full-capsule.lzi:25`, `:631`, `:756`):

```lazuli
feature customer_tags
  uses org, user, customer

  domain
    resource CustomerTagAssignment
      added_by: User required
```

For a shared value-type that is not an identity (e.g. `Address`) the same pattern applies: declare once in the most-specific feature that semantically owns the type, then reach via `uses`. Worked example:

```lazuli
# Right — `Address` lives in the feature that semantically owns it
feature account
  domain
    record Address
      street: Text required
      city: Text required
      state: Text required
      postal_code: Text required
      country: Text required

    resource User
      shipping: Address required

feature billing
  uses account              # ← reach for Address through here, do NOT redeclare
  domain
    resource Invoice
      bill_to: Address required
```

**Tie-breaker**: pick the most-specific feature where the type is structurally a "first-class" field, not derived or computed. Postal addresses → `account` (users carry them). Org-scoped tax addresses → `org` (an org has one). Create a dedicated `feature shared` only when (a) ≥ 2 consumers already exist AND (b) no feature semantically owns the type without forcing a contortion. The bar is intentionally high — `shared` is a sink that erodes ownership clarity.

**Anti-idiom**:

```lazuli
# AVOID — Address redeclared inside two features.
feature billing
  domain
    record Address
      street: Text
      city: Text
      country: Text

feature shipping
  domain
    record Address
      street: Text
      city: Text
      country: Text
```

**Doctor enforcement**: No structural duplicate-record lint today; convention-only. `lazuli inspect --expand=dependencies` surfaces the cross-feature edge graph that makes drift visible.

---

### 3. `updates X` declarative effect for "save input" commands; `@fn` handlers only for irreducible logic

**Rule**: When a command's body is a list of field assignments from `input.*` / `route.*` / `ctx.*` / `let` bindings to one resource, write the effect declaratively (`updates Resource ... <field> = <expr>`). Reach for a `@fn.<name>` handler only when the command crosses resources transactionally, calls external integrations, performs cryptographic work, or otherwise carries irreducibly imperative logic.

**Why**: Rule Zero (`docs/design-principles.md`) and `next-checklist` `VOCAB-HANDLER-HEAVY-001`. Declarative effects are inspectable: they show up in `lazuli inspect --expand=security`, derive the audit contract, and feed event-payload bindings (`emits <event> from updates`). Handler-side assignments hide the same intent behind opaque Go code, prevent the analyzer from deriving the event payload, and force every reviewer (and every agent on the next session) to read the handler to see what the command stores.

**Idiomatic example** — `examples/full-capsule/full-capsule.lzi:363-373` updates a single resource with no handler:

```lazuli
command update_tier
  route id: ID
  input
    tier: CustomerTier required
  policy @policy.update
  rate_limit "30 per minute per user"
  updates Customer
    tier = input.tier
  invalidates
    query.list
    query.by_id(id: route.id)
```

When the command DOES require imperative logic (external call, cross-resource transaction, password hash), keep the assignment declarative and isolate the imperative part behind a typed `let`. `examples/full-capsule/full-capsule.lzi:334-356` shows the pattern — `let resolved_owner = user.query.by_id(...)` resolves the lookup, then `updates Customer ... owner = resolved_owner` stays declarative:

```lazuli
command reassign
  route id: ID
  input
    owner_id: User.ID required
  let resolved_owner = user.query.by_id(id: input.owner_id)
  policy @policy.update
  audit actor, target.id, input.owner_id
    emit_to audit_log
  updates Customer
    owner = resolved_owner
  emits customer_reassigned
    to_owner_id = input.owner_id
```

**Anti-idiom**:

```lazuli
# AVOID — handler hides what the command stores.
command update_tier
  route id: ID
  input
    tier: CustomerTier required
  policy @policy.update
  handler "./handlers/update_tier.go"
```

**Doctor enforcement**: Tracked under `next-checklist` as `VOCAB-HANDLER-HEAVY-001` (warns when ≥ 70% of a feature's commands use `@fn`/`handler` instead of declarative effects). Conventions for transactional / external-call exceptions are covered by the same lint's false-positive guidance.

---

### 4. `lifecycle` block for resource state machines; not N hand-rolled transition commands

**Rule**: When a resource has a discriminator field (`status`, `phase`, `state`, etc.) that is mutated by a closed set of named transitions, declare the state machine inline on the resource via `lifecycle <name>`. Do not author one `command` per transition with `updates Resource ... status = "<next>"`.

**Why**: Rule Zero (`docs/design-principles.md`) and `docs/invariants.md` §Source And Derived Views (`workflow`/`lifecycle` is named vocabulary). A `lifecycle` block names states, transitions, per-transition policy, emitted events, and transition tests in one place; the analyzer derives the state graph, the audit trail, and the actor-matrix. Hand-rolled transition commands replicate the same shape across N command bodies, lose the closed-graph invariant, and force the reviewer to reconstruct the state machine from scattered `updates` lines.

**Idiomatic example** — `examples/full-capsule/full-capsule.lzi:62-111` declares `lifecycle status` inline on `Customer` with all four transitions, their `from`/`to`, their `policy`, their `emits`, and their tests:

```lazuli
lifecycle status
  state lead initial
  state active
  state paused
  state archived terminal

  transition activate
    from lead
    to active
    policy @policy.update
    emits customer_activated, customer_status_changed
    tests
      allows from lead
      denies from active
      denies from archived

  transition archive
    from active
    to archived
    policy @policy.update
    requires @policy.delete
    emits customer_archived, customer_status_changed
    previously migrated deactivate
```

**Anti-idiom**:

```lazuli
# AVOID — N transition commands that re-encode a state machine in command bodies.
command activate
  route id: ID
  policy @policy.update
  updates Customer
    status = CustomerStatus.active

command pause
  route id: ID
  policy @policy.update
  updates Customer
    status = CustomerStatus.paused

command archive
  route id: ID
  policy @policy.update
  requires @policy.delete
  updates Customer
    status = CustomerStatus.archived
```

**Doctor enforcement**: `VOCAB-LIFECYCLE-001` (deferred — lands with the lifecycle vocab proposal; see the `lifecycle-vocab` proposal (operational archive) v0.3 and the `doctor-vocabulary-lints` proposal (operational archive) §VOCAB-LIFECYCLE-001). Will fire when a feature declares ≥ 3 commands that all update the same resource's discriminator field with a constant value drawn from one enum.

---

### 5. Semantic types for PII and validated strings; not plain `Text`

**Rule**: For fields carrying a contract-validated string (email, phone, URL, UUID, currency code, geographic point, money) use the matching `@semantic.<Type>`. For locale-specific tax IDs and identity numbers (e.g. Brazilian CPF/CNPJ/CEP via `@plugin/scalars-br`) use the corresponding `@plugin/scalars-<locale>` semantic. Do not store these as plain `Text`. Locale plugins for other jurisdictions (EU IBAN, US SSN, etc.) ship as separate plugins when authored — they are not in scope for the framework core.

**Why**: `docs/canonical-semantics.md` §Reference Namespaces — `@semantic.*` carries built-in validation/formatting and travels through generated Go validators, React form validators, Expo client validation, and inspect security expansion. Plain `Text` defers the contract to a handler that every consumer must re-implement. PII classification (`@pii.*`) is orthogonal to the semantic type and must be declared alongside it so logs, retention, redaction, and export flows pick the field up. Locale-specific scalars stay in plugins per `docs/scope-discipline.md` (the framework does not absorb per-country validators).

**Closed core catalog** (today): `@semantic.Email`, `@semantic.Phone`, `@semantic.Url`, `@semantic.Uuid`, `@semantic.Currency`, `@semantic.GeoPoint`, `@semantic.Money`. Locale scalars (`@semantic.BrazilianCPF`, `@semantic.BrazilianCNPJ`, `@semantic.BrazilianCEP`, etc.) are contributed by `@plugin/scalars-<locale>` and require the plugin in `Lazurite.toml` `[plugins]`.

**Idiomatic example** — `examples/full-capsule/full-capsule.lzi:49` and `:300` declare email with both the semantic type and the PII class:

```lazuli
resource Customer
  email: @semantic.Email @pii.contact required

command capture_lead
  input
    name: Text required
    email: @semantic.Email @pii.contact required
```

Locale-specific PII (using the plugin form):

```lazuli
# Requires `[plugins] "@plugin/scalars-br" = "1.0"` in Lazurite.toml.
resource Customer
  tax_id: @semantic.BrazilianCPF @pii.legal_id required
  legal_entity_id: @semantic.BrazilianCNPJ @pii.legal_id optional
```

**Anti-idiom**:

```lazuli
# AVOID — semantics buried in handler-side validator + opaque Text storage.
resource Customer
  email: Text required
  validates @validator.is_email
  cpf: Text required
  validates @validator.is_cpf
```

| Do                                            | Avoid                                  |
| --------------------------------------------- | -------------------------------------- |
| `email: @semantic.Email @pii.contact`         | `email: Text validates @validator.is_email` |
| `phone: @semantic.Phone @pii.contact`         | `phone: Text` + handler-side mask      |
| `revenue: @semantic.Money`                    | `revenue: Decimal` + currency comment  |
| `tax_id: @semantic.BrazilianCPF @pii.legal_id` | `tax_id: Text validates @validator.is_cpf` |

**Doctor enforcement**: `VOCAB-SEMANTIC-001` family in the `doctor-vocabulary-lints` proposal (operational archive) (§VOCAB-SEMANTIC-PERCENT-001 is the template; per-type variants land alongside their semantic). `INLINE-VALIDATOR-TYPE-MISMATCH` (`docs/invariants.md:471`) rejects inline constraint keywords applied to the wrong base type.

---

### 6. `<Type>[]` for known-shape arrays; reserve `JSON` for opaque or extensible payloads

**Rule**: Declare collection results, list-shaped fields, and known-shape arrays as `<Type>[]` where `<Type>` is a `resource`, `record`, or scalar in the closed catalog. Reach for `JSON` only when the payload's shape is genuinely opaque (third-party webhook bodies, schema-less upload rows, free-form metadata) and you cannot enumerate the keys.

**Why**: `docs/canonical-semantics.md` §Resources And Relations — the closed scalar catalog plus `record` give the analyzer enough information to derive Go structs, TS interfaces, OpenAPI schemas, and audit projections. `JSON` opts out of that derivation: callers receive `map[string]any` in Go and `unknown` in TS, validators cannot fire, doctor cannot lint the contents, and every consumer reinvents the same shape lookups. Use `JSON` only when the shape is honestly external; otherwise the analyzer should see the keys.

**Idiomatic example** — `examples/full-capsule/full-capsule.lzi:122-135` declares records and `examples/full-capsule/full-capsule.lzi:182-191` returns them as typed arrays:

```lazuli
record CustomerLtv
  customer_id: ID
  amount: @semantic.Money
  currency: Text

record CustomerRiskRow
  customer_id: ID
  risk_score: Integer
  reason: Text

query.sql lifetime_value
  returns CustomerLtv[]

  scope
    org = ctx.user.org

  sql "./queries/customer_lifetime_value.sql"

query.sql churn_risk
  returns CustomerRiskRow[]
```

**Honest `JSON` example** — `examples/full-capsule/full-capsule.lzi:779` stores opaque inbound CSV rows that the validator inspects but the schema does not declare:

```lazuli
resource CustomerImportRow
  batch: CustomerImportBatch required
  row_number: Integer required
  raw: JSON required
  error: Text optional
  validates @validator.row_check
```

**Anti-idiom**:

```lazuli
# AVOID — fields you control declared as JSON, hiding a record from the analyzer.
record CustomerSummary
  customer_id: ID
  details: JSON           # actually holds { ltv, risk, last_order_at }

# AVOID — query.sql returning JSON loses the column contract.
query.sql lifetime_value
  returns JSON
  sql "./queries/customer_lifetime_value.sql"
```

**Doctor enforcement**: `SQL-RETURN-TYPE-UNRESOLVED-001` (`docs/invariants.md:399-401`) rejects SQL-backed queries whose `returns` does not resolve to a resource / record / contract. No dedicated lint for "field declared as `JSON` should be a record"; convention-only, reviewer call.

---

## Out of scope

The style guide does NOT cover:

- **Naming conventions for handler files**. `handlers/<fn>.go` and `domain/<fn>.go` paths are normative in `docs/project-structure.md` and the `lazurite-scaffold` proposal (operational archive); this guide does not duplicate them.
- **Indent unit (tabs vs spaces, 2 vs 4)**. The grammar lexer fixes the unit from the file's first indent (`docs/grammar.lzi.md` §1.2); `lazuli fmt` normalizes to two spaces.
- **Commit-message style and PR descriptions**. Authoring discipline (Co-Authored-By trailers, message format, grading discipline) lives in `CLAUDE.md` / `AGENTS.md`.
- **Generated code conventions**. `dist/go/*.go` and `dist/ts-*/` are derived; nothing in this guide applies to them.
- **`.lzx` layout conventions** beyond shared vocabulary with `.lzi`. Surface/audience/route idioms live in `docs/lzx-grammar.md` and the L0 #6 terminal-grammar proposal.
- **Plugin authoring**. `@plugin/<name>` adapter shape, repo layout, and registration mechanics belong to `docs/plugin-authoring.md`.
- **Test coverage thresholds and CI gates**. The `tests` block syntax is normative (`docs/canonical-semantics.md` §Tests); coverage policy is per-project in `Lazurite.toml`.

## References

- `docs/design-principles.md` — Rule Zero (Vocabulary Over Mechanism), Self-Contained Declarations, Operational Systems First, No Cascade.
- `docs/invariants.md` — Closed grammar/IR constraints. The style guide assumes invariants pass.
- `docs/canonical-semantics.md` — Closed semantic-type catalog (`@semantic.*`), closed scalar catalog (`Text`, `Integer`, `Decimal`, `Date`, `DateTime`, `JSON`, `ID`, `Boolean`), reference-namespace catalog (`@role.*`, `@scope.*`, `@actor.*`, `@policy.*`, `@semantic.*`, `@cap.*`, `@pii.*`, `@key.*`).
- `docs/grammar.lzi.md` — Parser-shaped grammar reference; reserved-word catalog.
- `docs/error-contract.md` — Generated error kinds and source mapping.
- `docs/scope-discipline.md` — 80/20 framework boundary; locale-specific scalars and per-vendor adapters live in plugins, not core.
- the `doctor-vocabulary-lints` proposal (operational archive) — `VOCAB-*` rule catalog (`VOCAB-UNION-001`, `VOCAB-DERIVED-READ-001`, `VOCAB-LIFECYCLE-001`, `VOCAB-AUDIT-001`, `VOCAB-EVENT-PAYLOAD-001`, `VOCAB-SEMANTIC-PERCENT-001`, plus deferred entries).
- the `lifecycle-vocab` proposal (operational archive) — Canonical `lifecycle` form; replaces hand-rolled transition commands.
- the `semantic-types-money-brazilian` proposal (operational archive) — Plugin-contributed semantic type pattern (`@plugin/scalars-<locale>`).
- the operational next-checklist — Tracked follow-up lints (`VOCAB-HANDLER-HEAVY-001`, `VOCAB-TESTS-MISSING-001`).
- `examples/full-capsule/full-capsule.lzi` — Canonical fixture cited throughout this guide.
