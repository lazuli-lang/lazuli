# Bucket Cycle: Notifications expandidas (L0→L2)

**Run**: `/lazuli-bucket-cycle bucket=notifications-expanded mode=design`
**Date**: 2026-05-11
**Pre-requisite**: none. `notification` kind shipped via Phase L Tier 3
(row 33, commits `e89ff27` → `53a5d1a`). Parser, IR struct, doctor
(`NOTIF-CHANNEL-001`), inspect projection, and Lazuli Go stubs already
exist. This cycle is **additive** — four decorators on top of the
shipped contract — and does not require a scope-out.

## Summary (pt-BR)

`notification` foi para L1 em Phase L Tier 3: parser canonical-indent
lifteia o bloco (`crates/lazuli_syntax/src/parser.rs:2333`), IR carrega
shape tipado (`crates/lazuli_ir/src/lib.rs:2217`), doctor faz catálogo
fechado de canais (`crates/lazuli_cli/src/doctor.rs:2383`), inspect
projeta via `--expand=notifications`, e o runtime Lazuli Go entrega o stub
(`runtime/go/lazuli/notifications/contract.go:49`). Roadmap §1.17
deixou quatro decorators marcados **SPECULATIVE**:

- `digest` — agrega N notificações em uma só por janela de tempo.
- `throttle` — limita taxa por destinatário (anti-spam).
- `delivery_receipt` — confirma entrega do canal (SMTP 250, FCM token
  ack, Slack 200).
- `read_receipt` — confirma leitura via tracking pixel / device callback /
  in-app marker.

Os quatro foram listados juntos no audit §19
(`docs/audit/framework-coverage-1400.md:276`) mas o fixture só usa
`channel email` (`full-capsule.lzi:820`) e `channel email, in_app`
(`full-capsule.lzi:831`). Os outros canais (`sms`, `push`, `slack`,
`discord`, `webhook`) estão no catálogo mas sem pressão de pilot —
classificados aqui como deferred.

**Recomendação**: promover `digest` e `throttle` como decorators
declarativos (L1 contract + doctor + LSP + Lazuli Go stub field).
`delivery_receipt` e `read_receipt` ficam **SPECULATIVE/deferred** —
ambos exigem mecânica de callback que vaza para adapter (webhook
inbound + token rotation) sem benefício de shape declarativo. Promover
sem pilot reproduz o erro do `passkeys` em `auth-lowering-scope.md`.

## Contexto

The `notification` kind closed pilot bucket #3 alongside `job` and
`webhook`. Cycle-close evidence in `docs/roadmap.md:25` confirms
L1 maturity:

- Surface authored: `examples/full-capsule/full-capsule.lzi:819-839`
  (two notifications: `welcome_email` channel email, `archive_survey`
  channel email,in_app).
- Parser: `parse_notification` at `crates/lazuli_syntax/src/parser.rs:2333`.
- IR struct: `Notification` at `crates/lazuli_ir/src/lib.rs:2217`.
- Inspect: `--expand=notifications` shipped row 32.
- Doctor: `NOTIF-CHANNEL-001` at `crates/lazuli_cli/src/doctor.rs:2402`
  + closed catalog at `:2268`.
- LSP: notification kind hover and child-keyword catalog
  (`crates/lazuli_lsp/src/lib.rs`, ~row 33).
- Lazuli Go stub: `runtime/go/lazuli/notifications/contract.go:49`
  + `ChannelDispatcher` interface at `:87`.

What is missing is the **second-wave decorators** that audit §19
(`docs/audit/framework-coverage-1400.md:276`) and roadmap §1.17
(`docs/roadmap.md:190-192`) list as DL:

| Decorator | Status today | Audit anchor |
|---|---|---|
| `digest` | not parsed | §19 line 276 + roadmap line 190 |
| `throttle` | not parsed | §19 line 276 + roadmap line 191 |
| `delivery_receipt` | not parsed | §19 line 276 + roadmap line 192 |
| `read_receipt` | not parsed | §19 line 276 + roadmap line 192 |

The four were flagged SPECULATIVE in roadmap §1.17 — meaning "second
wave, post-bucket-cycle". The §0 pilot cycle is closed language-side
(commit `b3fc39e` per `next-checklist.md:60-73`), so this is the
expected entry point.

This proposal grades each decorator against three gates:

