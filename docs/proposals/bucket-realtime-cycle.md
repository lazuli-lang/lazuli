# Bucket Cycle: Realtime (MVP — `channel` kind)

**Date**: 2026-05-15 (Wave B revision).

**Status**: **active**. MVP `channel <name>` kind lands with this
proposal. Pre-requisite: `docs/proposals/bucket-realtime-scope.md`
fixes the 80/20 boundary (channel = framework; presence /
subscription / broadcast = deferred plugins or future cycle pending
pilot evidence).

## Contexto

Realtime is the §1.18 bucket from `docs/roadmap.md:454` and the largest
unfilled horizontal in
`docs/audit/framework-coverage-1400.md:280-288`. The four bucket-piloto
cycles (auth / storage / jobs / observability) shipped without a single
socket, validating the scope-discipline gate.

This cycle ships **only the minimum viable primitive**: `channel <name>`
as a typed, tenant-scoped, policy-gated declaration of a stream of
typed messages. It is to push transport what `event <name>` is to the
durable bus.

Three kinds (`presence`, `subscription`, `broadcast`) and one surface
locator (`subscribe` in `.lzx`) explored in the previous draft are
**deferred** pending ≥3-app pilot evidence per
`docs/scope-discipline.md`. The detailed analysis is preserved in
`bucket-realtime-scope.md` so the future promotion run does not
relitigate.

## Linguagem proposta (MVP)

One new feature-level kind. Closed grammar. Three required children.
Zero optional children in v0.

### `channel <name>` (feature-level kind)

```
feature customer
  uses org

  channel customer_activity
    tenant_from org
    policy @policy.read
    payload CustomerActivityEvent
```

Required children:

- `tenant_from <axis>` — tenancy axis the channel scopes by. Mirrors
  `job.tenant_from`, `webhook.tenant_from`,
  `notification.tenant_from`. The axis must resolve to a `defaults
  tenancy <axis>` reachable via `uses`. Doctor cross-checks via the
  existing tenant-axis lattice (`tenant_axis_diagnostics`); the
  channel inherits that infrastructure for free.

- `policy @policy.<name>` — read policy. Subscribers must satisfy
  this; broadcasters (once `broadcast` graduates) must satisfy ≥
  this. Same `PolicyRef` shape as commands / jobs / webhooks.

- `payload <RecordType>` — name of a typed value-record (`record`)
  or `resource` declared in this feature or an imported feature
  reachable via `uses`. This is the load-bearing axis the MVP doctor
  enforces: when the type doesn't resolve, `CHANNEL-PAYLOAD-001`
  fires.

That's the entire MVP grammar.

### Anti-proposals (rejected; cited so we don't relitigate)

- **`transport ws` / `transport sse` / `transport <provider>`** —
  transport is adapter-resolved. The runtime negotiates WS vs SSE on
  `Accept` header (`docs/proposals/bucket-realtime-scope.md`
  §Wire-of-X). No keyword.
- **`broker redis` / `provider pusher`** — provider mechanics; lives
  in `@plugin/<vendor>` or `@runtime/<commodity>`. Never in core.
- **`mode broadcast` / `mode unicast`** — cardinality is derived from
  the tenant axis and the (future) subscription filter shape.
- **`retention "30 days"`** — adapter-side property. Redis stream
  cap, NATS JetStream window. The language doesn't model durability.
- **`reconnect <strategy>`** — runtime tuning. The view declares
  what to subscribe to (once subscriptions ship); the runtime decides
  how to reconnect.
- **Inline `payload` record** — the channel's payload type is a
  reference to a declared `record` or `resource`. Inline anonymous
  records would create a polysemy with notification recipients and
  break the determinism axis of the grading rubric.

## IR proposto (MVP)

One new IR struct, one extension to `Feature`. Both in
`crates/lazuli_ir/src/lib.rs`.

### `Channel` struct

