# Realtime Surface-Locator Scope (pre-design)

**Status**: pre-design investigation. Resolves a side-quest discovered while
drafting `bucket-realtime-cycle.md`: realtime adds **push-based read pressure**
on `.lzx` surfaces, and the existing view locator catalog (`source`, `submit`,
`action`, `block`) is pull-only. Stage 3 of the realtime cycle cannot land
without a locator decision.

**Audience**: language team, surface (`.lzx`) maintainers, anyone touching the
realtime bucket cycle.

**Date**: 2026-05-11.

**Cut**: F — Cut realtime gated. This scope-out is **design only**; nothing
implements until pilot evidence justifies the realtime bucket (per
`docs/roadmap.md:454` / `docs/audit/framework-coverage-1400.md:288`).

## Context

The realtime cycle proposes four new feature-level kinds — `channel`,
`presence`, `broadcast`, `subscription` (see `bucket-realtime-cycle.md`
§Linguagem). Three of them live cleanly inside `feature <name>` blocks
(`.lzi`) and parallel the existing `event` / `job` / `notification` shape:
they declare a contract, doctor cross-checks tenant / policy / channel
binding, codegen emits Lazuli Go registry calls.

The fourth — `subscription` — is different. A subscription is not a
contract a feature *declares*; it is a contract a **view consumes**. The
shape mirrors `source customer.query.by_id(id: route.id)` (pull read) but
on a push channel. There is no view locator today that expresses this.
Adding the kind to `.lzi` is necessary but not sufficient; the view in
`.lzx` needs a locator to bind to it.

If we land `subscription <name>` in `.lzi` without resolving the surface
binding, the surface side falls through to a `block @client.<custom>`
escape hatch and every realtime feature accretes hand-written client glue.
That is exactly the cliff the bucket-piloto strategy
(`docs/roadmap.md:23-45`) was designed to prevent.

## Why the existing locators don't cover this

The `.lzx` view locator catalog as it stands (every form authored in
`examples/full-capsule/full-capsule.lzx`):

| Locator | Direction | Example | Semantics |
|---|---|---|---|
| `source <query-ref>` | Pull, request/response | `source customer.query.list` (`full-capsule.lzx:59`) | View reads from a `query`; codegen wires React Query / Expo SWR; invalidation is event-driven via `invalidates query.X` in commands. |
| `submit <command-ref>` | Push, request/response | `submit customer.command.capture_lead` (`full-capsule.lzx:85`) | View renders a form that dispatches a command. |
| `action <name> -> <ref>` | Push, navigation/dispatch | `action reassign -> customer.command.reassign(id: route.id)` (`full-capsule.lzx:69`) | View renders a button/link that invokes a command or transition. |
| `block <client-ref>` | Embed, opaque | `block @client.activity_timeline` (`full-capsule.lzx:67`) | View embeds an author-written client component (escape hatch). |
| `opens <ref>` | Navigation | `opens detail(id: row.id)` (`full-capsule.lzx:60`) | Row click navigates to another view. |

Realtime needs a **subscribe** locator: the view holds an open channel,
the server pushes events on it, the view re-renders. None of the five
above expresses that. `source` is the closest (read intent), but its
contract is request/response with cache invalidation — not a live socket.