1. **Fixture pressure** — does any pattern in `full-capsule` benefit?
2. **Shape boundary** — is the decorator declarative (closed catalog,
   one canonical form) or does it leak mechanics into the language?
3. **Runtime fit** — does the runtime contract for the decorator stay
   typed (Lazuli describes intent, the Lazuli Go runtime owns dispatch)
   or pull provider mechanics into Lazuli?

## Baseline

Inventory of the four candidate decorators against the L0/L1/L2 grid.
`Surface` is "would author in the fixture today"; the rest mirror
`bucket-jobs-cycle.md:48-87`.

| Decorator | Surface | Grammar | IR | Doctor/LSP | Codegen | Runtime | L-level |
|---|---|---|---|---|---|---|---|
| `digest` (window + group_by) | candidate (no fixture use yet) | none | none | none | none | none | L-1 (pre-L0) |
| `throttle` (per-recipient rate-limit) | candidate (no fixture use yet) | none | none | none | none | none | L-1 (pre-L0) |
| `delivery_receipt` | candidate | none | none | none | none | none | L-1 (pre-L0) |
| `read_receipt` | candidate | none | none | none | none | none | L-1 (pre-L0) |

**Summary**: All four are below L0. The fixture exercises **none** of
them. Promoting any of them requires either (a) extending the fixture
with a new notification that authors the decorator, or (b) deferring
until a pilot exercises it.

For comparison, the existing `notification` decorators that **are**
L1 today and serve as the reference shape for new ones:

| Existing decorator | Shape | Site |
|---|---|---|
| `channel <list>` | closed catalog (`email|in_app|sms|push|slack|discord|webhook`) | `parser.rs:2379`, `ir:2222`, `doctor.rs:2268` |
| `recipient <path>` | path verbatim | `parser.rs:2383`, `ir:2228` |
| `trigger event|schedule <ref>` | discriminated | `parser.rs:2387`, `ir:2220` |
| `tenant_from <path>` | typed via `TenantFromSpec` | `parser.rs:2391`, `ir:2234` |
| `idempotency by <path>` | typed via `IdempotencyKey` | `parser.rs:2395`, `ir:2236` |
| `retry <N> backoff <strategy>` | closed catalog backoff | `parser.rs:2399`, `ir:2238` |
| `template "<path>"` | path string | `parser.rs:2403`, `ir:2230` |
| `policy @policy.<n>` | `PolicyRef` | `parser.rs:2407`, `ir:2232` |
| `emits <event>` | event ref | `parser.rs:2411`, `ir:2240` |

Every existing child is a **single line with a closed shape**. New
decorators must clear the same bar.

## Linguagem proposta

Four candidates evaluated. Two recommended for promotion as L1
contracts; two deferred until pilot pressure surfaces.

### 1. `digest` — RECOMMEND for promotion

**Surface**:

```
notification daily_activity_summary
  channel email
  recipient target.email
  trigger event customer.customer_updated
  digest
    window "24h"
    group_by target.email
  template "./outreach/daily_activity.mjml"
  policy @policy.notify
```

**Why declarative**:

- The contract is **"batch N triggers into 1 dispatch per window per
  group"** — a shape-level statement, not transport mechanics.
- The window + group_by pair is the canonical form everywhere
  (Mailchimp, Sendgrid digest, Slack daily summary, PagerDuty alert
  grouping). One way to say it.
- Doctor can cross-check `group_by <path>` against the trigger event
  payload schema — same machinery as `tenant_from`.
- Closed catalog for child keys: only `window <duration>` and
  `group_by <path>` are accepted. No nested predicate language.

**Shape boundary**:

- Window is a string duration following the same parser as
  `auth sessions ttl "7 days"` (`crates/lazuli_syntax/src/parser.rs:1820`).
  Reuses `DurationSpec` (or its equivalent) — does not introduce a new
  duration grammar.
- `group_by` is a payload path verbatim, like `tenant_from`. No
  expression language.

**Runtime fit**:

- Runtime-side adds `DigestSpec { Window time.Duration; GroupBy string }`
  to `runtime/go/lazuli/notifications/contract.go:49`'s
  `NotificationContract`. The dispatcher loop holds an in-process
  bucket (or Postgres/Redis bucket per adapter) keyed by `(notification,
  group_by)` and flushes on window expiry. No new adapter contract —
  `ChannelDispatcher` stays unchanged because digest is a layer
  **before** dispatch.

