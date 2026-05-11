# Bucket Cycle: Webhooks Expanded (L0→L2)

**Run**: `/lazuli-bucket-cycle bucket=webhooks-expanded mode=design`
**Date**: 2026-05-11
**Pre-requisite**: none. Tier 3 webhooks already shipped (rows 32–33
of `docs/next-checklist.md`). This is the **second-onda** expansion of
the inbound-webhook contract — additive on top of the existing
`Webhook` IR struct, the `parse_webhook` slice, and the
`WEBHOOK-SCOPE-001` doctor diagnostic.

## Contexto

The Tier 3 cycle shipped the inbound-webhook primitive end-to-end on
the language side: `parse_webhook` lifts the body
(`crates/lazuli_syntax/src/parser.rs:2159-2266`), the IR carries
`structured_verify`, `tenant_from`, `idempotency`, `policy`,
`handler`, `emits`, `previous_names`
(`crates/lazuli_ir/src/lib.rs:2121-2150`), and doctor cross-checks
`WEBHOOK-SCOPE-001` against `tenant_from`
(`crates/lazuli_cli/src/doctor.rs:2353-2380`). The Drusa runtime stub
matches that shape exactly
(`runtime/go/lazuli/webhooks/contract.go:30-58`).

What's missing — and what `docs/roadmap.md:152-153` calls out as
"segunda onda" delivery — is the **operational envelope** around
inbound webhooks once they survive verification:

1. **No declared payload contract for what we expect to receive.** The
   fixture's `webhook crm_customer_upsert`
   (`examples/full-capsule/full-capsule.lzi:775-783`) verifies HMAC
   and extracts `payload.org_id` / `payload.external_id` for tenant +
   idempotency, but the **shape of `payload` is invisible to Lazuli**:
   no record reference, no field types, no semantic/PII annotations.
   The author's intent ("CRM sends us an upsert envelope with org +
   external id + email") lives in the handler file
   (`./integrations/upsert_customer_from_crm.go`) — exactly the
   leakage the language is supposed to close.
2. **No declared replay contract.** Real webhook integrations
   (Stripe, GitHub, MercadoPago, CRMs) routinely re-deliver the same
   event on consumer error. Today `idempotency by payload.org_id,
   payload.external_id` already gates re-execution, but **whether
   re-delivery is allowed and on what window is undeclared**. The
   adapter has to assume "always allow within X" with no SLA visible
   to the consumer.
3. **No declared dead-letter behavior.** When retries exhaust, Drusa's
   `runtime/go/lazuli/webhooks/contract.go` typed errors
   (`ErrWebhookHmacInvalid`, `ErrWebhookIdempotent`,
   `ErrWebhookTenantUnscoped`) bubble back to the receiver but
   nothing in the IR captures "after N attempts, route to
   `<destination>`". Same hole the jobs cycle left explicitly open
   (`bucket-jobs-cycle.md:131-148`, "Sketch only … on_exhausted dlq")
   — webhooks have stronger evidence (real fixture authors a webhook
   feeding `process_import`, which retries 5×;
   `full-capsule.lzi:765`) and pilots will hit it sooner because
   external-provider re-delivery is the canonical reason DLQs exist.

The cycle's first deliverable is `webhook_event` as a **registry-side
catalog** (typed external envelope shape, provider-neutral); second
deliverable is `replay` + `dlq` children on the inbound `webhook`
kind. Both are **extensions of the shipped surface, not parallel
kinds** — the §1.11 row in `docs/roadmap.md:152-153` lists them as
decorators precisely because they decorate the existing primitive.

The audit's §13 entry (`docs/audit/framework-coverage-1400.md:227`)
classifies this trio as **DL** with no ambiguity: webhook event
registry, webhook replay, webhook DLQ. No N items here, no F gates
beyond pilot evidence for tuning.

## Baseline

