# Bucket Cycle: Jobs (L0→L2)

**Run**: `/lazuli-bucket-cycle bucket=jobs mode=design`
**Date**: 2026-05-10
**Pre-requisite**: `bucket-jobs-scope.md` (jobs/webhooks/event_groups inspect
projection) must land first.

## Contexto

Jobs is pilot bucket #3 of the §0 roadmap strategy. The fixture already
expresses every job pattern Lazuli needs to support at L0 — event-triggered
reactors (`job recompute_score_after_invoice`,
`examples/full-capsule/full-capsule.lzi:392`), scheduled fanout
(`job recompute_scores`, `:404`), event-triggered queued workers
(`job process_import`, `:760`) with `queue`, `timeout`, `calls`, and `retry`
— and the matching `webhook` (`crm_customer_upsert`, `:773`) and
`notification` (`welcome_email`, `:817`) declarations that share the same
trigger/idempotency/tenant_from vocabulary. Invariants in
`docs/invariants.md:294-331` already pin the contract; doctor and LSP already
catch the canonical mistakes (`event_job_tenant_from_diagnostics`,
`scheduled_job_tenancy_diagnostics`, `webhook_tenant_from_diagnostics`,
`idempotency_key_diagnostics`).

What is missing is L2: nothing runs. `runtime/go/lazuli/eventbus.go` is the
sole job-adjacent file in Drusa — an in-process best-effort pub/sub that
ignores tenant scope, has no retry, no DLQ, no schedule entry, no
queue lane, no webhook receiver, no notification dispatcher. The IR's `Job`
struct (`crates/lazuli_ir/src/lib.rs:1684`) drops half the fixture's
declarations on the floor (no `fanout`, no `timeout`, no `tenant_from`, no
typed `calls`), and `Notification` / `EventGroup` aren't IR structs at all.
`InspectFeature` (`crates/lazuli_cli/src/main.rs:454`) exposes
`notifications` with a full shape but **only the names** of `jobs` and
`webhooks` — codegen has no input to consume.

The "closed cycle" gate for jobs reads: an event-triggered job declared in
the fixture compiles to a Go worker that registers with River (the primary
queue adapter), receives the post-commit emit, dedupes by the declared
`idempotency by` key, runs inside the declared `tenant_from` axis, retries
on the declared backoff, and survives `cargo test -q` against a synctest
fake clock. The same loop must close for scheduled jobs (cron entry, fanout
per tenant), webhooks (HMAC verify, idempotency, tenant_from), and
notifications (typed channel dispatch through a mailer/push/sms adapter).
Until that loop closes for at least one job kind, the rest of §1 cannot
safely grow horizontally — the surface keeps accreting and the runtime
never catches up.

## Baseline

Inventario L0/L1/L2 dos constructs já presentes no fixture, parser, IR,
doctor/LSP, codegen, e runtime. `Surface` é "lê do fixture canônico";
`Grammar` é "parser reconhece"; `IR` é "struct dedicado em
`lazuli_ir`"; `Doctor/LSP` é "diagnostic cross-checa"; `Codegen` é
"`lazuli_codegen_go` produz arquivo Go"; `Runtime` é "Drusa
executa".