**Anti-proposal**: `digest by`, `digest every`, `digest within` — pick
one syntactic shape. **Recommendation**: `digest` block with named
children `window`/`group_by`, matching the style of `auth password`
and `app.tracing` (both are sub-blocks with named child lines).

**Cost**: small. One parser branch + one IR field + one doctor rule
(group_by must resolve in the trigger payload) + one LSP hover.

**Value**: medium-high. Every B2B SaaS that emits per-user activity
events benefits. The fixture can be extended with a third notification
exercising digest (e.g., `daily_admin_digest` over
`customer_status_changed` events).

### 2. `throttle` — RECOMMEND for promotion

**Surface**:

```
notification password_reset_email
  channel email
  recipient target.email
  trigger event customer.password_reset_requested
  throttle
    per recipient
    max 3
    window "1h"
  template "./outreach/password_reset.mjml"
  policy @policy.notify
```

**Why declarative**:

- The contract is **"reject dispatch if recipient has received >=N
  notifications of this kind in the last <window>"** — a shape-level
  rate-limit on the notification itself.
- `rate_limit` already lives in the language for `agent`
  (`parser.rs:1312`), `auth password` (`parser.rs:1574`), and
  `expose http` (`parser.rs:2609`). The invariants doc
  (`docs/invariants.md:178`) already lists `rate_limit` as an
  optional child of `notification` — **the contract was promised
  but never implemented**. Promoting it now closes the gap between
  invariant and parser.
- **Naming pivot**: invariants say `rate_limit`; this proposal
  recommends **`throttle`** as the notification-side keyword because
  the semantics differ from `agent.rate_limit` (which is per-agent-
  invocation across all callers) and `expose http.rate_limit` (which
  is per-route HTTP rate). Notification throttle is **per-recipient
  per-notification-kind**, which is a tighter contract. Update
  `docs/invariants.md:178` to read `throttle` instead of `rate_limit`.

**Shape boundary**:

- Three required children: `per <axis>` (closed catalog:
  `recipient | tenant | global`), `max <N>` (uint), `window
  <duration>` (string duration).
- Cross-check: `per recipient` requires `recipient <path>` to resolve
  (same path used as the throttle key).

**Runtime fit**:

- `runtime/go/lazuli/notifications/contract.go` adds
  `ThrottleSpec { Per Axis; Max uint32; Window time.Duration }`. The
  dispatcher checks the bucket against the typed spec before dispatch.
  No new adapter interface — throttle lives in the dispatcher core.

**Anti-proposal**: `throttle 3/hour per recipient` as a one-liner.
Rejected: the one-liner conflates three independent axes
(`max`/`window`/`per`) into a single string that the parser would
have to split, which is brittle. The block form mirrors `digest` and
`retry`-with-backoff and keeps one canonical shape.

**Cost**: small. One parser branch + one IR field + one doctor rule
(`per recipient` requires `recipient` declaration) + one LSP hover.

**Value**: high. Password-reset, magic-link, OTP, and verification-code
notifications **all** need per-recipient throttle as a security control.
Even without a pilot, the fixture should grow a `password_reset_email`
that exercises this (the auth bucket has `auth password` but no
notification-side reset flow today).

### 3. `delivery_receipt` — DEFER (SPECULATIVE)

**Surface (sketch only)**:

```
notification welcome_email
  channel email
  delivery_receipt
    callback @adapter.notification.email.delivery
    timeout "24h"
    on_failed_emit welcome_email_undeliverable
```

**Why defer**:

- The contract is **"the adapter reports back whether the channel
  accepted/rejected/bounced the message"**. The shape inevitably
  pulls in:
  - A **callback URL** that has to round-trip through an inbound
    webhook (Sendgrid Event Webhook, Twilio status callback, FCM
    onMessageStatusChange).
  - A **retry/expiry policy** for the callback itself.
  - **Provider-specific status codes** (SMTP 250 vs 552 vs 5.5.0; FCM
    `NOT_REGISTERED` vs `UNAVAILABLE`; SNS `DeliveryFailure` vs
    `Complaint`) that don't map to a single closed catalog without
    inventing one.

- The closed-catalog requirement breaks: every adapter has a different
  set of delivery outcomes. Lazuli would either (a) leak the union of
  all adapter codes into core syntax (boundary violation), or (b)
  ship a degraded `delivered|failed|bounced|complained` shadow catalog
  that loses fidelity.

- There's a clean alternative without a new primitive: declare the
  status callback as a **`webhook`** (already L1) that emits a domain
  event the notification consumes. This is more verbose but stays
  inside existing primitives:

