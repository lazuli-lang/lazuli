# Bucket Cycle: Realtime (L0→L2, Cut realtime gated)

**Run**: `/lazuli-bucket-cycle bucket=realtime mode=design`
**Date**: 2026-05-11
**Cut**: **F — Cut realtime gated** (per `docs/roadmap.md:454` and
`docs/audit/framework-coverage-1400.md:288`). This proposal is **design
only**; nothing here lands in `main` until a pilot product proves
collaboration / presence / live-dashboard pressure that the existing
pull-based `source` + `invalidates` cache pattern cannot satisfy.
**Pre-requisite (if promoted)**: `bucket-realtime-scope.md` (surface
locator decision) must land first.

## Contexto

Realtime is the §0 bucket-piloto strategy's
(`docs/roadmap.md:23-45`) largest single horizontal gap. Section §20 of
the framework-coverage audit
(`docs/audit/framework-coverage-1400.md:280-288`) opens with the diagonal:

> **L**: nenhum. **DL**: kind `channel`, kind `presence`, kind `broadcast`,
> kind `subscription`. **DF**: WebSocket server, SSE server, pub/sub,
> presence tracking, reconnect handling, backpressure, heartbeats,
> connection draining, realtime metrics/tracing. **DA**: Redis/NATS/Kafka
> streaming. **F**: live reload, live updates, live dashboards (Cut admin
> gated), collaborative events. **Destaque**: Realtime é o **maior buraco
> horizontal** — 0 cobertas. Decisão de design: adiar até pilot.

Zero L0, four DL kinds proposed, twelve DF capabilities, four DA targets,
five F sub-features. By volume this is the largest unfilled axis in the
1.400-feature audit. By urgency it is also the most defensible to defer:
the four shipped bucket-piloto cycles (auth / storage / jobs /
observability — `next-checklist.md:62-73`) covered the canonical fixture's
authored pressure without ever needing a socket.

The roadmap is explicit
(`docs/roadmap.md:454,673-676,494`): realtime ships only after a pilot
product exercises live UX (collaboration cursors, presence indicators,
push-driven dashboards) that the existing pull-based pattern visibly
fails. The state-of-AI-first overview echoes this at a higher level
(`docs/state-of-ai-first.md:230`): "Streaming protocol differentiation
(SSE vs WS vs gRPC) — Pack / adapter", marking even the runtime question
as deferred.

This proposal does the design pass against that fixed constraint. The
output is a self-contained substrate ready to lift when (and only when)
the pilot signal arrives. **Authoring this design now buys the same
discipline auth / storage / jobs / observability got**: pre-cycle
clarity so that the implementation run, once promoted, is mechanical and
the surface does not drift under pressure.

The canonical fixture (`examples/full-capsule/`) does not author any
realtime construct today; the closest signals are the `event.trace
score_recomputed` declaration (`full-capsule.lzi:196`) and the
`@client.activity_timeline` opaque block in the customer detail view
(`full-capsule.lzx:67`). Both are pull-shaped; the timeline opens as a
React component that polls. The cycle's first deliverable, if promoted,
is to extend the fixture with one realistic realtime view, one channel,
and one subscription.

## Baseline

Inventário L0/L1/L2 dos construct mappings que existem hoje vs. precisam
existir. `Surface` é "lê do fixture canônico"; `Grammar` é "parser
reconhece"; `IR` é "struct dedicado em `lazuli_ir`"; `Doctor/LSP` é
"diagnostic cross-checa"; `Codegen` é "`lazuli_codegen_go` produz arquivo
Go"; `Runtime` é "Drusa executa".

| Construct | Surface | Grammar | IR | Doctor/LSP | Codegen | Runtime | L-level |
|---|---|---|---|---|---|---|---|
| `channel <name>` (feature) | no | no | no | no | no | no | **L=0** |
| `presence <name>` (feature) | no | no | no | no | no | no | **L=0** |
| `broadcast` (command/event child) | no | no | no | no | no | no | **L=0** |
| `subscription <name>` (feature) | no | no | no | no | no | no | **L=0** |
| `subscribe <ref>` (`.lzx` view locator) | no | no | no | no | no | no | **L=0** (side-quest; see `bucket-realtime-scope.md`) |
| capability `realtime <name>` (registry) | no | no | no | no | no | no | **L=0** |
| `runtime unit realtime` (`app.lzi`) | no | no | no | no | no | no | **L=0** |
| Closest existing primitive: `event <name>` + `invalidates query.X` | yes (`full-capsule.lzi:179,259-261`) | line-based | yes (`Event`, `Invalidate`) | yes (event-name resolution + invalidate refs) | partial | partial | L1 (pull-based; not realtime) |
| Closest existing primitive: `block @client.<name>` | yes (`full-capsule.lzx:67`) | locator-based | yes (`ViewLocator::Block`) | yes (client ref resolution) | no | no | L1 (opaque escape hatch) |
| Closest existing primitive: `event.trace <name>` | yes (`full-capsule.lzi:196`) | line-based | yes (`Event{kind:Trace}`) | yes (4 diagnostics) | no | no | L1 (observability, not transport) |