| Construct | Surface | Grammar | IR | Doctor/LSP | Codegen | Runtime | L-level |
|---|---|---|---|---|---|---|---|
| `job <name>` (reactor) | yes (`:392`) | line-based (`main.rs:933`) | partial (`ir:1684`; no `tenant_from`/`fanout`/`timeout`/`audit`/`calls` typed) | yes (`event_job_tenant_from`, `INT-CALL-004`, `idempotency_key`) | no | no | L1 partial |
| `job <name>` (scheduled fanout) | yes (`:404`) | line-based | as above | yes (`scheduled_job_tenancy`) | no | no | L1 partial |
| `job <name>` (queued worker) | yes (`:760`) | line-based | as above | yes | no | no | L1 partial |
| `trigger event <ref>` | yes (`:393`,`:761`) | line-based | yes (`JobTrigger::Event`) | yes (`event_trace_trigger`, payload drift) | no | no | L1 |
| `trigger schedule "<cron>"` | yes (`:405`) | line-based | yes (`JobTrigger::Schedule`) | yes (cron-presence check) | no | no | L1 |
| `idempotency by <path>` | yes (`:395`,`:407`,`:764`,`:779`,`:822`) | line-based | yes (`IdempotencyKey`) | yes (`idempotency_key_diagnostics`) | no | no | L1 |
| `tenant_from payload.<axis>_id` | yes (`:394`,`:763`,`:778`,`:821`) | line-based | **missing field in `Job`/`Webhook`/`Notification`** | yes (5 diagnostics) | no | no | L0+doctor |
| `fanout tenants <axis>` | yes (`:406`) | line-based | **missing IR field** | yes (`scheduled_job_tenancy`) | no | no | L0+doctor |
| `queue <lane>` | yes (`:762`) | line-based | yes (`queue: Option<String>` in `Job`) | no | no | no | L1 |
| `retry <N> backoff <strategy>` | yes (`:408`,`:765`,`:823`) | line-based | yes (`RetryPolicy`) | yes (`INT-CALL-003`) | no | no | L1 |
| `timeout "<duration>"` (job) | yes (`:769`) | line-based | **missing IR field on `Job`** | partial (job-level cross-check; calls-level present) | no | no | L0+doctor |
| `calls <slot>.<op>` (job) | yes (`:766-768`) | line-based | typed for inspect (`InspectExternalCall`); no `JobExternalCall` IR mirror | yes (4 INT-CALL diagnostics) | no | no | L1 partial |
| `handler "./..."` (job) | yes (`:409`,`:770`) | line-based | yes (`JobHandler`) | no | no | no | L1 |
| `emits <event>` (job) | yes (`:400-402`) | line-based | yes (`Job.emits: Vec<String>`) | yes (event-name resolution) | no | no | L1 |
| `webhook <name>` | yes (`:773`) | line-based (`doctor.rs:922`) | yes (`Webhook`) | yes (security + tenant_from) | no | no | L1 |
| `verify hmac sha256` | yes (`:775`) | line-based | partial (typed as `PathRef` only; missing structured `verify`) | yes (`webhook_security`) | no | no | L0+doctor |
| `notification <name>` | yes (`:817`) | line-based (`main.rs:3029`) | **missing IR struct** | yes (`notification_contract`) | inspect-only | no | L0+inspect |
| `event_group <pattern> on <Resource>` | yes (`:173`) | line-based (`main.rs:3803`) | **missing IR struct** | yes (event_group walkers) | no | no | L0+doctor |
| `event <name>` (domain) | yes (`:179`) | line-based | yes (`Event{kind:Domain}`) | yes (resolution + drift) | no | no | L1 |
| `event.trace <name>` | yes (`:196`) | line-based | yes (`Event{kind:Trace}`, `built_in_trace_events`) | yes (4 diagnostics) | no | no | L1 |
| capability `queue <name>` (registry) | yes (`registry.lzi:14`) | line-based | yes (`AppCapability`) | partial (no adapter binding cross-check) | no | no | L0+inspect |
| capability `event_bus <name>` (registry) | yes (`registry.lzi:17`) | line-based | yes | partial | no | no | L0+inspect |
| `runtime unit worker runs jobs *` | yes (`app.lzi:85`) | line-based | yes (`AppRuntimeUnit`) | partial | no | no | L0+inspect |
| `runtime unit scheduler runs schedules *` | yes (`app.lzi:88`) | line-based | yes | partial | no | no | L0+inspect |

**Summary**: Surface and doctor/LSP are L1-mature. IR is L1-partial (Job and
Webhook structs exist but drop authored fields; Notification and EventGroup
have no IR struct). Inspect is L1-blocked: jobs/webhooks/event_groups don't
surface as feature-level projections. Codegen is **zero**. Runtime is
**zero** beyond a best-effort EventBus.

The cycle's first deliverable is the inspect projection (scope-out
document); everything else stacks on top.

## Linguagem proposta

The bucket is **already L0-expressive** for canonical patterns. The
language work in this proposal is **closing tight contracts**, not adding
primitives. Three additions, all pilot-supported by the fixture itself:

### 1. `job ... approval` (sibling of command approval; pilot-gated)

The Cut A.9 `approval` block (committed b0304b4) lives only on commands.
Jobs that trigger destructive effects (delete-style retention sweeps,
billing reversals) would benefit from the same gate. Mirror the surface
verbatim — same children, same closed catalog. **Defer until a fixture
job needs it** — none in the current capsule does.

```
job purge_archived_customers
  trigger schedule "0 3 * * 0"
  fanout tenants org
  idempotency by tenant.org_id, schedule.week
  approval
    required_when fanout.count > 1000
    by @role.dpo
    timeout "24h"
    then deny
  handler "./jobs/purge_archived.go"
```

Cost: small (mirror existing Cut A.9 IR). Value: pilot-dependent. **Mark
`pilot-gated` — do not implement until fixture demand exists.**

### 2. `audit` child on `job` and `webhook` (parallel to commands)

`docs/invariants.md:93` already declares this as supported, but the IR
`Job` and `Webhook` structs don't expose the field. Surface-side it's
authored as `audit actor, target.id, payload.external_id` (mirroring
commands). The scope-out document (Fix 1) brings it into IR.