```
webhook email_delivery_callback
  path "/webhooks/email-delivery"
  verify hmac sha256 ...
  tenant_from payload.tenant_id
  emits email_delivery_status_changed

notification welcome_email
  ...
  emits welcome_email_sent
```

- The downstream consumer subscribes to `email_delivery_status_changed`
  via a `job`. The contract is fully declarative without a new
  decorator.

**Pilot gate**: defer until a fixture-grade pilot needs **structured
delivery state** as a first-class contract on notification (not as a
generic webhook → event flow). Likely candidates: regulated comms
(HIPAA delivery audit), transactional billing receipts where
delivery proof is a legal requirement. Neither is in the canonical
fixture.

**Marking**: SPECULATIVE in `docs/roadmap.md:192`. Do not promote in
this cycle.

### 4. `read_receipt` — DEFER (SPECULATIVE)

**Why defer**:

- Even worse boundary problem than `delivery_receipt`. Read-receipt
  mechanics differ wildly:
  - **Email**: tracking pixel + UTM beacon. Privacy-regulated in GDPR
    jurisdictions; some providers (Apple Mail Privacy Protection)
    actively fake reads. The contract is fundamentally unreliable.
  - **Push**: requires SDK callback from the device. APNs has no
    server-side read receipt; FCM has `notificationOpened` but only
    when the device app implements it.
  - **In-app**: requires the in-app UI to call back the server. This
    is a **product feature**, not a notification contract — it lives
    in the resource that owns the in-app inbox.
  - **SMS**: read receipts don't exist as a protocol.

