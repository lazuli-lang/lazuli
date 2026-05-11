# Bucket Jobs — Scope Resolution (Pre-Cycle)

**Status**: blocker scope-out before running Stages 3-9 of `/lazuli-bucket-cycle bucket=jobs`.

This document is the analogue of `bucket-auth-cycle.md`'s "auth lowering scope"
side-quest: surface authored in the fixture that does not reach the IR / inspect
projection layer. Until this is resolved, the L1→L2 design for jobs cannot land
because codegen and runtime have no typed shape to consume.

The cycle proposal (`bucket-jobs-cycle.md`) is written **against the scope this
document defines** — not against the current `Job` / `Webhook` IR structs.

---

## What's broken (single fact)

`InspectFeature` (in `crates/lazuli_cli/src/main.rs:454`) carries detailed
projections for `agents` (`InspectAgent`, `:3075+`) and `notifications`
(`InspectNotification`, `:3029+`), but **not for `jobs` or `webhooks`**. Today
those two only surface as **flat name lists inside `summary`**
(`crates/lazuli_cli/src/main.rs:618-619`).

Concretely:

```bash
cargo run -q -p lazuli_cli -- inspect examples/full-capsule/full-capsule.lzi --format=json \
  | python -c "import json,sys; d=json.load(sys.stdin); \
    [print(f['name'],'->',sorted(f.keys())) for f in d['features']]"
```

returns

```
customer          -> ['agents', 'name']
customer_auth     -> []
customer_tags     -> []
customer_import   -> ['external_calls', 'name', 'requirements']
customer_outreach -> ['name', 'notifications']
```

The `customer` feature owns `job recompute_score_after_invoice`
(`examples/full-capsule/full-capsule.lzi:392`) and
`job recompute_scores` (`:404`); `customer_import` owns `job process_import`
(`:760`) and `webhook crm_customer_upsert` (`:773`). Not one of those declarations
emits trigger / idempotency / tenant_from / retry / fanout / queue / timeout /
handler / emits / calls as inspect projection facts.

Compare with notifications, which carry the full shape
(`InspectNotification` at `crates/lazuli_cli/src/main.rs:3029`):

```json
{
  "name": "welcome_email",
  "channels": ["email"],
  "recipient": "target.email",
  "trigger": "event customer.customer_activated",
  "template": "./outreach/welcome_email.mjml",
  "policy": "@policy.notify",
  "tenant_from": "payload.org_id",
  "idempotency": "by envelope.id",
  "retry": "3 backoff exponential",
  "origin": "notification"
}
```

That's the shape `jobs` and `webhooks` should have. Today they don't.

---

## Why this blocks the L0→L2 cycle

1. **Codegen has no input.** `lazuli_codegen_go` cannot emit a typed Job runner,
   scheduler entry, or webhook receiver if the only thing inspect reports is a
   string name. `dist/go/customer/customer.gen.go` confirms this — it's hand-
   written today and contains zero job/webhook/notification scaffolding.
2. **Drusa has nothing to register.** `runtime/go/lazuli/eventbus.go` is the
   only job-adjacent file in the runtime, and it's an in-process best-effort
   pub/sub for `EventEmit`. There is no `JobRunner`, `WebhookHandler`,
   `NotificationDispatcher`. Without an IR-driven inspect shape, generators
   can't materialise `Register(...)` calls per kind.
3. **Doctor cross-checks already exist** (`event_job_tenant_from_diagnostics`,
   `scheduled_job_tenancy_diagnostics`, `webhook_tenant_from_diagnostics`,
   `idempotency_key_diagnostics`, `notification_contract_diagnostics`). Those
   run against the **text source**, not the IR projection — so doctor sees the
   surface and codegen doesn't. Eight-month tech debt waiting to happen.
4. **IR `Job` struct exists** at `crates/lazuli_ir/src/lib.rs:1684` but is
   **not connected to inspect**. The `IR -> InspectFeature.jobs: Vec<InspectJob>`
   conversion never landed. Same for `Webhook` (`:1769`).
5. **IR is incomplete on real surface**. The fixture authors `fanout tenants
   org` (`:406`), `queue customer_imports` (`:762`), `timeout "30s"` (`:769`),
   `calls crm.normalize_import_batch` (`:766-768`), and per-channel
   `notification` (no IR struct at all). The IR `Job` (`:1684`) only knows
   about `trigger`, `queue`, `idempotency`, `retry`, `policy`, `body`, `emits`,
   `previous_names`. Fields that the fixture writes are dropped. Notification
   has no IR struct.

The cycle proposal cannot ship until inspect/IR/codegen agree on what a Job is.

---

## What needs to happen (this side-quest)

Three concrete fixes — all design, the implementation lands inside the
`bucket-jobs-cycle.md` Phase 1 of the eventual Cut J:

### Fix 1: lift `Job` and `Webhook` IR to match fixture surface

Extend `crates/lazuli_ir/src/lib.rs:1684` `Job` struct with the missing fields
that the fixture writes:

```rust
pub struct Job {
    pub name: String,
    pub trigger: JobTrigger,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<IdempotencyKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRef>,
    pub body: JobBody,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,

    // --- NEW (this scope) ---

    /// `tenant_from payload.<axis>_id` — tenant axis derivation for the
    /// envelope. Required when the feature has a tenant axis unless
    /// `fanout tenants <axis>` is set. Doctor cross-checks today as text;
    /// post-fix it cross-checks IR.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_from: Option<TenantFromSpec>,

    /// `fanout tenants <axis>` — scheduled job runs once per tenant on the
    /// given axis. Mutually exclusive with `tenant_from` (the runtime
    /// derives tenant per fanout slot, not from payload).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fanout: Option<FanoutSpec>,

    /// `timeout "30s"` — wall-clock cap for one invocation. Adapter default
    /// applies when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,

    /// `calls <slot>.<operation>` — external operations dispatched inside
    /// the job. IR mirror of what's already in `external_calls`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_calls: Vec<JobExternalCall>,

    /// `audit` child — already supported on commands; mirror it on jobs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<AuditSpec>,
}
```

`Webhook` gets the same treatment for `tenant_from`, `verify` shape
(`hmac sha256` + `secret env.X` + `header "X-..."`), and a typed
`scope <global|tenant>` discriminator with `reason` for the global escape.

`JobBody::Handler` already exists; add a sibling that captures `calls` directly
so handler-less, declarative jobs (the `recompute_score_after_invoice` style)
materialise their effects in IR too.

### Fix 2: add `Notification` and `EventGroup` IR structs

Today both are surface-only. Notification has an `InspectNotification` (in CLI
text-based parsing) but no IR. EventGroup has neither.

```rust
pub struct Notification {
    pub name: String,
    pub channels: Vec<NotificationChannel>,  // enum: Email|Push|Sms|InApp
    pub recipient: Path,
    pub trigger: NotificationTrigger,         // mirrors JobTrigger today; reuse
    pub template: PathRef,
    pub policy: PolicyRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_from: Option<TenantFromSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<IdempotencyKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

pub struct EventGroup {
    pub pattern: String,       // `customer_*`
    pub on_resource: QualifiedName,
    pub payload: Vec<EventGroupPayloadField>,  // typed bindings
    pub events: Vec<Event>,    // already typed via existing Event struct
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}
```

Today `inspect_events` (`crates/lazuli_cli/src/main.rs:2185`) already resolves
event_group inheritance text-side. The IR struct makes it referenceable from
codegen and other inspect projections.

### Fix 3: wire IR -> InspectFeature for jobs/webhooks/notifications

Extend `InspectFeature` (`crates/lazuli_cli/src/main.rs:454`) with three new
fields paralleling `agents` and `notifications`:

```rust
struct InspectFeature {
    // ... existing ...
    #[serde(skip_serializing_if = "Vec::is_empty")]
    jobs: Vec<InspectJob>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    webhooks: Vec<InspectWebhook>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    event_groups: Vec<InspectEventGroup>,
    // notifications already exists
}
```

Mirror the `InspectNotification` pattern (`:3029`) for each:

```rust
struct InspectJob {
    name: String,
    operational_kind: String,   // "scheduled" | "reactor" | "queued_worker"
    trigger: String,            // "event customer_activated" | "schedule '0 2 * * *'"
    queue: Option<String>,
    tenant_from: Option<String>,
    fanout: Option<String>,
    idempotency: Option<String>,
    retry: Option<String>,
    timeout: Option<String>,
    policy: Option<String>,
    handler: Option<String>,
    emits: Vec<String>,
    external_calls: Vec<String>,   // `crm.normalize_import_batch`
    audit: Option<String>,
    origin: &'static str,          // "job"
}

struct InspectWebhook {
    name: String,
    path: String,
    verify: InspectWebhookVerify,  // method + secret-ref + header
    tenant_from: Option<String>,
    scope: Option<String>,         // "tenant" | "global"
    scope_reason: Option<String>,
    idempotency: Option<String>,
    policy: Option<String>,
    handler: String,
    returns: Option<String>,
    emits: Vec<String>,
    audit: Option<String>,
    origin: &'static str,          // "webhook"
}

struct InspectEventGroup {
    pattern: String,
    on_resource: String,
    payload: Vec<InspectEventGroupField>,
    events: Vec<String>,           // concrete event names declared under this group
    origin: &'static str,          // "event_group"
}
```

This is the **canonical projection** the cycle proposal designs against.

---

## What this scope is NOT

- Not a parser change. The text-based `top_level_blocks` (`:2691`) and `inspect_*`
  walkers already work; the scope wires their results into typed IR structs.
- Not a new primitive. `job`, `webhook`, `notification`, `event_group`,
  `tenant_from`, `fanout`, `idempotency`, `retry`, `queue`, `timeout`, `calls`
  are already invariant-protected surface (`docs/invariants.md:294-331`).
- Not a runtime change. Drusa work happens in the cycle proposal, on top of
  this scope.
- Not a doctor change. Doctor stays text-based for this scope; later cuts can
  migrate it to consume IR (parallel to Phase L for commands).

---

## Acceptance check

After this scope lands, the inspect output for `customer` and
`customer_import` features carries `jobs`, `webhooks`, and `event_groups`
arrays with the full shape above. The current notification shape is the model.

Then — and only then — the cycle proposal in `bucket-jobs-cycle.md` is
implementable against typed IR.