Cost: trivial. Value: declarative audit log without ad-hoc handler files.

### 3. `dlq` / `dead_letter` declaration on `job` (PILOT-NEEDED)

Sketch only. Today retry exhaustion implicitly drops to whatever the
adapter does (River dead-letters by default). The fixture doesn't declare
a dead-letter destination, and Drusa hasn't shipped one. **Do not promote**
unless a pilot needs custom DLQ routing — most products are fine with the
adapter default. If pressure shows up, the surface lands as:

```
job process_import
  ...
  retry 3 backoff exponential
  on_exhausted dlq "./jobs/process_import_dlq.go"
```

Closed catalog: `dlq "<path>"` (handler-style) or `dlq emit <event>` (re-emit
as event for separate observability pipeline). The runtime never picks the
"silent drop" path.

### Anti-proposals (rejected here)

- **`outbox` / `inbox` / `event_store` kinds.** Already classified F in
  `framework-coverage-1400.md:246,254`; gates on pilot evidence. Do not
  promote in this cycle.
- **`chain` / `batch` / `lock` / `leader` kinds.** Listed `DL` in roadmap
  §1.13 but no fixture pressure today. Defer.
- **`subscriber` / `upcaster` kinds.** Listed `DL` in §1.14. Same defer
  reasoning — the existing `emits` + cross-feature event resolution covers
  the canonical reaction graph. Subscribers and upcasters belong to a
  versioned-events cut, not the L0→L2 jobs cycle.
- **Provider keywords.** `river`, `asynq`, `nats`, `kafka`, `rabbitmq`, `sqs`
  never appear in core syntax. Registry capability `queue <name>` resolves
  through `@adapter.<name>` or `@runtime/...` package refs.

## IR proposto

The bulk of IR work is **structural lift** captured in
`bucket-jobs-scope.md`. Recap with cycle-specific notes:

### `Job` struct extension (`crates/lazuli_ir/src/lib.rs:1684`)

Add `tenant_from`, `fanout`, `timeout`, `external_calls`, `audit` fields.
Reuse:

- `TenantFromSpec` — new typed struct: `{ path: Path }`. Single shape used
  by `Job`, `Webhook`, `Notification`.
- `FanoutSpec` — new typed struct: `{ axis: String, scope: FanoutScope }`
  where `FanoutScope::Tenants` is the v0 variant. Reserved for future
  `fanout regions <name>` / `fanout shards <name>` without renaming.
- `JobExternalCall` — mirror of `Command.external_calls` if not already
  shared; reuses the inspect-side `InspectExternalCall` shape verbatim.
- `AuditSpec` — already exists from Cut 3 (`audit actor, target.id, ...`).
  Reused, not duplicated.

### `Webhook` struct extension (`crates/lazuli_ir/src/lib.rs:1769`)

Add `tenant_from`, structured `verify`, optional `scope` + `scope_reason`:

```rust
pub struct WebhookVerify {
    pub algorithm: WebhookVerifyAlgorithm, // HmacSha256 | HmacSha512 | None
    pub secret: EnvRef,
    pub header: String,
    pub reason: Option<String>, // required only when algorithm == None
}
```

The `verify "./path.go"` escape hatch lowers to
`WebhookVerify { algorithm: Custom, custom_path: Some(...) }` — keeps the
typed shape closed.

### New `Notification` struct

Shape in `bucket-jobs-scope.md`. Goes after `Webhook` in `lib.rs:1788`.

### New `EventGroup` struct

Shape in `bucket-jobs-scope.md`. Today `event_group` resolution is
text-side inside `inspect_events` (`main.rs:2185`). The IR lift means the
pattern matching and payload-inheritance logic moves into the analyzer
(`crates/lazuli_analyzer/src/lib.rs`) once and inspect just projects.

### `JobOperationalKind` exposure

`JobOperationalKind` enum already exists at `lib.rs:1718` (`Scheduled` /
`Reactor` / `QueuedWorker`). Today it's derived but **not surfaced in
inspect**. Lift into `InspectJob.operational_kind` so codegen targets the
right Drusa registration without recomputing.

### Diagnostics added on top of the scope-out