Inventario do que existe hoje no caminho do construct. `Surface` é "lê
do fixture canônico"; `Grammar` é "parser reconhece"; `IR` é "struct
dedicado em `lazuli_ir`"; `Doctor/LSP` é "diagnostic cross-checa";
`Codegen` é "`lazuli_codegen_go` produz arquivo Go"; `Runtime` é "Drusa
executa".

| Construct | Surface | Grammar | IR | Doctor/LSP | Codegen | Runtime | L-level |
|---|---|---|---|---|---|---|---|
| `webhook <name>` (inbound) | yes (`full-capsule.lzi:775`) | line-based (`parser.rs:2159`) | yes (`ir:2122`) | `WEBHOOK-SCOPE-001` (`doctor.rs:2353`) + hover catalog | none | typed stub (`webhooks/contract.go:47`) | L1 |
| `verify hmac sha256` | yes (`:777`) | yes (`parser.rs:2266`) | yes (`VerifySpec`, `ir:2183-2189`) | `webhook_security` (LSP) | none | typed (`VerifySpec`, runtime:30) | L1 |
| `tenant_from payload.<axis>_id` | yes (`:780`) | yes | yes (`TenantFromSpec`) | 5 diagnostics | none | typed (runtime:40) | L1 |
| `idempotency by <path>` | yes (`:781`) | yes | yes (`IdempotencyKey`) | `idempotency_key_diagnostics` | none | typed (runtime:53) | L1 |
| `handler "./..." returns <Type>` | yes (`:782`) | yes | yes (`handler: PathRef`, `returns: TypeRef`) | none specific | none | typed (`HandlerFunc`, runtime:74) | L1 |
| `emits <event>` | yes (`:783`) | yes | yes (`emits: Vec<String>`) | event-name resolution | none | typed (runtime:57) | L1 |
| **`webhook_event <name>` (registry)** | **no** | no | no | no | no | no | **missing** |
| **`replay <window>` child on `webhook`** | no | no | no | no | no | no | **missing** |
| **`dlq <handler-or-emit>` child on `webhook`** | no | no | no | no | no | no | **missing** |
| `event_group <pattern>` (cross-check axis) | yes (`:173`) | yes | yes (`EventGroup`, `ir:2255`) | `EVENTGROUP-NESTING-001` + pattern-prefix rule | none | none | L1 |

**Summary**: Inbound webhook is L1-mature for verify/tenant/idempotency
but has zero declared shape for **what the external provider is
sending** and zero declared behavior for **what happens after retries
exhaust or after the same delivery comes back**. The three additions
below close that triangle without touching the shipped fields.

## Linguagem proposta

The trio is **closed and additive**. All three children land inside
the existing `Webhook` IR struct or as one new registry kind; no new
top-level kind, no new namespace, no new operator. The `webhook_event`
registry kind is the **only** new namespace surface.

### 1. `webhook_event <name>` (registry kind)

Registry-side catalog of expected inbound envelope shapes. Mirrors the
existing `record <name>` shape under `registry` but tagged as
"external-origin": Lazuli does not assume the source is trustworthy,
only that the contract is what the provider documents. Reuses the
existing record-field grammar verbatim, so adding it is one new
keyword + reuse of `parse_record_body`.

Surface (canonical):

```lzi
registry
  webhook_events
    crm_customer_upsert
      external_id: Text required
      org_id: ID required
      email: @semantic.Email @pii.contact optional
      payload_version: Text required
      received_at: Timestamp required

    stripe_invoice_paid
      id: Text required
      customer: Text required
      amount_paid: Money required
      currency: Text required
```

Usage at the inbound site (closed pointer, single occurrence):

```lzi
webhook crm_customer_upsert
  path "/webhooks/crm/customer-upsert"
  payload from webhook_events.crm_customer_upsert
  verify hmac sha256
    secret env.CRM_WEBHOOK_SECRET
    header "X-CRM-Signature"
  tenant_from payload.org_id
  idempotency by payload.org_id, payload.external_id
  handler "./integrations/upsert_customer_from_crm.go" returns Customer
  emits customer_webhook_received
```

