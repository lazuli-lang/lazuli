# Phase L Tier 3 — Job/Webhook Effect Scope (pre-implementation)

**Status**: pre-implementation investigation. Resolves the Tier 3 blocker
identified by the Phase L agent (commit `f60f6bf` worktree note) so the
eventual Tier 3 + jobs bucket run (rows 32-34 in
`docs/next-checklist.md`) can land in scope.

**Audience**: language team (Lazuli core), runtime team (Drusa).

**Date**: 2026-05-11.

## Contexto

Phase L Tier 1 (`parse_auth`, commit `e1d8521`) and Tier 2
(`@cap.File(...)` lowering, commit `f60f6bf`) closed the auth bucket
(`docs/next-checklist.md:62-64`, rows 26-28) and storage bucket
(`docs/next-checklist.md:65-67`, rows 29-31). Tier 3 is the next
block the bucket-cycle queue needs: jobs / webhooks / notifications
(rows 32-34) still live entirely on the text-pattern doctor +
LSP path because the canonical-indent slice silently ignores every
`job`, `webhook`, `notification`, and `event_group` child of a feature
(`crates/lazuli_syntax/src/parser.rs:1185-1189`).

The fixture authors four representative bodies the slice ignores
today:

- `job recompute_score_after_invoice`
  (`examples/full-capsule/full-capsule.lzi:392-402`): event reactor with
  `target`, `let`, `updates`, and `emits` declarative body.
- `job recompute_scores` (`:404-409`): scheduled fanout with
  `fanout tenants org`, `retry`, and `handler`.
- `job process_import` (`:760-771`): queued worker with `queue`,
  `tenant_from`, `idempotency`, `retry`, `calls`, `timeout`, `handler`,
  `emits`.
- `webhook crm_customer_upsert` (`:773-781`): inbound HMAC webhook with
  `path`, `verify`, `tenant_from`, `idempotency`, `handler`, `emits`.

Plus `notification welcome_email` (`:817-826`) and
`event_group customer_*` (`:173-198`) which share most of the same
sub-grammar (trigger, tenant_from, idempotency, retry, emits).

The Phase L agent paused Tier 3 with this note on commit `f60f6bf`:

> Tier 3 é fragilmente grande. `ir::Job`/`ir::Webhook` exigem
> `JobBody`/`IdempotencyKey`/`TargetExpr`/`LetBinding`/`CommandEffect`/
> `BackoffStrategy` — toda a estrutura de expressões/efeitos. Fazer
> parse-only sem lowering seria honesto, mas exigiria também um
> sub-IR temporário (`JobSkeleton`/`WebhookSkeleton`) pra não
> introduzir um tipo IR meio-feito.

The note is *partially* wrong: this proposal demonstrates that the
expression/effect spine the note worried about
(`TargetExpr`, `LetBinding`, `CommandEffect`, `IdempotencyKey`,
`BackoffStrategy`) **already exists** in
`crates/lazuli_ir/src/lib.rs`. The gap that remains is narrower than
the note implies — a handful of additive fields plus two new
structs — and **not** the full expression/effect infrastructure.

The bucket-jobs scope-out (`docs/proposals/bucket-jobs-scope.md`,
landed before Tier 1) already enumerated the gaps. Tier 3 closes them.

## Estado atual do IR de job/webhook

### What exists today

| Type | Location | Notes |
|---|---|---|
| `Job` struct | `crates/lazuli_ir/src/lib.rs:1781-1800` | `name`, `trigger`, `queue`, `idempotency`, `retry`, `policy`, `body`, `emits`, `previous_names`, `span_ref`. |
| `JobTrigger` enum | `crates/lazuli_ir/src/lib.rs:1803-1809` | `Event { event: QualifiedName }`, `Schedule { cron: String }`. Full coverage. |
| `JobOperationalKind` enum | `crates/lazuli_ir/src/lib.rs:1814-1819` | `Scheduled` / `Reactor` / `QueuedWorker`. Derived. |
| `IdempotencyKey` struct | `crates/lazuli_ir/src/lib.rs:1821-1825` | `by: Path`. Full coverage. |
| `RetryPolicy` + `BackoffStrategy` | `crates/lazuli_ir/src/lib.rs:1827-1837` | `count: u32`, `backoff: Fixed | Exponential`. |
| `JobBody` enum | `crates/lazuli_ir/src/lib.rs:1842-1846` | `Handler(JobHandler)` + `Declarative(JobDeclarative)`. |
| `JobHandler` struct | `crates/lazuli_ir/src/lib.rs:1848-1854` | `path: PathRef`, `returns: Option<TypeRef>`. |
| `JobDeclarative` struct | `crates/lazuli_ir/src/lib.rs:1856-1863` | `target: Option<TargetExpr>`, `lets: Vec<LetBinding>`, `effect: CommandEffect`. |
| `TargetExpr` struct | `crates/lazuli_ir/src/lib.rs:547-550` | Already shared with `Command`. `query: QualifiedName`, `args: Vec<NamedArg>`. |
| `LetBinding` struct | `crates/lazuli_ir/src/lib.rs:559-562` | `name: String`, `value: Expr`. |
| `CommandEffect` enum | `crates/lazuli_ir/src/lib.rs:565-574` | `Creates / Updates / Deletes / Returns / None` — shared with commands. |
| `Expr` enum | `crates/lazuli_ir/src/lib.rs:755-762` | `Path / String / Integer / Boolean / Enum / Nil`. v0 closed. |
| `Webhook` struct | `crates/lazuli_ir/src/lib.rs:1866-1884` | `name`, `route`, `verify: PathRef`, `idempotency`, `policy`, `handler: PathRef`, `returns`, `emits`, `previous_names`, `span_ref`. |

