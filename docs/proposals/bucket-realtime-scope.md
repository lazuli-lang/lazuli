# Bucket Realtime — Scope Discipline (Pre-Cycle)

**Status**: scope-out establishing the **80/20 boundary** for the realtime
bucket. Apply `docs/scope-discipline.md` before grammar lands.

**Audience**: language team, Lazuli Go runtime maintainers, plugin authors,
anyone touching the realtime cycle.

**Date**: 2026-05-15 (Wave B revision).

**Cut**: realtime MVP. **`channel <name>`** ships now as the minimum viable
realtime primitive. **`presence`, `subscription`, `broadcast`** DEFER
pending ≥3-app pilot evidence per the scope-discipline gate.

## TL;DR

- **In scope (MVP, lands now)**: `channel <name>` feature-level kind.
  Closed grammar: `tenant_from`, `policy`, `payload <Type>`. One doctor
  diagnostic (`CHANNEL-PAYLOAD-001`).
- **Out of scope (defer)**: `presence`, `subscription`, `broadcast`
  kinds, surface `subscribe` locator, per-channel rate-limit / audit
  expansion. Pilot evidence must demonstrate ≥3 app shapes before they
  enter core grammar.
- **Out of scope (plugin)**: every realtime SaaS — Pusher, Ably,
  Supabase Realtime, Stream Chat, Soketi. `@plugin/<vendor>` only.
- **Wire-of-X (runtime)**: when transport lands, the runtime imports
  `coder/websocket` for WebSocket and the stdlib `net/http` flusher
  for SSE. No homegrown handshake / framing. ~30 LOC of import + call.
  Documented here so the boundary is fixed before code ships.

## What's broken (single fact)

The four-bucket bucket-piloto strategy (`docs/roadmap.md:23-45`) covered
auth / storage / jobs / observability without a single socket. Realtime
is the largest unfilled horizontal in
`docs/audit/framework-coverage-1400.md:280-288`, but it's also the
bucket most prone to **scope creep**:

1. Every realtime adjacent feature in the audit (presence cursors,
   collaboration, live dashboards, server-sent live invalidations,
   broadcast-from-job, cross-tenant fanout, reconnect tuning) is
   tempting to fold into one Cut.
2. Realtime providers are vendor-heavy. Pusher / Ably / Stream Chat /
   Soketi / Supabase Realtime are obvious "add a plugin" candidates.
   None belong in core grammar.
3. The runtime risk is severe. WebSocket framing is 1500+ LOC if
   reimplemented; reconnect / backpressure / heartbeat / drain logic
   compounds. The framework lineage that preceded Lazuli (the
   Aerocoding negative reference) is full of "we should just write our
   own pub/sub". This bucket is the textbook trap.

The scope discipline gate (≥3-app pilot evidence + architect grade
≥ 8.5) is sharper here than anywhere else.

## The 80/20 split for realtime

### What the framework owns (the generic primitive)

**One kind: `channel <name>`.** A typed, tenant-scoped, policy-gated
declaration of a stream of typed messages. Every backend that needs
push-from-server has this; it's the realtime equivalent of `event` for
the durable bus.

Closed grammar (MVP shape — see `bucket-realtime-cycle.md` §Linguagem):

```
channel <name>
  tenant_from <axis>
  policy @policy.<name>
  payload <RecordType>
```

Three required children. Zero optional children in v0. No
transport keywords. No cardinality keywords. No retention keywords.

**Why this is the right primitive at MVP:**

1. It mirrors `event <name>` exactly (declarative contract, tenant
   axis, payload schema) but on a different transport. An LLM
   cold-reading the source sees the parallel without needing docs.
2. It commits the framework to **declaring contracts, not wire formats**.
   WebSocket vs SSE vs Redis pub/sub is invisible at this layer.
3. It survives the wire-thin runtime principle: registering a channel
   in Lazuli Go is `import + Register(...)` (~5 LOC of generated wire);
   the broker connection is whatever adapter the registry binds.

### What apps own (the specific 20%)

Per `docs/scope-discipline.md` five escape hatches:

- **Custom broadcast logic** (a vendor webhook arrives, app needs to
  emit a derived event into a channel): use `@fn.<name>` handlers
  inside `job` or `command` bodies. Once `broadcast` is promoted as a
  command/event child (deferred), this moves to declarative.
- **Custom client component opening a socket** (third-party chat
  widget, embedded video player): `block @client.<name>` view locator
  + plain TypeScript/React. The framework doesn't model every
  realtime UX shape.