**Summary**: zero L on every realtime construct. The bucket inherits no
language surface, no IR struct, no doctor coverage, no codegen path, no
runtime adapter. The only adjacent primitives are pull-based
(`event` + `invalidates`) or opaque (`block @client.*`). Realtime is the
cleanest greenfield bucket of the four-bucket strategy because no legacy
text-pattern rules exist to migrate; everything starts as IR.

This baseline matters because it sets a different cost shape from the
other buckets. Auth had IR but no parser lowering; storage had partial
typing; jobs had partial IR + heavy text-pattern doctor; realtime has
**nothing**. The cycle, when promoted, ships entirely new substrate.

## Linguagem proposta

Four new feature-level kinds plus one surface locator (the locator is
the subject of `bucket-realtime-scope.md` Route A; this cycle assumes
that decision lands).

All kinds follow the existing closed-grammar pattern — indent-based,
PascalCase, no provider names, no DI mechanics, no transport details.
Tenant axis and policy are declared on the channel, not on individual
operations.

### 1. `channel <name>` (feature-level kind)

Declares a logical realtime channel. Tenant-scoped, policy-gated. The
channel is the unit of authorization; subscribers and broadcasters
both reach the channel through it. No provider names: which transport
(WebSocket / SSE / Redis pub/sub / NATS) carries the channel is an
adapter decision resolved through `registry.lzi` capability
`realtime <name>` + `app.lzi` `bindings`.

```
feature customer
  uses org

  channel customer_activity
    tenant_from org
    policy @policy.read
    payload
      kind: ActivityKind
      customer_id: ID
      at: DateTime
      by_id: ID optional when @actor.user
    audit
      member_joined
      member_left
      messages_per_minute
```

Required children: `tenant_from`, `policy`, `payload`. Optional:
`audit` (closed catalog of channel-level audit events — joins, leaves,
rate). The `payload` record is the typed message shape; doctor cross-
checks all `broadcast` calls and `subscribe` locators against it.

The channel does **not** declare transport (no `transport ws`, no
`transport sse`), does not declare cardinality (no `mode broadcast`
vs `unicast`), does not declare retention. Those are
runtime/adapter concerns. The language declares the **contract** —
who can subscribe, what shape flows, which tenant axis scopes it.

### 2. `presence <name>` (feature-level kind)

Declares presence tracking bound to a channel. Closed-grammar children:

```
  presence customer_activity_viewers
    channel customer_activity
    member
      user_id: User.ID required
      since: DateTime required
      activity: ViewerActivity optional
    heartbeat "15s"
    timeout "60s"
    audit
      member_joined
      member_left
```

Required children: `channel <ref>` (must resolve to a `channel` in
this feature or an imported feature whose policy is reachable),
`member <record>` (the typed presence entry shape), `heartbeat
<duration>`, `timeout <duration>`. Optional: `audit`.

Doctor cross-check: the `channel`'s policy must be `>=` the
`presence`'s implicit read policy (presence list is a derived read of
the channel's membership). `heartbeat` must be strictly less than
`timeout` (catch the canonical mistake).

### 3. `broadcast` (child of `command` / `event`)

Declares that a command (or event handler) emits a message into a
channel. The shape mirrors the existing `emits <event>` child but
targets a channel, not an event-bus event.

```
  command annotate_customer
    route id: ID
    input
      message: Text required
    policy @policy.update
    updates Customer
      ... # noop here; in real fixture this would be a comment record
    broadcast customer_activity
      kind = ActivityKind.annotation
      customer_id = route.id
      at = ctx.now
      by_id = ctx.user.id
    emits customer_annotated from broadcast
```

Required children inside `broadcast`: typed field bindings matching
the channel's `payload` record. Doctor cross-check: every field in
the channel's payload either has an `optional when ...` clause or
must be bound in every `broadcast` body. Missing fields are an error
the same way missing `emits` fields are an error today.

The `emits ... from broadcast` form is the realtime sibling of
`emits ... from creates` — the event-bus payload is derived from the
broadcast payload so cross-system listeners (jobs, audit log) still
see the same record without re-authoring.

Broadcast is **not** a separate top-level kind. It is always a child
of an authored command or event — broadcasts that have no authoring
context (e.g. "the server emits randomly") do not exist in Lazuli's
contract model. If a runtime job needs to broadcast, the job declares
`broadcast <channel>` as a child the same way it declares `emits`.

### 4. `subscription <name>` (feature-level kind)

Declares a typed subscription contract — what a client may subscribe
to, scoped by policy and tenant. This is what `.lzx` views bind to
via the `subscribe` locator (`bucket-realtime-scope.md` Route A).

```
  subscription activity_feed
    channel customer_activity
    filter
      customer_id = params.customer_id
    params
      customer_id: ID required
    policy @policy.read
    rate_limit "120 events per minute per user"
```

Required children: `channel <ref>`, `policy`. Optional: `filter`
(server-side filtering of the channel stream — same predicate
language as `query.list ... filters`), `params` (the args the
subscription accepts at open-time, identical to `query` `params`),
`rate_limit` (events-per-window throttle; closed catalog).

Doctor cross-checks: `channel` resolves; `policy` lattice satisfies
the channel's policy; every `filter` predicate type-checks against
the channel `payload` and the subscription `params`.

