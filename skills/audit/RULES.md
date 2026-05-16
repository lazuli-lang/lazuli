# Lazuli Audit Skill Rule Catalog

This file is the LLM-readable projection described by [docs/proposals/audit-skill-mvp.md](https://github.com/lazuli-lang/lazuli/blob/main/docs/proposals/audit-skill-mvp.md). It mirrors the 13 vocabulary doctor rules declared in `crates/lazuli_doctor/src/vocab/mod.rs`.

The skill is not authoritative — the Rust source at `crates/lazuli_doctor/src/vocab/` is canonical. If a divergence appears, file the divergence as a skill-fidelity bug.

## VOCAB-AUDIT-001 — mutating command without an explicit `audit` child

**Source**: [crates/lazuli_doctor/src/vocab/vocab_audit_001.rs](https://github.com/lazuli-lang/lazuli/blob/main/crates/lazuli_doctor/src/vocab/vocab_audit_001.rs)
**Severity**: `warning` (strict-profile), `error` (production-profile)
**Reference**: [docs/proposals/doctor-vocabulary-lints.md §VOCAB-AUDIT-001](https://github.com/lazuli-lang/lazuli/blob/main/docs/proposals/doctor-vocabulary-lints.md), [docs/invariants.md:93-97](https://github.com/lazuli-lang/lazuli/blob/main/docs/invariants.md#L93-L97)

### Trigger

Fires when a `command` has a write effect or emits events but declares no `audit` child.

- Write effects are `creates`, `updates`, and `deletes`.
- A pure `returns` command also triggers if it declares at least one `emits` target.
- The accepted audit forms are `audit default`, `audit <field>, <field>`, and `audit none`.

### Example — violation

```lzi
feature publishing
  resource Publication
    status: Text required
  command update_status
    updates Publication
      status = "archived"
```

### Example — canonical fix

```lzi
feature publishing
  resource Publication
    status: Text required
  command update_status
    updates Publication
      status = "archived"
    audit default
```

### Message

> command `update_status` has a write effect or emits events but declares no `audit` child — add `audit default`, `audit <fields>`, or `audit none` (with a reason). Audit declarations give compliance tooling a typed contract instead of relying on event-name conventions. See docs/invariants.md:93-97.

## VOCAB-AUDIT-002 — handler-only command on capability-tagged fields lacks audit

**Source**: [crates/lazuli_doctor/src/vocab/vocab_audit_002.rs](https://github.com/lazuli-lang/lazuli/blob/main/crates/lazuli_doctor/src/vocab/vocab_audit_002.rs)
**Severity**: not specified in the module header
**Reference**: not specified in the module header

### Trigger

Fires for the conservative IR-visible case where a handler-only command can mutate sensitive capability-tagged fields without an audit contract.

- The command has no `audit` child.
- The command is handler-like: `returns` / no effect.
- The command invalidates a same-feature resource whose fields include sensitive `@cap.*` tiers: `Encrypted`, `Token`, `Hashed`, or `PII`.
- Structured write effects are handled by `VOCAB-AUDIT-001`, not this rule.

### Example — violation

```lzi
feature integrations
  resource Connection
    access_token: @cap.Encrypted required
  command refresh_tokens
    returns Boolean
    invalidates Connection
```

### Example — canonical fix

```lzi
feature integrations
  resource Connection
    access_token: @cap.Encrypted required
  command refresh_tokens
    returns Boolean
    invalidates Connection
    audit default
```

### Message

> handler-only command `refresh_tokens` invalidates `Connection` which has 1 field(s) with sensitive @cap.* tier (access_token) but declares no `audit` child — handler-side mutation of capability-tagged fields requires an explicit audit contract. Add `audit default` or `audit <fields>` with a documented reason.

## VOCAB-CAP-MISSING-001 — `@pii.*` field without a crypto/storage capability

**Source**: [crates/lazuli_doctor/src/vocab/vocab_cap_missing_001.rs](https://github.com/lazuli-lang/lazuli/blob/main/crates/lazuli_doctor/src/vocab/vocab_cap_missing_001.rs)
**Severity**: `error` (strict), `warning` (prototype)
**Reference**: not specified in the module header

### Trigger

Fires when a resource field carries a sensitive `@pii.<class>` marker but the field type is not one of the storage-semantic capability tiers.

- Sensitive PII tags include `contact`, `financial`, `health`, `government_id`, `auth_secret`, `external`, `credential`, and `identifier`.
- Allowed capability tiers are `@cap.Hashed`, `@cap.Encrypted`, `@cap.E2ee`, and `@cap.Token`.
- `@pii.derived`, `@pii.public`, and fields with `derived from` are carve-outs.
- The practical v0 path scans raw `.lzi` resource field lines because the IR does not yet preserve trailing `@pii.*` decorators.

### Example — violation

```lzi
feature customer
  resource Customer
    email: Text required @pii.contact
```

### Example — canonical fix

```lzi
feature customer
  resource Customer
    email: @cap.Encrypted required @pii.contact
```

### Message

> field `Customer.email` carries `@pii.contact` but no `@cap.Hashed/Encrypted/Token` - sensitive data stored in plaintext

## VOCAB-DERIVED-READ-001 — handler-computed read-only field drift

**Source**: [crates/lazuli_doctor/src/vocab/vocab_derived_read_001.rs](https://github.com/lazuli-lang/lazuli/blob/main/crates/lazuli_doctor/src/vocab/vocab_derived_read_001.rs)
**Severity**: `warning` (strict-profile), `warning` (production-profile)
**Reference**: [docs/invariants.md:89-92](https://github.com/lazuli-lang/lazuli/blob/main/docs/invariants.md#L89-L92)

### Trigger

Fires when a resource field looks like a computed read-only value but does not use the existing `derived from <expr>` vocabulary.

- The field is optional, is not `id`, has no explicit `default`, and has no `@cap.*` capability tier.
- The field has no `derived from <expr>` annotation.
- The field is never assigned by any declarative command or job `creates` / `updates` site.
- A `creates <Resource> from input` write suppresses the whole resource to avoid false positives.

### Example — violation

```lzi
feature post
  resource Post
    id: ID required
    title: Text required
    canonical_url: Text
```

### Example — canonical fix

```lzi
feature post
  resource Post
    id: ID required
    title: Text required
    canonical_url: Text derived from "https://example.com/p/{{id}}"
```

### Message

> field `Post.canonical_url` is never written by any command or job — if it is computed at read time, consider `derived from <expr>` (docs/invariants.md:89-92)

## VOCAB-EVENT-ORPHAN-001 — event declared but no command emits it

**Source**: [crates/lazuli_doctor/src/vocab/vocab_event_orphan_001.rs](https://github.com/lazuli-lang/lazuli/blob/main/crates/lazuli_doctor/src/vocab/vocab_event_orphan_001.rs)
**Severity**: `warning` (strict-profile), `warning` (production-profile)
**Reference**: not specified in the module header

### Trigger

Fires when a feature-level domain event declaration with a typed payload is never emitted by a same-feature command or job.

- `payload none` is an explicit opt-out and does not trigger.
- Trace events do not trigger because they are outside the feature reaction graph.
- Cross-feature emissions are intentionally out of scope for v1.

### Example — violation

```lzi
feature customer
  event archived
    payload
      customer_id: ID required
```

### Example — canonical fix

```lzi
feature customer
  event archived
    payload
      customer_id: ID required
  command archive_customer
    emits archived
```

### Message

> event `archived` is declared but no command or job in this feature emits it - either remove the orphan declaration or attach `emits archived` to the relevant command. Orphan events leak abstraction.

## VOCAB-EVENT-PAYLOAD-001 — `emits` without typed payload

**Source**: [crates/lazuli_doctor/src/vocab/vocab_event_payload_001.rs](https://github.com/lazuli-lang/lazuli/blob/main/crates/lazuli_doctor/src/vocab/vocab_event_payload_001.rs)
**Severity**: `warning` (strict-profile), `warning` (production-profile)
**Reference**: [docs/proposals/doctor-vocabulary-lints.md §VOCAB-EVENT-PAYLOAD-001](https://github.com/lazuli-lang/lazuli/blob/main/docs/proposals/doctor-vocabulary-lints.md)

### Trigger

Fires when a command declares `emits <event.name>` and the emitted event lacks a typed contract.

- Undeclared: the emitted event has no matching feature-level `event <name>` declaration.
- Missing payload: the event is declared but has neither typed payload fields nor `payload none`.
- `payload none` is the catalog-fixed opt-out for intentionally payload-less events.

### Example — violation

```lzi
feature post
  command archive_post
    emits post.archived
```

### Example — canonical fix

```lzi
feature post
  event archived
    payload
      post_id: ID required
  command archive_post
    emits archived
```

### Message

> command emits `post.archived` but the event is not declared at the feature level; add `event post.archived payload <Type>` (or `payload none` to opt out explicitly). Unregistered event names are invisible to doctor, codegen, and the reaction graph.

Alternate declared-but-empty message:

> event `archived` is declared but has no payload; add `payload <Type>` to give subscribers a typed contract, or `payload none` to explicitly opt out (required for intentionally payload-less events such as heartbeats).

## VOCAB-EVENT-PRODUCER-001 — mutating command without IR-visible emits

**Source**: [crates/lazuli_doctor/src/vocab/vocab_event_producer_001.rs](https://github.com/lazuli-lang/lazuli/blob/main/crates/lazuli_doctor/src/vocab/vocab_event_producer_001.rs)
**Severity**: not specified in the module header
**Reference**: not specified in the module header

### Trigger

Fires when a mutating command changes a resource for which plausible feature-level events already exist but the command declares no `emits` clause.

- The command has `creates`, `updates`, or `deletes`.
- The command has no `emits`.
- A same-feature event name matches the mutated resource or command prefix, such as `post.archived` for `Post`.
- Read-named commands (`get_`, `find_`, `list_`) and `creates` commands with `audit none` are suppressed.

### Example — violation

```lzi
feature post
  event post.archived
    payload
      post_id: ID required
  command archive_post
    updates Post
      status = "archived"
```

### Example — canonical fix

```lzi
feature post
  event post.archived
    payload
      post_id: ID required
  command archive_post
    updates Post
      status = "archived"
    emits post.archived
```

### Message

> command `archive_post` mutates a resource for which event(s) ["post.archived"] exist, but declares no `emits` clause. If the handler emits events out-of-band, the IR can't see them and audit / projections drift. Add `emits <event>` from creates/updates/deletes.

## VOCAB-GRAMMAR-FORM-001 — deprecated `.lzi` grammar forms

**Source**: [crates/lazuli_doctor/src/vocab/vocab_grammar_form_001.rs](https://github.com/lazuli-lang/lazuli/blob/main/crates/lazuli_doctor/src/vocab/vocab_grammar_form_001.rs)
**Severity**: `warning` (strict), `error` (production)
**Reference**: `migrations/recipes/v0.X-to-v0.Y/VOCAB-GRAMMAR-FORM-001.md` if present

### Trigger

Fires on compatibility forms that still parse during migration windows but are no longer canonical authoring vocabulary.

- `validates resource @validator.X`
- `validates field <name> @validator.X`
- Inline `previously migrated <old>` or `previously alias <old>` on a kind or field header
- `validate "./path.go"`

### Example — violation

```lzi
feature account
  resource Account previously alias Customer
    email: Text required previously migrated email_address
    validates resource @validator.account
    validates field email @validator.email
    validate "./account.go"
```

### Example — canonical fix

```lzi
feature account
  resource Account
    previously alias Customer
    email: Text required
      previously migrated email_address
    validates @validator.account
    validates @validator.email
    validates field <name> "./account.go"
```

### Message

> deprecated form 'validates resource @validator.account'; use 'validates @validator.account'. Hint: see migrations/recipes/v0.X-to-v0.Y/VOCAB-GRAMMAR-FORM-001.md if present.

## VOCAB-HANDLER-HEAVY-001 — feature with a high handler-heavy command ratio

**Source**: [crates/lazuli_doctor/src/vocab/vocab_handler_heavy_001.rs](https://github.com/lazuli-lang/lazuli/blob/main/crates/lazuli_doctor/src/vocab/vocab_handler_heavy_001.rs)
**Severity**: `warning`
**Reference**: [docs/next-checklist.md §VOCAB-HANDLER-HEAVY-001](https://github.com/lazuli-lang/lazuli/blob/main/docs/next-checklist.md)

### Trigger

Fires once per feature when at least three commands exist and at least 70% are classified as handler-heavy instead of declarative.

- `Command.effect == CommandEffect::None` counts as handler-heavy.
- Non-empty `Command.external_calls` counts as handler-heavy.
- Any command `let` binding or declarative assignment containing an `@fn.<name>` path counts as handler-heavy.

### Example — violation

```lzi
feature demo
  command resolve_a
    let out = @fn.resolve_a(input)
  command resolve_b
    let out = @fn.resolve_b(input)
  command resolve_c
    let out = @fn.resolve_c(input)
```

### Example — canonical fix

```lzi
feature demo
  command update_a
    updates Ticket
      status = input.status
  command update_b
    updates Ticket
      name = input.name
  command resolve_c
    let out = @fn.resolve_c(input)
```

### Message

> feature `demo` has 3/3 commands routed through `@fn.<name>` handlers (>70%). Consider converting commands that just assign input fields to a resource into `updates X { field = input.field }` declarative form. Keep `@fn` for cross-resource transactions, OAuth, OTP, or other irreducibly imperative work. See docs/next-checklist.md.

## VOCAB-JSON-TYPED-001 — untyped JSON bag + sibling closed-catalog enum

**Source**: [crates/lazuli_doctor/src/vocab/vocab_json_typed_001.rs](https://github.com/lazuli-lang/lazuli/blob/main/crates/lazuli_doctor/src/vocab/vocab_json_typed_001.rs)
**Severity**: not specified in the module header
**Reference**: not specified in the module header

### Trigger

Fires when a resource has a `JSON` field while the same feature declares a related enum that is not referenced by any typed slot.

- The resource must have at least two fields and at least one `JSON` field.
- The enum must be same-feature and unreferenced by resources, records, events, command route/input/return types, queries, jobs, or webhooks.
- The enum name must be thematically related to the JSON field or resource, such as `QuizQuestionType` next to `Quiz.questions: JSON`.

### Example — violation

```lzi
feature quiz
  enum QuizQuestionType
    MultipleChoice
    TrueFalse
  resource Quiz
    title: Text required
    questions: JSON required
```

### Example — canonical fix

```lzi
feature quiz
  enum QuizQuestionType
    MultipleChoice
    TrueFalse
  record QuizQuestion
    kind: QuizQuestionType required
    text: Text required
  resource Quiz
    title: Text required
    questions: Many<QuizQuestion> required
```

### Message

> resource `Quiz` has untyped `questions: JSON` field with sibling enum `QuizQuestionType` that documents the shape but isn't referenced anywhere — consider a discriminated union OR a `record` type so the IR carries the constraint, not just the documentation.

## VOCAB-TESTS-MISSING-001 — feature with resources or commands but no tests

**Source**: [crates/lazuli_doctor/src/vocab/vocab_tests_missing_001.rs](https://github.com/lazuli-lang/lazuli/blob/main/crates/lazuli_doctor/src/vocab/vocab_tests_missing_001.rs)
**Severity**: `warning` (strict-profile), `warning` (production-profile)
**Reference**: [docs/next-checklist.md §VOCAB-TESTS-MISSING-001](https://github.com/lazuli-lang/lazuli/blob/main/docs/next-checklist.md)

### Trigger

Fires once per feature when the feature has authored behavior but no inline `test` blocks anywhere in the feature.

- A feature is in scope if it declares at least one `resource` or `command`.
- Any command test, rule test, workflow-transition test, or lifecycle-transition test satisfies the rule.
- v0 does not parse the planned `# doctor:allow VOCAB-TESTS-MISSING-001 — reason "..."` opt-out.
- v0 also does not implement the planned touched-in-last-N-commits false-positive filter.

### Example — violation

```lzi
feature post
  resource Post
    title: Text required
```

### Example — canonical fix

```lzi
feature post
  resource Post
    title: Text required
  command publish
    test publishes_draft
      expect ok
```

### Message

> feature `post` declares resources or commands but has no inline `test` blocks — add at least one `test` block to make expected behavior visible to doctor, codegen, and review tooling. If the omission is intentional, add `# doctor:allow VOCAB-TESTS-MISSING-001 — reason "..."` near the feature.

## VOCAB-UNION-001 — enum + correlated-optional-fields drift

**Source**: [crates/lazuli_doctor/src/vocab/vocab_union_001.rs](https://github.com/lazuli-lang/lazuli/blob/main/crates/lazuli_doctor/src/vocab/vocab_union_001.rs)
**Severity**: not specified in the module header
**Reference**: not specified in the module header

### Trigger

Fires when a resource has an enum-typed discriminator field plus optional fields that are meaningful only for specific variants.

- The enum must be declared in the same feature.
- The resource must have at least one optional non-enum field.
- The v0.1 implemented signal is name convention: an optional field starts with `<variant_lowercase>_`.
- Handler-graph branch analysis and `# only-when kind=<tag>` source pragmas are deferred.

### Example — violation

```lzi
feature route
  enum RouteKind
    Bridge
    Direct
  resource Route
    kind: RouteKind required
    amount: Decimal required
    bridge_fee: Decimal
```

### Example — canonical fix

```lzi
feature route
  union Route
    Bridge
      amount: Decimal required
      fee: Decimal required
    Direct
      amount: Decimal required
```

### Message

> resource `Route` declares enum field `kind` plus 1 optional field(s) (bridge_fee) only meaningful for one tag — consider a discriminated `union` type

## VOCAB-UNION-002 — polymorphic FK (enum discriminator + untyped id)

**Source**: [crates/lazuli_doctor/src/vocab/vocab_union_002.rs](https://github.com/lazuli-lang/lazuli/blob/main/crates/lazuli_doctor/src/vocab/vocab_union_002.rs)
**Severity**: `warning` (strict), `error` (production)
**Reference**: not specified in the module header

### Trigger

Fires on the polymorphic foreign-key pair shape where an enum discriminator controls an untyped sibling id.

- Discriminator field name is one of `target`, `subject`, `attachment_target`, or `parent_target`.
- The discriminator type is a same-feature enum with at least two variants.
- The sibling FK field is `<discriminator>_id`.
- The FK field type is `ID` or `Text`, not a typed resource reference.

### Example — violation

```lzi
feature comment
  enum CommentTarget
    Issue
    Customer
  resource Comment
    target: CommentTarget required
    target_id: ID required
    body: Text required
```

### Example — canonical fix

```lzi
feature comment
  union Comment
    OnIssue
      issue: Issue required
      body: Text required
    OnCustomer
      customer: Customer required
      body: Text required
```

### Message

> resource `Comment` declares `target` as enum `CommentTarget` (Issue, Customer) plus untyped FK `target_id`; suggestion: split into a discriminated union OR sibling typed-FK resources.
>
> union Comment
>     OnIssue
>       issue: Issue required
>       ...common fields...
>     OnCustomer
>       customer: Customer required
>       ...common fields...
>
> Or, for the typed-FK-per-resource form (vocabulary already supports this):
>   resource IssueComment
>     issue: Issue required
>     ...
>   resource CustomerComment
>     customer: Customer required
>     ...