Closed catalog for the registry kind: every field uses the same
catalog as authored records (typed primitives + `@semantic.*` +
`@pii.*`). No transport/method declaration — `webhook_event` is the
envelope shape, **not the route**. Routing stays on the inbound
`webhook` block.

Cost: small. Reuses `parse_record_body`; adds one keyword
(`webhook_events`) and one IR struct (`WebhookEvent`) + one field on
`RegistryManifest` (`webhook_events: Vec<WebhookEvent>`). Value:
makes external payload shape **statically referenceable** by
`tenant_from`, `idempotency by`, doctor cross-checks, and codegen.

### 2. `replay <window>` child on `webhook`

Declarative replay contract. Two surface forms, closed catalog of
windows + booleans:

```lzi
webhook stripe_invoice_paid
  ...
  replay
    allow within "24h"
    dedupe by id
```

Or short form when defaults are fine:

```lzi
webhook stripe_invoice_paid
  ...
  replay allow within "24h"
```

Closed catalog:

| Slot | Required | Type | Closed catalog |
|---|---|---|---|
| `allow` / `deny` | required when `replay` block authored | atom | exactly two values |
| `within "<duration>"` | required if `allow` | quoted duration | parsed by adapter |
| `dedupe by <path>` | optional, defaults to `idempotency` path | path expression | reuses idempotency-path grammar |

Cost: trivial. New `ReplaySpec` struct in IR
(`{ mode: ReplayMode, window: Option<Duration>, dedupe_by: Option<Path> }`).
Reuses duration-parsing and path-parsing already present.
Value: gives consumers a typed, doctor-checkable replay SLA the
adapter consumes verbatim. Also unblocks pilot work that today writes
"reject duplicate within 24h" inside the handler file.

### 3. `dlq <destination>` child on `webhook`

Dead-letter routing after retry exhaustion. **Closed three-variant
catalog**, matches the jobs sketch (`bucket-jobs-cycle.md:131-148`)
verbatim so the runtime team can share the implementation:

```lzi
webhook crm_customer_upsert
  ...
  retry 5 backoff exponential
  dlq emit crm_customer_upsert_dead_lettered
```

Or:

```lzi
webhook stripe_invoice_paid
  ...
  retry 8 backoff exponential
  dlq handler "./integrations/stripe_dlq.go"
```

Or:

```lzi
webhook generic_provider
  ...
  retry 3 backoff exponential
  dlq drop
    reason "Provider re-delivers; downstream tolerates loss."
```

Closed catalog (mutually exclusive, exactly one):

| Variant | Required children | Notes |
|---|---|---|
| `dlq emit <event>` | resolves to a declared `event.trace` or `event` in the feature | typed event-name resolution; doctor cross-checks against feature events |
| `dlq handler "./path.go"` | path required | adapter-side responsibility; doctor warns if path missing |
| `dlq drop` | `reason "..."` required | explicit waiver, mirrors `verify none reason "..."` pattern (invariants.md:399) |

Cost: small. New `DlqSpec` enum in IR. Reuses path-ref grammar,
event-name resolution, and the `reason "..."` pattern already used
for `verify none`. Value: closes the silent-drop gap — today retry
exhaustion is the adapter's problem.

### Cross-checks against `event_group`

The `event_group <pattern>` IR struct
(`crates/lazuli_ir/src/lib.rs:2255`) already binds concrete events to
their owning resource + pattern. Two new edges:

1. **`dlq emit <event>`** must resolve to an `event.trace` or `event`
   inside the same feature (or via `uses`), preferring `event.trace`
   for DLQ-style emissions. If the emitted event matches an
   `event_group <pattern>`, the doctor confirms the name is within
   the pattern prefix (reuses `event_group_pattern_prefix_diagnostics`,
   row 34).