A subscription is **not** a `query` and is **not** a `channel`. It
is the surface-facing slice — same separation as `command` vs
`workflow.transition` vs `effect`. The cardinality matters: a
channel may have many subscriptions (different filters, different
audiences); a subscription has exactly one channel.

### 5. `subscribe <ref>` (`.lzx` view locator) — see scope-out

The view-side locator is designed in `bucket-realtime-scope.md`
(Route A). This cycle assumes that decision. Surface:

```
view detail
  route id: Customer.ID
  source customer.query.by_id(id: route.id)
  subscribe customer.subscription.activity_feed(customer_id: route.id)
  block @client.activity_timeline
```

### 6. `runtime unit realtime` (app.lzi)

Operational declaration parallel to `unit api` / `unit worker`. Today
`app.lzi` declares (`examples/full-capsule/app.lzi:76-89`):

```
runtime
  unit api
    serves queries, commands, webhooks, apis
    healthcheck "/healthz"
    readiness "/readyz"

  unit web
    serves surfaces web

  unit worker
    runs jobs *

  unit scheduler
    runs schedules *
```

Realtime adds:

```
  unit realtime
    serves channels *
    healthcheck "/realtime/healthz"
    readiness "/realtime/readyz"
```

`serves channels *` is the closed-catalog declaration that this
process owns the realtime port. Codegen wires the WebSocket / SSE
listener; the registry binding selects the adapter (Redis pub/sub
default, NATS / Kafka secondary). One unit per app — Lazuli does not
declare multi-realtime-unit topologies (that is runtime/Drusa).

### 7. Capability `realtime <name>` (registry.lzi)

```
capabilities
  database postgres
  queue background_jobs
  object_storage files
  mailer transactional
  event_bus internal
  realtime live_channels    # new
  tracing optional
  integration crm
```

Adapter binding lives in `registry.lzi` `integrations` or `app.lzi`
`bindings` the same way every other capability binds. Provider names
(Redis, NATS, Kafka) never appear in `.lzi`; they appear in adapter
source as `@runtime/realtime-redis`, `@plugin/<publisher>/nats`,
etc.

### Anti-proposals (rejected here)

- **`transport ws`/`transport sse` keyword on channel.** Transport is
  adapter-resolved. The language declares contract, not wire format.
  An app deployed against a Redis pub/sub adapter pipes the same
  channel to both WebSocket and SSE clients depending on what the
  edge supports; that decision is runtime, not source.
- **`mode broadcast` / `mode unicast` keyword.** The cardinality is
  derived from the channel's tenant + filter shape. A channel with
  `tenant_from org` is broadcast within the tenant; a subscription
  with `filter customer_id = params.customer_id` is effectively
  unicast within that tenant. Promoting cardinality to a keyword
  would push DI mechanics into source.
- **`presence cursors` / `presence selection` per-subfield kinds.**
  Speculative collaboration UI; not in any pilot. The closed catalog
  for `presence` stays minimal (member + heartbeat + timeout).
- **Provider keywords.** `websocket`, `sse`, `redis`, `nats`, `kafka`,
  `pusher`, `ably` never appear in core syntax.
- **`channel ... retention "30 days"` declaration.** Channel
  retention is a runtime/adapter concern (Redis stream cap, NATS
  JetStream window, etc.). The language does not declare it.
- **`flow` / `connection` / `socket` kinds.** All three would imply
  the language reasons about the wire-level session, not the
  message-level contract. They belong to the runtime.
- **`subscribe ... reconnect <strategy>` per-view tuning.** Runtime
  adapter concern; promoting it to surface is the
  magic-discovery-without-visibility failure mode (skill rules).

## IR proposto

Five new IR structs, one extension to an existing struct, one extension
to `AppManifest`. All in `crates/lazuli_ir/src/lib.rs`.

### `Channel` struct

```rust
pub struct Channel {
    pub name: String,
    pub tenant_from: TenantFromSpec,         // reuses jobs/webhooks/notifications shape
    pub policy: QualifiedName,
    pub payload: Record,                     // reuses existing Record shape
    pub audit: Option<ChannelAuditSpec>,
    pub origin: Origin,                      // canonical-indent slice provenance
}

pub struct ChannelAuditSpec {
    pub events: Vec<ChannelAuditEvent>,      // closed catalog: MemberJoined / MemberLeft / MessagesPerMinute / SlowConsumerDrop
}
```

Goes after `Notification` in `lib.rs` (the four-bucket additive lift
landed `Notification` + `EventGroup` in row 32 of the checklist;
realtime follows the same pattern). Reuses `TenantFromSpec` and
`Record` to keep IR vocabulary tight.

### `Presence` struct

```rust
pub struct Presence {
    pub name: String,
    pub channel: QualifiedName,              // resolves to Channel in this or imported feature
    pub member: Record,
    pub heartbeat: Duration,
    pub timeout: Duration,
    pub audit: Option<ChannelAuditSpec>,
    pub origin: Origin,
}
```

### `Subscription` struct

```rust
pub struct Subscription {
    pub name: String,
    pub channel: QualifiedName,
    pub filter: Option<Predicate>,           // reuses query filters predicate language
    pub params: Vec<RecordField>,            // reuses query params shape
    pub policy: QualifiedName,
    pub rate_limit: Option<RateLimit>,       // reuses existing rate_limit struct
    pub origin: Origin,
}
```