| Code | Severity | Trigger |
|---|---|---|
| `JOB-TIMEOUT-001` | Warning | `job` with `calls <slot>.<op>` but no `timeout` declared at job level (today's `calls` `timeout` lives per-call; warn when neither is set). |
| `JOB-FANOUT-001` | Error | `fanout tenants <axis>` references an axis not declared in any `defaults tenancy <axis>` reachable via `uses`. |
| `JOB-FANOUT-002` | Error | `fanout` and `tenant_from` both declared on the same job (mutually exclusive). |
| `WEBHOOK-SCOPE-001` | Error | `webhook` with `scope global` but no `reason "..."` child. (Already covered text-side; promote to IR.) |
| `NOTIF-CHANNEL-001` | Error | `notification` with `channel push` or `channel sms` but no adapter binding for the corresponding registry capability. |
| `EVENTGROUP-NESTING-001` | Error | concrete `event <name>` inside `event_group <pattern>` whose name doesn't match the prefix (`customer_*` requires `customer_<...>`). |

Six new IR-driven diagnostics. Five of them already exist as
text-pattern checks in `crates/lazuli_lsp/src/lib.rs`; the lift promotes
them from "single-file LSP rule" to "cross-feature doctor rule".

### JSON shape (`lazuli inspect --format=json`)

Per `InspectFeature` after the scope-out:

```json
{
  "name": "customer",
  "jobs": [
    {
      "name": "recompute_score_after_invoice",
      "operational_kind": "reactor",
      "trigger": "event billing.invoice_paid",
      "tenant_from": "payload.org_id",
      "idempotency": "by envelope.id",
      "policy": null,
      "emits": ["customer_score_recomputed"],
      "external_calls": [],
      "origin": "job"
    },
    {
      "name": "recompute_scores",
      "operational_kind": "scheduled",
      "trigger": "schedule \"0 2 * * *\"",
      "fanout": "tenants org",
      "idempotency": "by tenant.org_id, schedule.day",
      "retry": "3 backoff exponential",
      "handler": "./jobs/recompute_scores.go",
      "origin": "job"
    }
  ],
  "webhooks": [],
  "event_groups": [
    {
      "pattern": "customer_*",
      "on_resource": "Customer",
      "payload": [
        { "name": "customer_id", "type": "ID", "expression": "id" },
        { "name": "org_id", "type": "ID", "expression": "org.id" },
        { "name": "by_id", "type": "ID", "expression": "ctx.user.id", "condition": "@actor.user" }
      ],
      "events": ["customer_created", "customer_status_changed", "customer_activated", "customer_paused", "customer_archived", "customer_reassigned"],
      "origin": "event_group"
    }
  ]
}
```

## Codegen proposto

`lazuli_codegen_go` produces three new files per feature carrying jobs,
webhooks, or notifications. Each file imports the runtime package
(`lazuli.dev/runtime/lazuli`) and a per-bucket subpackage where adapter
wiring lands (see `## Runtime proposto`).

### File 1: `dist/go/<feature>/jobs.gen.go`

One file per feature with jobs. Emits a `RegisterJobs(r *lazuli.JobRegistry)`
function the boot path calls.

```go
// path: dist/go/customer/jobs.gen.go
// Code generated by lazuli; DO NOT EDIT.
package customer

import (
	"context"
	"time"

	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/jobs"
)

// RecomputeScoreAfterInvoicePayload is the typed envelope payload for the
// `billing.invoice_paid` event subscription, derived from the cross-feature
// event contract via inspect resolution at generate-time.
type RecomputeScoreAfterInvoicePayload struct {
	CustomerID lazuli.ID `json:"customer_id"`
	OrgID      lazuli.ID `json:"org_id"`
}

func recomputeScoreAfterInvoice(ctx *lazuli.Ctx, env lazuli.Envelope, p RecomputeScoreAfterInvoicePayload) error {
	target, err := QueryByID(ctx, p.CustomerID)
	if err != nil {
		return err
	}
	newScore, err := riskScore(ctx, target)
	if err != nil {
		return err
	}
	return lazuli.WithTx(ctx, func(tx lazuli.Tx) error {
		if _, err := tx.Exec(ctx, updateScoreSQL, newScore, target.ID); err != nil {
			return err
		}
		return lazuli.Publish(ctx, lazuli.Event{
			Name:    "customer.customer_score_recomputed",
			Tenant:  &lazuli.Tenant{Axis: "org", ID: p.OrgID},
			Payload: map[string]any{"score": newScore, "reason": "invoice_paid"},
		})
	})
}

func RegisterJobs(r *lazuli.JobRegistry) {
	r.Register(jobs.Spec{
		Name:           "customer.recompute_score_after_invoice",
		OperationalKind: jobs.Reactor,
		Trigger:        jobs.EventTrigger{Event: "billing.invoice_paid"},
		TenantFrom:     "payload.org_id",
		Idempotency:    "envelope.id",
		Run:            jobs.Wrap(recomputeScoreAfterInvoice),
	})

	r.Register(jobs.Spec{
		Name:           "customer.recompute_scores",
		OperationalKind: jobs.Scheduled,
		Trigger:        jobs.ScheduleTrigger{Cron: "0 2 * * *"},
		Fanout:         jobs.FanoutSpec{Axis: "org"},
		Idempotency:    "tenant.org_id, schedule.day",
		Retry:          jobs.Retry{Count: 3, Backoff: jobs.Exponential},
		Handler:        recomputeScoresHandler, // author-supplied in ./jobs/recompute_scores.go
	})
}
```

`jobs.Wrap` is a generic that wraps `func(*Ctx, Envelope, P) error` into the
`func(context.Context, []byte) error` shape River wants. Codegen knows the
payload type because `event billing.invoice_paid` resolves through the
external contract / cross-feature event graph at inspect time.

### File 2: `dist/go/<feature>/webhooks.gen.go`

```go
// path: dist/go/customer_import/webhooks.gen.go
// Code generated by lazuli; DO NOT EDIT.
package customer_import

import (
	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/webhooks"
)

type CrmCustomerUpsertPayload struct {
	OrgID      lazuli.ID `json:"org_id"`
	ExternalID string    `json:"external_id"`
	// ...
}

func RegisterWebhooks(r *webhooks.Registry) {
	r.Register(webhooks.Spec{
		Name:        "customer_import.crm_customer_upsert",
		Path:        "/webhooks/crm/customer-upsert",
		Verify:      webhooks.HmacSha256{SecretEnv: "CRM_WEBHOOK_SECRET", Header: "X-CRM-Signature"},
		TenantFrom:  "payload.org_id",
		Idempotency: "payload.org_id, payload.external_id",
		Handler:     upsertCustomerFromCRMHandler, // author-supplied
		Emits:       []string{"customer_webhook_received"},
	})
}
```

### File 3: `dist/go/<feature>/notifications.gen.go`

```go
// path: dist/go/customer_outreach/notifications.gen.go
// Code generated by lazuli; DO NOT EDIT.
package customer_outreach

import (
	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/notifications"
)

func RegisterNotifications(r *notifications.Registry) {
	r.Register(notifications.Spec{
		Name:        "customer_outreach.welcome_email",
		Channels:    []notifications.Channel{notifications.Email},
		Recipient:   "target.email",
		Trigger:     notifications.EventTrigger{Event: "customer.customer_activated"},
		Template:    "./outreach/welcome_email.mjml",
		TenantFrom:  "payload.org_id",
		Idempotency: "envelope.id",
		Retry:       notifications.Retry{Count: 3, Backoff: notifications.Exponential},
		Emits:       []string{"welcome_email_sent"},
	})
}
```

### Boot wiring (`dist/go/main.gen.go`)

Boot composes `RegisterJobs`/`RegisterWebhooks`/`RegisterNotifications` from
every generated feature into the runtime registries. The composed boot is
deterministic — alphabetical by feature, alphabetical by name inside each
file.

No provider names anywhere in generated code. `JobRegistry`,
`webhooks.Registry`, `notifications.Registry` are Drusa-side; River/Sendgrid/
any provider wiring happens through the registry adapter binding (see
`## Runtime proposto`).

## Runtime proposto

Drusa entrega three new subpackages under `runtime/go/lazuli/`. Each is the
Lazuli-side typed contract; concrete adapters live in `runtime/go/lazuli/<bucket>/
<adapter>` (e.g. `runtime/go/lazuli/jobs/river` for River, `notifications/sendgrid`
for Sendgrid). Adapters never leak into Drusa's stable surface.

### `runtime/go/lazuli/jobs/`

Capabilities exposed to generated code:

- `Spec` — typed registration shape (`Name`, `OperationalKind`, `Trigger`,
  `TenantFrom`, `Fanout`, `Idempotency`, `Retry`, `Timeout`, `Run`/`Handler`).
- `JobRegistry` (alias `lazuli.JobRegistry`) — typed registry that
  generated `RegisterJobs` calls. Resolves capability `queue <name>` from
  `app.lzi` runtime unit `runs jobs *` to an adapter binding.
- `EventTrigger`, `ScheduleTrigger` — discriminated triggers.
- `FanoutSpec`, `Retry`, `Exponential`/`Fixed` constants.
- `Wrap[P]` — generic adapter that converts typed-payload reactor functions
  into the byte-slice handler the queue adapter consumes.

Adapter contract (`runtime/go/lazuli/jobs.Adapter` interface):

```go
type Adapter interface {
    // Enqueue is called by the runtime post-commit (`EventBus` extends to
    // hand-off to Adapter when the consumer is a Job, not an in-process Subscriber).
    Enqueue(ctx context.Context, lane string, spec EnqueueSpec) error

    // Schedule installs a cron entry. Called once at boot for each Scheduled job.
    Schedule(spec ScheduleSpec) error

    // Start launches the worker pool. Drusa's `Boot` calls this with the
    // app.lzi `unit worker` configuration.
    Start(ctx context.Context, cfg WorkerConfig) error

    // Shutdown drains in-flight jobs respecting Timeout.
    Shutdown(ctx context.Context) error
}
```

Primary adapter: **River** (`runtime/go/lazuli/jobs/river`). Backed by
`pgx` against the same Postgres pool used by queries — keeps the same
transactional boundary the runtime already maintains in `db.go` /
`handle.go`. River is **not** mentioned in any generated `.gen.go` file
and not in any Lazuli source.

Secondary adapter (DA, future): Asynq (Redis), in-process for tests.

Configuration consumed:

- `app.lzi` `runtime unit worker runs jobs *` → adapter `Start` config.
- `app.lzi` `runtime unit scheduler runs schedules *` → adapter `Schedule`
  calls for every `JobOperationalKind::Scheduled` spec.
- `registry.lzi` `capabilities queue <name>` → adapter selection (resolved
  via `bindings <feature>.<slot> = <adapter>` in `app.lzi`, same pattern as
  CRM integration today).

Lifecycle:

- Boot: `lazuli.Boot` instantiates the adapter, calls `Start`, then
  `RegisterJobs`/`RegisterWebhooks`/`RegisterNotifications` from every
  generated feature module.
- Hot reload (future, F): adapter is allowed to be a no-op; generated
  specs are re-registered idempotently by name.

Typed errors:

```go
type JobError struct {
    Kind     JobErrorKind  // EnqueueFailed | ScheduleConflict | HandlerPanic | IdempotencyViolation | TimeoutExceeded
    Job      string
    Cause    error
    Tenant   *lazuli.Tenant
}
```

Surfaces to `expose client 5xx code` when an `api` indirectly triggers a
job (the few cases where it matters). `IdempotencyViolation` is **not** an
error — it's a no-op for the caller and a `slog.Info` event for ops.

### `runtime/go/lazuli/webhooks/`

Typed registry + adapter contract for inbound webhook routing. Adapter is
trivial in v0: a `chi` router mounted at the configured prefix
(`/webhooks/...`) inside the same HTTP unit declared by
`runtime unit api`. HMAC verification uses `crypto/subtle.ConstantTimeCompare`
against the env-resolved secret.

Lifecycle:

- Boot: `RegisterWebhooks` from every feature → mount routes.
- Adapter switching (multiple providers): not v0 — one webhook receiver,
  many specs.

### `runtime/go/lazuli/notifications/`

Typed registry + adapter contract for outbound notifications. Channels are
closed enum (`Email | Push | Sms | InApp`). Each channel maps to a separate
adapter slot (`@adapter.notification.email`, `@adapter.notification.push`,
…) bound via registry. Primary email adapter: `runtime/go/lazuli/notifications/sendgrid`.

Notifications are themselves jobs at the runtime level — they enqueue
through the same `jobs.Adapter` for retry/dedupe. Codegen exposes them as
notification specs (cleaner author surface), but Drusa wires them to the
queue lane named `notifications` by default.

### Extending `runtime/go/lazuli/eventbus.go`

Today `Publish` runs subscribers in-process synchronously. The cycle's
runtime work extends it: when a registered consumer is a `Job`
(operational kind = reactor), `Publish` calls `Adapter.Enqueue` instead of
running the subscriber directly. In-process subscribers (UI invalidation
callbacks, etc.) remain inline. The split is invisible to Lazuli source:
the author writes `trigger event` either way; codegen picks the path.

### No leaks

- No River type in any Lazuli source.
- No SQS, Kafka, NATS, RabbitMQ name in any Lazuli source.
- No Sendgrid, Twilio, APN, FCM name in any Lazuli source.
- All provider mechanics flow through `@adapter.<name>` and the runtime's
  adapter contract above.

## Evals/Testes propostos

The cycle closes when at least one end-to-end loop passes. Tests are
layered the same way as auth — surface contract checks, IR golden, codegen
golden, runtime synctest integration.

### Eval / case (declarative test surface)

Jobs don't have `evals` blocks today — that's an agent construct. The
declarative test surface for jobs is the existing `tests` block on
commands the job triggers, plus a new `case`-style block on the job
itself (PILOT-NEEDED — defer until at least two pilots ask). For v0, the
golden file is the inspect projection:

```bash
cargo run -q -p lazuli_cli -- inspect examples/full-capsule/full-capsule.lzi \
  --format=json --expand=summary > /tmp/got.json
diff /tmp/got.json crates/lazuli_cli/tests/fixtures/full-capsule-jobs.golden.json
```

The golden file pins the post-scope-out inspect shape; any unintended drift
fails CI.

### Go test (`runtime/go/lazuli/jobs/river/river_test.go`)

```go
func TestRecomputeScoreAfterInvoice_ReactsToEvent_WithTenant(t *testing.T) {
    synctest.Run(func() {
        ctx, db := testCtx(t)
        defer db.Close(ctx)

        // Boot runtime with in-process River + test publisher
        reg := jobs.NewRegistry(t)
        customer.RegisterJobs(reg)
        startWorkers(t, ctx, reg)

        // Publish the cross-feature event
        lazuli.Publish(ctx, lazuli.Event{
            Name:   "billing.invoice_paid",
            Tenant: &lazuli.Tenant{Axis: "org", ID: lazuli.ID("org_1")},
            Payload: map[string]any{
                "customer_id": "cus_1",
                "org_id":      "org_1",
            },
        })

        // Wait for job idempotency to commit
        synctest.Wait()

        var score int
        err := db.QueryRow(ctx,
            "SELECT score FROM customers WHERE id = $1 AND org_id = $2",
            "cus_1", "org_1").Scan(&score)
        if err != nil { t.Fatal(err) }
        if score == 0 { t.Errorf("expected score recompute; got 0") }

        // Idempotency: re-publishing the same event must not run twice
        lazuli.Publish(ctx, /* same event */ )
        synctest.Wait()
        // assertions: single insert in `customer_score_recomputed_log`
    })
}
```

`synctest` (Go 1.26) replaces the spike's real clocks; River's cron
scheduler honors the fake clock for `TestRecomputeScores_FiresAt2AM`.

### Doctor test (`crates/lazuli_cli/src/doctor.rs:test`)

```rust
#[test]
fn canonical_warns_for_job_fanout_and_tenant_from_conflict() {
    let source = "
feature billing
  defaults
    tenancy org

  job nightly_sync
    trigger schedule \"0 2 * * *\"
    fanout tenants org
    tenant_from payload.org_id
    handler \"./jobs/nightly.go\"
";
    let diags = run_doctor(source);
    assert!(diags.iter().any(|d| d.code == "JOB-FANOUT-002"));
}
```

Five new doctor tests, one per IR-promoted diagnostic.

### LSP test (`crates/lazuli_lsp/src/lib.rs:test`)

Hover on `trigger event customer.customer_archived` shows resolution to
`customer.event customer_archived` payload schema with origin
`event_group:customer_*`. Completion inside a `job` body after `tenant_from `
suggests `payload.org_id` (sourced from the resolved trigger event payload).

```rust
#[test]
fn lsp_hover_on_job_trigger_event_resolves_payload() {
    let lsp = LspHarness::new(FULL_CAPSULE_SOURCES);
    let hover = lsp.hover("full-capsule.lzi", line=761, character=20);
    assert!(hover.contains("customer_import_uploaded"));
    assert!(hover.contains("batch_id: ID"));
    assert!(hover.contains("org_id: ID"));
}
```

## Doctor/LSP propostos

Diagnostics promoted from text-based LSP rules to IR-driven doctor cross-checks
(no change in surface coverage; better cross-file resolution). Plus the new
five.

| Code | Severity | Message | Trigger |
|---|---|---|---|
| `JOB-TIMEOUT-001` | Warning | `Job '<n>' calls external operations but declares no \`timeout\` at job level; the runtime will apply an adapter default. Declare \`timeout "<duration>"\` to make the contract explicit.` | `job` has `calls` child but no `timeout` and no per-call `timeout` either. |
| `JOB-FANOUT-001` | Error | `Job '<n>' \`fanout tenants <axis>\` references axis '<axis>' that is not declared in any reachable \`defaults tenancy\`.` | Axis not in `defaults.tenancy` of this feature or `uses`-d features. |
| `JOB-FANOUT-002` | Error | `Job '<n>' declares both \`fanout tenants <axis>\` and \`tenant_from <path>\`; they are mutually exclusive. Use fanout for scheduled-per-tenant, tenant_from for event-derived tenant.` | Both children present. |
| `WEBHOOK-SCOPE-001` | Error | `Webhook '<n>' declares \`scope global\` but no \`reason "..."\` child explaining the cross-tenant exposure.` | `scope global` without reason. (Lift from existing LSP rule.) |
| `NOTIF-CHANNEL-001` | Error | `Notification '<n>' declares \`channel <ch>\` but registry has no \`@adapter.notification.<ch>\` binding. Add the adapter to \`registry.lzi\` or remove the channel.` | Channel without bound adapter. |
| `EVENTGROUP-NESTING-001` | Error | `Concrete event '<n>' under \`event_group <pattern>\` does not match the pattern prefix; rename to '<expected>' or move outside the group.` | Pattern mismatch. (Lift from existing LSP rule.) |

LSP keyword/hover catalog additions:

- `tenant_from` — already in hover catalog; verify hover text references
  `payload.<axis>_id` and links to invariants `:320-322`.
- `fanout` — add to hover catalog with link to invariants `:329-330`.
- `audit` — already in hover catalog (Cut 3); confirm coverage on `job`
  and `webhook` after IR lift.

No new `@<namespace>` additions — the bucket lives inside the existing
closed set (`@policy`, `@actor`, `@role`, `@scope`, `@adapter`).

## Critério de "ciclo fechado"

The bucket cycle closes when **every** box is checked for at least one
end-to-end job + one webhook + one notification from the fixture.

- [ ] Authored in `examples/full-capsule/` — done at L0 today.
- [ ] `lazuli check` accepts — done at L0 today.
- [ ] `lazuli inspect` reports the full IR shape (jobs, webhooks,
      event_groups, notifications) per feature.
- [ ] `lazuli doctor` runs at least the six new diagnostics + the existing
      five tenant/idempotency cross-checks against typed IR.
- [ ] `lazuli generate` emits `dist/go/<feature>/jobs.gen.go`,
      `webhooks.gen.go`, `notifications.gen.go` and the composed boot.
- [ ] Drusa executes one event-triggered reactor, one scheduled fanout,
      one inbound webhook, one outbound notification end-to-end against a
      Postgres + River + Sendgrid test rig.
- [ ] At least one `synctest`-backed Go test per kind in
      `runtime/go/lazuli/jobs/` / `webhooks/` / `notifications/`.
- [ ] LSP serves hover on `trigger event <ref>` resolving to the event
      payload schema with `event_group` provenance.

## Próximo passo

1. Land the scope-out (`bucket-jobs-scope.md`) first. Inspect projection
   for jobs/webhooks/event_groups + IR Notification + IR EventGroup +
   `Job`/`Webhook` field lift. This is the only design-heavy gate.
2. Author the six new doctor diagnostics from IR (table above). Migrate
   the text-based LSP rules into doctor cross-checks where the table says
   "lift".
3. Hand off codegen + Drusa to the runtime team with the three subpackage
   contracts under `runtime/go/lazuli/jobs/`, `webhooks/`, `notifications/`.
   The agent in this profile stays in language/IR/doctor territory.
4. Close the cycle on `recompute_score_after_invoice` first (simplest:
   event-triggered, no fanout, no handler file, no external calls). Then
   `recompute_scores` (scheduled fanout). Then `process_import` (queued,
   external calls, retries). Then `crm_customer_upsert` webhook. Then
   `welcome_email` notification.

Cycle-close evidence: the `bucket-jobs-cycle.golden.json` inspect file
plus a green `cargo test -q -p lazuli_runtime --features=integration` run.

## Rows sugeridas para `docs/next-checklist.md`

| Order | Cut | Status | Notes |
|-------|-----|--------|-------|
| 26 | Jobs/Webhooks/Notifications IR lift (scope-out) | proposed | Bring `Job` + `Webhook` IR fields up to fixture surface (`tenant_from`, `fanout`, `timeout`, `external_calls`, `audit`); add `Notification` and `EventGroup` IR structs; wire `InspectFeature.{jobs, webhooks, event_groups}` projections so codegen has typed input. Pre-requisite for the L0→L2 jobs cycle. See `docs/proposals/bucket-jobs-scope.md`. |
| 27 | Jobs L0→L2 closure (cycle) | proposed | Six new IR-driven diagnostics (`JOB-TIMEOUT-001`, `JOB-FANOUT-001/002`, `WEBHOOK-SCOPE-001`, `NOTIF-CHANNEL-001`, `EVENTGROUP-NESTING-001`) + codegen for `dist/go/<feature>/{jobs,webhooks,notifications}.gen.go` + Drusa subpackages `runtime/go/lazuli/{jobs,webhooks,notifications}` + River as primary queue adapter + Sendgrid as primary email adapter. Closes pilot bucket #3 of roadmap §0. See `docs/proposals/bucket-jobs-cycle.md`. |
| 28 | `event_group` doctor pattern-prefix rule | proposed | Promote `event_group_can_own_short_event_declarations` LSP rule (`canonical_event_group_can_own_short_event_declarations` at `crates/lazuli_lsp/src/lib.rs:14522`) to a doctor cross-feature rule once `EventGroup` IR struct lands. Catches concrete events under a group whose names don't match the pattern prefix. Cheap lift, high value for cold-readability. |