2. **`payload from webhook_events.<name>`** is the **typed envelope**
   the inbound webhook will receive after verify. The `emits <event>`
   on the same webhook must declare an event whose payload is a
   **strict subset of** `webhook_events.<name>` fields + an optional
   handler-derived `output` (when `handler "./..." returns <Type>` is
   declared). Doctor rule: emitted-event payload fields must each
   exist in either the `webhook_events.<name>` envelope or the
   `returns <Type>` resource.

These cross-checks are the reason `webhook_event` is the registry
catalog instead of a per-feature record: the inbound payload contract
is **provider-side** and should live in `registry.lzi` alongside the
integration definition.

### Anti-proposals (rejected here)

- **`webhook_outbound` as a separate kind.** Outbound webhooks (us
  POSTing to a partner) are already covered by `calls
  <slot>.<operation>` in commands/jobs against an `integration`
  catalog entry. Adding `webhook_outbound` would duplicate that path.
  If real pilot pressure surfaces typed-payload + retry + DLQ for
  outbound, the work goes on `webhook_event` (provider expects this
  shape) + an integration operation, not on a new kind.
- **`webhook_event` carrying transport.** No `transport http`, no
  `method POST`, no `path "..."` on the registry kind. Those live on
  the inbound `webhook` block. The registry kind is the **shape**, not
  the route.
- **Per-channel DLQ (email/push/sms).** Notifications already have
  `channel <name>` (`Notification` IR `:2229`) but no DLQ. **Defer**
  — the runtime adapters handle SMTP/push retry today; a typed DLQ
  decorator on notifications waits for pilot pressure.
- **`webhook_replay` / `webhook_dlq` as standalone kinds.** They are
  decorators inside `webhook`, not separate kinds. The roadmap row
  (`docs/roadmap.md:153`) wording suggested they might be kinds; the
  fixture and the existing IR shape both demand they decorate the
  inbound `webhook` block.
- **Provider names anywhere.** No `stripe`, `mercadopago`, `github`
  keywords in the language. Provider envelopes go into
  `registry.webhook_events.<name>` as opaque shape; provider routing
  through `@adapter.<name>`.

## IR proposto

All additions are additive on the shipped IR. No field renames; no
schema-breaking changes; no on-disk JSON consumer touches these
fields today.

### 1. `WebhookEvent` registry kind

New struct in `crates/lazuli_ir/src/lib.rs` next to existing typed
records:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub name: String,
    pub fields: Vec<RecordField>, // reuse existing RecordField shape
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}
```

New field on `RegistryManifest`:

```rust
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub webhook_events: Vec<WebhookEvent>,
```

### 2. `Webhook` struct extension

Three additive fields:

```rust
pub struct Webhook {
    // ... existing fields preserved ...

    /// `payload from webhook_events.<name>` — typed envelope reference
    /// resolved against `registry.webhook_events`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_from: Option<String>,

    /// `replay` child — declarative replay contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay: Option<ReplaySpec>,

    /// `dlq <variant>` child — declarative dead-letter routing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dlq: Option<DlqSpec>,

    /// Optional retry policy on inbound webhooks. Today implicit at
    /// adapter level; promoting to IR mirrors the jobs `RetryPolicy`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
}
```

### 3. `ReplaySpec`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplaySpec {
    pub mode: ReplayMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within: Option<String>, // quoted duration verbatim
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedupe_by: Option<Path>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayMode { Allow, Deny }
```