### `BroadcastSpec` (child of `Command` / `Event`)

```rust
pub struct BroadcastSpec {
    pub channel: QualifiedName,
    pub bindings: Vec<FieldBinding>,         // reuses existing FieldBinding from creates/updates
}

pub struct Command {
    // ... existing fields
    pub broadcasts: Vec<BroadcastSpec>,      // new field; serde-default empty
}

pub struct Event {
    // ... existing fields  
    pub broadcasts: Vec<BroadcastSpec>,      // new field
}
```

Additive serde-default empty vec on both. No on-disk JSON consumer
breaks because nothing reads `broadcasts` yet.

### `Feature` extension

```rust
pub struct Feature {
    // ... existing fields
    pub channels: Vec<Channel>,              // new
    pub presences: Vec<Presence>,            // new
    pub subscriptions: Vec<Subscription>,    // new
}
```

### `AppRuntimeUnit` extension

The existing `AppRuntimeUnit` (`crates/lazuli_ir/src/lib.rs` after the
observability bucket landed) gains one new `serves` variant:
`ServesKind::Channels` — closed catalog with a wildcard form
(`channels *`).

### `AppCapability` extension

Add `AppCapability::Realtime { name: String }` to the existing
closed catalog (`database`, `queue`, `object_storage`, `mailer`,
`event_bus`, `tracing`, `integration`).

### Diagnostics added in this cycle

| Code | Severity | Trigger |
|---|---|---|
| `CHANNEL-PAYLOAD-001` | Error | `broadcast <channel>` body has a field not in the channel's `payload` record. |
| `CHANNEL-PAYLOAD-002` | Error | `broadcast <channel>` body misses a required field from the channel's `payload` record (a field without `optional when ...`). |
| `CHANNEL-TENANT-001` | Error | `channel` with `tenant_from <axis>` references an axis not declared in any `defaults tenancy <axis>` reachable via `uses`. (Mirror of `JOB-FANOUT-001` from the jobs cycle.) |
| `CHANNEL-POLICY-001` | Error | `broadcast` body is reachable from a policy weaker than the channel's `policy` (the command's policy must be `>=` the channel's). |
| `PRESENCE-HEARTBEAT-001` | Error | `presence` declares `heartbeat <d1>` and `timeout <d2>` where `d2 <= d1` (timeout must exceed heartbeat by at least 2x; warn if less than 2x but allow). |
| `PRESENCE-CHANNEL-001` | Error | `presence ... channel <ref>` does not resolve. |
| `PRESENCE-CHANNEL-002` | Error | `presence` references a channel whose `policy` is not satisfied by the presence's effective read policy. |
| `SUBSCRIPTION-CHANNEL-001` | Error | `subscription ... channel <ref>` does not resolve. |
| `SUBSCRIPTION-POLICY-001` | Error | `subscription ... policy` is weaker than the bound `channel`'s `policy`. |
| `SUBSCRIPTION-FILTER-001` | Error | `subscription ... filter` references a field not in the channel's `payload` or a `params` entry not declared. |
| `SUBSCRIPTION-RATE-001` | Warning | `subscription` without `rate_limit` and bound to a channel whose `audit messages_per_minute` is declared — the rate-limit declaration is recommended to give Drusa a typed knob. |
| `APP-REALTIME-001` | Error | `app.lzi` `bindings` include a realtime channel without a `runtime unit realtime` declared, or vice versa. |

Twelve new IR-driven diagnostics. All run cross-feature against typed
IR; none are text-pattern.

### JSON shape (`lazuli inspect --format=json`)

Per `InspectFeature` (after the realtime cycle lands):

```json
{
  "name": "customer",
  "channels": [
    {
      "name": "customer_activity",
      "tenant_from": "org",
      "policy": "@policy.read",
      "payload": [
        { "name": "kind", "type": "ActivityKind" },
        { "name": "customer_id", "type": "ID" },
        { "name": "at", "type": "DateTime" },
        { "name": "by_id", "type": "ID", "condition": "@actor.user" }
      ],
      "audit": ["member_joined", "member_left", "messages_per_minute"],
      "origin": "channel"
    }
  ],
  "presences": [],
  "subscriptions": [
    {
      "name": "activity_feed",
      "channel": "customer.channel.customer_activity",
      "filter": "customer_id = params.customer_id",
      "params": [{ "name": "customer_id", "type": "ID", "required": true }],
      "policy": "@policy.read",
      "rate_limit": "120 events per minute per user",
      "origin": "subscription"
    }
  ]
}
```

`InspectCommand.broadcasts` and `InspectEvent.broadcasts` mirror the
existing `emits` shape.

## Codegen proposto

`lazuli_codegen_go` produces three new files per feature carrying
channels, presences, or subscriptions. Each file imports the runtime
package (`lazuli.dev/runtime/lazuli`) and a per-bucket subpackage where
adapter wiring lands (see `## Runtime proposto`).

### File 1: `dist/go/<feature>/channels.gen.go`

One file per feature with channels. Emits a `RegisterChannels(r *lazuli.ChannelRegistry)`
function the boot path calls.