Trying to overload `source` is a polysemy trap. `source customer.query.list`
already means "fetch and cache with React Query"; adding "and also keep a
WebSocket open" silently to the same locator is exactly the magic-discovery
violation listed in the language boundaries (skill rules §"Magic discovery
requires visibility"). An LLM cold-reading the surface cannot infer which
behaviour applies.

## Routes A vs B vs C

Three ways to resolve the surface-binding gap. Each is design-only —
implementation gates on the realtime cycle promotion.

### Route A — new `subscribe` locator (sibling of `source`)

Add `subscribe <subscription-ref>` to the view locator catalog. Mirrors
`source` in syntax but binds to a `subscription <name>` kind, not a
`query`.

```
view live_inbox
  route id: Customer.ID
  source customer.query.by_id(id: route.id)          # initial load
  subscribe customer.subscription.activity_feed(customer_id: route.id)
  block @client.activity_timeline
```

The view holds two bindings: `source` for the initial snapshot (HTTP),
`subscribe` for the live stream (WebSocket/SSE). Codegen materializes
both. Closed catalog — no `subscribe ./path.go` escape.

**Cost**: one new locator in the `.lzx` grammar (`ViewLocator::Subscribe`),
one new IR field on `InspectView` (`subscriptions: Vec<InspectSubscription>`),
one doctor cross-check (subscription ref resolves), LSP hover + completion.
Mechanical extension of the existing locator family.

**Value**: every realtime view has a verbatim shape. The split between
"initial snapshot" and "live updates" is explicit in source — an LLM
authoring this gets both right or fails fast.

### Route B — `source` with `live` modifier

Promote `source` to accept a `live` decorator. The query becomes
subscription-aware; codegen picks pull vs push based on the modifier.

```
view live_inbox
  route id: Customer.ID
  source customer.query.by_id(id: route.id) live customer.subscription.activity_feed
  block @client.activity_timeline
```

**Cost**: smaller patch (one modifier on existing locator), but the
locator's grammar becomes ambiguous — `source X live Y` reads as "X, but
also Y" without explaining the relationship. The `live` modifier is
non-trivial: it carries its own bindings (which subscription, with what
args), so the modifier surface grows wider than a simple flag.

**Value**: discourages views that subscribe without an initial pull.
Forces the author to declare both halves in one line, which is
ergonomically pleasant — but the cost is that the line gets dense and
the polysemy on `source` widens.

### Route C — `subscription` lowering only; surface uses `block @client.<x>`

Ship the four new `.lzi` kinds (`channel`, `presence`, `broadcast`,
`subscription`) but do **not** add a surface locator. Views that need
realtime use the existing `block @client.<name>` escape hatch and the
client component opens the socket itself.

```
view live_inbox
  route id: Customer.ID
  source customer.query.by_id(id: route.id)
  block @client.activity_timeline   # opens the subscription internally
```

**Cost**: zero new locator. The author still gets typed `subscription`
declarations in `.lzi` (doctor checks tenant/policy), but the view
half is opaque.

**Value**: minimum surface; **regresses the AI-first bar**. The view
no longer self-explains what it subscribes to; an LLM cold-reading the
view cannot trace the live data flow without opening the client
component file. This is the same failure mode that justified
`source <query-ref>` in the first place — we did not leave reads to
`@client.*` then, and we should not leave subscriptions there now.

### Comparison

| Axis | Route A (locator) | Route B (modifier) | Route C (escape only) |
|---|---|---|---|
| Upfront cost | ~1 cell of grammar + IR + doctor + LSP. Mechanical. | ~0.5 cell, but the modifier grammar accretes faster than a sibling locator. | Zero on `.lzx`. |
| Polysemy risk | None — `subscribe` is new, has one meaning. | High — `source X live Y` widens `source`'s meaning. | None on `.lzx`; pushes opacity into `@client.*` instead. |
| AI-first cold-read | Best: two locators with two intents, both declarative. | OK: one line, but the relationship between `X` and `Y` is implicit. | Worst: client component owns the contract, not the view. |
| Cross-feature doctor | `subscribe` ref resolves to `subscription` kind, tenant axis matches, policy reachability checked — all from IR. | Same checks possible, but the modifier grammar makes the IR shape larger (one locator with two refs). | Cross-check is impossible — subscription ref lives inside `@client.*` opaque body. |
| Boundary with the runtime | Clean. View declares the binding; runtime materializes the socket. | Same as A, modulo the grammar friction. | Subscription declaration in `.lzi` is detached from its surface consumer; doctor cannot trace "this subscription is reachable from this audience". |
| Escape hatch survival | `block @client.*` still works for non-Lazuli realtime needs (e.g., third-party chat widget). | Same. | This *is* the escape hatch; no upgrade path. |

### Recommendation

**Route A.** The locator family already has the right shape — one locator
per intent, closed catalog, no modifiers — and realtime is a new intent
that earns its own locator. Route B compresses two intents into one line
at the cost of locator polysemy that the language has elsewhere paid down
(see `docs/design-decisions.md` on dual-form resolution). Route C is a
regression of the locator family back to opaque client components and
should be rejected.

The same boundary discipline as the auth-lowering decision applies:
realtime extends typed `.lzx` directly, not text-pattern client glue.

## Pilot-needed vs Speculative

The realtime cycle's locator surface is small. Distinguish what would
land with Route A from what waits for pilot pressure.

### PILOT-NEEDED — required if Route A lands

| Construct | Justification |
|---|---|
| `subscribe <subscription-ref>` locator | The whole point of Route A. Without it the cycle has no surface binding. |
| `route <name>: <Type>` slots bind into subscribe args | Mirrors `source customer.query.by_id(id: route.id)`. The grammar reuses the locator-arg shape; nothing new. |
| Doctor: `subscribe_ref_unknown` | The named `subscription` must exist in an imported feature. |
| Doctor: `subscribe_tenant_axis_mismatch` | The subscription's tenant axis must match the view's audience's tenant axis. (Realtime leaking cross-tenant is the single worst failure mode.) |
| Doctor: `subscribe_policy_unreachable` | Same lattice check as `source` — the view's audience must satisfy the subscription's `policy`. |
| LSP: hover on `subscribe` shows subscription contract (channel, payload type, policy, tenant axis). | Same hover quality as `source`. |
| LSP: completion on `subscribe ` suggests subscriptions reachable from the view's audience. | Same completion as `source`. |

### SPECULATIVE — defer to pilot

| Construct | Why defer |
|---|---|
| `subscribe ... reconnect <strategy>` per-view reconnect tuning | Reconnect policy belongs to the runtime adapter, not the surface contract. The view declares *what* it subscribes to; the runtime decides *how* to reconnect. |
| `subscribe ... batch_window "<duration>"` UI-side batching | Adapter / runtime concern. If a pilot proves view-level batching has different semantics from adapter-level batching, promote then. |
| `presence` locator on view (separate from `subscribe`) | Today `presence` is a feature-level kind; surfaces could just `subscribe` to it via the channel it broadcasts. Pilot needed to prove separate locator pays off. |
| `broadcast` locator (view emits into channel) | A view that broadcasts is conceptually a `submit <command>` whose command's body issues a `broadcast`. The `submit` locator already covers this — no new surface needed. |
| `subscribe ... debounce "<duration>"` UI render gating | Pure UI ergonomics. Belongs to the React/Expo adapter codegen layer if anywhere. |

The pilot-needed subset is exactly what one realtime view in a pilot
product would author. Speculative additions wait for collaboration /
presence / live-dashboard product pressure that has not arrived.

## Closed-cycle criterion for the surface-locator side

Adapted from the bucket-piloto checklist
(`docs/roadmap.md:34-43`) to the locator-specific deliverable:

- [ ] **Fixture authors the locator.** When the realtime cycle is
  promoted, `examples/full-capsule/full-capsule.lzi` declares at least one
  `subscription <name>` and `examples/full-capsule/full-capsule.lzx`
  authors a `subscribe customer.subscription.<name>(...)` locator inside
  a `view`. Pre-promotion: no fixture changes.
- [ ] **`lazuli check` accepts the syntax.** The `.lzx` parser
  (`crates/lazuli_syntax/src/parser.rs` view locator slice) recognises
  `subscribe <ref>(args)`.
- [ ] **`lazuli inspect --expand=views` projects subscriptions.**
  `InspectView.subscriptions: Vec<InspectSubscription>` reuses the
  existing locator-arg shape; codegen has typed input.
- [ ] **`lazuli doctor` carries ≥3 cross-feature diagnostics.**
  `subscribe_ref_unknown`, `subscribe_tenant_axis_mismatch`,
  `subscribe_policy_unreachable`. Concrete codes mirroring the existing
  `source_*` family.
- [ ] **`lazuli generate` produces React/Expo binding.** Out-of-scope for
  language; runtime-team deliverable. Lazuli stops at typed inspect
  output.
- [ ] **LSP hover + completion on `subscribe`.** Same shape as `source`.

The first four items are language-team Stage 3 deliverables (gated on
realtime cycle promotion). Items 5 is runtime-team.

## Recommendation

1. **Take Route A** (new `subscribe` locator sibling of `source`). The
   locator catalog already has the right shape; realtime is a new
   intent that earns its own locator without polysemy on `source`.
2. **Scope Stage 3 design to the PILOT-NEEDED subset only.** One new
   locator, three doctor diagnostics, one LSP hover, one completion. No
   modifiers, no per-view reconnect/batch knobs.
3. **Defer SPECULATIVE additions** (per-view reconnect, batch_window,
   debounce, separate `presence`/`broadcast` locators) until the realtime
   cycle surfaces real pilot pressure.
4. **Land this scope-out only when the realtime cycle is promoted.**
   Until then, the proposal sits next to `bucket-realtime-cycle.md` as
   design archaeology so a future pilot does not relitigate the locator
   decision.
5. **Do not edit `docs/next-checklist.md`** as part of this proposal.
   Phase L Tier 4 (row 24) does not depend on this. The realtime cycle
   row lands in `next-checklist.md` only at pilot promotion.

When the realtime cycle is promoted, Stage 3 (design-language) runs on
the shipped substrate from `bucket-realtime-cycle.md` and produces the
focused proposal covering at most the three doctor diagnostics named in
the closed-cycle criterion plus the `--expand=views` subscription
projection. Stage 4 (Lazuli Go codegen) then has a stable IR JSON to consume.
