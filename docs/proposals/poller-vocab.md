# Proposal — `kind poller` Async Resolution Loops

**Status:** L0 v0.1 — 2026-05-14. First draft. Not yet architect-graded.
**Author:** Claude Opus 4.7 (orchestrator)
**Audit-ready target:** ≥ 9.0 via `lazuli-language-architect`
**Roadmap row:** `docs/proposals/corbanx-class-readiness.md` gap #5.
**Depends on:** `Job`/`Webhook` IR lift (`docs/proposals/bucket-jobs-scope.md`), existing `audit` / `emits` / `policy` / `retry` / `tenant_from` / `idempotency` vocabulary on jobs.
**Companion runtime cell:** `runtime/go/lazuli/poller/` (sibling of `runtime/go/lazuli/jobs/`); design sketch in §6, full cell deferred to L2.
**Honors:** `docs/invariants.md`, `docs/design-principles.md` (Rule Zero — Vocabulary Over Mechanism), `docs/architecture.md` §"Founding principle" (poller NAMES the resolution loop; runtime is wire over `time.Ticker` + `pgx`).

---

## §1. Problem

Real production backends carry one pattern Lazuli's current `kind job` vocabulary cannot express **without falling back to handlers**: a **persistent cursor row that is revisited until resolved**.

The anchoring concrete case is `c:/Users/lucas/dev-trabalho/corbanx/apps/api/src/features/multi-bank/multi-bank.repo.ts` (~ 460 LOC of Express + Drizzle) plus the `v8_pending_consults` and `v8_pending_multibank_consults` tables (`c:/Users/lucas/dev-trabalho/corbanx/packages/database/src/schema/introspected.ts`). The shape repeats across **three** vendor integrations in the same product (V8, Drex, GoFintech) and across **four** other client backends the author has seen in the field. The same fields recur each time:

| Field | Purpose |
|---|---|
| `cpf, name, birth_date, mother_name, phone, gender` | Subject identity carried into the vendor call |
| `status_v8` (or `status_drex`, `status_gofintech`) | Last vendor-reported state for this row |
| `consult_id` | External request handle (returned by first call) |
| `attempts` | Counter; bounded by `max_attempts` policy |
| `next_check_at` | Earliest wall-clock time the row is eligible again |
| `resolved_at` | Nullable terminal marker; `IS NULL` means still pending |
| `final_status, final_resultado` | Frozen value once terminal |
| `gender_retry_count` | One-shot retry quirk: flip ambiguous gender once, re-call |

The polling loop reads the table where `resolved_at IS NULL AND next_check_at <= NOW()`, calls the bank API for each row, and on the response either (a) updates `status_v8`, increments `attempts`, recomputes `next_check_at` via exponential backoff, or (b) writes `resolved_at = NOW()`, `final_status`, `final_resultado`. A separate "gender flip" quirk handles ambiguous responses: if `status_v8 == "GENDER_AMBIGUOUS"` and `gender_retry_count < 1`, flip the gender field and re-call before declaring failure.

### §1.1 What today's vocabulary forces authors to write

In Lazuli vocabulary as of 2026-05-14, the only construct that even comes close to this pattern is **`job` with `trigger schedule`**. Authors who try to express the multi-bank pattern with `job` end up writing something like:

```lzi
job poll_v8_pending
  trigger schedule "*/30 * * * * *"   # every 30 seconds — uncomfortable
  fanout tenants org
  idempotency by tenant.org_id, schedule.tick
  retry 3 backoff exponential
  handler "./jobs/poll_v8_pending.go"  # 250+ LOC, opaque
```

The handler then:

1. Selects pending rows (own SQL).
2. Loops per row, calls the vendor (own HTTP).
3. Updates row state (own SQL).
4. Computes `next_check_at` (own backoff math).
5. Handles the gender-flip quirk (own conditional).
6. Idempotency conditional on `attempts` (own optimistic concurrency).

Six concerns. **Zero of them are declared in `.lzi`.** Doctor, inspect, LSP, and SDK projection know only that "a job runs every 30 seconds with a handler". The actual **shape of the pending row, the cursor fields, the terminal contract, the retry quirk, the multi-tenant flow** are all invisible. The framework lights up zero static checks on what is genuinely **the load-bearing piece** of the integration.

This is textbook Rule Zero pressure: a pattern that repeats across products (V8, Drex, GoFintech, plus three external client references), forces ~250 LOC of handler per occurrence, and gives the language nothing to typecheck. Today's `job` is a one-shot fire-and-forget; pollers carry **persistent cursor state**, and that's the distinguishing primitive.

### §1.2 Why `job` cannot just absorb this

A schedule-triggered `job` and a `poller` are different contracts on five axes that matter for cold-readability:

| Axis | `job trigger schedule` | `poller` |
|---|---|---|
| **State** | Stateless; each tick is independent | Per-row persistent cursor (`attempts`, `next_check_at`, `resolved_at`) |
| **Lifetime** | One execution per tick, cron-bounded | One row exists for hours/days until it resolves or exhausts retries |
| **Idempotency surface** | Tick-level (one execution per cron slot) | Row-level (one resolution per pending row, conditional on `attempts` counter) |
| **Termination** | Implicit (next tick replaces it) | Explicit terminal state (`resolved_at` set; row no longer eligible) |
| **Quirks** | Backoff is between failed ticks | Backoff is between consecutive checks of the **same row**; per-state retry quirks like gender-flip apply |

Folding pollers into `job` would lose all five axes from the declared surface — exactly the Rule Zero anti-pattern. The right move is **name the contract**.

### §1.3 What `poller` is NOT (boundary)

Same closed-table discipline as `lifecycle-vocab.md` §2:

| `poller` CAN declare | `poller` CANNOT declare |
|---|---|
| "this resource is the cursor table; these fields are the cursor (`next_check_at`, `attempts`, `resolved_at`)" | "render the pending row as a UI table" |
| "transition pending → resolved when `@fn.poll_v8` returns `resolved: true`" | "call provider X with auth Y" — vendor mechanics live in `@fn.<handler>` or `@adapter.<name>` |
| "max_attempts 30, backoff exponential base 30s cap 10m" | "after 30 minutes alert someone" — alerting is `notification` |
| "per-state quirk: flip gender once if `status_v8 == \"GENDER_AMBIGUOUS\"`" | "if attempts > 5 and tenant tier is gold then…" — predicate engine, REJECTED |
| "terminal states {resolved, failed} freeze the row" | "after terminal, run cleanup job X" — that's a separate `job trigger event` |

Every primitive in §3 is tested against that table. If a child would force a predicate engine or an unbounded user-defined sublanguage into the IR, it's **rejected** (annotated in §9).

---

## §2. Distinct-from neighbors (the polysemy guard)

`poller` is novel vocabulary. Cold-readers must immediately know what it is **not**.

| Construct | Trigger | Persistent cursor row? | Terminal contract |
|---|---|---|---|
| `job trigger event <ref>` | Event published | No | Implicit; one handler execution |
| `job trigger schedule "<cron>"` | Cron tick | No | Implicit; one handler execution per tick |
| `webhook <name>` | Inbound HTTP | No (request is the cursor) | Implicit; one verification + handler |
| `notification <name>` | Event published | No (one dispatch per recipient) | Implicit; delivered or DLQ'd |
| `agent <name>` | Direct call | No | Synchronous answer (or stream) |
| `workflow <name> on <Resource>.<field>` | Command-driven | Resource carries state, but not a cursor | Authored transitions |
| `lifecycle <field>` (proposal `lifecycle-vocab.md`) | Command-driven | Single discriminator field on the resource itself | Authored terminal states |
| **`poller <name>`** (this proposal) | **Wall-clock tick scanning a source resource** | **Yes — explicit `cursor` fields on `source`** | **Authored terminal states; row is "resolved" or "failed"** |