### 4. `DlqSpec`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum DlqSpec {
    Emit { event: String },
    Handler { path: PathRef },
    Drop { reason: String },
}
```

Mutually-exclusive shape baked into the discriminator; the parser
returns `ParseError` if more than one child is present.

### JSON shape (`lazuli inspect --format=json`)

Per `InspectFeature.webhooks[]`:

```json
{
  "name": "crm_customer_upsert",
  "route": "/webhooks/crm/customer-upsert",
  "payload_from": "webhook_events.crm_customer_upsert",
  "verify": {"scheme": "hmac", "algorithm": "sha256", "secret_env": "CRM_WEBHOOK_SECRET", "header": "X-CRM-Signature"},
  "tenant_from": "payload.org_id",
  "idempotency": "by payload.org_id, payload.external_id",
  "retry": {"count": 5, "backoff": "exponential"},
  "replay": {"mode": "allow", "within": "24h", "dedupe_by": null},
  "dlq": {"kind": "emit", "event": "crm_customer_upsert_dead_lettered"},
  "handler": "./integrations/upsert_customer_from_crm.go",
  "returns": "Customer",
  "emits": ["customer_webhook_received"],
  "origin": "webhook"
}
```

Per `InspectRegistry`:

```json
{
  "webhook_events": [
    {
      "name": "crm_customer_upsert",
      "fields": [
        {"name": "external_id", "type": "Text", "required": true},
        {"name": "org_id", "type": "ID", "required": true},
        {"name": "email", "type": "Text", "capabilities": ["@semantic.Email", "@pii.contact"], "required": false},
        {"name": "payload_version", "type": "Text", "required": true},
        {"name": "received_at", "type": "Timestamp", "required": true}
      ]
    }
  ]
}
```

## Codegen proposto

`lazuli_codegen_go` extends the existing `dist/go/<feature>/webhooks.gen.go`
(designed in `bucket-jobs-cycle.md` §"File 2") with the new fields. No
new files.

```go
// path: dist/go/customer_import/webhooks.gen.go
// Code generated by lazuli; DO NOT EDIT.
package customer_import

import (
    "lazuli.dev/runtime/lazuli/webhooks"
)

// CrmCustomerUpsertPayload is generated from
// `registry.webhook_events.crm_customer_upsert`.
type CrmCustomerUpsertPayload struct {
    ExternalID     string             `json:"external_id"`
    OrgID          string             `json:"org_id"`
    Email          string             `json:"email,omitempty"`
    PayloadVersion string             `json:"payload_version"`
    ReceivedAt     time.Time          `json:"received_at"`
}

func RegisterWebhooks(r *webhooks.Registry) {
    r.Register(webhooks.Spec{
        Name:        "customer_import.crm_customer_upsert",
        Path:        "/webhooks/crm/customer-upsert",
        Verify:      webhooks.HmacSha256{SecretEnv: "CRM_WEBHOOK_SECRET", Header: "X-CRM-Signature"},
        TenantFrom:  "payload.org_id",
        Idempotency: "payload.org_id, payload.external_id",
        Retry:       webhooks.Retry{Count: 5, Backoff: webhooks.Exponential},
        Replay:      webhooks.ReplaySpec{Mode: webhooks.ReplayAllow, Within: "24h"},
        DLQ:         webhooks.DlqEmit("crm_customer_upsert_dead_lettered"),
        PayloadType: webhooks.TypeOf[CrmCustomerUpsertPayload](),
        Handler:     upsertCustomerFromCRMHandler, // author-supplied
        Emits:       []string{"customer_webhook_received"},
    })
}
```

No provider names. `webhooks.HmacSha256`, `webhooks.ReplaySpec`,
`webhooks.DlqEmit` live in `runtime/go/lazuli/webhooks/` — the
Drusa-side typed contract.

## Runtime proposto

Drusa extends the existing `runtime/go/lazuli/webhooks/` package — no
new subpackage. Three additions, all additive to the shipped
`WebhookContract`:

```go
// runtime/go/lazuli/webhooks/contract.go (extended)

type Retry struct {
    Count   int
    Backoff Backoff // Fixed | Exponential
}

type ReplayMode int
const (
    ReplayDeny ReplayMode = iota
    ReplayAllow
)

type ReplaySpec struct {
    Mode     ReplayMode
    Within   time.Duration
    DedupeBy string // path expr
}