```rust
/// Realtime bucket cycle MVP — `channel <name>` declaration.
///
/// Typed, tenant-scoped, policy-gated declaration of a push stream.
/// Sibling of `event` (durable bus) but on a push transport. The
/// MVP grammar locks three children: `tenant_from <axis>`,
/// `policy @policy.<name>`, `payload <RecordType>`. Optional
/// children (audit, rate_limit, presence, broadcast wiring) are
/// deferred pending pilot evidence; see
/// `docs/proposals/bucket-realtime-scope.md`.
///
/// Doctor cross-check today: `CHANNEL-PAYLOAD-001` resolves the
/// payload reference against `Feature.records` / `Feature.resources`
/// and the same-package imports. Additional diagnostics
/// (tenant axis, policy lattice) ride the existing
/// `tenant_axis_diagnostics` / `policy_lattice_diagnostics`
/// infrastructure and don't require new code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub name: String,
    /// `tenant_from <axis>` — axis name verbatim (`org`, `team`).
    /// Resolved against the feature's `defaults.tenancy` lattice.
    pub tenant_from: TenantFromSpec,
    /// `policy @policy.<name>` — read policy. Standard `PolicyRef`.
    pub policy: PolicyRef,
    /// `payload <RecordType>` — verbatim type name. Doctor resolves
    /// it against `Feature.records` / `Feature.resources` /
    /// imported features.
    pub payload: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}
```

### `Feature` extension

```rust
pub struct Feature {
    // ... existing fields ...
    /// Realtime bucket cycle MVP — `channel <name>` declarations.
    /// Sibling slot of `events` / `notifications` / `pollers`.
    /// Lifted from the canonical-indent slice; empty for
    /// pre-realtime fixtures (serde-default).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<Channel>,
}
```

Additive. Serde-default empty vec; existing on-disk JSON
deserializes without changes.

### Diagnostics added in this cycle

| Code | Severity | Trigger |
|---|---|---|
| `CHANNEL-PAYLOAD-001` | Error | `channel <name> payload <Type>` references a type that doesn't resolve to a `record` or `resource` in this feature or an imported feature reachable via `uses`. |

**One diagnostic.** Per `docs/scope-discipline.md`: ship the minimum
that proves the cycle, defer the rest. The full twelve-diagnostic
catalog explored in earlier drafts (CHANNEL-TENANT-001,
CHANNEL-POLICY-001, PRESENCE-HEARTBEAT-001, etc.) is reserved for
later cycles when the kinds those diagnostics check actually exist.

Tenant axis resolution and policy lattice checks ride the existing
diagnostic infrastructure for free — the doctor passes that consume
`Feature.events` and `Feature.jobs` find `Feature.channels` with the
same plumbing once the IR slot exists.

### JSON shape (`lazuli inspect --format=json`)

Per `InspectFeature` projection:

```json
{
  "name": "customer",
  "channels": [
    {
      "name": "customer_activity",
      "tenant_from": "org",
      "policy": "@policy.read",
      "payload": "CustomerActivityEvent",
      "origin": "channel"
    }
  ]
}
```

The CLI `--expand=channels` projection lifts this from
`Feature.channels` exactly as it lifts `--expand=notifications` from
`Feature.notifications`. That wiring is reserved for the
inspect-projection cycle (mechanical follow-up, not blocking).

## Codegen proposto (MVP)

**Out of scope for this cycle.** Codegen ships when the runtime
contract stub lands (next cycle).

When it ships, the shape is:

```go
// path: dist/go/customer/channels.gen.go
// Code generated by lazuli; DO NOT EDIT.
package customer

import "lazuli.dev/runtime/lazuli/realtime"

func RegisterChannels(r *realtime.ChannelRegistry) {
    r.Register(realtime.ChannelSpec{
        Name:       "customer.customer_activity",
        TenantAxis: "org",
        Policy:     "@policy.read",
        PayloadFn:  func() any { return CustomerActivityEvent{} },
    })
}
```