- Across the seven channels in the catalog, read receipt has zero
  unified semantics. Promoting it forces Lazuli to either invent a
  fictional unified contract or expose per-channel polysemy
  (`read_receipt` means "pixel" in email, "callback" in push, "UI
  ack" in in-app) — that's a vocabulary disaster.

- The clean alternative is feature-level: declare an in-app event
  `notification_read` that the UI emits, and let the consumer
  `notification` declaration subscribe to it via the existing
  `trigger event` primitive. Nothing new to add to the language.

**Pilot gate**: defer indefinitely unless a pilot demonstrates that
the **product** treats read-receipt as a domain concept (e.g.,
marketing-tool dashboards with open rates) **and** the contract can
be expressed without per-channel polysemy. Until then, the existing
event-graph machinery covers the use case.

**Marking**: SPECULATIVE in `docs/roadmap.md:192`. Do not promote in
this cycle.

### Anti-proposals (rejected here)

- **`localization` / `i18n` on notification template**. Listed in audit
  §19 as DL. Not in this cycle: localization is a cross-cutting concern
  (resources, errors, view labels, notifications) and should be designed
  as a single language primitive in its own bucket, not bolted onto
  notification.
- **`preferences` block on notification kind**. Listed in audit §19.
  Notification preferences are a **resource shape** (user opts in/out
  per channel per category), not a notification-kind concern. Today
  authors use a `resource NotificationPreference` with capability fields
  — no new kind needed.
- **`template per channel`** (multiple templates indexed by channel).
  Not in this cycle: today's `template "./path"` plus a per-channel
  templating convention inside the adapter is sufficient. Promoting
  to per-channel template shape requires evidence that authors hit
  the limitation; not in the fixture.
- **`bulk` decorator (batch-of-recipients)**. Listed in audit §19 as DF.
  Bulk dispatch is a runtime/adapter concern (the queue lane handles
  fanout). No language primitive needed.
- **Provider keywords**. `sendgrid`, `mailgun`, `ses`, `twilio`, `fcm`,
  `apn`, `slack`, `discord` never appear in core syntax. Channel adapters
  resolve through `@adapter.notification.<channel>` and registry bindings.

## IR proposto

Two additive extensions to `Notification` (`crates/lazuli_ir/src/lib.rs:2217`).
No struct splits, no breaking changes.

### `DigestSpec` (new struct)

```rust
/// Notification-level digest contract. Aggregates triggers into a
/// single dispatch per window per group_by key. `group_by` is a path
/// against the trigger event payload, resolved cross-feature via the
/// same machinery as `tenant_from`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DigestSpec {
    /// `window "24h"` — closed-catalog duration string.
    pub window: String,
    /// `group_by target.email` — path against trigger event payload.
    pub group_by: String,
}
```

### `ThrottleSpec` (new struct)

```rust
/// Notification-level throttle contract. Rejects dispatch if the
/// `per` axis has received >= `max` notifications of this kind in
/// the last `window`. Distinct from `agent.rate_limit` and
/// `expose http.rate_limit` because the throttle key is the
/// notification's `recipient` path, not the caller or route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThrottleSpec {
    /// `per recipient | tenant | global` — closed catalog.
    pub per: ThrottleAxis,
    /// `max <N>` — bucket capacity.
    pub max: u32,
    /// `window "<duration>"` — closed-catalog duration string.
    pub window: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThrottleAxis {
    Recipient,
    Tenant,
    Global,
}
```

### `Notification` struct extension

Add two fields at `crates/lazuli_ir/src/lib.rs:2245` (after `span_ref`):

```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<DigestSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throttle: Option<ThrottleSpec>,
```

Additive — every field defaults to `None`, every existing consumer
keeps working. The `--expand=notifications` projection at
`crates/lazuli_cli/src/main.rs` automatically picks up the new fields
through serde because the IR struct is the projection.

### Diagnostics added on top

Six new IR-driven doctor rules attached to `tier3_notification_diagnostics`
at `crates/lazuli_cli/src/doctor.rs:2383`:

| Code | Severity | Trigger |
|---|---|---|
| `NOTIF-DIGEST-001` | Error | `digest` block missing `window` child. |
| `NOTIF-DIGEST-002` | Error | `digest` block missing `group_by` child. |
| `NOTIF-DIGEST-003` | Error | `digest group_by <path>` references a path not present in the trigger event payload. Cross-feature via `Tier3FeatureFacts`. |
| `NOTIF-THROTTLE-001` | Error | `throttle` block missing one or more of `per`, `max`, `window`. |
| `NOTIF-THROTTLE-002` | Error | `throttle per <axis>` with `<axis>` outside the closed catalog (`recipient | tenant | global`). |
| `NOTIF-THROTTLE-003` | Error | `throttle per recipient` declared but the notification has no `recipient <path>` child (would be unreachable). |

All six fit the existing `Tier3FeatureFacts` infrastructure
(`crates/lazuli_cli/src/doctor.rs:83-89`); no new fact family needed.

### JSON shape (`lazuli inspect --format=json --expand=notifications`)

```json
{
  "name": "customer",
  "notifications": [
    {
      "name": "daily_activity_summary",
      "channels": ["email"],
      "recipient": "target.email",
      "trigger": "event customer.customer_updated",
      "tenant_from": "payload.org_id",
      "template": "./outreach/daily_activity.mjml",
      "policy": "@policy.notify",
      "digest": {
        "window": "24h",
        "group_by": "target.email"
      },
      "throttle": null,
      "origin": "notification"
    },
    {
      "name": "password_reset_email",
      "channels": ["email"],
      "recipient": "target.email",
      "trigger": "event customer.password_reset_requested",
      "tenant_from": "payload.org_id",
      "template": "./outreach/password_reset.mjml",
      "policy": "@policy.notify",
      "digest": null,
      "throttle": {
        "per": "recipient",
        "max": 3,
        "window": "1h"
      },
      "origin": "notification"
    }
  ]
}
```

## Codegen proposto

Codegen for `dist/go/<feature>/notifications.gen.go` already exists in
the shape sketched in `bucket-jobs-cycle.md:394-419`. The expansion is
**three additive lines** on `notifications.Spec`:

```go
func RegisterNotifications(r *notifications.Registry) {
    r.Register(notifications.Spec{
        Name:        "customer.daily_activity_summary",
        Channels:    []notifications.Channel{notifications.Email},
        Recipient:   "target.email",
        Trigger:     notifications.EventTrigger{Event: "customer.customer_updated"},
        Template:    "./outreach/daily_activity.mjml",
        TenantFrom:  "payload.org_id",
        // NEW:
        Digest:      &notifications.DigestSpec{Window: 24 * time.Hour, GroupBy: "target.email"},
    })

    r.Register(notifications.Spec{
        Name:        "customer.password_reset_email",
        Channels:    []notifications.Channel{notifications.Email},
        Recipient:   "target.email",
        Trigger:     notifications.EventTrigger{Event: "customer.password_reset_requested"},
        Template:    "./outreach/password_reset.mjml",
        TenantFrom:  "payload.org_id",
        // NEW:
        Throttle:    &notifications.ThrottleSpec{Per: notifications.ThrottlePerRecipient, Max: 3, Window: time.Hour},
    })
}
```

`time.Duration` is parsed from the IR string at codegen time using
`time.ParseDuration`. Doctor (or codegen pre-check) rejects strings
`time.ParseDuration` cannot parse — the closed-catalog duration set is
"anything Go's stdlib accepts" because that catalog is fixed by the
runtime target.

## Runtime proposto

The Lazuli Go runtime ships two additions to `runtime/go/lazuli/notifications/`:

### `DigestSpec` + dispatcher loop

```go
// runtime/go/lazuli/notifications/contract.go (extension)

type DigestSpec struct {
    Window  time.Duration
    GroupBy string
}

// NotificationContract.Digest *DigestSpec    // added field
```

Digest accumulation lives **in-process** by default (a per-spec
map keyed by `groupByValue`), with a flush goroutine driven by
`testing/synctest`-friendly tickers. For cross-process correctness,
the Lazuli Go runtime exposes a `DigestStore` interface (in-memory
implementation shipped; Redis/Postgres implementations ship as
`@adapter.notification.digest.<store>` packs). No provider names in
the runtime core.

### `ThrottleSpec` + bucket check

```go
// runtime/go/lazuli/notifications/contract.go (extension)

type ThrottleAxis string

const (
    ThrottlePerRecipient ThrottleAxis = "recipient"
    ThrottlePerTenant    ThrottleAxis = "tenant"
    ThrottlePerGlobal    ThrottleAxis = "global"
)

type ThrottleSpec struct {
    Per    ThrottleAxis
    Max    uint32
    Window time.Duration
}

// NotificationContract.Throttle *ThrottleSpec    // added field
```

The dispatcher checks `ThrottleStore.Allow(key, spec)` before
dispatch; same store interface pattern as `DigestStore`. Typed errors:

```go
var (
    ErrNotificationThrottled = errors.New("notifications: throttle bucket exhausted")
    ErrNotificationDigested  = errors.New("notifications: pending digest window")
)
```

`ErrNotificationThrottled` is **not** an error for the upstream caller
— it's a no-op + a `slog.Info` event for ops. Same pattern as
`IdempotencyViolation` in `bucket-jobs-cycle.md:507-512`.

### No leaks

- No Sendgrid, Mailgun, SES, Twilio, FCM, APN, Slack, Discord name in
  any Lazuli source.
- No Redis, Memcached, Postgres name in any Lazuli source — store
  selection flows through `@adapter.notification.<store>`.
- All provider mechanics stay in the channel dispatcher /
  digest-store / throttle-store adapter slots.

## Evals/Testes propostos

Tests are layered the same way as the jobs cycle.

### Golden eval / fixture extension

Extend `examples/full-capsule/full-capsule.lzi` with two new
notifications exercising the decorators:

```
notification daily_activity_summary
  channel email
  recipient target.email
  trigger event customer.customer_updated
  tenant_from payload.org_id
  idempotency by envelope.id
  template "./outreach/daily_activity.mjml"
  policy @policy.notify
  digest
    window "24h"
    group_by target.email

notification password_reset_email
  channel email
  recipient target.email
  trigger event customer.password_reset_requested
  tenant_from payload.org_id
  idempotency by envelope.id
  template "./outreach/password_reset.mjml"
  policy @policy.notify
  throttle
    per recipient
    max 3
    window "1h"
```

(Requires `password_reset_requested` event to be added to the customer
`event_group customer_*` — small fixture addition.)

The golden file at
`crates/lazuli_cli/tests/fixtures/full-capsule-notifications.golden.json`
pins the post-extension inspect shape. Any drift fails CI.

### Doctor tests (`crates/lazuli_cli/src/doctor.rs:test`)

Six new tests, one per new diagnostic:

```rust
#[test]
fn canonical_warns_for_digest_missing_window() { ... }

#[test]
fn canonical_warns_for_digest_group_by_unknown_payload_path() { ... }

#[test]
fn canonical_warns_for_throttle_per_axis_outside_catalog() { ... }

#[test]
fn canonical_warns_for_throttle_per_recipient_without_recipient_declaration() { ... }
```

### Go test (`runtime/go/lazuli/notifications/digest_test.go`)

```go
func TestDailyActivitySummary_BatchesByEmailWithinWindow(t *testing.T) {
    synctest.Run(func() {
        ctx, reg := testCtx(t)
        registerDailyActivitySummary(reg)

        // Three triggers within 24h, same recipient → one dispatch.
        for i := 0; i < 3; i++ {
            reg.Trigger(ctx, notifications.Envelope{
                Channel:   notifications.Email,
                Recipient: "alice@example.com",
                Payload:   map[string]any{"target_email": "alice@example.com"},
            })
            time.Sleep(time.Hour)
        }
        synctest.Wait()

        assertDispatched(t, reg, "alice@example.com", 1)

        // Past the 24h window, a new trigger dispatches again.
        time.Sleep(22 * time.Hour)
        reg.Trigger(ctx, /* ... */)
        synctest.Wait()
        assertDispatched(t, reg, "alice@example.com", 2)
    })
}

func TestPasswordResetEmail_ThrottlesPerRecipient(t *testing.T) {
    synctest.Run(func() {
        ctx, reg := testCtx(t)
        registerPasswordResetEmail(reg)

        for i := 0; i < 3; i++ {
            err := reg.Dispatch(ctx, /* ... alice ... */)
            if err != nil { t.Fatalf("expected first 3 to succeed; %v", err) }
        }

        err := reg.Dispatch(ctx, /* ... alice (4th) ... */)
        if !errors.Is(err, notifications.ErrNotificationThrottled) {
            t.Fatalf("expected throttle; got %v", err)
        }

        // After 1h, the bucket refills.
        time.Sleep(time.Hour)
        synctest.Wait()
        err = reg.Dispatch(ctx, /* ... alice ... */)
        if err != nil { t.Fatalf("expected refill; %v", err) }
    })
}
```

### LSP test (`crates/lazuli_lsp/src/lib.rs:test`)

Hover on `digest` shows the closed-catalog children (`window`,
`group_by`) with link to `docs/invariants.md`. Completion inside a
`throttle` body after `per ` suggests `recipient`, `tenant`, `global`.

## Doctor/LSP propostos

Six new diagnostics summarized:

| Code | Severity | Message | Trigger |
|---|---|---|---|
| `NOTIF-DIGEST-001` | Error | `notification '<n>' \`digest\` block requires a \`window <duration>\` child.` | Missing `window`. |
| `NOTIF-DIGEST-002` | Error | `notification '<n>' \`digest\` block requires a \`group_by <path>\` child.` | Missing `group_by`. |
| `NOTIF-DIGEST-003` | Error | `notification '<n>' \`digest group_by <path>\` references '<path>' which is not present in the trigger event '<event>' payload.` | Path not in payload. |
| `NOTIF-THROTTLE-001` | Error | `notification '<n>' \`throttle\` block requires \`per\`, \`max\`, and \`window\` children.` | Missing required child. |
| `NOTIF-THROTTLE-002` | Error | `notification '<n>' \`throttle per <axis>\` uses '<axis>' which is not in the closed catalog (recipient, tenant, global).` | Axis outside catalog. |
| `NOTIF-THROTTLE-003` | Error | `notification '<n>' \`throttle per recipient\` requires a \`recipient <path>\` declaration on the notification.` | per recipient + no recipient. |

LSP keyword/hover catalog additions:

- `digest` — block keyword; children `window`/`group_by`.
- `throttle` — block keyword; children `per`/`max`/`window`.
- `window` — duration string, links to `time.ParseDuration` doc.
- `group_by` — payload path.
- `per` — closed catalog `recipient | tenant | global`.

Closed-catalog completion: `per recipient`, `per tenant`, `per global`
suggested after `per ` typed inside a `throttle` body.

No new `@<namespace>` additions — both decorators are local children
inside the existing `notification` block.

## Critério de "ciclo fechado"

The bucket cycle closes when the four boxes hold:

- [ ] Authored in `examples/full-capsule/` — two new notifications
      added, each exercising one decorator. Fixture extension is part
      of the cut.
- [ ] `lazuli check` accepts the new syntax.
- [ ] `lazuli inspect --expand=notifications` reports `digest` and
      `throttle` shapes in the JSON projection (additive serde).
- [ ] `lazuli doctor` runs the six new diagnostics + the existing
      `NOTIF-CHANNEL-001`.
- [ ] `lazuli generate` emits `dist/go/<feature>/notifications.gen.go`
      with `Digest`/`Throttle` fields populated.
- [ ] Lazuli Go executes one digest cycle (three triggers → one dispatch
      after window) and one throttle cycle (4th request rejected, 1h
      later succeeds) under `testing/synctest`.
- [ ] LSP serves hover on `digest` / `throttle` / `window` / `group_by`
      / `per` + closed-catalog completions on `per <axis>`.

## Próximo passo

1. Update `docs/invariants.md:178` — replace the unimplemented
   `rate_limit` mention on `notification` with the recommended
   `throttle` keyword + `digest` keyword. Add the closed-catalog
   children to the invariant.
2. Extend `crates/lazuli_ir/src/lib.rs:2217` with `DigestSpec`,
   `ThrottleSpec`, `ThrottleAxis`, and two additive fields on
   `Notification`.
3. Extend `crates/lazuli_syntax/src/parser.rs:2333` (`parse_notification`)
   with two new block branches: `digest` and `throttle`. Reuse
   `parse_job_retry`-style child parsing (block with three or two
   named children).
4. Add the six diagnostics to
   `crates/lazuli_cli/src/doctor.rs:2383`
   (`tier3_notification_diagnostics`). Cross-check
   `digest group_by` against the lifted trigger event payload via
   `Tier3FeatureFacts`.
5. Extend `runtime/go/lazuli/notifications/contract.go:49` with
   `DigestSpec`, `ThrottleSpec`, `ThrottleAxis`, and two pointer
   fields on `NotificationContract`. Add `DigestStore`, `ThrottleStore`
   interfaces.
6. Extend the fixture with `daily_activity_summary` (digest) and
   `password_reset_email` (throttle) — including the
   `password_reset_requested` event addition to the `customer_*`
   event_group.
7. Pin `crates/lazuli_cli/tests/fixtures/full-capsule-notifications.golden.json`
   to the post-extension inspect output.
8. Hand off the Lazuli Go dispatcher loop + in-memory store implementations to
   the runtime team. The language team stays in surface/IR/doctor
   territory.
9. **Do not** promote `delivery_receipt` or `read_receipt` in this
   cycle. Their roadmap entries (`docs/roadmap.md:192`) stay
   SPECULATIVE until pilot pressure justifies the boundary cost.

Cycle-close evidence: the `bucket-notifications-expanded-cycle.golden.json`
inspect file plus green `cargo test -q -p lazuli_runtime --features=integration`
covering the digest + throttle Go tests.

## Rows sugeridas para `docs/next-checklist.md`

| Order | Cut | Status | Notes |
|-------|-----|--------|-------|
| 38 | Notifications expanded — `digest` + `throttle` decorators (IR + parser + doctor + LSP) | proposed | Two additive decorators on the shipped `Notification` IR (`crates/lazuli_ir/src/lib.rs:2217`). Six new IR-driven diagnostics (`NOTIF-DIGEST-001/002/003`, `NOTIF-THROTTLE-001/002/003`) cross-checking against `Tier3FeatureFacts`. Two new closed-catalog hovers/completions. Fixture extends with `daily_activity_summary` (digest) and `password_reset_email` (throttle). Replaces the unimplemented `rate_limit` mention in `docs/invariants.md:178` with `throttle`. See `docs/proposals/bucket-notifications-expanded-cycle.md` §Linguagem. |
| 39 | Notifications expanded — Lazuli Go dispatcher + `DigestStore`/`ThrottleStore` interfaces | proposed | `runtime/go/lazuli/notifications/contract.go:49` gains `DigestSpec`/`ThrottleSpec`/`ThrottleAxis` + two pointer fields on `NotificationContract`. New `DigestStore`/`ThrottleStore` adapter interfaces (in-memory implementation in core; Redis/Postgres ship as `@adapter.notification.{digest,throttle}.<store>` packs). Two `testing/synctest` tests covering the canonical digest-window and throttle-bucket cycles. The runtime team owns store implementations. See `docs/proposals/bucket-notifications-expanded-cycle.md` §Runtime/§Evals. |
| 40 | Notifications expanded — `delivery_receipt` / `read_receipt` deferred | pilot-gated | Both decorators classified SPECULATIVE in this cycle. Boundary cost (provider-specific outcome codes for delivery; per-channel polysemy for read) does not clear the closed-catalog bar today. Existing webhook+event primitives cover the use case. Reopen only when a fixture-grade pilot demonstrates structured delivery state as a domain-level contract. See `docs/proposals/bucket-notifications-expanded-cycle.md` §Linguagem §3-4. |