### What is missing on `Job`

The fixture writes five fields the IR drops:

1. `tenant_from payload.<axis>_id`
   (`examples/full-capsule/full-capsule.lzi:394,763`) — no `tenant_from`
   field on `Job`. Bucket-jobs-scope §Fix 1 proposed
   `tenant_from: Option<TenantFromSpec>` with `TenantFromSpec { path: Path }`.
2. `fanout tenants org` (`:406`) — no `fanout` field on `Job`.
   Bucket-jobs-scope §Fix 1 proposed
   `fanout: Option<FanoutSpec>` with
   `FanoutSpec { axis: String, scope: FanoutScope }` and
   `FanoutScope::Tenants` v0.
3. `timeout "30s"` (`:769`) — no `timeout` field at job level.
   Bucket-jobs-scope §Fix 1 proposed `timeout: Option<String>`.
4. `calls crm.normalize_import_batch` (`:766-768`) — no
   `external_calls` field on `Job`. Bucket-jobs-scope §Fix 1 proposed
   `external_calls: Vec<JobExternalCall>` mirroring the inspect-side
   `InspectExternalCall` shape.
5. `audit` child (proposed in bucket-jobs-cycle §"Linguagem proposta"
   #2) — declarative audit log mirroring `audit` on commands. **Not in
   fixture today** but declared as invariant-protected in
   `docs/invariants.md:93`; the IR field is additive.

### What is missing on `Webhook`

Three fields, more structured than `Job`'s gaps:

1. `tenant_from payload.<axis>_id` (`:778`) — same `TenantFromSpec` as
   `Job`. Re-uses the same type.
2. `verify hmac sha256 / secret env.X / header "X-..."` (`:775-777`) —
   today `Webhook.verify` is a bare `PathRef`. The fixture authors a
   *structured* verify spec. Bucket-jobs-scope §Fix 1 proposed
   `verify: WebhookVerify` with `algorithm: WebhookVerifyAlgorithm`,
   `secret: EnvRef`, `header: String`. The bare `PathRef` is wrong:
   no fixture verify is a path file today.
3. `scope <global|tenant>` + optional `reason "..."` — declared as
   invariant-protected (cross-tenant exposure rationale) in bucket-jobs
   §"Doctor/LSP propostos" (`WEBHOOK-SCOPE-001`). **Not in fixture
   today** but the LSP already runs the cross-check
   (`webhook_security_diagnostics` at
   `crates/lazuli_lsp/src/lib.rs:9146`); IR lift is additive.

### What is missing entirely (new IR structs)

1. `Notification` struct — surface authored in
   `examples/full-capsule/full-capsule.lzi:817-837`, has an
   `InspectNotification` shape in
   `crates/lazuli_cli/src/main.rs:3029+`, but no IR struct. Shape from
   bucket-jobs-scope §Fix 2: `name`, `channels`, `recipient`, `trigger`
   (reuses `JobTrigger`), `template`, `policy`, `tenant_from`,
   `idempotency`, `retry`, `rate_limit`, `emits`, `span_ref`.
2. `EventGroup` struct — surface authored
   (`examples/full-capsule/full-capsule.lzi:173-198`), inspect resolves
   inheritance text-side at
   `crates/lazuli_cli/src/main.rs:2185`, no IR struct. Shape from
   bucket-jobs-scope §Fix 2: `pattern`, `on_resource`, `payload`,
   `events: Vec<Event>` (already-typed), `span_ref`.

### What was speculated but is **not** actually needed

The Phase L agent's commit note named six prerequisites:
`TargetExpr`, `LetBinding`, `CommandEffect`, `BackoffStrategy`,
`JobBody`, `IdempotencyKey`. **All six already exist in IR** and
already lower from commands. The fixture's `job
recompute_score_after_invoice`
(`examples/full-capsule/full-capsule.lzi:392-402`) reuses the same
declarative spine as `command create_customer`: `target query.by_id(...)`,
`let new_score = ...`, `updates Customer ... emits ...`. The Job IR
already structurally supports this through `JobBody::Declarative`
(`crates/lazuli_ir/src/lib.rs:1856-1863`).

The genuinely new IR shapes are the **eight** additive items above
(5 on `Job`, 3 on `Webhook`) plus **two** new structs (`Notification`,
`EventGroup`). Not a full expression/effect spine.

## Estado atual de doctor / LSP / inspect para jobs/webhooks

Diagnostics that exist today, all text-pattern:

- `event_job_tenant_from_diagnostics`
  (`crates/lazuli_lsp/src/lib.rs:9875-9930`).
- `scheduled_job_tenancy_diagnostics`
  (`crates/lazuli_lsp/src/lib.rs:9973-10028`).
- `webhook_tenant_from_diagnostics`
  (`crates/lazuli_lsp/src/lib.rs:9258-9310`).
- `webhook_security_diagnostics`
  (`crates/lazuli_lsp/src/lib.rs:9146-9258`).
- `idempotency_key_diagnostics`
  (`crates/lazuli_lsp/src/lib.rs:6043-6130`).
- `INT-CALL-001…INT-CALL-004` cross-checks on `external_calls`
  (`crates/lazuli_cli/src/doctor.rs:2088-2150`) — these *are* in
  doctor but read from `ExternalCallFact` (text-pattern,
  `crates/lazuli_cli/src/doctor.rs:924-1010`).

Inspect status: `InspectFeature` (`crates/lazuli_cli/src/main.rs:454`)
carries `notifications` with a full structured projection
(`InspectNotification` at `:3029`) **derived from text parsing**, not
from IR. `jobs` and `webhooks` are flat name lists inside `summary`
only (`:618-619`). `event_groups` resolves inheritance text-side
(`:2185`).

That is, the **same five LSP checks already work file-local**; what
Tier 3 buys is doctor cross-feature coverage (e.g. fanout axis is
declared in a `uses`-d feature's `defaults.tenancy`).

## Rotas A vs B vs C

Three ways to close the Tier 3 gap. All honour the Lazuli/Drusa
boundary (no provider names, no DI mechanics, no transport).

### Route A — full lowering in one run

`parse_job` + `parse_webhook` + `parse_notification` +
`parse_event_group` recognise every child the fixture authors today
**and** the additive fields proposed in bucket-jobs-scope. IR
extensions land first, parser/lowering second, inspect projections
third — same order Tier 1 used after the `auth-lowering-scope`
erratum (`docs/proposals/auth-lowering-scope.md:21-23`).

Concrete IR work:

1. Extend `Job` with `tenant_from`, `fanout`, `timeout`,
   `external_calls`, `audit` (5 fields).
2. Extend `Webhook` with `tenant_from`, structured `verify`,
   `scope`/`scope_reason` (3 fields; one is a type swap from
   `PathRef` to a typed enum).
3. Add `Notification` struct (~12 fields).
4. Add `EventGroup` struct (~5 fields).
5. Add `TenantFromSpec`, `FanoutSpec`, `FanoutScope`,
   `WebhookVerify`, `WebhookVerifyAlgorithm`, `WebhookScope`,
   `JobExternalCall`, `NotificationChannel`,
   `EventGroupPayloadField` (8-9 small new types).
6. Add `Auth.span_ref` parity already done in Tier 1; copy the
   `span_ref: Option<SpanRef>` field to every new struct.

Parser work: four new top-level recognisers
(`parse_job`, `parse_webhook`, `parse_notification`,
`parse_event_group`), each in the same shape as `parse_auth`. Each
recogniser fans into 2-4 child parsers (e.g. `parse_job_trigger`,
`parse_job_idempotency`, `parse_job_calls_block`,
`parse_job_handler_or_declarative`).

**Cost (in cells, baseline = Tier 1+2 ≈ 2 cells each)**:

- IR extensions + new structs: ~1.5 cells (more types than Tier 1's
  2 extensions, but each is structurally trivial — `serde(default)`
  + skip_serializing_if_none pattern).
- Parser `parse_job`: ~1.5 cells (largest of the four — declarative
  body reuses `target`/`let`/`creates`/`updates`/`emits`
  grammar, mostly already in command-parser territory but the slice
  doesn't have command parsing today).
- Parser `parse_webhook`: ~0.75 cells (smaller — flat list of
  children).
- Parser `parse_notification`: ~0.75 cells.
- Parser `parse_event_group`: ~1 cell (nested `event` children;
  payload binding grammar is novel).
- Inspect projections (`InspectFeature.jobs/webhooks/event_groups`):
  ~1 cell (mechanical mirror of `InspectAgent`/`InspectNotification`).

**Route A total: ~6.5 cells.**

**Risk**: wide touch surface. The declarative job body
(`target`/`let`/`updates`/`emits`) is a near-duplicate of the
command body grammar that **does not yet have a canonical-indent
parser** (commands still live in text-pattern facts per
`docs/next-checklist.md:60` row 24). Building `parse_job` first
either (a) introduces grammar that `parse_command` will need to
re-parse identically later, or (b) implicitly commits to a shared
sub-recogniser the command parser will inherit when Tier 4
materialises. Either way the *first* declarative-body parser the
slice grows lives in `parse_job`, and getting its shape wrong now
makes Tier 4 (commands) harder.

### Route B — parse-only, IR sidecar temporary

`parse_job` / `parse_webhook` / `parse_notification` /
`parse_event_group` produce **`JobSkeleton` / `WebhookSkeleton` /
`NotificationSkeleton` / `EventGroupSkeleton`** in
`crates/lazuli_syntax/src/ast.rs` (next to `FeatureSkeleton.agents`
and `FeatureSkeleton.auth`). The lowered IR `Job`/`Webhook` structs
stay as-is for now. Doctor's existing text-pattern facts continue
to run against the source unchanged. `--expand=jobs` projects the
*skeleton* (parser output), marking unlowered fields like
`body: HandlerSkeleton { raw: String }` or
`effect: { raw: "updates Customer\n  score = new_score" }`.

**Cost**: ~3 cells (the four parser recognisers plus skeleton-only
inspect projection). Smaller than Route A because:

- No IR extensions land. The five missing `Job` fields, three
  missing `Webhook` fields, and two new structs (`Notification`,
  `EventGroup`) stay deferred.
- Declarative-body grammar is captured as `raw: String` lines, not
  lowered into `TargetExpr`/`LetBinding`/`CommandEffect`. Avoids
  the Tier 4 inheritance question entirely.
- Doctor stays exactly where it is today: five text-pattern
  diagnostics in LSP, four in doctor (INT-CALL). No promotion to
  IR-driven cross-checks yet.

**Risk**: every consumer that needs typed jobs (codegen for rows
33-34, runtime team) has to migrate again later. The bucket-jobs
cycle (`docs/proposals/bucket-jobs-cycle.md`) cannot ship; rows 32-34
stay blocked because typed inspect input is the gate for codegen.
The "IR meio-feito" concern the Phase L agent raised is real here —
`JobSkeleton.body: HandlerSkeleton | DeclarativeSkeleton(raw:
String)` produces an IR shape that consumers will have to special-case
until Route A eventually lands. Choosing B implicitly commits to *two*
implementations.

### Route C — subset scoping (PILOT-NEEDED only)

Identify the **strict subset** of `job`/`webhook` children the
canonical fixture actually authors today (8 distinct child kinds
across the four job/webhook blocks; see §"PILOT-NEEDED vs
SPECULATIVE" below). Cover only those. SPECULATIVE additions
(`audit` child, `scope`/`scope_reason`, `expose http` on jobs,
custom backoff strategies) stay deferred. Declarative job body
(`target`/`let`/`updates`/`emits`) stays as `raw: String` Route-B
style **until a command parser lands in Tier 4**, then is filled in
by reusing the command parser's sub-recognisers.

Concrete IR work:

1. Extend `Job` with `tenant_from`, `fanout`, `timeout`,
   `external_calls` (4 fields, drop `audit`).
2. Extend `Webhook` with `tenant_from`, structured `verify` (2
   fields, drop `scope`/`scope_reason`).
3. Add `Notification` struct.
4. Add `EventGroup` struct.
5. Add typed support shapes for the above
   (`TenantFromSpec`, `FanoutSpec`, `WebhookVerify`,
   `JobExternalCall`, `NotificationChannel`).
6. `JobBody::Declarative` lowering stays partial:
   `JobDeclarative { raw_target: Option<String>, raw_lets:
   Vec<String>, raw_effect: String }` — same string-bag shape Route B
   would produce for the entire job. `JobBody::Handler` is fully
   lowered.

Parser work: same four recognisers as Route A, but each recogniser
**accepts only the subset** the fixture authors. Declarative body
captured as `raw_*` strings; handler body fully lowered.

**Cost**: ~4.5 cells.

- IR extensions: ~1 cell.
- Parser `parse_job` (handler-only fully lowered, declarative as
  raw): ~1 cell.
- Parser `parse_webhook`: ~0.5 cell.
- Parser `parse_notification`: ~0.5 cell.
- Parser `parse_event_group`: ~0.75 cell.
- Inspect projections: ~0.75 cell.

**Risk**: the `raw_*` carve-out in `JobDeclarative` is honest IR
debt that consumers must handle. Codegen for `recompute_score_after_invoice`
(declarative job, no handler file) can't ship until Tier 4 fills in
the declarative spine. But codegen for `recompute_scores` and
`process_import` (both `handler` jobs) and `crm_customer_upsert`
(webhook with `handler`) and `welcome_email` (notification with
`template`) **does** ship — three of the four end-to-end loops the
bucket-jobs cycle wants to close (`bucket-jobs-cycle.md:723-727`).

The carve-out is also disciplined: it draws the boundary exactly
along the line Phase L is forcing
(`crates/lazuli_syntax` slice = leaf grammars; declarative
expression/effect spine = next Tier). The boundary moves once when
Tier 4 lands, not twice.

### Comparison

| Axis | Route A (full) | Route B (skeleton) | Route C (subset) |
|---|---|---|---|
| Upfront cost (cells) | ~6.5 | ~3 | ~4.5 |
| Maintenance cost | One canonical home; future fields extend `parse_*` and add IR fields. | Two homes (parser skeleton + future IR lift); every consumer special-cases `raw_*` until the lift. | One canonical home for handler-backed jobs; one bounded carve-out (`raw_*`) for declarative jobs that closes when Tier 4 lands. |
| Fixture coverage | 100% — every child of every block in `:392-410`, `:760-781`, `:817-837`, `:173-198`. | Surface-only — children parsed but bodies opaque. | Handler-backed: 100%. Declarative-backed: surface fields lowered, body still opaque. Three of four canonical end-to-end loops unblocked. |
| Risk of redesign | Medium — the declarative body grammar is built before the command parser, so Tier 4 inherits the choice. Wrong shape now = harder Tier 4. | Low — nothing typed lands; nothing breaks if Tier 4 invents a different shape. | Low — declarative body explicitly deferred; Tier 4 fills it. |
| Phase L compat | Aligned — shrinks the slice's skip-list maximally in one cut. | Misaligned — Phase L's goal is to *eliminate* text-pattern facts, not to add a sidecar IR. | Aligned — shrinks the skip-list to the next natural boundary (commands + declarative bodies). |
| Unblocks rows 32-34? | Yes, fully. | No — codegen still has no typed input for jobs/webhooks. | Yes for handler-backed jobs / webhooks / notifications. Declarative jobs blocked until Tier 4. |
| Tier 4 (commands) story | `parse_job` parses declarative body grammar first; `parse_command` will need to share or duplicate. Shared sub-recognisers a likely outcome. | Tier 4 is unaffected by Tier 3; it lands independently. | Tier 4 fills the `raw_*` carve-out by sharing the command parser's `target` / `let` / `creates` / `updates` recognisers; same code reused for `parse_command` and `parse_job`. |
| Doctor promotion | All 5 LSP rules eligible for promotion (IR-driven). | None eligible. | All 5 LSP rules eligible (the missing IR fields the rules need are all in the subset). |

### Recomendação

**Route C.** Three reasons:

1. **It draws the Phase L boundary at a natural line.** Tier 1 lifted
   leaf grammars (auth). Tier 2 lifted leaf typing
   (`@cap.File(...)`). Tier 3 lifts header + leaf children of
   `job`/`webhook`/`notification`/`event_group` but **defers the
   declarative expression/effect spine to Tier 4** where it lives
   naturally alongside the command parser. Splitting the work at
   that line means each Tier extends the slice by one layer of
   nesting rather than two.
2. **It unblocks 3 of 4 fixture end-to-end loops with the smaller
   cost.** `process_import` (queued worker with handler),
   `crm_customer_upsert` (webhook with handler), and `welcome_email`
   (notification with template) ship with full IR + inspect + doctor
   coverage. Only `recompute_score_after_invoice` (declarative reactor)
   waits for Tier 4. That's the same ordering the
   bucket-jobs-cycle's "Próximo passo" already implies
   (`docs/proposals/bucket-jobs-cycle.md:723-727`) — easier loops
   first, hardest last.
3. **Route A's declarative-body parser is premature.** Building
   declarative-body parsing in `parse_job` before `parse_command`
   exists means either coupling commands to the job parser's
   shape (bad ordering — commands are the larger surface) or
   building it twice. Route C avoids the question by deferring.

Route B is rejected: it adds a sidecar IR layer (`JobSkeleton`) that
violates the Phase L direction of travel
(`crates/lazuli_syntax::ast` already has `FeatureSkeleton` carrying
fully-lowered `Agent` and `Auth`; introducing a half-lowered
`JobSkeleton` next to them is exactly the inconsistency the
Phase L agent's note worried about).

## PILOT-NEEDED vs SPECULATIVE

Classification of every `Job` / `Webhook` / `Notification` /
`EventGroup` field bucket-jobs-cycle.md §IR proposed against fixture
evidence.

### PILOT-NEEDED — exercised by the canonical fixture today

| Field | Construct | Fixture evidence | Tier 3 fate |
|---|---|---|---|
| `Job.tenant_from: Option<TenantFromSpec>` | `tenant_from payload.<axis>_id` | `:394`, `:763` | **Lift now.** Three doctor diagnostics already cross-check this text-side. |
| `Job.fanout: Option<FanoutSpec>` | `fanout tenants <axis>` | `:406` | **Lift now.** `scheduled_job_tenancy_diagnostics` already cross-checks text-side. |
| `Job.timeout: Option<String>` | `timeout "30s"` | `:769` | **Lift now.** No diagnostic today; Tier 3 enables `JOB-TIMEOUT-001` per bucket-jobs-cycle §Doctor table. |
| `Job.external_calls: Vec<JobExternalCall>` | `calls <slot>.<op>` block | `:766-768` | **Lift now.** Doctor already harvests `ExternalCallFact` text-side; IR makes it consumable from codegen. |
| `Webhook.tenant_from: Option<TenantFromSpec>` | `tenant_from payload.<axis>_id` | `:778` | **Lift now.** `webhook_tenant_from_diagnostics` cross-checks text-side today. |
| `Webhook.verify: WebhookVerify` (replace `PathRef`) | `verify hmac sha256` + `secret env.X` + `header "X-..."` | `:775-777` | **Lift now.** `webhook_security_diagnostics` cross-checks text-side; structured IR carries the algorithm/secret/header axes the diagnostic reads. |
| `Job.body: JobBody::Handler` (full lowering) | `handler "./..."` | `:409`, `:770` | **Lift now.** `JobHandler` IR exists; parser just needs to recognise the line. |
| `Webhook.handler: PathRef` + `Webhook.returns: Option<TypeRef>` | `handler "./..." returns Customer` | `:780` | **Lift now.** IR already has both fields; parser just needs to recognise. |
| `Notification` (new struct) | `notification welcome_email` | `:817-826`, `:828-837` | **Lift now.** Replaces text-based `InspectNotification` with IR-derived projection. |
| `EventGroup` (new struct) | `event_group customer_* on Customer` | `:173-198` | **Lift now.** Same reasoning. |
| Trigger / idempotency / retry on `Notification` | `trigger event ...`, `idempotency by ...`, `retry N backoff ...` | `:820-823` | **Lift now.** Reuses existing `JobTrigger`, `IdempotencyKey`, `RetryPolicy`. |
| `Webhook.path: String` (rename of `route`?) | `path "/webhooks/crm/customer-upsert"` | `:774` | **No change.** IR already has `Webhook.route: String` (`:1869`). The keyword in source is `path`; the IR field is `route`. Cosmetic; rename is a separate bikeshed. |
| Job `queue <lane>` | `queue customer_imports` | `:762` | **Already lifted.** `Job.queue: Option<String>` exists at `:1786`. Parser needs to read it. |
| `JobBody::Declarative` (target/let/updates/emits) | `target query.by_id(...)`, `let new_score = @fn.risk_score(target)`, `updates Customer score = new_score`, `emits customer_score_recomputed score = new_score reason = "invoice_paid"` | `:396-402` | **Deferred to Tier 4 (Route C carve-out).** IR field becomes `JobDeclarative { raw_target: Option<String>, raw_lets: Vec<String>, raw_effect: String }` until Tier 4 lands the shared declarative parser. |

### SPECULATIVE — not in the fixture; defer until pilot evidence

| Field | Status | Why defer |
|---|---|---|
| `Job.audit: Option<AuditSpec>` | Declared in `docs/invariants.md:93` as invariant-protected; bucket-jobs-cycle §"Linguagem proposta" #2 proposed it. **Not in any fixture job today.** | Pilot needed: a job whose effect set is irreducible to an `emits` audit trail. Today every fixture job's audit story is "the emitted event covers the audit log". Adding the field pre-pilot risks designing the shape against speculation. |
| `Webhook.scope: WebhookScope` + `scope_reason: Option<String>` | LSP already has the rule (`webhook_security_diagnostics`); bucket-jobs-cycle §Doctor `WEBHOOK-SCOPE-001` would promote it to IR. **Not in fixture today** — the only webhook is `crm_customer_upsert` and it has tenant-scoped `tenant_from`. | Pilot needed: a webhook whose authoring needs to declare cross-tenant exposure explicitly. Today the LSP rule fires on missing `tenant_from` rather than missing `scope` — the latter has no surface authoring evidence. |
| `Job.approval` (mirror of Cut A.9 `approval` on commands) | bucket-jobs-cycle §"Linguagem proposta" #1 sketched it; explicitly marked `pilot-gated`. Not in fixture. | Pilot needed: a destructive scheduled job whose authoring needs the approval gate. The Cut A.9 sketch (`purge_archived_customers`) is illustrative, not authored. |
| `Job.expose http` (mirror of Cut A.7 on agents) | Not in fixture; not in any proposal. | Pilot needed: a job authoring needs an HTTP trigger. Webhooks already cover the inbound-HTTP-to-effect contract; jobs exposing HTTP would be duplicative without pilot pressure. |
| Custom `BackoffStrategy` variants (linear, decorrelated_jitter, etc.) | IR has `Fixed | Exponential` (`:1833-1837`); fixture authors only `exponential`. | Pilot needed: a job whose retry pattern is irreducible to the closed two-variant catalog. Both major queue adapters (River, Asynq) ship sensible exponential defaults; adapter-specific jitter is a Drusa concern, not IR. |
| `Job.dlq` / `on_exhausted` | bucket-jobs-cycle §"Linguagem proposta" #3 sketched it explicitly as PILOT-NEEDED. | Pilot needed: a product whose retry-exhausted path needs declarative routing. Today River dead-letters by default; surface is unnecessary. |
| `NotificationChannel::Push` / `Sms` | Fixture authors `email` and `in_app` only. | Pilot needed: a product authoring push/SMS notification flows. The closed enum can include `Push | Sms` as variants without lowering pressure today, but the supporting registry capability binding (`@adapter.notification.push`) is the gate. |
| `EventGroup` payload conditional bindings (`when @actor.user`) | Fixture authors `by_id = ctx.user.id when @actor.user` (`:177`). | **Already PILOT-NEEDED.** Move to the table above — this is a closed actor catalog cross-check the IR shape needs to capture. (See "Closed-cycle criterion" below.) |

Net result: **12 PILOT-NEEDED items** land in Tier 3 (Route C). **7
SPECULATIVE items** defer. The split is much narrower than the
Phase L agent's note implied because most of the items in the agent's
list (`TargetExpr`, `LetBinding`, `CommandEffect`, `JobBody`,
`IdempotencyKey`, `BackoffStrategy`) already exist in IR.

## Closed-cycle criterion para Tier 3

Adapted from the auth Tier 1 closed-cycle criterion
(`docs/proposals/auth-lowering-scope.md:192-238`).

- [ ] **Fixture parses through the canonical-indent slice.** Every
      `job` / `webhook` / `notification` / `event_group` in
      `examples/full-capsule/full-capsule.lzi` lowers without
      falling through to the silent-skip branch
      (`crates/lazuli_syntax/src/parser.rs:1185-1189`).
      `parse_feature_skeletons` returns `FeatureSkeleton` carrying
      populated `jobs`, `webhooks`, `notifications`, `event_groups`
      vectors. Declarative-body jobs land with `JobBody::Declarative`
      carrying the `raw_*` carve-out (Tier 4 erratum applies, same
      shape as auth Tier 1's erratum at lines 12-25).
- [ ] **`lazuli check` accepts every block today's text path accepts.**
      No regression — the legacy text-pattern path stays running
      until Tier 3 lands as a new branch; once Tier 3 is in, the
      text-pattern facts for the affected constructs are deleted
      one diagnostic at a time. (See Tier 1's deletion of
      `auth_password_*` text walkers.)
- [ ] **`lazuli inspect --expand=jobs` projects the IR.** New
      projection. Mirrors `--expand=agent` / `--expand=auth` /
      `--expand=storage`. The projection carries
      `jobs`/`webhooks`/`event_groups` arrays on `InspectFeature`.
      Per bucket-jobs-scope §Fix 3, the shape mirrors
      `InspectNotification` verbatim (which itself becomes
      IR-derived in this Tier).
- [ ] **`lazuli doctor` promotes ≥5 LSP diagnostics to IR-driven
      cross-feature.** Concrete promotions
      (cross-checked against `crates/lazuli_lsp/src/lib.rs`):
      - `event_job_tenant_from_diagnostics` → `event_job_tenant_from_facts`
        (doctor). Cross-feature payload axis resolution.
      - `scheduled_job_tenancy_diagnostics` → `scheduled_job_tenancy_facts`.
        Reads `Job.fanout`.
      - `webhook_tenant_from_diagnostics` → `webhook_tenant_from_facts`.
        Reads `Webhook.tenant_from`.
      - `webhook_security_diagnostics` → `webhook_verify_facts`. Reads
        `Webhook.verify` (now structured).
      - `idempotency_key_diagnostics` → `idempotency_key_facts`. Reads
        `IdempotencyKey` on `Job` / `Webhook` / `Notification`.
- [ ] **Five new IR-driven diagnostics from bucket-jobs-cycle
      §Doctor table** are deliverable on the lifted IR:
      `JOB-TIMEOUT-001`, `JOB-FANOUT-001`, `JOB-FANOUT-002`,
      `NOTIF-CHANNEL-001`, `EVENTGROUP-NESTING-001`. `WEBHOOK-SCOPE-001`
      is SPECULATIVE per the table above — gates on pilot evidence.
- [ ] **`lazuli generate` produces Go that compiles for handler-backed
      bodies.** Runtime-team deliverable for the bucket-jobs cycle
      (row 33). Tier 3 only needs the IR to be ready. The
      declarative-body case lands when Tier 4 closes.
- [ ] **LSP hovers + completions on every new construct.** Today
      LSP has hovers on `tenant_from` and `fanout`
      (bucket-jobs-cycle §"Doctor/LSP propostos" lists them).
      Confirm coverage extends to `verify`/`secret`/`header`/`scope`
      under `webhook`, `channel`/`recipient`/`template` under
      `notification`, `pattern`/`on`/`payload` under `event_group`.
- [ ] **Inspect golden file pinned.** Mirrors Tier 1's
      `tests/fixtures/full-capsule-auth.golden.json`. Bucket-jobs-cycle
      §Evals expects `bucket-jobs-cycle.golden.json` —
      same fixture, Tier 3 produces the initial pinned shape.

The first five items are language-team Tier 3 deliverables. Item 6 is
Drusa-team (row 33). Items 7-8 are small additive deliverables that
land alongside Tier 3.

## Recomendação

1. **Take Route C** (subset scoping). Estimated scope: **~4.5 cells**
   (IR extensions ~1 + four parser recognisers ~3.25 + inspect
   projections ~0.75). Larger than Tier 1 (~2 cells) and Tier 2 (~2
   cells) but bounded by an explicit carve-out (`raw_*` on
   `JobDeclarative`) that closes when Tier 4 lands.
2. **Land the IR extensions first, then parsers, then inspect** — the
   same ordering Tier 1's erratum
   (`docs/proposals/auth-lowering-scope.md:21-23`) settled. Lowering
   that writes IR fields the IR doesn't carry yet wastes a review
   cycle.
3. **Promote 5 LSP diagnostics to doctor cross-checks in the same
   commit window as the IR lift.** Each promotion is a small atomic
   diff once the IR field exists. The LSP rules stay running on
   single-file paths; the doctor rules add cross-feature coverage.
   This is the same pattern row 27 used for auth.
4. **Defer 7 SPECULATIVE items** (`Job.audit`, `Webhook.scope`,
   `Job.approval`, `Job.expose http`, custom backoff variants,
   `Job.dlq`, push/SMS channels). Each gates on pilot pressure that
   the §0 bucket cycle would surface in a subsequent run. None of
   them land in Tier 3.
5. **Update `docs/next-checklist.md` row 24** (Phase L) after Tier 3
   lands to record that `job`, `webhook`, `notification`,
   `event_group` join the slice's coverage. Row 32 (Jobs scope-out)
   becomes "done — see Tier 3" once Tier 3 ships; rows 33-34
   (cycle closure, event_group rule) stay open and become the
   runtime-team queue.
6. **Tier 4 picks up the declarative spine** — `parse_command`,
   `parse_resource`, `parse_query`, `parse_record`. When Tier 4
   lands, the `raw_*` carve-out in `JobDeclarative` is replaced by
   reusing the shared declarative-body recogniser, in the same
   commit that introduces `parse_command`. Tier 4 inherits the
   declarative shape from a real command parser, not from a job
   parser building it first.

When Tier 3 is implemented, the bucket-jobs cycle
(`docs/proposals/bucket-jobs-cycle.md`) runs against the shipped IR
and Drusa codegen has typed input for 3 of 4 fixture end-to-end
loops. The last loop (`recompute_score_after_invoice` declarative
reactor) closes after Tier 4.