type DlqSpec struct {
    Kind        DlqKind // Emit | Handler | Drop
    EmitEvent   string
    HandlerPath string
    DropReason  string
}

// PayloadType is the generic typed envelope reference — codegen
// instantiates from the registry.webhook_events catalog and threads
// it through verify+decode before invoking the handler.
type PayloadType interface { isPayloadType() }

// New fields on WebhookContract (extending the existing struct):
type WebhookContract struct {
    // ... existing fields preserved ...
    Retry       *Retry
    Replay      *ReplaySpec
    DLQ         *DlqSpec
    PayloadType PayloadType
}

// New typed errors:
var (
    ErrWebhookReplayWindowExpired = errors.New("webhooks: replay window expired")
    ErrWebhookReplayDenied        = errors.New("webhooks: replay denied for this contract")
    ErrWebhookDeadLettered        = errors.New("webhooks: routed to DLQ after retry exhaustion")
)
```

Lifecycle:

- **Verify** (existing): HMAC check via `crypto/subtle.ConstantTimeCompare`.
- **Decode** (new): if `PayloadType != nil`, decode body into typed
  struct. Failures route to the same DLQ path as handler failures.
- **Idempotency dedupe** (existing for idempotency_key; extended for
  `replay.dedupe_by` when present).
- **Replay window check** (new): if `replay.mode == deny` or the
  envelope's `received_at`/`idempotency_key` falls outside the
  window, return `ErrWebhookReplayDenied` /
  `ErrWebhookReplayWindowExpired`.
- **Retry** (new): on handler error, requeue with backoff for `retry.count`
  attempts. The retry path runs through the **same `jobs.Adapter`** the
  jobs cycle proposes (`bucket-jobs-cycle.md:454-472`) — webhooks are
  jobs at the runtime level once verify passes. No new adapter
  contract.
- **DLQ** (new): after exhaustion, route per `DlqSpec`:
  - `Emit`: publish event via `lazuli.Publish`.
  - `Handler`: invoke author-supplied `func(ctx, envelope, error)`.
  - `Drop`: `slog.Info` with `dlq_drop_reason` field, no further action.

### No leaks

- No Stripe / GitHub / MercadoPago / River / Asynq names in any
  Lazuli source or generated Go.
- DLQ routing through `jobs.Adapter` keeps the queue-backend choice in
  registry bindings, not in source.
- Provider envelopes are catalog entries (`registry.webhook_events`),
  not keywords.

## Evals/Testes propostos

Golden inspect projection covers the three new shapes:

```bash
cargo run -q -p lazuli_cli -- inspect examples/full-capsule/full-capsule.lzi \
  --format=json --expand=webhooks > /tmp/got.json
diff /tmp/got.json crates/lazuli_cli/tests/fixtures/full-capsule-webhooks-expanded.golden.json
```

Go test (`runtime/go/lazuli/webhooks/replay_dlq_test.go`):

```go
func TestWebhookReplay_AllowWithin24h(t *testing.T) {
    synctest.Run(func() {
        ctx, db := testCtx(t)
        spec := webhooks.WebhookContract{
            // ... shipped fields ...
            Replay: &webhooks.ReplaySpec{
                Mode: webhooks.ReplayAllow, Within: 24 * time.Hour,
            },
        }
        // 1) First delivery -> handler runs once
        // 2) Re-delivery same idempotency_key within 24h -> 200, no handler
        // 3) Re-delivery same idempotency_key after 25h -> ErrWebhookReplayWindowExpired
    })
}