```go
// path: dist/go/customer/channels.gen.go
// Code generated by lazuli; DO NOT EDIT.
package customer

import (
    "lazuli.dev/runtime/lazuli"
    "lazuli.dev/runtime/lazuli/realtime"
)

type CustomerActivityPayload struct {
    Kind       ActivityKind `json:"kind"`
    CustomerID lazuli.ID    `json:"customer_id"`
    At         lazuli.Time  `json:"at"`
    ByID       *lazuli.ID   `json:"by_id,omitempty"`
}

func RegisterChannels(r *realtime.ChannelRegistry) {
    r.Register(realtime.ChannelSpec{
        Name:       "customer.customer_activity",
        TenantAxis: "org",
        Policy:     "@policy.read",
        PayloadFn:  func() any { return CustomerActivityPayload{} },
        Audit: realtime.ChannelAudit{
            MemberJoined:        true,
            MemberLeft:          true,
            MessagesPerMinute:   true,
            SlowConsumerDrop:    false,
        },
    })
}
```

### File 2: `dist/go/<feature>/subscriptions.gen.go`

```go
// path: dist/go/customer/subscriptions.gen.go
// Code generated by lazuli; DO NOT EDIT.
package customer

import (
    "lazuli.dev/runtime/lazuli/realtime"
)

type ActivityFeedParams struct {
    CustomerID lazuli.ID `json:"customer_id"`
}

func RegisterSubscriptions(r *realtime.SubscriptionRegistry) {
    r.Register(realtime.SubscriptionSpec{
        Name:    "customer.activity_feed",
        Channel: "customer.customer_activity",
        Policy:  "@policy.read",
        Filter: func(p any, ev any) bool {
            params := p.(ActivityFeedParams)
            event := ev.(CustomerActivityPayload)
            return event.CustomerID == params.CustomerID
        },
        RateLimit: realtime.Rate{
            Events: 120, Window: time.Minute, Scope: realtime.PerUser,
        },
    })
}
```

### File 3: `dist/go/<feature>/presences.gen.go`

```go
// path: dist/go/customer/presences.gen.go
// Code generated by lazuli; DO NOT EDIT.
package customer

import "lazuli.dev/runtime/lazuli/realtime"

type CustomerActivityViewer struct {
    UserID   lazuli.ID       `json:"user_id"`
    Since    lazuli.Time     `json:"since"`
    Activity *ViewerActivity `json:"activity,omitempty"`
}

func RegisterPresences(r *realtime.PresenceRegistry) {
    r.Register(realtime.PresenceSpec{
        Name:      "customer.customer_activity_viewers",
        Channel:   "customer.customer_activity",
        MemberFn:  func() any { return CustomerActivityViewer{} },
        Heartbeat: 15 * time.Second,
        Timeout:   60 * time.Second,
    })
}
```

### Command `broadcasts` lowering

`Command.broadcasts` translates into a `realtime.Publish` call inside
the command's generated body, post-commit, parallel to the existing
`Publish` call for `emits`:

```go
return lazuli.WithTx(ctx, func(tx lazuli.Tx) error {
    // ... existing updates ...
    if err := realtime.Publish(ctx, realtime.PublishSpec{
        Channel: "customer.customer_activity",
        TenantAxis: "org",
        Payload: CustomerActivityPayload{
            Kind:       ActivityKind_Annotation,
            CustomerID: input.ID,
            At:         lazuli.Now(ctx),
            ByID:       lazuli.OptionalUser(ctx),
        },
    }); err != nil {
        return err
    }
    return lazuli.Publish(ctx, /* existing emits */)
})
```

### Boot wiring (`dist/go/main.gen.go`)

Boot composes `RegisterChannels` / `RegisterPresences` /
`RegisterSubscriptions` from every generated feature into the runtime
registries. Composed boot is deterministic (alphabetical by feature,
alphabetical by name).

No provider names anywhere in generated code. `ChannelRegistry`,
`SubscriptionRegistry`, `PresenceRegistry` are Drusa-side; Redis / NATS
/ Kafka wiring happens through the registry adapter binding.

### `.lzx` `subscribe` locator codegen

The view-side codegen target (React for web, Expo for mobile) emits a
hook that opens the WebSocket / SSE connection via the runtime's
realtime client, holds the `react-query` (or React Native equivalent)
state for both pull and push paths, and re-renders on push:

```typescript
// dist/web/customer/views/detail.gen.tsx (excerpt)
export function useDetailView({ id }: { id: CustomerID }) {
    const initial = useQuery({
        queryKey: ["customer", "by_id", id],
        queryFn: () => api.customer.queryByID({ id }),
    });

    const live = useSubscription({
        subscription: "customer.activity_feed",
        params: { customer_id: id },
        onEvent: (ev) => initial.refetch(),  // codegen-derived from `invalidates` rules
    });

    return { initial, live };
}
```

`useSubscription` is the Drusa-side React hook. Author writes nothing
client-side; the locator and the subscription contract drive it.

## Runtime proposto

Drusa entrega one new subpackage under `runtime/go/lazuli/realtime/`.
Concrete adapters live in `runtime/go/lazuli/realtime/<adapter>`
(e.g. `runtime/go/lazuli/realtime/redis` for Redis pub/sub).

### `runtime/go/lazuli/realtime/`