`CustomerActivityEvent` is the Go struct already generated from the
`record CustomerActivityEvent` declaration — no new codegen pathway
needed; the channel reuses the typed record output.

No provider names anywhere in generated code. `ChannelRegistry` is
runtime-side; Redis / NATS / in-process wiring resolves through the
registry adapter binding once the capability slot is lifted.

## Runtime proposto (MVP)

**Stub only.** A contract type declaration in
`runtime/go/lazuli/realtime/channel.go`. ~30 LOC including imports.
Full edge (WebSocket + SSE) lands in the next cycle.

```go
package realtime

import "context"

// Channel is the runtime-side handle to a declared channel.
// Generated code calls Register() against a ChannelRegistry; the
// registry hands out Channel handles that broadcast/subscribe code
// uses.
type Channel[T any] struct {
    Name       string
    TenantAxis string
    Policy     string
}

// ChannelSpec is the registration shape the generated code uses.
// PayloadFn returns a zero value of T so the registry can decode
// incoming events without reflection at the hot path.
type ChannelSpec struct {
    Name       string
    TenantAxis string
    Policy     string
    PayloadFn  func() any
}

// ChannelRegistry is the boot-time registry generated RegisterChannels
// functions populate. Adapters consume registered specs at Boot time.
type ChannelRegistry struct {
    specs map[string]ChannelSpec
}

func (r *ChannelRegistry) Register(spec ChannelSpec) { /* ... */ }
func (r *ChannelRegistry) Lookup(name string) (ChannelSpec, bool) { /* ... */ }
```

That's the entire MVP runtime surface. No transport, no edge, no
adapter. Per `CLAUDE.md` founding principle: ~30 LOC of contract +
wire when codegen arrives. The previous draft's 700+ LOC of adapter
contract + WS edge + SSE edge + Redis adapter is reserved for when
distributed adapters are pilot-pressured.

## Evals/Testes propostos (MVP)

### Parser tests (this cycle)

Three cases in `crates/lazuli_syntax/src/parser.rs`:

1. Parses minimal `channel` with the three required children.
2. Rejects `channel` missing `payload`.
3. Rejects `channel` with an unknown child key.

### Doctor test (this cycle)

Five cases in `crates/lazuli_cli/src/doctor/correctness/channel_payload_unresolved_001.rs`:

1. Positive: `payload UnknownType` with no record / resource by that
   name → `CHANNEL-PAYLOAD-001` fires.
2. Negative: `payload CustomerActivityEvent` with a matching `record`
   declared in `domain` → no diagnostic.
3. Negative: payload resolves to a `resource` declaration.
4. Negative: feature has no channels → rule short-circuits.
5. Positive: multiple channels; only the unresolved one fires.

### Inspect projection test

Reserved for the inspect-projection follow-up cycle. The IR slot
exists; the `--expand=channels` projection lift is mechanical.

## Doctor/LSP propostos (MVP)

LSP catalog additions (in
`crates/lazuli_lsp/src/lib.rs::hover_for_keyword`):

- `channel` — extended hover covers both the new feature-level kind
  (`channel <name>` block with required `tenant_from`, `policy`,
  `payload <RecordType>`) and the existing `notification.channel`
  delivery-list child. The two uses are disambiguated by indent level
  + parent kind.

Syntax highlighting: added `channel` to the feature-child keyword
match in `editors/vscode/syntaxes/lazuli.tmLanguage.json`.

No new namespace. `@policy.*` is the only `@*` referenced; it's
already in the closed catalog.

## Critério de "ciclo fechado" (MVP)

The MVP cycle closes when each box is checked.

- [x] **Authored in `examples/full-capsule/`.** One `channel
      customer_activity` block lands in the `customer` feature, with a
      `record CustomerActivityEvent` declared in `domain` so the
      payload resolves.
- [x] **`parse_feature_skeletons` accepts the syntax.** The parser
      slice has a `channel` arm parallel to `notification` /
      `poller` / `tenant_migration`.