The diagnostic identity of `poller` is the **persistent cursor row revisited until resolved**. Without that, the right vocabulary is `job` or `lifecycle`.

`POLLER-DUAL-SCHEDULER-001` (§5) statically rejects authoring **both** a `poller` and a `job trigger schedule` over the same source resource — that's the cold-reader trap (two clocks ticking on the same table).

---

## §3. Grammar — closed catalog

### §3.1 Block shape

`poller <name>` is a **feature-level kind**, parallel siblings to `command`, `query`, `job`, `webhook`, `notification`, `agent`. Block indentation matches existing feature children.

```lzi
feature multi_bank
  purpose "Resolve pending consults against V8 with per-row cursors and bounded retry."

  defaults
    tenancy org
    timestamps

  uses org

  requires integration v8: ConsultProvider

  domain
    enum V8Status
      pending
      gender_ambiguous
      consult_completed
      consult_failed

    resource V8PendingConsult
      org: Organization required
      cpf: Text required
      name: Text required
      birth_date: Date required
      mother_name: Text required
      phone: Text required
      gender: Gender required
      status_v8: V8Status = pending
      consult_id: Text
      attempts: Integer = 0
      next_check_at: DateTime required
      resolved_at: DateTime
      final_status: ConsultFinalStatus
      final_resultado: JSON
      gender_retry_count: Integer = 0

  policies
    read: @scope.same_org
    operate: @actor.system

  poller v8_consult_resolver
    source V8PendingConsult
    cursor
      eligible_when next_check_at, resolved_at
      attempts attempts
    retry
      max_attempts 30
      backoff exponential base 30s cap 10m
    states
      pending intermediate
      gender_ambiguous intermediate
      resolved terminal
      failed terminal
    resolve via @fn.poll_v8
    terminal_status_field final_status
    terminal_result_field final_resultado
    tick every 15s batch 100
    tenant_from row.org_id
    idempotency by row.id, row.attempts
    audit default
    emits v8_consult_resolved
    emits v8_consult_failed
    retry_quirk gender_flip_once
      when row.status_v8 == "gender_ambiguous"
      counter gender_retry_count
      mutate row.gender = flip(row.gender)
```

The closed catalog of `poller` children: **`source`** (required), **`cursor`** (required), **`retry`** (required), **`states`** (required, ≥2), **`resolve`** (required), **`terminal_status_field`** (optional), **`terminal_result_field`** (optional), **`tick`** (optional, runtime defaults), **`tenant_from`** (required when the feature has a tenant axis), **`idempotency`** (required), **`audit`** (optional, defaults to `audit default`), **`emits`** (0..N), **`retry_quirk`** (0..N from a closed catalog).

No other keyword is accepted at the `poller` indent level — closed-catalog discipline like `notification.throttle` (`docs/invariants.md:181-188`) and `lifecycle` (`docs/proposals/lifecycle-vocab.md`).

### §3.2 `source` — the cursor table

```
source <Resource>
```