func TestWebhookDlq_EmitsDeadLetteredEvent(t *testing.T) {
    synctest.Run(func() {
        // Handler always returns error; retry.count=2 exhausts;
        // expect `crm_customer_upsert_dead_lettered` event published.
    })
}
```

Doctor test (`crates/lazuli_cli/src/doctor.rs:test`):

```rust
#[test]
fn dlq_emit_must_resolve_to_declared_event() {
    let source = "
feature customer_import
  webhook crm_customer_upsert
    path \"/webhooks/crm/customer-upsert\"
    verify hmac sha256
      secret env.CRM_WEBHOOK_SECRET
      header \"X-CRM-Signature\"
    tenant_from payload.org_id
    idempotency by payload.org_id, payload.external_id
    handler \"./integrations/upsert_customer_from_crm.go\"
    dlq emit not_declared_anywhere
";
    let diags = run_doctor(source);
    assert!(diags.iter().any(|d| d.code == "WEBHOOK-DLQ-001"));
}

#[test]
fn payload_from_must_resolve_to_registry_webhook_event() {
    let source = "
registry
  webhook_events
    crm_customer_upsert
      external_id: Text required

feature customer_import
  webhook crm_customer_upsert
    path \"/webhooks/crm/customer-upsert\"
    payload from webhook_events.unknown_envelope
    ...
";
    let diags = run_doctor(source);
    assert!(diags.iter().any(|d| d.code == "WEBHOOK-PAYLOAD-001"));
}
```

Four new doctor tests, one per IR-promoted diagnostic.

## Doctor/LSP propostos

| Code | Severity | Message | Trigger |
|---|---|---|---|
| `WEBHOOK-PAYLOAD-001` | Error | `Webhook '<n>' references \`webhook_events.<X>\` but no such envelope is declared in \`registry.webhook_events\`.` | `payload from webhook_events.<X>` resolution fails. |
| `WEBHOOK-PAYLOAD-002` | Warning | `Webhook '<n>' uses \`tenant_from payload.<axis>_id\` but \`webhook_events.<X>\` declares no \`<axis>_id\` field; the runtime will fail at decode time.` | `tenant_from` path not in declared envelope. |
| `WEBHOOK-REPLAY-001` | Error | `Webhook '<n>' declares \`replay allow\` but no \`within "<duration>"\` window. The adapter has no SLA to enforce.` | `replay allow` without `within`. |
| `WEBHOOK-REPLAY-002` | Warning | `Webhook '<n>' declares \`replay\` but no \`idempotency by ...\` — replay dedupe has no key.` | `replay` block present, `idempotency` missing. |
| `WEBHOOK-DLQ-001` | Error | `Webhook '<n>' \`dlq emit <X>\` references event '<X>' that is not declared in the feature or any \`uses\` import.` | `dlq emit <event>` resolution fails. |
| `WEBHOOK-DLQ-002` | Error | `Webhook '<n>' \`dlq drop\` requires \`reason "..."\`. Silent drops on dead-letter must be explicit waivers.` | `dlq drop` without `reason`. |
| `WEBHOOK-DLQ-003` | Warning | `Webhook '<n>' declares \`retry <N>\` but no \`dlq\`. After exhaustion the runtime falls back to adapter default (silent drop on River).` | `retry` present without `dlq`. |
| `WEBHOOK-EVENT-001` | Error | `\`webhook_events.<X>\` declared in registry but referenced by no \`webhook ... payload from\` declaration.` | Dead-letter registry entry. |

Eight new diagnostics, all IR-driven (no text-pattern bridge).

LSP hover catalog additions:

- `webhook_events` (registry-level) — links to invariants entry.
- `payload from webhook_events.<X>` — hover shows the resolved
  envelope schema with `@semantic`/`@pii` annotations.
- `replay`, `allow`, `deny`, `within`, `dedupe by` — closed catalog
  completion.
- `dlq`, `emit`, `handler`, `drop`, `reason` — closed catalog
  completion; on `dlq emit <X>` cursor, completion suggests events
  declared in scope.
- `retry` (on webhook) — already in hover catalog for jobs;
  parametrize for webhook context.

No new `@<namespace>` additions. `webhook_events.<X>` is a registry
catalog path, not an `@`-prefixed reference.