- [x] **IR `Feature.channels: Vec<Channel>` populated.** Lowering
      `lower_feature_skeleton` constructs one entry per parsed
      `channel`.
- [x] **`lazuli doctor` runs `CHANNEL-PAYLOAD-001` against IR.**
      Five test cases in
      `crates/lazuli_cli/src/doctor/correctness/channel_payload_unresolved_001.rs`.
- [ ] **`lazuli inspect --expand=channels` projects the IR.**
      Reserved for the next cycle — IR slot exists, projection lift
      is mechanical.
- [ ] **Lazuli Go runtime contract stub.** Reserved for the next
      cycle. ~30 LOC of `runtime/go/lazuli/realtime/channel.go`.
- [ ] **WebSocket / SSE edge.** Reserved. `coder/websocket` library
      locked in `bucket-realtime-scope.md` §Wire-of-X.
- [ ] **Distributed adapters** (`@runtime/realtime-redis`,
      `@runtime/realtime-nats`). Reserved until in-process v0 hits
      pilot pressure.

The first four are this commit. The rest are downstream cycles with
explicit pilot gates.

## Próximo passo

1. **MVP code lands in this commit**: IR struct, parser, ONE doctor
   diagnostic, fixture extension, LSP keyword.
2. **Inspect projection** lands in a follow-up cycle. Mechanical: lift
   `Feature.channels` into `InspectFeature.channels` with the same
   shape `notifications` uses.
3. **Runtime contract stub** lands in the next runtime cycle. ~30 LOC
   of `runtime/go/lazuli/realtime/channel.go`. No edge yet.
4. **WebSocket / SSE edge + in-process adapter** land in the next
   cycle after that. `coder/websocket` library is committed in
   `bucket-realtime-scope.md`.
5. **Distributed adapters** (`@runtime/realtime-redis`, NATS) reserved
   until ≥3 apps demonstrate multi-process pressure.
6. **`presence`, `subscription`, `broadcast`, `subscribe` locator,
   `capability realtime`, `runtime unit realtime`** all reserved until
   ≥3-app pilot evidence per
   `docs/scope-discipline.md`. No row in `docs/next-checklist.md` for
   them.

Cycle-close evidence (this proposal):
`cargo test -p lazuli_syntax -p lazuli_cli` green with
`parse_channel` + `CHANNEL-PAYLOAD-001` coverage;
`cargo run -q -p lazuli_cli -- doctor examples/full-capsule` stays
green with the new `channel customer_activity` block.

## Architect grade (proposal v0 against `docs/grading-rubric.md`)

Self-graded; weighted average **8.92 / PASS**. See
`bucket-realtime-scope.md` §"Architect grade" for the anchored table —
the cycle proposal inherits the scope-out's scoring because they
commit to the same MVP shape.

**Boundary check**: no provider names; no DI mechanics in source;
deferred kinds explicitly gated on pilot evidence; transport is
adapter-resolved. No violations.

## PT-BR summary (para o time)

`channel <name>` é o único kind do bucket realtime que entra agora.
3 filhos obrigatórios (`tenant_from`, `policy`, `payload <Tipo>`),
zero opcional. 1 diagnostic (`CHANNEL-PAYLOAD-001`). MVP só. Resto
(`presence`, `subscription`, `broadcast`, `subscribe` locator)
**esperam ≥3 apps de pressão** per scope-discipline.md.

Wire-of-X locked: `coder/websocket` pra WS, stdlib pra SSE, ~50 LOC
de in-process adapter. Total runtime quando transport landa: ~180
LOC. Nenhuma linha disso no MVP — só o contrato.

Provider names ficam em `@plugin/<vendor>` (Pusher, Ably, Supabase,
StreamChat). `@runtime/realtime-redis` / `@runtime/realtime-nats`
são commodity-side e ficam pro próximo ciclo.

Nada de novo `app.lzi` / `registry.lzi` / `runtime unit` — esses
deferidos com os outros kinds. O bucket cresce só quando piloto
pressiona.