Capabilities exposed to generated code:

- `ChannelSpec`, `PresenceSpec`, `SubscriptionSpec` — typed
  registration shapes.
- `ChannelRegistry`, `PresenceRegistry`, `SubscriptionRegistry` —
  typed registries that generated `Register*` calls.
- `Publish(ctx, PublishSpec)` — broadcast entry point called from
  generated command bodies post-commit.
- `Subscribe(ctx, subscription string, params any) (<-chan Event, error)` —
  server-side subscribe helper (used by the SSE/WS edge for fan-out).
- `Adapter` interface (`Subscribe`, `Publish`, `Presence` methods).
- Typed errors (`ErrChannelUnknown`, `ErrPolicyDenied`,
  `ErrTenantMismatch`, `ErrSubscriptionRateExceeded`, `ErrBackpressure`).

Adapter contract:

```go
type Adapter interface {
    // Publish broadcasts to all subscribers of a channel, scoped by tenant.
    Publish(ctx context.Context, channel string, tenant lazuli.Tenant, payload any) error

    // Subscribe opens a server-side stream from the channel.
    Subscribe(ctx context.Context, channel string, tenant lazuli.Tenant) (<-chan Envelope, error)

    // PresenceJoin / PresenceLeave manage presence membership.
    PresenceJoin(ctx context.Context, channel string, tenant lazuli.Tenant, member any) error
    PresenceLeave(ctx context.Context, channel string, tenant lazuli.Tenant, memberID string) error
    PresenceList(ctx context.Context, channel string, tenant lazuli.Tenant) ([]any, error)

    // Boot launches connection pools / pub-sub goroutines.
    Boot(ctx context.Context, cfg AdapterConfig) error
    Shutdown(ctx context.Context) error
}
```

Primary adapter: **Redis pub/sub** (`runtime/go/lazuli/realtime/redis`).
Backed by `redis/go-redis/v9`. Same Redis pool the session / cache
adapters use. Redis is **not** mentioned in any generated `.gen.go`
file and not in any Lazuli source.

Secondary adapters (DA, future): NATS (`runtime/go/lazuli/realtime/nats`),
Kafka (`runtime/go/lazuli/realtime/kafka`). Each lands when a pilot
needs the specific consistency / fan-out properties Redis cannot
deliver.

### Edge surface (WebSocket / SSE)

`runtime/go/lazuli/realtime/edge/` carries the wire-format
implementations:

- `edge/ws` — WebSocket server (chi-mounted at `/realtime/ws`). Uses
  `gorilla/websocket` or `coder/websocket`.
- `edge/sse` — SSE server (chi-mounted at `/realtime/sse`).
- `edge/auth` — middleware that translates the session cookie / token
  into the realtime tenant + actor context.

The edge picks WS vs SSE based on the client `Accept` header (WS-first
with SSE fallback). The decision is invisible to Lazuli source.

### Configuration consumed

- `app.lzi` `runtime unit realtime serves channels *` → edge boot.
- `app.lzi` `bindings <feature>.realtime = registry.integrations.<name>`
  → adapter selection.
- `registry.lzi` `capabilities realtime <name>` → adapter declaration.

### Lifecycle

- Boot: `lazuli.Boot` instantiates the adapter, calls `Boot`, then
  `RegisterChannels` / `RegisterPresences` / `RegisterSubscriptions`
  from every generated feature module, then starts the edge listeners.
- Shutdown: drain in-flight connections, close pub/sub goroutines,
  flush presence with `PresenceLeave`.

### Typed errors

```go
type RealtimeError struct {
    Kind    RealtimeErrorKind  // ChannelUnknown | PolicyDenied | TenantMismatch | RateExceeded | Backpressure | ConnectionClosed
    Channel string
    Cause   error
    Tenant  *lazuli.Tenant
}
```

`PolicyDenied` and `TenantMismatch` surface as WS close codes
(`4403`, `4404` per RFC 6455 application range) and SSE event
`error` payloads. They are **never** silent — the edge always tells
the client why it disconnected.

### Cross-cutting with existing buckets

- **Observability**: a new built-in trace event `channel_publish` is
  reserved (parallel to `agent_run` / `command_run` / `job_run` /
  `webhook_run` — row 35 of `next-checklist.md`). Payload:
  `channel`, `tenant_axis`, `tenant_id`, `subscriber_count`,
  `payload_bytes`, `latency_ms`. Subscribed via `@trace.channel_publish`
  the same way the four existing built-in traces are.
- **Audit**: `channel.audit messages_per_minute` lowers to a Drusa
  metric emitted to the audit log (resolved through
  `audit emit_to <event_group>` if declared, per row 37).
- **Jobs**: a job declaring `broadcast <channel>` reuses the
  command-side IR shape verbatim. No new IR field.

### No leaks

- No `redis-go` type in any Lazuli source.
- No `nats.go`, `confluent-kafka-go`, `pusher`, `ably` name in any
  Lazuli source.
- No `gorilla/websocket` or `coder/websocket` import in any
  `.gen.go` file.
- All provider mechanics flow through `@adapter.<name>` and the
  runtime's `Adapter` contract above.

## Evals/Testes propostos

The cycle closes when at least one end-to-end loop passes. Tests
layered the same way as jobs — surface contract checks, IR golden,
codegen golden, runtime synctest integration.