## Critério de "ciclo fechado"

The bucket cycle closes when **every** box is checked for at least
one inbound webhook + one `webhook_event` registry entry from the
fixture.

- [ ] Authored in `examples/full-capsule/` — registry `webhook_events
      crm_customer_upsert` + extended `webhook crm_customer_upsert` with
      `payload from`, `replay allow within "24h"`, `retry 5 backoff
      exponential`, `dlq emit crm_customer_upsert_dead_lettered`.
- [ ] `lazuli check` accepts the new syntax.
- [ ] `lazuli inspect --format=json --expand=webhooks` shows the new
      fields; `--expand=registry` shows the `webhook_events` catalog.
- [ ] `lazuli doctor` runs all eight new diagnostics on the fixture
      and on negative-case fixtures.
- [ ] `lazuli generate` emits `dist/go/<feature>/webhooks.gen.go` with
      typed `PayloadType`, `Retry`, `Replay`, `DLQ`.
- [ ] Drusa executes one inbound delivery + one replay-within-window
      + one retry-to-DLQ end-to-end against a Postgres + River +
      stub-HMAC test rig.
- [ ] At least two `synctest`-backed Go tests
      (`TestWebhookReplay_*`, `TestWebhookDlq_*`).
- [ ] LSP serves hover on `payload from webhook_events.<X>` resolving
      to the registry envelope; completion on `dlq emit ` suggests
      declared events.

## Próximo passo

1. Land the IR + parser changes (`WebhookEvent` registry kind +
   four new fields on `Webhook` + `ReplaySpec` + `DlqSpec`). Reuse
   `parse_record_body` for the registry kind; reuse path/duration
   grammar for the children.
2. Land the eight new doctor diagnostics from IR. None of these need
   text-pattern facts — IR is already lifted by Tier 3.
3. Update `examples/full-capsule/registry.lzi` with
   `webhook_events crm_customer_upsert` and extend
   `full-capsule.lzi:775-783` with the new children. The fixture
   becomes the cycle's primary test surface.
4. Hand off codegen + Drusa to the runtime team with the extended
   `runtime/go/lazuli/webhooks/contract.go` shape. The agent in this
   profile stays in language/IR/doctor territory.
5. Close the cycle on `crm_customer_upsert` first (already authored,
   carries `payload.org_id`/`payload.external_id` references that
   demand a typed envelope). Then `stripe_invoice_paid` once a pilot
   surfaces.

Cycle-close evidence: the
`bucket-webhooks-expanded-cycle.golden.json` inspect file plus a
green `cargo test -q -p lazuli_runtime --features=integration` run.

## Rows sugeridas para `docs/next-checklist.md`

| Order | Cut | Status | Notes |
|-------|-----|--------|-------|
| 38 | Webhooks expanded — `webhook_event` registry kind | proposed | New registry-side catalog (`registry.webhook_events.<name>`) carrying typed envelope shape (reuses `RecordField` grammar). One new IR struct `WebhookEvent`, one new field `RegistryManifest.webhook_events`. Parser reuses `parse_record_body`. Two new doctor diagnostics (`WEBHOOK-PAYLOAD-001/002`, `WEBHOOK-EVENT-001`). See `docs/proposals/bucket-webhooks-expanded-cycle.md`. |
| 39 | Webhooks expanded — `replay` + `dlq` decorators on `webhook` | proposed | Four additive fields on existing `Webhook` IR struct (`payload_from`, `replay`, `dlq`, `retry`). Two new IR structs (`ReplaySpec`, `DlqSpec`). Five new doctor diagnostics (`WEBHOOK-REPLAY-001/002`, `WEBHOOK-DLQ-001/002/003`). Drusa runtime extension under existing `runtime/go/lazuli/webhooks/` (no new subpackage). DLQ retry path runs through the jobs adapter — no new queue contract. See `docs/proposals/bucket-webhooks-expanded-cycle.md`. |