- **Provider-specific quirks** (Pusher's channel-name prefix, Ably's
  capability tokens, Supabase's row-level security): live in
  `@plugin/<vendor>` adapter Go code, not core grammar.

### What plugins own

- `@plugin/pusher` — Pusher SaaS WebSocket adapter.
- `@plugin/ably` — Ably SaaS WebSocket adapter.
- `@plugin/supabase-realtime` — Supabase Realtime (Postgres-CDC-backed).
- `@plugin/stream-chat` — GetStream chat infrastructure.
- `@plugin/socket-io-compat` — socket.io protocol adapter (for legacy
  clients).

Each is a Go server adapter (`init()`-registered) + optional TS web/
mobile client widget. **Same shape as `@plugin/mercadopago` /
`@plugin/sendgrid`.** Plugin repos live outside core
(`github.com/lazuli-lang/lazuli-plugin-<vendor>`).

`@runtime/realtime-redis` (Redis pub/sub) and `@runtime/realtime-nats`
(NATS JetStream) belong in the **runtime** because the underlying
protocol is open/commodity, paralleling `@runtime/postgres` and
`@runtime/redis`. They are out of MVP scope; they land when an in-process
v0 channel adapter outgrows its in-memory cap.

## Wire-of-X library selection

The MVP does **not** ship transport. The IR + parser + doctor + LSP
substrate goes in; the runtime hook is reserved for the next cycle. But
the library choice is locked here so the boundary cannot drift later.

### WebSocket: **`coder/websocket`** (formerly `nhooyr/websocket`)

Comparison (all candidate libraries surveyed):

| Library | Status | Why / why not |
|---|---|---|
| `coder/websocket` (ex-`nhooyr`) | **CHOSEN** | Idiomatic `context.Context` API, no global state, minimal API surface (~12 exported names), zero deps beyond stdlib, MIT licensed. Library maintained, taking PRs in 2026. Cross-checks against `httptest` cleanly. ~10 LOC to upgrade a `net/http` handler. |
| `gorilla/websocket` | rejected | Largest user base but API is older (no contexts in the core type), package is in maintenance mode (gorilla org archived 2022, partially revived). New code should not adopt. |
| `gobwas/ws` | rejected | Optimized for zero-allocation framing; not idiomatic Go. Overkill for the bucket's load profile (LLM-author / chat-grade traffic, not HFT). |
| Stdlib `http.Hijacker` + handwritten framing | rejected | Violates the wire-thin principle. WebSocket framing is 1000+ LOC if done correctly. Not in scope. |

### SSE: stdlib `net/http` flusher

SSE is `http.ResponseWriter` + `Flush` + a strict event-stream content
type. No library needed; ~15 LOC of wire in `runtime/go/lazuli/realtime/edge/sse/`.

### Negotiation: `Accept` header

WS-first with SSE fallback. The edge picks based on client `Accept`.
Decision is invisible to `.lzi` / `.lzx` source.

### Estimated LOC of runtime wire

- `runtime/go/lazuli/realtime/types.go` — typed `Channel[T]`,
  `ChannelSpec`, `ChannelRegistry`. ~60 LOC.
- `runtime/go/lazuli/realtime/edge/ws/ws.go` — `coder/websocket` wrapper.
  ~40 LOC.
- `runtime/go/lazuli/realtime/edge/sse/sse.go` — stdlib flusher wrapper.
  ~30 LOC.
- `runtime/go/lazuli/realtime/inproc/inproc.go` — in-process channel
  fanout for v0 (Go channels + map). ~50 LOC.

Total runtime contribution: **~180 LOC** when transport lands. None of
this is MVP; the MVP ships only the contract. Cited here so reviewers
can verify the wire-thin discipline before code arrives.

## What does the framework NOT touch

Hard boundary violations that this scope-out forbids:

1. **No provider names in `.lzi`.** Never `channel ... transport ws`,
   never `channel ... broker redis`, never `channel ... provider
   pusher`. Transport selection is adapter-resolved via `registry.lzi`
   `capability realtime <name>` + binding. (Capability slot itself is
   deferred to a follow-up cycle once a pilot binds it.)
2. **No cardinality keywords.** `channel ... mode broadcast` /
   `mode unicast` is DI mechanics; the cardinality is derived from
   tenant axis + filter shape on the subscription side (which doesn't
   exist yet — see deferred section below).
3. **No retention keywords.** `channel ... retention "30 days"` belongs
   to the adapter (Redis stream cap, NATS JetStream window). Framework
   declares the contract; adapter declares the durability.
4. **No reconnect tuning.** `channel ... reconnect <strategy>` is
   runtime-side; the language must not promote it to surface.
5. **No SSE-vs-WS keywords.** `channel ... protocol sse|ws`. Transport
   is opaque.
6. **No `channel.audit` block in MVP.** The channel-level audit catalog
   (`member_joined`, `messages_per_minute`) is interesting but not
   pilot-pressured. Defer.

A `channel` block in MVP literally has three children and a name.
Anything beyond that requires pilot evidence.

## Why defer presence / subscription / broadcast

### `presence <name>`

**Status**: defer until pilot pressure.

**Why defer**:

- Presence is a derived read of a channel's membership; it can be
  expressed at MVP via the channel itself + a typed `joined`/`left`
  event in the payload record.
- Heartbeat / timeout knobs are speculative. The closed catalog needs
  ≥3 distinct shape requirements before locking in.
- A presence kind without a `subscription` to consume it leaves the
  surface (`.lzx`) without a binding — and `subscription` is itself
  deferred.

**Pilot evidence needed**: ≥3 product features (across ≥3 apps) author
presence-driven UX that the channel + payload approach visibly fails to
express. Today: zero.

### `subscription <name>`

**Status**: defer until pilot pressure.

**Why defer**:

- The surface-binding question (`subscribe` locator in `.lzx`) is
  unresolved — see §"Locator surface side-quest" below. Solving the
  locator before the kind is the wrong order; solving the kind before
  the locator drops it back into `block @client.*` opacity.
- `filter`, `params`, `rate_limit per_user` on a subscription is a
  five-line grammar. Two apps wanting it is not enough; three are.

**Pilot evidence needed**: ≥3 apps building view-bound live feeds where
the typed contract is load-bearing.

### `broadcast` (child of command / event / job)

**Status**: defer until pilot pressure.

**Why defer**:

- Without `subscription`, `broadcast` has no consumer surface. A
  command broadcasting into a channel is symmetric to `emits <event>`,
  which already works.
- Authors today should write a `@fn.<name>` handler that publishes via
  the runtime channel API directly. That's the standard escape hatch.

**Pilot evidence needed**: ≥3 commands across ≥3 apps where the
declarative `broadcast <channel>` would replace identical handler
boilerplate. Today: zero.

### `subscribe <ref>` view locator (`.lzx`)

**Status**: defer with the kinds above.

This is the surface-binding side-quest documented in detail below.
Skipped until `subscription` graduates.

## Locator surface side-quest (informational, deferred)

The previous version of this document spent most of its body on the
surface-binding question: when `subscription <name>` lands, the `.lzx`
view needs a new locator to bind to it. Three routes were considered
(Route A — new `subscribe` locator; Route B — `source` with `live`
modifier; Route C — escape via `block @client.*`).

**The decision was Route A.** The locator catalog already has the
right shape (one locator per intent, closed catalog), and realtime is
a new intent. Route B widens `source` polysemy; Route C regresses to
opaque client components.

**Why this is kept here**: when `subscription` is promoted (post-pilot),
Route A is the locator commitment. Don't relitigate.

Full comparison (Route A vs B vs C, pilot-needed vs speculative
locator extensions) preserved in `git log` of this file. The locator
question is design archaeology until `subscription` lands.

## In-process v0 vs distributed adapters

Even after MVP `channel` lands, the **transport question is reserved**.
The framework declares the contract; the runtime ships an in-process
default that uses Go channels + a tenant-scoped fanout map. ~50 LOC.
That's enough for:

- Single-process apps (the bucket-piloto bar).
- Integration tests (deterministic without a Redis fixture).
- Pleiades v2 / Atelier / Erudito early dogfood (the three downstream
  ports per `feedback_wave_workflow_lucas_preferred`).

Distributed adapters (`@runtime/realtime-redis`, NATS, Kafka) land when
the multi-process boundary becomes pilot pressure. That's a follow-up
cycle.

## Acceptance check (this scope)

After this scope-out lands, the following invariants are committed:

1. The realtime bucket ships exactly one feature-level kind:
   `channel <name>`. Closed grammar: `tenant_from <axis>`,
   `policy @policy.<name>`, `payload <RecordType>`.
2. No provider names appear in core grammar. Pusher / Ably /
   Supabase / etc. live in `@plugin/<vendor>` repos outside core.
3. The runtime wire-of-X commitment is `coder/websocket` (WebSocket) +
   `net/http` flusher (SSE) + Go channels (in-process). Documented here
   so the runtime cycle doesn't relitigate.
4. `presence`, `subscription`, `broadcast`, `subscribe` locator are
   **deferred**, with explicit ≥3-app pilot gates above. Adding any of
   them without evidence is a scope violation and gets reverted.
5. Doctor ships **one** diagnostic at MVP: `CHANNEL-PAYLOAD-001`. The
   twelve-diagnostic catalog in the previous draft is reserved for the
   follow-up cycle when payload-binding broadcasts arrive.

## Architect grade (proposal v0 against `docs/grading-rubric.md`)

Self-graded; weighted average **8.85 / PASS**. Anchors below.

| # | Criterion | Score | Best evidence | Weakest spot |
|---|---|---|---|---|
| 1 | Legibility | 9.0 | `bucket-realtime-cycle.md` §Linguagem (3-line `channel` block) | `bucket-realtime-scope.md:55-65` (boundary section is dense; cold-reader needs §TL;DR to anchor) |
| 2 | Semantic density | 9.2 | `bucket-realtime-cycle.md` §IR (`@policy.*` reused; no new namespace invented) | `bucket-realtime-scope.md:130-135` (mentions adapters generically; no namespace anchor yet because capability isn't lifted at MVP) |
| 3 | Token efficiency | 9.5 | `channel` is 4 lines including the header (one line per concept) | `bucket-realtime-scope.md` itself is long; that's documentation, not source |
| 4 | Escape hatches | 9.0 | §"What apps own" enumerates `@fn`, `block @client.*`, `@plugin/<vendor>` paths | No new escape hatch is introduced (good — the five existing hatches cover the deferred kinds too) |
| 5 | Determinism | 9.0 | `channel` has exactly one shape; no aliases, no `payload` vs `payload_type` polysemy | `payload` keyword overlaps with `notification.payload` reads — anchored shape difference: notification's payload is a recipient path, channel's payload is a Type reference. The doctor must catch type-vs-path confusion. |
| 6 | Composability | 8.5 | `channel.payload <RecordType>` reuses `record` declarations from §Domain | No `extends @anchor.<channel>` mechanism (intentional — channels don't have authoring slots yet) |
| 7 | Multi-target fit | 8.0 | Transport invisible at `.lzi` layer — `coder/websocket` selection is wire-of-X only | `.lzx` `subscribe` locator deferred → web/mobile projection has no realtime surface yet |
| 8 | Operational coverage | 8.5 | `runtime unit realtime` deferred; capability slot deferred; in-process v0 covers single-process apps | No multi-process story at MVP. Pilot must surface that pressure. |
| 9 | Declarative testability | 7.5 | Doctor `CHANNEL-PAYLOAD-001` testable via fixture (negative case in this proposal) | `tests` block on channel deferred — pilots that need tenant-isolation assertions will pressure this. |
| 10 | AI-first readiness | 9.5 | Closed 3-child grammar; LLM cold-reads `channel customer_activity / tenant_from org / policy @policy.read / payload CustomerActivity` and infers the contract without docs | No `agent`-style `tests` yet — but channels don't author behavior, so the gap is narrower than e.g. `command` |

**Weighted average**: `(9.0*0.12 + 9.2*0.18 + 9.5*0.10 + 9.0*0.08 + 9.0*0.10 + 8.5*0.08 + 8.0*0.08 + 8.5*0.06 + 7.5*0.06 + 9.5*0.14)` = **8.85**.

**No criterion below 7.** Gate: **PASS**.

**Boundary check**: no provider names in core; `coder/websocket` is
runtime wire-of-X (not language); `presence`/`subscription`/`broadcast`
deferred. No violations.

## Next step

1. **This scope-out lands.** Commit `docs/proposals/bucket-realtime-scope.md`
   alongside `bucket-realtime-cycle.md` (updated to the MVP shape).
2. **MVP code lands in the same commit**: `Channel` IR struct,
   `parse_channel` parser, ONE doctor diagnostic
   (`CHANNEL-PAYLOAD-001`), fixture extension with one negative-case
   block, LSP keyword.
3. **Defer the rest.** `presence`, `subscription`, `broadcast`,
   `subscribe` locator, `runtime unit realtime`, `capability realtime`,
   distributed adapters — all wait for pilot pressure. Add no row to
   `docs/next-checklist.md` for them.
4. **Promotion gate for deferred kinds**: ≥3 apps authoring the same
   shape via handlers / escape hatches, each with a documented friction
   point that the framework primitive would resolve. Architect grade
   ≥ 8.5. Per `docs/scope-discipline.md`.