### Inspect golden

```bash
cargo run -q -p lazuli_cli -- inspect examples/full-capsule/full-capsule.lzi \
  --format=json --expand=channels > /tmp/got.json
diff /tmp/got.json crates/lazuli_cli/tests/fixtures/full-capsule-realtime.golden.json
```

Three new goldens land alongside: `channels`, `subscriptions`,
`presences` — one per projection.

### Go test (`runtime/go/lazuli/realtime/redis/redis_test.go`)

```go
func TestActivityFeed_ChannelBroadcastsToSubscriber_WithTenant(t *testing.T) {
    synctest.Run(func() {
        ctx, db := testCtx(t)
        defer db.Close(ctx)

        adapter := redis.NewAdapter(t)
        reg := realtime.NewRegistry(t)
        customer.RegisterChannels(reg.Channels)
        customer.RegisterSubscriptions(reg.Subscriptions)

        // Open a subscription as user "u_1" in tenant "org_1"
        events, err := adapter.Subscribe(ctx, "customer.customer_activity",
            lazuli.Tenant{Axis: "org", ID: "org_1"})
        if err != nil { t.Fatal(err) }

        // Broadcast from another goroutine within the same tenant
        go func() {
            adapter.Publish(ctx, "customer.customer_activity",
                lazuli.Tenant{Axis: "org", ID: "org_1"},
                customer.CustomerActivityPayload{
                    Kind: customer.ActivityKind_Annotation,
                    CustomerID: "cus_1",
                    At: lazuli.Now(ctx),
                })
        }()

        synctest.Wait()
        ev := <-events
        // assert payload roundtrip + tenant isolation
    })
}
```

`TestActivityFeed_CrossTenant_DoesNotLeak` follows the same shape but
asserts a subscriber in `org_2` never sees the `org_1` broadcast —
the **single load-bearing test** of the bucket.

### Doctor test (`crates/lazuli_cli/src/doctor.rs:test`)

```rust
#[test]
fn canonical_warns_when_broadcast_misses_payload_field() {
    let source = "
feature customer
  defaults
    tenancy org

  channel customer_activity
    tenant_from org
    policy @policy.read
    payload
      kind: ActivityKind
      customer_id: ID
      at: DateTime

  command annotate
    route id: ID
    policy @policy.update
    updates Customer
    broadcast customer_activity
      kind = ActivityKind.annotation
      customer_id = route.id
      # missing `at` — required
";
    let diags = run_doctor(source);
    assert!(diags.iter().any(|d| d.code == "CHANNEL-PAYLOAD-002"));
}
```

Twelve new doctor tests, one per IR-driven diagnostic.

### LSP test

Hover on `subscribe customer.subscription.activity_feed` shows
resolved channel, payload schema, policy, rate_limit. Completion
inside a `broadcast` body after a payload field name suggests the
declared channel's `payload` record fields.

## Doctor/LSP propostos

Twelve new diagnostics (table in `## IR proposto` above) plus LSP
keyword/hover catalog additions:

- `channel` — feature-level kind; hover shows tenant axis + policy +
  payload record summary.
- `presence` — feature-level kind; hover shows bound channel +
  heartbeat/timeout window.
- `subscription` — feature-level kind; hover shows bound channel +
  policy + rate_limit.
- `broadcast` — child of command/event; hover shows resolved channel
  and target payload shape.
- `subscribe` — `.lzx` view locator; hover shows resolved
  subscription contract.
- `tenant_from` — already in hover catalog (jobs/webhooks/
  notifications); extended to `channel`.

No new `@<namespace>` additions. `@trace.channel_publish` extends the
existing `@trace.*` namespace shipped in row 35 of `next-checklist.md`.

## Critério de "ciclo fechado"

The realtime bucket cycle closes when **every** box is checked for
at least one end-to-end channel + one subscription + one presence
from the (extended) fixture.

- [ ] Authored in `examples/full-capsule/` — pre-promotion: no fixture
  changes. Post-promotion: one channel + one subscription + one
  presence + one `subscribe` locator land in the fixture.
- [ ] `lazuli check` accepts the syntax.
- [ ] `lazuli inspect --expand=channels` / `--expand=subscriptions`
  / `--expand=presences` reports the full IR shape.
- [ ] `lazuli doctor` runs at least the twelve new diagnostics
  against typed IR.
- [ ] `lazuli generate` emits `dist/go/<feature>/channels.gen.go`,
  `subscriptions.gen.go`, `presences.gen.go` and the composed boot;
  view codegen emits `useSubscription` hooks in `dist/web/` and
  `dist/mobile/`.
- [ ] Drusa executes one broadcast + one subscription receiving the
  message + one presence join/leave end-to-end against a Redis test
  rig, **with tenant isolation verified**.
- [ ] At least one `synctest`-backed Go test in
  `runtime/go/lazuli/realtime/redis/`, plus the cross-tenant
  isolation test.
- [ ] LSP serves hover on `subscribe customer.subscription.<n>`
  resolving to the subscription contract + channel payload schema.

## Próximo passo

1. **This proposal sits.** No code lands. Until a pilot product
   surfaces collaboration / presence / live-dashboard pressure that
   the pull pattern visibly fails, the realtime cycle remains F.