- `<Resource>` is a same-feature resource. Cross-feature is REJECTED in v0.1 (the cursor table belongs to the feature owning the poller; that's the same ownership rule resource→command has today). OQ-1 tracks the cross-feature need.
- The resource MUST declare cursor fields (§3.3). Doctor: `POLLER-CURSOR-MISSING-001` lists the missing fields verbatim.
- The resource MUST carry a tenant FK when the feature declares `defaults tenancy <axis>`. Doctor: `POLLER-TENANT-FIELD-MISSING-001`.

### §3.3 `cursor` — required field shape

```
cursor
  eligible_when <next_at_field>, <resolved_at_field>
  attempts <attempts_field>
```

Three closed slots, each naming a field on `source`:

| Slot | Type | Semantics |
|---|---|---|
| `eligible_when <next_at_field>` | `DateTime required` | The wall-clock field the tick reads; a row is eligible iff `<next_at_field> <= NOW()`. |
| `eligible_when ..., <resolved_at_field>` | `DateTime` (nullable) | The terminal marker; a row is eligible iff `<resolved_at_field> IS NULL`. |
| `attempts <attempts_field>` | `Integer required default 0` | Counter incremented on every non-terminal handler return. Required for idempotency. |

The `eligible_when` slot takes **two field names** as a fixed-position pair (same closed-catalog discipline as `lifecycle` `invariant single <state> per <scope_field>`). v0.1 fixes this pair; v0.2+ may grow if a real fixture genuinely needs a third axis (e.g. `pause_until`), gated on triple-dogfood evidence (OQ-2).

Doctor: `POLLER-CURSOR-FIELD-TYPE-001` enforces:

- `next_at_field` resolves to `DateTime required` on `source`.
- `resolved_at_field` resolves to `DateTime` (nullable) on `source`.
- `attempts_field` resolves to `Integer required default 0` on `source`.

If any of the three is missing or wrong-typed, the doctor diagnostic includes the **verbatim correct field declaration** as a fix suggestion.

### §3.4 `retry` — bounded policy

```
retry
  max_attempts <int>
  backoff <strategy> [base <duration>] [cap <duration>]
```

- `max_attempts` is a positive integer. Required. No default — the canonical case is "we WILL stop polling eventually", and silent unboundedness is a footgun (`POLLER-MAX-RETRIES-UNBOUNDED-001`).
- `backoff` strategy is **closed catalog**: `fixed`, `linear`, `exponential`. Same set as `job retry` (`docs/invariants.md` §"Events"), reused.
- `base <duration>` is the first wait duration. Required for `linear` and `exponential`. Optional for `fixed` (defaults to the runtime default; `fixed` without `base` is allowed because every row uses the runtime's default cadence).
- `cap <duration>` is the upper bound for `exponential`. Optional but **strongly recommended**; runtime caps at 1h if omitted to prevent runaway delays. Doctor: `POLLER-EXPONENTIAL-NO-CAP-001` warns when omitted.

Duration literal catalog: `<integer><s|m|h|d>` (same closed set as `@cap.Token.ttl` and notification throttle — `docs/invariants.md:413-415`).

When `attempts` reaches `max_attempts` and the handler still hasn't returned terminal, the runtime writes a synthetic terminal state (declared in `states`; doctor enforces at least one `terminal` state exists). `terminal_status_field` (§3.7) captures the synthetic value (defaults to `failed`).

### §3.5 `states` — declared response space

```
states
  <name> [initial | intermediate | terminal]
  ...
```

States enumerate the **vendor-reported response space** for this poller. They are LOGICAL labels the handler returns; they don't need to map 1:1 to the source enum (the handler does the translation).

- Exactly one state may carry `initial`. If omitted, the first state listed is initial.
- At least one state must carry `terminal`. Doctor: `POLLER-NO-TERMINAL-001` (mirrors `LIFECYCLE-UNREACHABLE-STATE`).
- States carrying `terminal` are absorbing: handler-returned terminal states cause the runtime to set `resolved_at = NOW()` + `terminal_status_field = <state>` and stop revisiting the row.
- Intermediate states cause the runtime to recompute `next_check_at` via the `retry.backoff` strategy and re-enqueue the row for the next eligible tick.

### §3.6 `resolve via @fn.<name>`

```
resolve via @fn.<handler>
```

Names the typed handler the runtime calls per row. The handler's typed signature is **derived** from the poller declaration:

```go
// Codegen-derived signature for `poller v8_consult_resolver`.
func PollV8(ctx *lazuli.Ctx, row V8PendingConsult) (poller.ResolveResult[V8Status, ConsultFinalStatus, V8PendingConsultResult], error)
```

Where `poller.ResolveResult` is a typed sum carrying:

```go
type ResolveResult[State ~string, Terminal ~string, Result any] struct {
    // Set when the handler reached a terminal state.
    Terminal *TerminalResult[Terminal, Result]
    // Set when the handler is still pending; the row will be re-checked.
    Pending  *PendingResult[State]
}

type TerminalResult[Terminal ~string, Result any] struct {
    Status  Terminal       // mapped into `terminal_status_field`
    Result  Result         // mapped into `terminal_result_field` (JSON)
}

type PendingResult[State ~string] struct {
    Status        State       // mapped into the cursor's `status_*` mirror field (optional)
    NextCheckAt   *time.Time  // optional override; defaults to backoff-computed
    ConsultID     *string     // optional; set on first successful vendor handshake
}
```

The handler signature is **generated**; the author writes the body. This is the same shape as today's `@fn.<name>` handler binding (`docs/invariants.md` §Validation), specialized to the poller's row type + state enum.

The handler MAY be called twice for the same row + same `attempts` value (scheduler crash recovery — §10 risk #1). The handler MUST be idempotent against this; `idempotency by row.id, row.attempts` enforces conditional UPDATE at the runtime layer.

Doctor: `POLLER-HANDLER-ORPHAN-001` fires when `@fn.<handler>` isn't declared under feature `extensions`. `POLLER-HANDLER-SIGNATURE-001` fires when the handler is declared with a wrong type.

### §3.7 `terminal_status_field` / `terminal_result_field`

```
terminal_status_field <field>
terminal_result_field <field>
```

Both optional. Each names a same-resource field the runtime writes when a row enters a terminal state.

- `terminal_status_field` — same-resource field. Type must be a terminal-only enum (an `enum` declaration where every variant matches a `terminal` state in the poller's `states` block). Doctor: `POLLER-TERMINAL-FIELD-ENUM-001`.
- `terminal_result_field` — same-resource field of type `JSON`. Used to capture the vendor's response body. Doctor: `POLLER-TERMINAL-RESULT-JSON-001`.

If omitted, the terminal write only sets `resolved_at`; per-state outcome lives entirely in the source resource's existing fields (the handler is free to write them via the row's own update path).

### §3.8 `tick` — runtime cadence

```
tick every <duration> [batch <int>]
```

- `every <duration>` — wall-clock interval between scans. Defaults to `30s`. Closed unit catalog.
- `batch <int>` — maximum rows the scheduler processes per tick. Defaults to `100`. Positive integer.

Both optional; the framework's defaults work for the canonical CRM-style load. Doctor: `POLLER-TICK-TOO-FAST-001` warns when `every < 5s` (cold-readers spot the footgun; runtime is wire over `time.Ticker`).

### §3.9 `tenant_from row.<axis>_id`

```
tenant_from row.<axis>_id
```

Same vocabulary as `job tenant_from payload.<axis>_id` (`docs/invariants.md:320`), specialized for pollers — the cursor row is the producer, so the path roots at `row.*` instead of `payload.*`. Closed-form locator (parallels `payload.*` / `envelope.*` / `route.*` etc. in the locator catalog — `docs/invariants.md:296-298`).

Required when the feature declares `defaults tenancy <axis>`. Doctor: `POLLER-TENANT-FROM-MISSING-001` (lift of the existing `event_job_tenant_from_diagnostics`).

The handler runs inside the resolved tenant context — same generator as `job tenant_from` produces today.

### §3.10 `idempotency by row.<field>, ...`

```
idempotency by row.<field>[, row.<field>, ...]
```

Required. Identifies the row + counter that scope a single resolution attempt. Canonical form is `idempotency by row.id, row.attempts` — same row, same attempt counter, exactly one handler effect commits.

The runtime enforces idempotency via conditional UPDATE: `UPDATE <table> SET ... WHERE id = $1 AND attempts = $2`. A handler that ran twice (scheduler crash; §10) will fail the second commit silently — the row state already advanced.

Doctor: `POLLER-IDEMPOTENCY-MISSING-001` (required), `POLLER-IDEMPOTENCY-ATTEMPTS-MISSING-001` (`row.attempts` must be in the key list; warns otherwise — without it, retry-after-crash double-commits).

### §3.11 `audit` — reuses existing AuditSpec

```
audit default | audit <field>[, <field>] | audit none reason "..."
```

Same syntax as commands and jobs (`docs/invariants.md:93-97`). Reuses the existing `AuditSpec` IR struct. Defaults to `audit default` (records actor=`@actor.system`, target=`row.id`, terminal_status if set).

### §3.12 `emits` — reactive events

```
emits <event>
```

Same syntax as command/job `emits`. The runtime publishes the named event **after** the row commits a state change. The closed list:

- On terminal transition: `emits v8_consult_resolved` fires when `terminal_status_field` matches a non-failure terminal; `emits v8_consult_failed` fires when matches a failure terminal (or `max_attempts` is reached).
- Authors declare one `emits` per terminal state they care about (cold-reader sees the reactive graph).

Doctor: `POLLER-EMITS-TERMINAL-001` fires when an event is emitted but no transition declares it. The emitted event payload is derived from `event_group ... on <Resource>` (`docs/invariants.md:317-319`) — same shape as command emits.

### §3.13 `retry_quirk` — CLOSED CATALOG

The grammar most at risk of Rule Zero violation. **Resolution: only named catalog forms accept; no predicate sublanguage at the poller layer.** Same closed-catalog discipline as `lifecycle invariant`.

```
retry_quirk <kind>
  when <closed_predicate>
  counter <field>
  mutate <field> = <closed_transform>
```

v0.1 catalog of `<kind>`:

| Kind | Semantics | Example |
|---|---|---|
| `gender_flip_once` | Flip the row's `gender` field once when the predicate matches; increment `counter`. After the flip, the handler is re-called immediately (no backoff). Pre-condition: `counter < 1`. | Anchoring V8 example. |
| `field_swap_once` | Same as `gender_flip_once` but generic: swap the named field between two declared enum values once. Authors declare `from <value> to <value>`. | If 2+ products want it; OQ-3. |

The `when` clause uses the existing closed predicate language (the one already accepted in `rules`, `tests`, `filters` — `docs/invariants.md:472-475`). No new sublanguage.

The `mutate` clause uses **closed transform names**: v0.1 supports `flip(<field>)` (gender-only), `set(<field>, <literal>)`. No expression sublanguage. Authors needing arbitrary mutation drop to a separate `command` triggered from `emits`.

v0.1 ships ONE catalog form (`gender_flip_once`) and the generic skeleton — the runtime can pattern-match on `kind` and dispatch. Adding a form requires (a) ≥2 products needing it, (b) closed-form name, (c) doctor enforceability — same rule as `lifecycle invariant`.

Doctor: `POLLER-QUIRK-CATALOG-MISMATCH-001` fires when `<kind>` isn't in the catalog. `POLLER-QUIRK-FIELD-UNRESOLVED-001` fires when the `counter` / `mutate` field doesn't exist on `source`.

### §3.14 Grammar summary (EBNF)

```ebnf
poller = "poller" IDENT NEWLINE INDENT
         source_clause
         cursor_clause
         retry_clause
         states_clause
         resolve_clause
         [ terminal_status_clause ]
         [ terminal_result_clause ]
         [ tick_clause ]
         [ tenant_from_clause ]
         idempotency_clause
         [ audit_clause ]
         emits_clause*
         retry_quirk_clause*
         DEDENT

source_clause = "source" IDENT NEWLINE

cursor_clause = "cursor" NEWLINE INDENT
                "eligible_when" IDENT "," IDENT NEWLINE
                "attempts" IDENT NEWLINE
                DEDENT

retry_clause = "retry" NEWLINE INDENT
               "max_attempts" INTEGER NEWLINE
               "backoff" RETRY_STRATEGY [ "base" DURATION ] [ "cap" DURATION ] NEWLINE
               DEDENT

states_clause = "states" NEWLINE INDENT
                ( IDENT [ "initial" | "intermediate" | "terminal" ] NEWLINE )+
                DEDENT

resolve_clause = "resolve" "via" "@fn." IDENT NEWLINE

terminal_status_clause = "terminal_status_field" IDENT NEWLINE
terminal_result_clause = "terminal_result_field" IDENT NEWLINE

tick_clause = "tick" "every" DURATION [ "batch" INTEGER ] NEWLINE

tenant_from_clause = "tenant_from" "row." IDENT NEWLINE

idempotency_clause = "idempotency" "by" "row." IDENT ( "," "row." IDENT )* NEWLINE

audit_clause = "audit" ( "default" | IDENT ( "," IDENT )* | "none" "reason" STRING ) NEWLINE

emits_clause = "emits" IDENT NEWLINE

retry_quirk_clause = "retry_quirk" QUIRK_KIND NEWLINE INDENT
                     "when" PREDICATE NEWLINE
                     "counter" IDENT NEWLINE
                     "mutate" IDENT "=" QUIRK_TRANSFORM NEWLINE
                     DEDENT

QUIRK_KIND = "gender_flip_once"   (* v0.1 closed set *)
RETRY_STRATEGY = "fixed" | "linear" | "exponential"   (* closed catalog *)
DURATION = INTEGER ( "s" | "m" | "h" | "d" )           (* closed unit catalog *)
QUIRK_TRANSFORM = "flip" "(" IDENT ")"
                | "set" "(" IDENT "," LITERAL ")"
```

No ambiguity with existing feature grammar. The `poller` keyword is new and shares no surface with `job`/`webhook`/`agent`/`notification`/`lifecycle`/`workflow`. The `tick every <duration>` form is intentionally distinct from `trigger schedule "<cron>"` — pollers don't take cron expressions (the cadence is interval-based; cron is a `job` shape).

---

## §4. IR shape

### §4.1 New types

Additive on `crates/lazuli_ir/src/lib.rs`. Sibling of `Job` (`:1684`), `Webhook` (`:1769`), and `Notification` (`bucket-jobs-scope.md` Fix 2). Bumps `LZIR_SCHEMA` minor.

```rust
// crates/lazuli_ir/src/lib.rs — additive; bump LZIR_SCHEMA minor.

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Poller {
    pub name: String,

    /// Same-feature resource holding the pending rows.
    pub source: ResourceRef,

    /// Cursor field bindings (three named fields on `source`).
    pub cursor: PollerCursor,

    /// Bounded retry policy.
    pub retry: PollerRetry,

    /// Declared state space. ≥2 entries; ≥1 terminal.
    pub states: Vec<PollerState>,

    /// Resolution handler. `@fn.<name>` reference; declared under
    /// the feature's `extensions` block.
    pub resolve_handler: HandlerRef,

    /// Optional same-resource field receiving the terminal status.
    /// Type must be a closed enum over the terminal states.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_status_field: Option<String>,

    /// Optional same-resource field receiving the terminal result.
    /// Type must be `JSON`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_result_field: Option<String>,

    /// Tick cadence. Defaults applied at codegen if omitted in source.
    pub tick: PollerTick,

    /// Tenant axis derivation (`row.<axis>_id`). Required when the
    /// feature has a tenant axis. Mirrors `Job.tenant_from` shape but
    /// keyed on `row.*` instead of `payload.*`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_from: Option<TenantFromSpec>,

    /// Idempotency key. Required. Canonical: `row.id, row.attempts`.
    pub idempotency: IdempotencyKey,

    /// Audit subjects; defaults to `AuditSpec::Default` when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<AuditSpec>,

    /// Reactive events published after row commits a state change.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,

    /// Retry quirks — closed catalog (§3.13).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retry_quirks: Vec<PollerRetryQuirk>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollerCursor {
    /// Name of the `DateTime required` field whose value gates eligibility.
    pub next_at_field: String,
    /// Name of the nullable `DateTime` field whose `IS NULL` gates eligibility.
    pub resolved_at_field: String,
    /// Name of the `Integer required default 0` field tracking attempts.
    pub attempts_field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollerRetry {
    pub max_attempts: u32,
    pub backoff: PollerBackoff,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "strategy")]
pub enum PollerBackoff {
    Fixed { base: Option<Duration> },
    Linear { base: Duration, cap: Option<Duration> },
    Exponential { base: Duration, cap: Option<Duration> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollerState {
    pub name: String,
    pub kind: PollerStateKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PollerStateKind {
    Initial,
    Intermediate,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollerTick {
    /// Defaults to 30s if not declared in source.
    pub every: Duration,
    /// Defaults to 100 if not declared in source.
    pub batch: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum PollerRetryQuirk {
    /// `retry_quirk gender_flip_once when row.<field> == "<value>"
    ///   counter <counter_field>
    ///   mutate row.<gender_field> = flip(row.<gender_field>)`
    GenderFlipOnce {
        when: Predicate,
        counter_field: String,
        gender_field: String,
    },
    // Future: FieldSwapOnce { ... } — gated on triple-dogfood evidence.
}
```

### §4.2 Resource extension — additive

```rust
pub struct Feature {
    // ...existing fields unchanged...
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pollers: Vec<Poller>,   // NEW
}
```

ABI-additive: existing fixtures and snapshot tests with `Feature { pollers: vec![], .. }` continue to deserialize.

### §4.3 Inspect projection — new `--expand=pollers`

```rust
#[serde(skip_serializing_if = "Vec::is_empty")]
pollers: Vec<InspectPoller>,
```

Mirrors `InspectJob` from `bucket-jobs-scope.md` Fix 3:

```rust
struct InspectPoller {
    name: String,
    source: String,                          // `V8PendingConsult`
    cursor: InspectPollerCursor,             // { next_at_field, resolved_at_field, attempts_field }
    retry: InspectPollerRetry,               // { max_attempts, backoff, base, cap }
    states: Vec<InspectPollerState>,         // [{ name, kind }]
    resolve_handler: String,                 // `@fn.poll_v8`
    terminal_status_field: Option<String>,
    terminal_result_field: Option<String>,
    tick: InspectPollerTick,                 // { every, batch }
    tenant_from: Option<String>,             // `row.org_id`
    idempotency: String,                     // `by row.id, row.attempts`
    audit: Option<String>,
    emits: Vec<String>,
    retry_quirks: Vec<InspectPollerQuirk>,
    origin: &'static str,                    // "poller"
}
```

Inspect emits this under `lazuli inspect --expand=pollers` (new) and `lazuli inspect --expand=summary` (adds the count to the summary projection — cold-readability for LLM agents).

---

## §5. Doctor rules

Each rule is single-file under `crates/lazuli_cli/src/doctor/poller/` and registers in `mod.rs` via a one-line additive edit (same pattern as `lifecycle-vocab.md` §5).

| Rule | Severity (strict / prod) | Fires when |
|---|---|---|
| `POLLER-CURSOR-MISSING-001` | error / error | `source` resource lacks one or more cursor fields (`next_at`, `resolved_at`, `attempts`). Diagnostic includes the verbatim correct field block as suggestion. |
| `POLLER-CURSOR-FIELD-TYPE-001` | error / error | A cursor field exists but is wrong-typed (e.g. `attempts: Text` instead of `Integer`). |
| `POLLER-NO-TERMINAL-001` | error / error | `states` list has no `terminal` entry. (Same shape as `LIFECYCLE-UNREACHABLE-STATE` from `lifecycle-vocab.md` §5.) |
| `POLLER-HANDLER-ORPHAN-001` | error / error | `resolve via @fn.<name>` references a handler not declared under feature `extensions`. (Same shape as today's `validate_handler_orphan_diagnostics`.) |
| `POLLER-HANDLER-SIGNATURE-001` | error / error | The handler's `Function[...]` type in `extensions` doesn't match the row + state types derived from the poller. |
| `POLLER-MAX-RETRIES-UNBOUNDED-001` | error / error | `retry` block lacks `max_attempts`, OR `max_attempts > 1000` (sanity cap). |
| `POLLER-EXPONENTIAL-NO-CAP-001` | warning / error | `backoff exponential` declared without `cap` — wait can grow unboundedly. |
| `POLLER-DUAL-SCHEDULER-001` | error / error | Same feature declares **both** a `poller <name> source <Resource>` and a `job trigger schedule` whose handler walks `<Resource>`. Heuristic: any `job` with `handler "./..."` AND `trigger schedule` whose handler file matches `*<resource_snake_case>*` triggers the warning; explicit `target query.<...> over <Resource>` upgrades to error. |
| `POLLER-TENANT-FROM-MISSING-001` | error / error | Feature has `defaults tenancy <axis>` but poller lacks `tenant_from row.<axis>_id`. (Lift of `event_job_tenant_from_diagnostics`.) |
| `POLLER-TENANT-FIELD-MISSING-001` | error / error | `tenant_from row.<axis>_id` references a field that doesn't exist on `source`. |
| `POLLER-IDEMPOTENCY-MISSING-001` | error / error | No `idempotency` block. |
| `POLLER-IDEMPOTENCY-ATTEMPTS-MISSING-001` | warning / error | `idempotency by` doesn't include the cursor's `attempts_field` — opens crash-recovery double-commit window. |
| `POLLER-TERMINAL-FIELD-ENUM-001` | error / error | `terminal_status_field` names a field whose enum variants don't match the poller's terminal states. |
| `POLLER-TERMINAL-RESULT-JSON-001` | error / error | `terminal_result_field` names a field whose type isn't `JSON`. |
| `POLLER-EMITS-TERMINAL-001` | warning / error | `emits <event>` declared but no terminal state references it (orphan event). |
| `POLLER-TICK-TOO-FAST-001` | warning / warning | `tick every <duration>` is < 5s (foot-gun against the database). |
| `POLLER-QUIRK-CATALOG-MISMATCH-001` | error / error | `retry_quirk <kind>` isn't in the §3.13 catalog. |
| `POLLER-QUIRK-FIELD-UNRESOLVED-001` | error / error | `counter <field>` or `mutate <field>` names a field not on `source`. |
| `POLLER-SOURCE-CROSS-FEATURE-001` | error / error | `source <Resource>` references a resource from another feature. (v0.1 closed boundary; OQ-1.) |

19 native rules. All ship in the same wave as the language vocabulary — per Rule Zero, the lints and the named primitive land together.

`POLLER-DUAL-SCHEDULER-001` is the most novel and the most valuable for AI-first cold-readability: an LLM seeing both a `poller` and a `job trigger schedule` over the same resource will be confused; the diagnostic prevents the source from ever reaching that shape.

---

## §6. Codegen + Runtime (out-of-scope for this proposal; sketched for L2 cells)

### §6.1 Codegen sketch — `dist/go/<feature>/<feature>_poller.gen.go`

One file per feature with pollers. Emits a `RegisterPollers(r *lazuli.PollerRegistry)` function the boot path calls. Mirrors the `RegisterJobs` pattern from `bucket-jobs-cycle.md` §"Codegen proposto" verbatim.

```go
// path: dist/go/multi_bank/multi_bank_poller.gen.go
// Code generated by lazuli; DO NOT EDIT.
package multi_bank

import (
	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/poller"
)

func RegisterPollers(r *poller.Registry) {
	r.Register(poller.Spec[V8PendingConsult, V8Status, ConsultFinalStatus, V8PendingConsultResult]{
		Name:                  "multi_bank.v8_consult_resolver",
		Source:                "v8_pending_consults",
		Cursor: poller.Cursor{
			NextAtField:     "next_check_at",
			ResolvedAtField: "resolved_at",
			AttemptsField:   "attempts",
		},
		Retry: poller.Retry{
			MaxAttempts: 30,
			Backoff:     poller.Exponential{Base: 30 * time.Second, Cap: 10 * time.Minute},
		},
		States: []poller.State{
			{Name: "pending", Kind: poller.Initial},
			{Name: "gender_ambiguous", Kind: poller.Intermediate},
			{Name: "resolved", Kind: poller.Terminal},
			{Name: "failed", Kind: poller.Terminal},
		},
		Resolve:              pollV8Handler,  // author-supplied in ./domain/poll_v8.go
		TerminalStatusField:  "final_status",
		TerminalResultField:  "final_resultado",
		Tick:                 poller.Tick{Every: 15 * time.Second, Batch: 100},
		TenantFrom:           "row.org_id",
		Idempotency:          "row.id, row.attempts",
		Emits:                []string{"v8_consult_resolved", "v8_consult_failed"},
		RetryQuirks: []poller.Quirk{
			poller.GenderFlipOnce{
				When:           poller.Predicate{`row.status_v8 == "gender_ambiguous"`},
				CounterField:   "gender_retry_count",
				GenderField:    "gender",
			},
		},
	})
}
```

`poller.Spec[Row, State, Terminal, Result]` is generic over four type parameters; codegen knows them all from the IR (`source`'s row type; the auto-generated `<Source>Status` enum; the `terminal_status_field`'s enum; the `terminal_result_field`'s deserialized type). The handler signature is **derived**; the author writes the body in `./domain/poll_v8.go` matching the generated type.

### §6.2 Runtime sketch — `runtime/go/lazuli/poller/`

Wire-thin per `docs/architecture.md` founding principle and the `CLAUDE.md` "the runtime is wire" rule. Three files, totaling ~280 LOC:

```
runtime/go/lazuli/poller/
├── contract.go         # Registry, Spec, ResolveResult, Cursor, Quirk types       (~80 LOC)
├── scheduler.go        # Ticker loop, batch SELECT, conditional UPDATE             (~120 LOC)
└── poller_test.go      # synctest-based tests for the resolve loop                 (~80 LOC)
```

`scheduler.go` is the load-bearing file:

- A goroutine per registered poller. Each goroutine ticks at `Spec.Tick.Every`.
- Each tick: one parameterized SELECT (`WHERE <next_at_field> <= NOW() AND <resolved_at_field> IS NULL LIMIT <batch>`).
- For each row: run the handler under the resolved tenant context.
- Handler returns: if terminal, conditional UPDATE setting `<resolved_at_field> = NOW()` + `<terminal_status_field> = result.Status` + `<terminal_result_field> = result.Result`. If pending, conditional UPDATE incrementing `<attempts_field>` and setting `<next_at_field> = NOW() + backoff(attempts)`.
- Both UPDATEs use `WHERE id = $1 AND <attempts_field> = $2` for idempotency — if a second invocation races (scheduler crash recovery; §10 risk #1), the second commit is a no-op (`RowsAffected() == 0`).
- Retry quirks are applied as pre-handler hooks: if the quirk's `when` predicate matches and `counter < 1`, mutate the row and **re-call the handler immediately** without backing off.

The scheduler uses `pgx/v5` (already chosen — `project_db_driver_choice.md`) and `time.Ticker` (stdlib). No external dependency for the scheduler itself. The backoff math is `~10 LOC` of pure-function code (`func nextDelay(strategy, base, cap, attempts) time.Duration`).

**Founding-principle compliant**: ~280 LOC of wire over stdlib + `pgx`. Zero re-implementation of a scheduler library. Zero re-implementation of a state machine library. The poller IS the state machine, expressed in surface vocabulary, lowered to pure SQL UPDATEs.

### §6.3 Boot composition

`dist/go/main.gen.go` composes `RegisterPollers` from every generated feature into the runtime's `poller.Registry`. Deterministic order: alphabetical by feature, alphabetical by poller name. Identical to the `RegisterJobs` boot pattern.

The Lazuli runtime's `Boot` instantiates a single `poller.Scheduler`, calls `RegisterPollers` from every feature module, and launches one goroutine per registered spec. Shutdown drains in-flight handler calls respecting `tick.every` as a soft deadline.

### §6.4 No leaks

- No vendor name (`V8`, `Drex`, `GoFintech`, `Bankerize`, …) in the runtime.
- No `pgx` reference in any `.lzi` source.
- No `time.Ticker` reference in any `.lzi` source.
- All vendor mechanics flow through the `resolve via @fn.<handler>` escape — the handler's body lives in the consumer's repo, not in core.

---

## §7. AI-first sanity check

Lazuli's grading rubric gates on whether an LLM can author/read source cold. Three checks against the proposed surface:

### §7.1 Cold-read test

Give an LLM the §3.1 example (the `poller v8_consult_resolver` block alone, no docs) and ask "what does this do?". Expected answer the surface should produce verbatim:

> "This declares a polling resolution loop named `v8_consult_resolver` over the `V8PendingConsult` resource. Every 15 seconds, the runtime scans up to 100 rows where `next_check_at <= NOW()` and `resolved_at IS NULL`, calls `@fn.poll_v8` for each row, and either marks the row resolved (writing `final_status` and `final_resultado`) or schedules another check using exponential backoff (base 30s, cap 10m), with a maximum of 30 attempts. The loop runs scoped to the row's tenant (`org`), is idempotent on `(row.id, row.attempts)`, and applies a one-shot gender-flip quirk when the vendor returns `gender_ambiguous`. Terminal transitions publish `v8_consult_resolved` or `v8_consult_failed`."

Every clause in that answer maps 1:1 to a named child in the block. Zero clauses require external documentation.

### §7.2 Cold-author test

Spec: "Drex sync — poll `DrexPendingTransfer` until `final_status` is set, exponential backoff 60s base capped at 30m, max 20 attempts, terminal events `drex_transfer_completed` and `drex_transfer_failed`, multi-tenant by org, no retry quirks."

Expected LLM output (target):

```lzi
poller drex_transfer_resolver
  source DrexPendingTransfer
  cursor
    eligible_when next_check_at, resolved_at
    attempts attempts
  retry
    max_attempts 20
    backoff exponential base 60s cap 30m
  states
    pending initial
    completed terminal
    failed terminal
  resolve via @fn.poll_drex
  terminal_status_field final_status
  tenant_from row.org_id
  idempotency by row.id, row.attempts
  emits drex_transfer_completed
  emits drex_transfer_failed
```

An LLM trained only on this proposal + the `lifecycle-vocab.md` style should land this in one shot. No invented keywords. The closed catalogs (backoff strategy, state kind, duration unit) match the LLM's existing intuition from the rest of Lazuli vocabulary.

### §7.3 Negative-case the LSP catches

What does an LLM plausibly emit by mistake that doctor / LSP must catch?

| Likely LLM mistake | Diagnostic |
|---|---|
| Forgets `cursor` block | `POLLER-CURSOR-MISSING-001` with verbatim correct block |
| Names cursor fields that don't exist on `source` | `POLLER-CURSOR-FIELD-TYPE-001` |
| Forgets `max_attempts` | `POLLER-MAX-RETRIES-UNBOUNDED-001` |
| Forgets to mark any state `terminal` | `POLLER-NO-TERMINAL-001` |
| Reuses `tenant_from payload.org_id` from `job` vocabulary | Parser error (`tenant_from row.<field>` is the poller form); diagnostic suggests `row.org_id` |
| Writes `tick every "30s"` (string) instead of `tick every 30s` (literal) | Parser error; diagnostic shows the bare-literal form |
| Writes `retry_quirk my_custom_quirk` | `POLLER-QUIRK-CATALOG-MISMATCH-001` listing the valid catalog |

Every mistake an LLM could plausibly emit has a static diagnostic. Cold-read + cold-author + negative-case all pass.

---

## §8. Decomposition into L2 cells

Mechanical, single-file-per-cell where feasible (per `feedback_claude_plans_codex_executes.md`):

| Cell | Crate / Module | Files | LOC est. | Codex-able |
|---|---|---|---|---|
| **P.A.0** Refactor: factor the feature-body line dispatcher in `parser.rs` from flat else-if to handler-registry pattern if not already done (prereq for P.A.1/P.A.2 to run in parallel). | lazuli_syntax | parser.rs (refactor only) | +40 | Yes |
| **P.A.1** Parser: `poller <name>` block with `source` / `cursor` / `retry` / `states` / `resolve` / `terminal_status_field` / `terminal_result_field` / `tick` / `tenant_from` / `idempotency` / `audit` / `emits` children | lazuli_syntax | parser.rs | +220 | Yes |
| **P.A.2** Parser: closed-catalog `retry_quirk` (gender_flip_once + skeleton) | lazuli_syntax | parser.rs | +80 | Yes |
| **P.B.1** IR types — `Poller`, `PollerCursor`, `PollerRetry`, `PollerBackoff`, `PollerState`, `PollerStateKind`, `PollerTick`, `PollerRetryQuirk`; add `Feature.pollers: Vec<Poller>` | lazuli_ir | lib.rs | +180 | Yes |
| **P.B.2** Lowering — analyzer walks `PollerBlockAst → Poller IR`, resolves `@fn.<name>` against feature extensions, derives handler signature type | lazuli_analyzer | lowering.rs | +200 | Yes |
| **P.C.1** Codegen Go: emit `RegisterPollers` per feature; emit derived handler signature stubs | lazuli_codegen_go | poller.rs | +220 | Yes |
| **P.C.2** Codegen TS: emit type-only projection (admin SDK can list pending rows by their resource type; pollers themselves have no client-facing surface) | lazuli_codegen_ts | poller.rs | +60 | Yes |
| **P.C.3** Runtime: `runtime/go/lazuli/poller/contract.go` — Registry, Spec, ResolveResult, Cursor, Backoff, Quirk types | runtime/go/lazuli | poller/contract.go | +80 | Yes |
| **P.C.4** Runtime: `runtime/go/lazuli/poller/scheduler.go` — ticker loop, batched SELECT, conditional UPDATE, backoff math, quirk dispatch | runtime/go/lazuli | poller/scheduler.go | +120 | Yes |
| **P.C.5** Runtime: `runtime/go/lazuli/poller/poller_test.go` — synctest covering happy path, retry-after-crash idempotency, gender-flip quirk, max_attempts exhaustion | runtime/go/lazuli | poller/poller_test.go | +120 | Yes |
| **P.D.1** Doctor rule pack — nineteen structural rules from §5 (one file per rule, registered in `doctor/poller/mod.rs`) | lazuli_cli | doctor/poller/*.rs | +800 (~42 LOC × 19) | Yes (parallel cells, single file each) |
| **P.E.1** Inspect: `--expand=pollers` projection; add `poller` count to `--expand=summary` | lazuli_cli | inspect/poller.rs | +100 | Yes |
| **P.E.2** LSP: hover catalog additions for `poller`, `cursor`, `eligible_when`, `retry_quirk`, `tick`, `tenant_from row.*`; completion inside `poller` block | lazuli_lsp | lib.rs | +120 | Yes |
| **P.F.1** Fixture: add `poller v8_consult_resolver` style fixture to `examples/marketplace-mini/` (synthetic; not corbanx — public repo) | examples | examples/marketplace-mini/marketplace-mini.lzi | +60 | No (Claude) |
| **P.F.2** Highlight: `editors/vscode/syntaxes/lazuli.tmLanguage.json` — color `poller`, `cursor`, `eligible_when`, `tick`, `retry_quirk` | editors | lazuli.tmLanguage.json | +20 | Yes |

**Wave estimate:**
- Wave 0 (P.A.0): 1 cell, ~40 LOC, single Codex.
- Wave 1 (P.A.1 + P.A.2 + P.B.1): 3 cells, ~480 LOC, parallel via Codex.
- Wave 2 (P.B.2 lowering — sequential against Wave 1): 1 cell, ~200 LOC, single Codex.
- Wave 3 (P.C.1 + P.C.2 + P.C.3 + P.C.4 + P.C.5): 5 cells, ~600 LOC, parallel via Codex.
- Wave 4 (P.D.1 × 19 parallel + P.E.1 + P.E.2): 21 cells, ~1020 LOC, parallel via Codex.
- Wave 5 (P.F.1 + P.F.2): 2 cells, ~80 LOC, Claude for the fixture, Codex for the highlight.

Total: ~2420 LOC framework + ~60 LOC fixture. ~3 sessions if Codex waves go clean.

---

## §9. Out of scope (rejected on purpose)

- **Predicate-engine quirks.** `retry_quirk my_thing when row.foo > 5 and row.bar = "x" mutate row.baz = compute(row.qux)` is REJECTED. Only the §3.13 catalog forms accept. Authors needing arbitrary mutation drop to a separate `command` triggered from `emits`, or to a `job trigger event` on the row's emitted state-change event.
- **Cross-feature `source`.** `source other_feature.PendingThing` is REJECTED for v0.1. The cursor table belongs to the feature owning the poller. OQ-1 tracks the cross-feature need if pressure surfaces.
- **Cron-based pollers.** `poller … trigger schedule "0 */15 * * *"` is REJECTED. Pollers are interval-based (`tick every <duration>`). Cron is a `job` shape; mixing surfaces would re-introduce the polysemy `POLLER-DUAL-SCHEDULER-001` guards against.
- **Async handler signatures.** The `resolve via @fn.<handler>` handler is synchronous (per row, per attempt). Handlers needing async/parallel vendor calls per row use `errgroup` inside the handler body. The poller's job is row-level orchestration, not call-level parallelism.
- **Pause / resume / cancel as first-class.** A row marked terminal stays terminal (`terminal_immutable`-style guarantee). Pausing a row mid-flight requires writing the row's own update path (e.g. `mark_paused` command that sets `paused_at`). OQ-4 tracks if 3+ products need a first-class pause primitive.
- **Custom scheduler topology.** "Run this poller on a dedicated worker pool" is a runtime/topology concern, addressed by `app.lzi runtime unit worker runs pollers *` (mirrors today's `runs jobs *`). The vocabulary slot is reserved (OQ-5); the L0 proposal does not depend on it.
- **Auto-cleanup of resolved rows.** "Delete terminal rows after 30 days" is `retention` vocabulary (`docs/invariants.md` §"Security And Crypto"), not poller. The poller doesn't own the lifecycle of its source resource beyond cursor mutation.
- **Direct vendor SDK references.** No `@adapter.v8` / `@adapter.drex` / vendor names in the poller block. The vendor mechanics live in `@fn.poll_v8`'s handler body, which can call any integration declared via `requires integration <slot>: <Capability>`. Same boundary as `job calls <slot>.<operation>`.
- **Backwards-incompatible IR ABI.** `Poller` is additive on `Feature`. Existing fixtures continue to parse.

---

## §10. Risks

1. **Scheduler crash recovery semantics — the biggest design hazard.** When the scheduler process crashes after the handler ran but before the UPDATE committed, the row's `attempts` counter hasn't incremented. On restart, the next tick will re-call the handler with the same `(row.id, row.attempts)` pair. Two outcomes are possible:
   - **(a) Handler is idempotent** (the canonical case for vendor calls: the vendor returns the same response for the same `consult_id`). The second call returns the same `ResolveResult`; the conditional UPDATE commits exactly once because the `attempts` value advances in the same transaction as the handler's effect. **This is the contract `idempotency by row.id, row.attempts` enforces.**
   - **(b) Handler is not idempotent** (the vendor returns a different response, e.g. because it creates a duplicate consult on retry). The author MUST make the handler idempotent (deduplicate by `consult_id` if the row already has one). **Mitigation:** doctor cannot statically prove handler idempotency, so the proposal documents it loudly in the codegen-emitted handler stub doc-comment, and `POLLER-IDEMPOTENCY-ATTEMPTS-MISSING-001` warns when the author omits `attempts` from the idempotency key. The runtime is **wire-compliant**: it cannot guarantee what it didn't compute.

2. **`POLLER-DUAL-SCHEDULER-001` heuristic false-positives.** The "same feature has both a `poller` and a `job trigger schedule` touching the same resource" check is heuristic — it walks the job's handler file path looking for the resource name. A handler named `cleanup_orphaned_v8_pending_consults.go` would false-positive. **Mitigation:** the lint is a warning (not error) in strict, error in production; authors can suppress with `# lazuli-allow POLLER-DUAL-SCHEDULER-001` comment (existing comment-suppression mechanism). Heuristic tightens if pressure surfaces.

3. **`tick every <duration>` defaults invite footguns.** A 30s default means a 100-row pending table generates 3.3 SELECTs per second steady-state — fine for one poller, painful for ten. **Mitigation:** `POLLER-TICK-TOO-FAST-001` warns when `every < 5s`; `runtime unit worker runs pollers *` topology slot (reserved per OQ-5) eventually lets ops split pollers across workers. v0.1 ships with the warning; if pressure surfaces, v0.2 introduces a feature-default `tick.every`.

4. **Catalog overload — every product wants `retry_quirk my_thing`.** v0.1 ships ONE catalog form (`gender_flip_once`) and explicitly reserves the skeleton for ≥1 future form (`field_swap_once`). Without discipline the catalog grows monotonically. **Mitigation:** same rule as `lifecycle invariant` — new catalog form requires (a) ≥2 products genuinely needing it, (b) ≤4 tokens to express, (c) doctor enforceability without a predicate sublanguage, (d) explicit L0 review. Authors needing anything else drop to a separate `command` chained off `emits`.

5. **Distinguishing pollers from `lifecycle` for cold-readers.** Both name state machines. The diagnostic difference (lifecycle = command-driven discriminator field; poller = wall-clock-driven cursor table) is subtle on first reading. **Mitigation:** `lazuli inspect --expand=summary` shows both kinds separately (`lifecycles: [...]`, `pollers: [...]`); the docs cross-reference both with a comparison table (§2 above is the seed); doctor diagnostics on a `poller` with no `cursor` field always suggest "did you mean `lifecycle <field>` on a resource?" inline. Cold-reader confusion is mitigated by the naming itself — "poll" + "er" carries the right intuition.

6. **The `@fn.<handler>` handler signature derivation is non-trivial.** Codegen must compute the typed `ResolveResult[State, Terminal, Result]` type for each poller; that means lifting the `terminal_status_field`'s enum, the `terminal_result_field`'s JSON shape, and the `states`' state union into one Go generic call. **Mitigation:** the type composition is mechanical (same shape as today's `Command` payload type generation); it's L2 codegen work, not language design. The proposal's §3.6 example signature is the contract codegen must hit.

7. **`looplab/fsm` or any FSM library is intentionally NOT used.** Unlike `lifecycle-vocab.md` which wires `looplab/fsm`, pollers' state transitions are implicit in the handler's return value — there's no FSM table to wrap. The runtime is pure tick + SQL UPDATE. Less external surface = less risk; the trade-off is that the FSM intuition cold-readers carry from `lifecycle` doesn't transfer to `poller` (resolved by §2's comparison table and the §10.5 mitigation).

---

## §11. Acceptance / tests

- Parser round-trips the §3.1 example without loss.
- IR snapshot stable; `cargo test -p lazuli_ir` green.
- All 19 native doctor rules from §5 each fire once on the canonical violation, zero false positives on `examples/full-capsule/`, `examples/marketplace-mini/`, `examples/smoke-hello/`.
- `lazuli inspect --expand=pollers` returns deterministic JSON for the §3.1 example with the shape in §4.3.
- `lazuli inspect --format=json --expand=summary` includes a `pollers` count per feature.
- Runtime `poller.Scheduler` test (P.C.5) covers:
  - Happy path: row → resolved with `final_status` set; `resolved_at` filled; row no longer eligible.
  - Crash recovery: scheduler killed mid-tick → on restart, handler called again, `WHERE attempts = $old` makes second commit a no-op.
  - Max attempts: row exhausts retries → terminal `failed` state synthesized, `final_status = failed`, `resolved_at` set.
  - Gender-flip quirk: first call returns `gender_ambiguous`; quirk fires; handler re-called immediately; second call returns terminal.
  - Backoff: between retries, `next_check_at` advances by the strategy's computed delay.
- LSP hover on `poller`, `cursor`, `eligible_when`, `retry`, `backoff exponential`, `retry_quirk gender_flip_once` returns the doc-comment from invariants.
- `lazuli-language-architect` PASS at ≥ 8.5/10 (target ≥ 9.0) with no individual dimension < 7.

---

## §12. Open questions / future work

1. **OQ-1 — Cross-feature `source`.** v0.1 rejects `source other_feature.Resource`. Real cases: a centralized "billing.pending_charges" table polled by multiple integration features. Promote when 2+ products surface this pattern and the workaround (re-declaring the resource alias) proves brittle.

2. **OQ-2 — Cursor field extension.** v0.1 fixes `eligible_when` to a two-field pair (`next_at`, `resolved_at`). Real cases: a "pause_until" field used to hold a row out of rotation for ops reasons. Promote when 2+ products want it; growth shape would be `eligible_when next_check_at, resolved_at, pause_until` (positional, closed-catalog discipline).

3. **OQ-3 — Quirk catalog growth.** v0.1 ships `gender_flip_once`. Candidate growth set (NOT in v0.1; tracked):
   - `field_swap_once from <value> to <value>` — generic flip beyond gender (Drex needs swap between `prod`/`prod_v2` URLs).
   - `requote_once when <predicate>` — bounded re-fetch with a different field set (Bankerize-style refresh-token semantics).
   - `escalate_after <int> emits <event>` — emit an event after N intermediate attempts without resolving.

4. **OQ-4 — Pause/resume primitives.** Today: write a `mark_paused` command that sets a same-resource `paused_at` field; the cursor's `eligible_when` clause would need to grow (OQ-2). Promote if 3+ products want first-class pause vocabulary.

5. **OQ-5 — `app.lzi runtime unit worker runs pollers *` topology.** Mirrors today's `runs jobs *`. Reserved keyword. Lands as a small additive cell once OQ-3 or sustained production pressure shows the topology surface is needed.

6. **OQ-6 — `tests` block on `poller`.** Today: handler tests live in the `@fn.poll_v8`'s Go file; integration tests live in `runtime/go/lazuli/poller/poller_test.go`. The case for a declarative `tests` block on the poller block itself (mirroring `Command.tests`) is unclear — the poller's behavior is "call the handler, commit the result", and the handler is what needs testing. Defer until pilot shows the test surface helps cold-readers.

7. **OQ-7 — Polling against external (non-Lazuli) tables.** Some clients want to poll a vendor's own webhook-fed table that lives in a different schema or even a different database. v0.1 only supports same-schema resources. Promote if pilot pressure shows the need; design hazard is JSON-shape inference.

---

## §13. References

- `docs/design-principles.md` — Rule Zero (Vocabulary Over Mechanism); the principle this proposal operationalises.
- `docs/invariants.md` §"Source And Derived Views" + §"Events" (existing `audit` / `emits` / `policy` / `tenant_from` / `idempotency` discipline this proposal composes with).
- `docs/architecture.md` §"Founding principle" — the poller NAMES the resolution loop; runtime is wire over `pgx` + `time.Ticker`.
- `docs/proposals/bucket-jobs-cycle.md` + `bucket-jobs-scope.md` — sibling kind; this proposal explicitly distinguishes from. Job is one-shot; poller has persistent cursor.
- `docs/proposals/lifecycle-vocab.md` — recent well-graded L0 proposal style reference; the §2 boundary table and §3.13 retry-quirk catalog mirror `lifecycle`'s §2 and §3.4 invariant catalog disciplines.
- `docs/proposals/corbanx-class-readiness.md` — meta-roadmap, gap #5 (this proposal).
- `runtime/go/lazuli/jobs/dispatch.go` — sibling runtime surface; the poller's `Registry`/`Spec`/`Dispatch` mirror its shape.
- `crates/lazuli_ir/src/lib.rs:1684` — existing `Job` IR; the `Poller` IR struct is a sibling addition.
- `c:/Users/lucas/dev-trabalho/corbanx/apps/api/src/features/multi-bank/multi-bank.repo.ts` — anchoring real-world example. Confidential consumer code; pattern abstracted into this proposal.
- `c:/Users/lucas/dev-trabalho/corbanx/packages/database/src/schema/introspected.ts` — the `v8_pending_consults` / `v8_pending_multibank_consults` schemas concretising the cursor fields shape.
- `project_db_driver_choice.md` — pgx/v5 + `RowToStructByName` are the SQL primitives the scheduler uses.
- `project_handler_audit_lints_2026-05-14.md` — proposal lineage for the doctor-vocabulary-lints discipline this proposal extends with 19 new rules.