2. **Cite this proposal as the design anchor** when a pilot files
   pressure. The locator decision in `bucket-realtime-scope.md`
   binds Stage 3 ahead of time.
3. **When promoted**: land `bucket-realtime-scope.md` first (Route A
   locator), then the canonical-indent parser extension for the four
   feature kinds (mirroring Phase L Tier 3 mechanically). Then IR,
   doctor, LSP, inspect projections, codegen, runtime — in that order.
4. **Close the cycle on one channel first**: a single
   `channel customer_activity` + one `subscription activity_feed` +
   one `presence customer_activity_viewers`. Then extend to a second
   channel that exercises cross-tenant isolation under load. Then
   add adapters (NATS / Kafka) only if a pilot demands them.

Cycle-close evidence: the `bucket-realtime-cycle.golden.json` inspect
file plus a green
`cargo test -q -p lazuli_runtime --features=integration,realtime` run
against the Redis adapter + cross-tenant isolation assertion.

## Rows sugeridas para `docs/next-checklist.md`

**Do not add these rows now.** They land in `docs/next-checklist.md`
only when the realtime cycle is promoted. Documented here so the
promotion run does not re-derive them.

| Order | Cut | Status | Notes |
|-------|-----|--------|-------|
| TBD | Realtime bucket cycle — surface locator (Route A) | proposed (Cut realtime gated) | New `subscribe <subscription-ref>` view locator in `.lzx`. Three doctor diagnostics (`subscribe_ref_unknown`, `subscribe_tenant_axis_mismatch`, `subscribe_policy_unreachable`) + LSP hover + completion. Pre-requisite for the cycle's surface side. See `docs/proposals/bucket-realtime-scope.md`. |
| TBD | Realtime IR lift (parser + IR) | proposed (Cut realtime gated) | Extend `parse_feature_skeleton` with `parse_channel` / `parse_presence` / `parse_subscription`; add `BroadcastSpec` child to `Command` / `Event`; new IR structs `Channel`, `Presence`, `Subscription`; new `AppCapability::Realtime`; new `AppRuntimeUnit serves channels *`. New `--expand=channels` / `--expand=subscriptions` / `--expand=presences` projections. See `docs/proposals/bucket-realtime-cycle.md` §IR. |
| TBD | Realtime bucket cycle — L0→L2 closure | proposed (Cut realtime gated) | Twelve new IR-driven diagnostics (`CHANNEL-PAYLOAD-001/002`, `CHANNEL-TENANT-001`, `CHANNEL-POLICY-001`, `PRESENCE-HEARTBEAT-001`, `PRESENCE-CHANNEL-001/002`, `SUBSCRIPTION-CHANNEL-001`, `SUBSCRIPTION-POLICY-001`, `SUBSCRIPTION-FILTER-001`, `SUBSCRIPTION-RATE-001`, `APP-REALTIME-001`) + codegen for `dist/go/<feature>/{channels,subscriptions,presences}.gen.go` + view codegen `useSubscription` hook + Drusa subpackage `runtime/go/lazuli/realtime` + Redis pub/sub as primary adapter + WebSocket/SSE edge. **Cross-tenant isolation test gates promotion.** See `docs/proposals/bucket-realtime-cycle.md`. |

## PT-BR summary (para o time)

Realtime continua **F (Cut realtime gated)**: nada implementa até
pilot. Esta proposta é o desenho cold-storage para quando o pilot
chegar.

**O que muda no `.lzi`**: 4 kinds novos — `channel`, `presence`,
`broadcast` (child de command/event), `subscription`. Tenant-scoped,
policy-gated, payload tipado. Sem keywords de provider (Redis/NATS/
WebSocket/SSE ficam em adapter). Cardinalidade derivada de
`tenant_from` + `filter`, não declarada.

**O que muda no `.lzx`**: 1 locator novo — `subscribe <ref>`, irmão
de `source`. Bind para `subscription` kind. Side-quest resolvida em
`bucket-realtime-scope.md` (Route A — locator próprio, não modifier
em `source`).

**O que muda no `app.lzi`/`registry.lzi`**: `runtime unit realtime
serves channels *` + `capabilities realtime <nome>` + binding via
`integrations`. Encaixa no padrão dos outros buckets.

**IR**: 5 structs novas (`Channel`, `Presence`, `Subscription`,
`BroadcastSpec`, `ChannelAuditSpec`); 12 diagnostics; 3 projections
novos no `inspect`. Mecânico — segue padrão do bucket jobs.

**Drusa**: subpackage `runtime/go/lazuli/realtime/` com adapter
contract; Redis pub/sub primário; WS/SSE edge com fallback
automático. Cross-tenant isolation é o teste load-bearing — sem ele
não fecha o ciclo.

**Side-quest documentada**: `bucket-realtime-scope.md` resolve a
pressão de surface (push read sobre views) antes que o cycle
implementation precise improvisar. Sem ele, realtime cai em
`block @client.*` e o cycle regresa.

**Quando promover**: pilot com colaboração / presence / live-dashboard
que falhe visivelmente no pattern pull+invalidates. Antes disso, a
proposta fica como design archaeology. Cita esta proposta quando a
pressão chegar.
