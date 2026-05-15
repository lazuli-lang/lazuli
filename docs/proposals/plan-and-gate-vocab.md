# Plan & Gate Vocabulary (v0.1, design draft)

**Status**: design proposal. Cell #9 of
[`production-readiness.md`](production-readiness.md).
Architect grade **PASS-WITH-NOTES 8.62/10** (v0.2). Implementation
landed under cells `PG.PRE.1` → `PG.F` per the wave-2 plan-gate
phase set; see `git log --grep '^PG\\.' wave2-plan-gate`.

## v0.3 reconciliation note (2026-05-14)

- **PG.PRE.1** — gate polysemy resolved by extending `docs/invariants.md`
  with an explicit disambiguation between app-level deploy gates
  (prose) and callable-scope `gate` directives (closed grammar). The
  word `gate` is not a parser keyword anywhere else; the only on-disk
  appearance was the English phrase "deploy gates" in
  `invariants.md:15` and `lazurite.toml:501`. No surface rename.
- **PG.PRE.2** — `examples/billing.lzi` reconciled with v0.1 surface
  via PG.F: a `plans` catalog block (`free`/`pro` + `invoices_per_month`
  limit) lifts into a single `subscription resource` anchor under the
  enclosing app and the existing `command create` gains
  `gate quota plan.limit: invoices_per_month` so doctor exercises both
  the catalog union check and the post-success increment path. Existing
  `command create` semantics are unchanged: the gate runs before the
  policy check; existing rate_limit + audit + emits continue to apply.
- **PLAN-SUBSCRIPTION-TENANCY-001** promoted from §"Risks" to the v0.1
  doctor catalog per architect notes — folded into PG.B as the fifth
  PLAN-* error code alongside the four originally proposed.
- **GATE-EVAL-ORDER-001** added in PG.B as an explicit static check that
  authored `gate` directives appear before `policy` in their callable
  body so the source ordering matches §"Ordering and combinability".

**Audience**: Lazuli language team (Rust crates), Lazuli Go runtime
team, downstream product authors who ship subscription-gated SaaS
(production-grade-shaped: free/trial/pro/enterprise + per-feature flags +
per-period quotas).

**Date**: 2026-05-14.

**Pilot bucket**: new `plan` bucket — surface + IR + analyzer +
doctor + LSP + codegen + runtime contract. Sibling to `auth`,
`notifications`, `agent`, `storage`. Closes a production-grade app gap #9
(subscription / plan + feature gating).

**Companion**:
- [`docs/design-principles.md`](../design-principles.md) — Rule Zero
  ("Vocabulary Over Mechanism") and "Operational Systems First"
  cited throughout.
- [`docs/invariants.md`](../invariants.md) — closed namespace catalog,
  registry capability boundary.
- [`docs/proposals/bucket-storage-cycle.md`](bucket-storage-cycle.md)
  — structural template for L0→L2 bucket proposals.
- [`docs/proposals/auth-session-tenant-pin.md`](auth-session-tenant-pin.md)
  — adjacent authorization surface; this proposal MUST NOT overlap.
- [`docs/proposals/production-readiness.md`](production-readiness.md)
  — parent roadmap.

**First consumer**: a production-grade app port (downstream, private repo). The
canonical fixture exercise lands in
`examples/full-capsule/full-capsule.lzi` once the surface is
accepted — see §"Fixture exercise" below.

---

## Problem

Real subscription-driven SaaS bundles three concerns that today fall
through to free-form Go handlers:

1. **Plan catalog** — a closed set of tiers (`free`, `trial`, `pro`,
   `enterprise`) declared once. Each tier owns a feature set and a
   limit set.
2. **Feature gating** — at the boundary of a command/query/job/poller,
   refuse dispatch if the caller's active plan does not include the
   feature.
3. **Quota enforcement** — for callables that are metered, refuse
   dispatch if the caller has already consumed their period budget;
   when they pass, increment usage.

Today these live in scattered TypeScript:

```ts
// apps/api/src/features/auth/session.service.ts:253-311 (a production-grade app)
export async function checkSubscription(userId): Promise<{ valid, reason? }> {
  const subscription = supabase.from("subscriptions")
    .select("status, expires_at, plan_type")...
  if (expiresAt < now) { ... }   // trial-expiry policy lives here
  if (status !== "active") { ... } // status enum lives here
}

// apps/api/src/shared/utils/subscription-flags.ts:14-37
export async function isHandMaisEnabled(userId): Promise<boolean> {
  // per-feature flag tied to a tier — one hand-rolled query per feature
}
```

Symptoms an LLM cold-reading those files would surface:

- The **plan catalog is implicit** — the set `{free, trial, pro,
  enterprise}` is encoded in CHECK constraints, magic strings, and
  imported `Log.*` keys. Adding a fifth plan touches ~6 files.
- The **feature-to-plan mapping is opaque** — `hand_mais_enabled`,
  `drex_enabled`, etc. are boolean columns on `subscriptions`. A
  feature being added means a column migration + a new
  `isXEnabled(userId)` helper.
- The **trial policy is a runtime branch** — "free → pro for 14
  days, then revert" is a side effect of `expires_at` math, not a
  declared contract.
- The **quota story is missing** — there is no language artifact for
  `queries_per_month: 100`; product owners enforce it in dashboards
  and prayer.
- **Gates at the request boundary are repeated boilerplate** — each
  protected route calls `checkSubscription` + the matching
  `isXEnabled` helper before the actual business logic.

The boundary leak: every subscription tweak requires Go/TS handler
edits across several feature files, with no single source of truth a
doctor pass can audit. This is exactly the pattern Rule Zero asks us
to absorb into vocabulary.

## Why one cohesive `plan` (not three split kinds)

A natural temptation is to split this into `plan` (catalog),
`feature_flag` (gate), and `quota` (limit). **Reject.** In every
real subscription product these three ship together:

- A feature flag without a plan to attach to is just a boolean — the
  industry already has feature-flag platforms (LaunchDarkly,
  OpenFeature) for the **rollout / experiment** axis, and those are
  `@plugin/<provider>` problems, not a Lazuli kind.
- A quota without a plan is a meter without a contract — somebody
  has to declare "1000 calls/month is the pro limit", and that
  somebody is the plan.
- A trial without a plan to upgrade-from / revert-to is a date
  field — and a plan to upgrade-to is the trial's target.

Splitting them invites two failure modes the LLM cold-read test
explicitly flags:

1. **Feature/plan name drift.** `feature_flag bulk_consult` declared
   on its own; `plan pro` mentions `bulk_consult` in its feature
   list. Now two declarations must stay in sync; doctor would have
   to cross-reference symbol tables that aren't naturally local.
2. **Quota orphaning.** `quota queries_per_month: 100` declared on
   its own; some gate references `plan.limit: queries_per_month`.
   Same drift problem.

The `plan` kind owns all three. Gates at call sites read flat
identifiers (`plan.feature: <name>`, `plan.limit: <name>`) that
must resolve into the closed catalog formed by the union of all
`plan` declarations in the package.

## Design — surface

### Plan catalog

Declared at the **app/registry** level (one canonical catalog per
package; matches the way `app.environments` and `registry.capabilities`
are package-level). Authors put `plan` blocks in `app.lzi` directly
when the catalog is small, or extract into `registry.lzi` next to
capabilities and packs once the noise grows — the same convention as
env groups and integrations.

```lzi
# app.lzi (or registry.lzi)
plan free
  features search, view_history
  limits queries_per_month 100, banks_per_query 3

plan pro
  features search, view_history, export_csv, bulk_consult, hand_mais
  limits queries_per_month 5000, banks_per_query 20

plan enterprise
  features search, view_history, export_csv, bulk_consult, hand_mais,
           api_access, sso
  limits queries_per_month unlimited, banks_per_query unlimited

plan trial_pro
  trial duration 14d, then free
  features pro.features
  limits pro.limits
```

Required header: `plan <ident>` — the plan name is a stable identifier
matched by `[a-z][a-z0-9_]*`. The name is the catalog key.

Required children: at least one of `features` / `limits` (an empty
plan is rejected because gates would have nothing to bind to).

Optional children: `trial` — see §"Trial blocks" below.

#### `features <ident>, <ident>, ...`

Comma-separated identifier list. Each identifier is a flat feature
key in the package-wide closed catalog. Identifiers match
`[a-z][a-z0-9_]*` — same rule as everywhere else in the language.

**v0.1 scope:** features are **ID-only**. They do not carry config
or values. A feature is "in the plan" or "not". Carrying typed
config per feature (`hand_mais { credit_lookup: true }`) is a
v0.2 extension and explicitly out of scope here — see §"Out of
scope".

Cross-plan reference: `features <other_plan>.features` expands the
referenced plan's feature list inline (the union is computed at
analyzer time). This keeps `trial_pro` short. Composition is
**total replacement** — Lazuli has no cascade; `pro.features`
expands and the local `features` line is the complete list.
Adding `plus <name>` to extend would be cascade and is rejected.

#### `limits <name> <value>, <name> <value>, ...`

Comma-separated `<name> <value>` pairs. Each `<name>` is a flat
limit key (same regex as feature). Each `<value>` is either:

- A positive integer literal, optionally followed by a window
  shorthand: `100`, `5000` — interpreted as "per period of the
  declaring quota site" (see §"Quota gates").
- The literal keyword `unlimited` — the gate is decorative; the
  runtime emits no quota check.

**v0.1 scope:** values are integer-or-`unlimited`. Rates with
explicit windows (`100 per hour`) live on `rate_limit "..."` and
must not be conflated; quotas are **per-period** counters reset by
the runtime on the subscription's billing cycle (see §"Runtime").
Floats, durations, and booleans are out of scope.

Cross-plan reference: same `<other_plan>.limits` expansion semantics
as features.

### Trial blocks

A `trial` child declares a time-bounded upgrade:

```lzi
plan trial_pro
  trial duration 14d, then free
  features pro.features
  limits pro.limits
```

Required children of `trial`:
- `duration <duration_literal>` — closed unit catalog `s | m | h | d`
  (matches `@cap.Token(ttl:...)` from
  [`bucket-storage-cycle.md`](bucket-storage-cycle.md) and existing
  `auth sessions ttl`).
- `then <plan_name>` — the plan the subscription reverts to once
  the trial elapses. Must reference a declared plan in the catalog
  (doctor cross-checks).

The trial block is **declarative policy** — the runtime watches
`subscription.expires_at` (or equivalent provider field) and
schedules the revert. The plan catalog declares the contract; the
runtime owns the transition. Stripe-/MercadoPago-side trial setup
is `@plugin/<provider>`-adapter business, not surface.

A plan with a `trial` block is itself referenceable on a
subscription row exactly like any other plan. The `trial_pro` plan
in the catalog is the **plan a subscription has during the trial
period**; the `then free` line names what happens after. This keeps
plan resolution a flat lookup at call time.

### Subscription anchor in `app.lzi`

The language has to know which resource holds the active
subscription so codegen knows what to load. The author declares
one pointer:

```lzi
# app.lzi
app production-grade
  environments dev, staging, prod
  subscription resource users.subscription
  ...
```

`subscription resource <feature>.<field>` is a single-line directive
under `app`. The right-hand side is a fully-qualified resource
field path:

- `<feature>` is the feature that owns the subscription's parent
  resource (`users` in the example).
- `<field>` is the parent resource's edge to the subscription
  resource — either a `has_many subscription: Subscription` or a
  `subscription: Subscription required` direct field.

Multi-tenant: the subscription resource MUST carry the same
tenancy axis as the rest of the package (`tenancy org` on a
multi-tenant app; absent on a single-tenant app). Doctor enforces
parity with `app.defaults.tenancy`.

Single-tenant apps: the directive resolves to the user's
subscription row, full stop. Multi-tenant apps: it resolves to
the tenant's subscription row — a `subscription resource
orgs.subscription` declaration changes the lookup semantics
without changing the surface.

There is exactly **one** `subscription resource` directive per app.
Declaring two is a doctor error; declaring zero while any feature
uses a `gate behind plan.*` directive is also a doctor error
(PLAN-NO-SUBSCRIPTION-001 below).

### Gate directives

Gates are declared as **children of the callable** they protect.
Callable kinds today: `command`, `query.list` / `query.lookup` /
`query.sql`, `job`, `webhook`, `poller` (post-`poller-vocab.md`).
`api` blocks gain gates too because they dispatch a command/query
and the gate may need to evaluate before that dispatch.

Two gate forms:

```lzi
# in features/multi-bank/multi-bank.lzi
command multi_bank_consult
  input customer_id, bank_codes
  gate behind plan.feature: bulk_consult
  gate quota plan.limit: queries_per_month
  policy @policy.create
  creates ConsultRecord
    ...
```

#### `gate behind plan.feature: <feature_name>`

Boolean check. Resolution: load the caller's active subscription
via the `subscription resource` anchor; resolve its current plan;
refuse dispatch if `<feature_name>` is not in the plan's
`features` list. On refusal: 402 Payment Required (the IANA-defined
"reserved for future use" status that has become the de facto
"upgrade your plan" status; the runtime canonicalizes the error
code `plan.feature_forbidden`).

The `<feature_name>` is a flat identifier resolved against the
package-wide catalog (union of every `plan` block's `features`
list).

#### `gate quota plan.limit: <limit_name>`

Counter check. Resolution: same plan lookup, then read the
`<limit_name>` value from the plan's `limits` map:

- `unlimited` — gate is a no-op; runtime emits no counter read.
- positive integer — runtime reads the caller's current period
  usage from a Lazuli-managed `subscription_usage` table, refuses
  with 402 (`plan.quota_exceeded`) if usage ≥ limit, otherwise
  increments by 1 after the dispatch succeeds.

The increment is **post-success** by default: failed dispatches do
not consume quota. An optional `gate quota plan.limit: <name>
charge always` form forces accounting even on failure, for cases
where the work was attempted (e.g. external-call cost) — out of
scope for v0.1; mention here so the surface has room.

Multiple `gate quota` directives on the same callable are valid —
each meters a distinct limit. They evaluate in declaration order;
the first refusal short-circuits.

#### Ordering and combinability

A callable may declare any combination of `gate behind` and `gate
quota`. Evaluation order is:

1. `gate behind plan.feature: ...` (all of them, in declaration
   order)
2. `gate quota plan.limit: ...` (all of them, in declaration order)
3. Existing `policy @policy.*` evaluation (unchanged)
4. The callable's effects

Gates run **before** `policy` because a wrong-plan caller should
get a 402, not a 403 — the failure modes carry different remediation
(`upgrade your plan` vs. `you lack the role`). Doctor flags any
attempt to reorder via authoring.

#### Distinction from `rate_limit`

`rate_limit "100 per minute per user"` is per-call frequency
control. It exists today (`command rate_limit "..."`) and is owned
by the runtime's in-memory rate limiter (typically Redis-backed).
It does **not** read a subscription, does **not** vary by plan, and
**does** apply to free-tier abusers identically to paid users.

`gate quota plan.limit: ...` is per-period budget control. It
varies by plan, reads/writes a persistent table, and exists to
monetize.

The two are orthogonal. A callable may declare both; both run.

## Lowering (IR)

Two new top-level IR nodes and three new gate IR shapes.

### Plan catalog

```rust
// crates/lazuli_ir/src/lib.rs — additive
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanCatalog {
    /// Package-wide closed catalog. Key is the plan name.
    pub plans: BTreeMap<Ident, Plan>,
    /// Union of every plan's `features` list, computed at lowering.
    /// Used by doctor + LSP completion + inspect.
    pub feature_catalog: BTreeSet<Ident>,
    /// Union of every plan's `limits` keys, computed at lowering.
    pub limit_catalog: BTreeSet<Ident>,
    pub origin: SourceOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub name: Ident,
    pub features: BTreeSet<Ident>,
    pub limits: BTreeMap<Ident, LimitValue>,
    pub trial: Option<TrialPolicy>,
    pub origin: SourceOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum LimitValue {
    Integer(u64),
    Unlimited,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrialPolicy {
    pub duration: Duration,
    pub then_plan: Ident, // resolved against `plans` post-lowering
}
```

`PlanCatalog` is a sibling of `RegistryIntegrations` /
`RegistryPacks` on the package IR. There is exactly **one** per
package — the catalog merges plan blocks declared in `app.lzi` and
`registry.lzi`.

### Subscription anchor

```rust
// crates/lazuli_ir/src/lib.rs — additive on AppSpec
pub struct AppSpec {
    // ... existing fields ...
    pub subscription_anchor: Option<SubscriptionAnchor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionAnchor {
    pub owner_feature: Ident,     // "users"
    pub owner_resource: Ident,    // "User" (the parent)
    pub field: Ident,             // "subscription"
    pub target_feature: Ident,    // "users" again, or wherever
    pub target_resource: Ident,   // "Subscription"
    pub tenancy_axis: Option<Ident>, // "org" for multi-tenant; None otherwise
    pub origin: SourceOrigin,
}
```

The anchor is resolved at analyzer time. The `<feature>.<field>`
form on the surface lowers by:

1. Finding `<feature>` in the package.
2. Finding the resource whose declaring feature is `<feature>` and
   that owns `<field>` (Lazuli convention: the feature implicitly
   has one parent resource per the existing convention; we resolve
   `users.subscription` to "the parent resource of feature `users`
   has a field/edge named `subscription`").
3. Following the field/edge to the target resource.
4. Asserting tenancy parity.

### Gates on callables

Every callable IR (`Command`, `Query*`, `Job`, `Webhook`, `Poller`,
`Api`) gains a `gates: Vec<Gate>` field. Order is preserved (it
encodes evaluation order).

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum Gate {
    Behind { feature: Ident, origin: SourceOrigin },
    Quota { limit: Ident, origin: SourceOrigin },
}
```

Lowering reads the `gate behind plan.feature: <X>` or
`gate quota plan.limit: <X>` form, captures the bare identifier,
and registers a cross-ref from this callable to the catalog. The
analyzer rejects any other form (e.g. `gate behind plan.tier:
pro`) — the only supported gate axes in v0.1 are `plan.feature` and
`plan.limit`. Adding `plan.tier: <X>` (membership test against a
plan-name tier) is a v0.2 candidate; not in scope.

### Surface → IR mapping

| Surface | IR | Notes |
|---|---|---|
| `plan free` block | `PlanCatalog.plans["free"] = Plan { ... }` | Plan blocks merge across files; doctor rejects collision on same name. |
| `features a, b, c` | `Plan.features = {a, b, c}` | Cross-plan refs (`pro.features`) expand inline at lowering. |
| `limits q 100, b 3` | `Plan.limits = {q: Integer(100), b: Integer(3)}` | Cross-plan `pro.limits` same expansion. |
| `trial duration 14d, then free` | `Plan.trial = Some(TrialPolicy { duration: 14d, then_plan: "free" })` | `then` arg must resolve. |
| `subscription resource users.subscription` (in `app`) | `AppSpec.subscription_anchor = Some(...)` | Exactly zero or one per app. |
| `gate behind plan.feature: bulk_consult` | `<Callable>.gates.push(Gate::Behind { feature: "bulk_consult", ... })` | |
| `gate quota plan.limit: queries_per_month` | `<Callable>.gates.push(Gate::Quota { limit: "queries_per_month", ... })` | |

### Inspect projection

New `--expand=plans` (top-level catalog) and `--expand=gates`
(per-feature gate decoration) flags in `ExpandSet`
(`crates/lazuli_cli/src/main.rs:98-118`).

`lazuli inspect --format=json --expand=plans` emits at the top
level:

```json
{
  "plans": {
    "catalog": {
      "free":  { "features": ["search", "view_history"],
                 "limits":   { "queries_per_month": 100,
                               "banks_per_query": 3 },
                 "trial":    null,
                 "origin":   "app.lzi:14" },
      "pro":   { "features": ["search", "view_history", "export_csv",
                              "bulk_consult", "hand_mais"],
                 "limits":   { "queries_per_month": 5000,
                               "banks_per_query": 20 },
                 "trial":    null,
                 "origin":   "app.lzi:18" },
      "trial_pro": {
                 "features": ["search", "view_history", "export_csv",
                              "bulk_consult", "hand_mais"],
                 "limits":   { "queries_per_month": 5000,
                               "banks_per_query": 20 },
                 "trial":    { "duration": "14d", "then_plan": "free" },
                 "origin":   "app.lzi:26" }
    },
    "feature_catalog": [
      "api_access", "bulk_consult", "export_csv", "hand_mais",
      "search", "sso", "view_history"
    ],
    "limit_catalog": ["banks_per_query", "queries_per_month"],
    "subscription_anchor": {
      "owner_feature": "users",
      "owner_resource": "User",
      "field": "subscription",
      "target_feature": "users",
      "target_resource": "Subscription",
      "tenancy_axis": null,
      "origin": "app.lzi:5"
    }
  }
}
```

`lazuli inspect --format=json --expand=gates` adds, per feature
containing a gated callable:

```json
{
  "name": "multi_bank",
  "gates": [
    {
      "callable": "command.multi_bank_consult",
      "checks": [
        { "kind": "behind", "feature": "bulk_consult",
          "origin": "features/multi-bank/multi-bank.lzi:42" },
        { "kind": "quota",  "limit":   "queries_per_month",
          "origin": "features/multi-bank/multi-bank.lzi:43" }
      ]
    }
  ]
}
```

Normalisation rules (same convention as `--expand=storage`):

- `feature_catalog` and `limit_catalog` are sorted alphabetically
  to give deterministic agent context.
- Features without gated callables omit `gates` from the projection
  entirely.
- Without `--expand=plans` and without `--expand=gates`, neither
  key appears (no implicit emission).

## Codegen (Lazuli Go)

Codegen emits two artifacts:

### 1. One package-wide `dist/go/plan/catalog.gen.go`

```go
// path: dist/go/plan/catalog.gen.go
// Code generated by Lazuli. DO NOT EDIT.
package plan

import (
    "time"

    "lazuli.dev/runtime/lazuli/billing"
)

// Catalog is the package-wide plan catalog lowered from
// app.lzi `plan ...` blocks.
var Catalog = billing.PlanCatalog{
    Plans: map[string]billing.Plan{
        "free": {
            Name:     "free",
            Features: billing.NewFeatureSet("search", "view_history"),
            Limits: map[string]billing.LimitValue{
                "queries_per_month": billing.LimitInt(100),
                "banks_per_query":   billing.LimitInt(3),
            },
        },
        "pro": {
            Name:     "pro",
            Features: billing.NewFeatureSet("search", "view_history",
                                            "export_csv", "bulk_consult", "hand_mais"),
            Limits: map[string]billing.LimitValue{
                "queries_per_month": billing.LimitInt(5000),
                "banks_per_query":   billing.LimitInt(20),
            },
        },
        "enterprise": {
            Name:     "enterprise",
            Features: billing.NewFeatureSet("search", "view_history",
                                            "export_csv", "bulk_consult",
                                            "hand_mais", "api_access", "sso"),
            Limits: map[string]billing.LimitValue{
                "queries_per_month": billing.LimitUnlimited,
                "banks_per_query":   billing.LimitUnlimited,
            },
        },
        "trial_pro": {
            Name:     "trial_pro",
            Features: billing.NewFeatureSet(/* same as pro */),
            Limits:   /* same as pro */,
            Trial: &billing.TrialPolicy{
                Duration: 14 * 24 * time.Hour,
                ThenPlan: "free",
            },
        },
    },
    // Subscription anchor:
    Anchor: billing.SubscriptionAnchor{
        OwnerFeature:   "users",
        OwnerResource:  "User",
        Field:          "subscription",
        TargetResource: "Subscription",
        TenancyAxis:    "", // empty = single-tenant
    },
}

// LookupPlan resolves the active plan for the caller in ctx by
// consulting the anchor. The runtime owns the resolution; this
// shim is the wire.
func LookupPlan(ctx *lazuli.Ctx) (*billing.Plan, error) {
    return billing.LookupPlan(ctx, Catalog)
}
```

The generated code is **wire-thin**: the catalog is a literal
`map[string]Plan`, and `LookupPlan` is one call into the runtime.
~50 LOC for a 4-plan catalog — proportional to plan count, no
boilerplate per feature.

### 2. Per-gated-callable shims

For every command/query/job/webhook/poller/api carrying a `gate`,
codegen emits a one-liner at the top of the generated handler
that calls into the runtime:

```go
// dist/go/multi_bank/multi_bank_consult.gen.go
func (h *MultiBankConsultHandler) Dispatch(ctx *lazuli.Ctx, input MultiBankConsultInput) error {
    // generated gate prelude — order: behind, quota, policy
    if err := plan.CheckFeature(ctx, "bulk_consult"); err != nil {
        return err
    }
    if err := plan.CheckQuota(ctx, "queries_per_month"); err != nil {
        return err
    }
    // existing policy check + effects below
    if err := policy.CheckCreate(ctx, ...); err != nil { return err }
    // ...
    // post-success quota increment:
    if err := plan.IncrQuota(ctx, "queries_per_month"); err != nil {
        // increment failure is logged but does not fail the request —
        // the runtime handles eventual reconciliation
    }
    return nil
}
```

`plan.CheckFeature` / `plan.CheckQuota` / `plan.IncrQuota` are
three trivial wrappers over the runtime contract. Codegen
generates the three calls in order; nothing else changes.

Boundary discipline: codegen never names a provider. The generated
code references `lazuli.dev/runtime/lazuli/billing` only; the
adapter that talks to Stripe/MercadoPago/internal-billing sits
behind `@plugin/<provider>` and is selected via `registry.lzi`.

## Runtime contract (sketch — not shipped here)

Three new files under `runtime/go/lazuli/billing/`. Wire-thin gate
per CLAUDE.md founding principle:

### `runtime/go/lazuli/billing/plan.go`

```go
package billing

// Types lowered from IR — single source of truth at runtime.
type PlanCatalog struct {
    Plans  map[string]Plan
    Anchor SubscriptionAnchor
}

type Plan struct {
    Name     string
    Features FeatureSet     // small typed wrapper around map[string]struct{}
    Limits   map[string]LimitValue
    Trial    *TrialPolicy
}

type LimitValue struct {
    Unlimited bool
    Value     uint64
}

func LimitInt(v uint64) LimitValue { return LimitValue{Value: v} }
var LimitUnlimited = LimitValue{Unlimited: true}

type TrialPolicy struct {
    Duration time.Duration
    ThenPlan string
}

type SubscriptionAnchor struct {
    OwnerFeature   string
    OwnerResource  string
    Field          string
    TargetResource string
    TenancyAxis    string
}

// LookupPlan loads the active subscription via the anchor, resolves
// its current plan_name → Plan, and applies trial-revert logic if
// the subscription is in trial and expires_at has passed.
//
// Wire: one SELECT through the configured subscription store (default
// the same pgx pool as the rest of Lazuli; pluggable per
// @plugin/<provider> adapter for hosted-billing setups).
func LookupPlan(ctx *lazuli.Ctx, catalog PlanCatalog) (*Plan, error) {
    // ~30 LOC: anchor-driven SQL + plan lookup + trial revert
}
```

### `runtime/go/lazuli/billing/gate.go`

```go
package billing

func CheckFeature(ctx *lazuli.Ctx, featureName string) error {
    plan, err := LookupPlan(ctx, Catalog)
    if err != nil { return mapLookupErr(err) }
    if !plan.Features.Has(featureName) {
        return ErrPlanFeatureForbidden{Plan: plan.Name, Feature: featureName}
    }
    return nil
}

func CheckQuota(ctx *lazuli.Ctx, limitName string) error {
    plan, err := LookupPlan(ctx, Catalog)
    if err != nil { return mapLookupErr(err) }
    limit, ok := plan.Limits[limitName]
    if !ok { return ErrPlanLimitMissing{Plan: plan.Name, Limit: limitName} }
    if limit.Unlimited { return nil }
    used, err := readUsage(ctx, plan.Name, limitName)
    if err != nil { return err }
    if used >= limit.Value {
        return ErrPlanQuotaExceeded{Plan: plan.Name, Limit: limitName,
                                    Used: used, Max: limit.Value}
    }
    return nil
}

func IncrQuota(ctx *lazuli.Ctx, limitName string) error {
    // UPDATE subscription_usage SET <limit>_count = <limit>_count + 1
    //   WHERE subscription_id = ctx.Subscription AND period_start = currentPeriod()
    // Wire-thin: one SQL.
}
```

### `runtime/go/lazuli/billing/usage.go`

The `subscription_usage` table is Lazuli-managed:

```sql
CREATE TABLE subscription_usage (
    subscription_id uuid    not null,
    period_start    date    not null,
    limit_name      text    not null,
    used            bigint  not null default 0,
    primary key (subscription_id, period_start, limit_name)
);
```

The runtime owns the migration (declared in
`runtime/go/lazuli/billing/migrations/`); product authors do not
write SQL for usage tracking. Period rollover is computed from
`subscriptions.started_at` (or the equivalent provider field
mapped via adapter).

### Typed errors

- `ErrPlanFeatureForbidden` → 402, code `plan.feature_forbidden`,
  payload `{plan, feature}`.
- `ErrPlanQuotaExceeded` → 402, code `plan.quota_exceeded`, payload
  `{plan, limit, used, max, reset_at}`.
- `ErrPlanLimitMissing` → 500, code `plan.limit_missing` — should
  be impossible if doctor passes; this is the runtime safety net.
- `ErrPlanLookupFailed` → 503, code `plan.lookup_failed`.

All four are part of the public `expose client` mapping and get
generated into the per-frontend SDK error union.

### Adapter contract

The `subscription resource` anchor binds to a `SubscriptionStore`
interface in the runtime:

```go
type SubscriptionStore interface {
    LookupActive(ctx context.Context, subjectID lazuli.ID) (*ActiveSubscription, error)
}

type ActiveSubscription struct {
    SubscriptionID lazuli.ID
    PlanName       string
    Status         string // "active" | "trialing" | "expired" | "cancelled"
    ExpiresAt      time.Time
    StartedAt      time.Time
    TrialEndsAt    *time.Time
}
```

Default adapter (`@runtime/postgres-subscription`): plain SQL
through the existing pgx pool, reads from the table the anchor
points at. Hosted-billing adapters (`@plugin/stripe-subscription`,
`@plugin/mercadopago-subscription`) implement the same interface
talking to provider APIs / webhooks; that work is **out of scope**
for this proposal and lives in their respective plugin repos.

Wire-thin counts: every file in `runtime/go/lazuli/billing/` should
be ≤ 100 effective LOC. `plan.go` ≈ 60, `gate.go` ≈ 80, `usage.go`
≈ 50. No re-implementation of subscription state machines, no
hand-rolled Stripe-compatibility layer, no homegrown JSON RPC.

## Doctor (5 diagnostics)

### `PLAN-FEATURE-UNDECLARED-001` (error)

**Message**: "gate `behind plan.feature: <X>` references feature
`<X>` which is not in any declared plan; the feature catalog is
the union of every plan's `features` list."

**Trigger**: a callable's `gate behind plan.feature: <name>`
resolves to a feature name not in `PlanCatalog.feature_catalog`.

**Anchor**: `crates/lazuli_cli/src/doctor.rs` cross-feature pass.

**Test fixture**:
`crates/lazuli_cli/tests/fixtures/plans/feature_undeclared.lzi`:

```lzi
plan free
  features search

# no plan declares "missing_feature"
command x
  gate behind plan.feature: missing_feature
  policy @policy.create
  creates X
```

Doctor asserts exactly one `PLAN-FEATURE-UNDECLARED-001` at the
gate line.

### `PLAN-QUOTA-MISSING-001` (error)

**Message**: "gate `quota plan.limit: <X>` references limit `<X>`
which is not declared on every plan; quota gates must be honored
by every tier (set `<X> unlimited` to opt a tier out)."

**Trigger**: a callable's `gate quota plan.limit: <name>` resolves
to a limit declared on **some** plans but not **all**. (This is a
stronger check than "must be in the union catalog" — the rationale
is that a caller on a plan that doesn't list the limit at all has
ambiguous semantics; forcing `unlimited` is the explicit opt-out.)

**Anchor**: same.

**Test fixture**: `crates/lazuli_cli/tests/fixtures/plans/quota_missing.lzi`:
plan `free` declares `queries_per_month`; plan `pro` does not. A
command gates on `plan.limit: queries_per_month`. Assert one
`PLAN-QUOTA-MISSING-001` at the gate line, with the message naming
plan `pro` as the missing-declaration site.

### `PLAN-NO-SUBSCRIPTION-001` (error)

**Message**: "feature `<X>` declares `gate behind plan.feature` /
`gate quota plan.limit` but `app.lzi` does not declare
`subscription resource <feature>.<field>`; the runtime has no
anchor to resolve the active plan."

**Trigger**: any gate exists in any feature of the package, but
`AppSpec.subscription_anchor.is_none()`.

**Anchor**: `crates/lazuli_cli/src/doctor.rs` app-level pass.

**Test fixture**: `crates/lazuli_cli/tests/fixtures/plans/no_anchor.lzi`
+ stripped `app.lzi`. One diagnostic at the gate site, with the
fix-it suggestion pointing at `app.lzi`.

### `PLAN-TRIAL-FALLBACK-001` (error)

**Message**: "plan `<X>` declares `trial then <Y>` but plan `<Y>`'s
feature set is not a subset of `<X>`'s; trial revert would surprise
the caller by losing features they had during trial."

**Trigger**: A plan with a `trial then <Y>` block has at least one
feature in its own `features` list that is not in `<Y>.features`.
This is a **strong** check — the rationale is that a trial that
strips features on revert is fine from a billing perspective but
disastrous from a user-experience perspective; if the author wants
that, they declare it explicitly via a separate plan rather than
letting a trial silently revert mid-flow.

**Anchor**: `crates/lazuli_cli/src/doctor.rs` plan-catalog pass.

**Test fixture**: `crates/lazuli_cli/tests/fixtures/plans/trial_fallback.lzi`:
`trial_pro` carries `hand_mais` feature, reverts to `free`, `free`
does not carry `hand_mais`. Doctor asserts one
`PLAN-TRIAL-FALLBACK-001` at the trial block.

**Note for v0.2**: A future relaxation could flip this to a warning
once "graceful degradation" has a vocabulary (e.g.
`trial then free strip <feature_list>`). v0.1 is strict.

### `PLAN-GATE-RATE-LIMIT-COLLISION-001` (warning)

**Message**: "callable `<X>` declares both `rate_limit` and `gate
quota plan.limit: <Y>`; verify the two limits do not contradict —
`rate_limit` is per-frequency, `gate quota` is per-period."

**Trigger**: any callable carrying both `rate_limit` and at least
one `gate quota plan.limit`.

**Anchor**: `crates/lazuli_cli/src/doctor.rs` per-callable pass.

**Test fixture**: a command with `rate_limit "100 per minute per
user"` AND `gate quota plan.limit: queries_per_month`. Doctor warns
(not errors — the combination is legitimate, just often misauthored).

### Severity registration

All five codes register under `is_security_enforcement_code`
(`crates/lazuli_lsp/src/lib.rs:9527`) so the strict + production
security profiles upgrade severity uniformly (rationale: subscription
bypass is an authorization issue).

## LSP

### Hovers (new entries to `KEYWORD_HOVER`)

| Keyword | Hover summary |
|---|---|
| `plan` | "Subscription tier declaration. Declares a feature set and a limit set, optionally with a `trial` revert policy. Catalog is package-wide; the union of every plan's `features` / `limits` forms the closed catalog for `gate` directives." |
| `features` | "Comma-separated identifier list of features included in this plan. Identifiers are flat keys; references at call sites use `plan.feature: <name>`. Cross-plan reuse via `features <other_plan>.features`." |
| `limits` | "Comma-separated `<name> <value>` pairs. Value is positive integer or the literal `unlimited`. References at call sites use `plan.limit: <name>`. Cross-plan reuse via `limits <other_plan>.limits`." |
| `trial` | "Trial revert policy. `duration <d>, then <plan>` declares: subscription stays on this plan for `<d>`, then auto-reverts to `<plan>`. Runtime owns the transition; doctor cross-checks revert-feature-subset." |
| `unlimited` | "Limit value meaning 'no quota check'; the gate becomes a no-op at this tier. Use to opt a tier out of a quota declared on other tiers." |
| `subscription resource` | "App-level directive: declares which `<feature>.<field>` resource holds the active subscription. Required when any feature uses `gate behind plan.*` or `gate quota plan.*`. Exactly one per app." |
| `gate behind plan.feature` | "Boolean gate. Refuses dispatch with 402 (`plan.feature_forbidden`) if the caller's active plan does not list `<feature>` in its `features` set. Evaluates before `policy`." |
| `gate quota plan.limit` | "Counter gate. Refuses with 402 (`plan.quota_exceeded`) if the caller's period usage for `<limit>` has reached the plan's value; otherwise increments by 1 after successful dispatch." |

### Closed-catalog completions

- `gate behind plan.feature: |` → complete with every identifier in
  `PlanCatalog.feature_catalog`.
- `gate quota plan.limit: |` → complete with every identifier in
  `PlanCatalog.limit_catalog`.
- `trial duration <int>|` → `s`, `m`, `h`, `d` (same closed set as
  `@cap.Token(ttl:...)`).
- `trial ..., then |` → complete with every declared plan name.
- `limits <name> |` → suggest `unlimited` literal and prompt for integer.
- `subscription resource |` → complete with every feature name; after
  `.`, complete with the feature's resource field set.

### File-local diagnostics

LSP emits (file-local) variants of `PLAN-FEATURE-UNDECLARED-001`
and `PLAN-QUOTA-MISSING-001` when the gate references an identifier
that the file-local plan catalog does not satisfy. The cross-file
union check stays in doctor. This keeps editing fast — the LSP
flags the obvious typos; doctor flags the cross-package mismatches.

### Highlighting

`editors/vscode/syntaxes/lazuli.tmLanguage.json` additions to the
keyword scope:

- `plan`, `features`, `limits`, `trial`, `then`, `unlimited`,
  `subscription`, `gate`, `behind`, `quota`.
- `plan.feature` / `plan.limit` as dotted-keyword sub-scopes (same
  pattern as `query.list`, `query.lookup`).

## Fixture exercise

`examples/full-capsule/full-capsule.lzi` already has billing
shapes. v0.1 adds **one** plan catalog block at the top of
`app.lzi` (package-level), **one** subscription anchor, and **one**
gated callable in a feature to validate the round-trip.

Proposed catalog (in the existing `full-capsule` shape):

```lzi
# app.lzi additions
app full_capsule
  # ... existing ...
  subscription resource customer.subscription

plan free
  features customer_list, customer_lookup
  limits exports_per_month 5

plan pro
  features customer_list, customer_lookup, customer_export, customer_import
  limits exports_per_month 1000

plan trial_pro
  trial duration 14d, then free
  features pro.features
  limits pro.limits
```

Feature-side change (single gated callable for exercise):

```lzi
# features/customer/customer.lzi additions
command customer_export
  input ...
  gate behind plan.feature: customer_export
  gate quota plan.limit: exports_per_month
  policy @policy.read
  # ...
```

This exercises:
- 3 plans with cross-plan reference (`pro.features` / `pro.limits`).
- 1 trial block.
- 1 callable with both `gate behind` and `gate quota`.
- 1 subscription anchor in a single-tenant app.

Multi-tenant fixture (`examples/auth-multi-tenant/`) gets a parallel
exercise once v0.1 lands — out of scope to write here, but the
boundary discipline is identical.

## Cells

Lifted from the structural template
[`bucket-storage-cycle.md`](bucket-storage-cycle.md) §"Próximo passo":

### S1 — IR types + lowering pass

**Files**:
- `crates/lazuli_ir/src/lib.rs` — additive: `PlanCatalog`,
  `Plan`, `LimitValue`, `TrialPolicy`, `SubscriptionAnchor`, `Gate`.
- `crates/lazuli_analyzer/src/lib.rs` — lowering: parse
  `plan ...` blocks at package level, collect into one catalog;
  parse `subscription resource ...` on `app`; parse `gate ...`
  children on every callable kind.

**Tests**: roundtrip JSON serde for the new IR shapes; analyzer
test asserts the catalog union (`feature_catalog`, `limit_catalog`)
is computed correctly; cross-plan ref (`pro.features`) expands.

**Commit message**: `ir: plan catalog + gate IR nodes`.

### S2 — Parser slice for `plan`, `gate`, `subscription resource`

**File**: `crates/lazuli_syntax/src/parser.rs`.

**Spec**: closed grammar for the three new constructs. Existing
canonical-indent slice covers `agent`; this cell extends the same
pipeline for `plan` (top-level), `gate` (child of every callable),
and `subscription resource` (child of `app`).

**Tests**: parse-only round-trip on the fixture additions above;
parse-error cases (malformed `trial`, missing `then`, gate on a
non-callable, two anchors).

**Commit message**: `syntax: plan/gate/subscription parser slice`.

### S3 — Doctor diagnostics (5 codes)

**File**: `crates/lazuli_cli/src/doctor.rs`.

**Spec**: §"Doctor" above. 5 codes, fixtures land under
`crates/lazuli_cli/tests/fixtures/plans/`.

**Commit message**: `doctor: PLAN-* diagnostics (5 codes)`.

### S4 — LSP hovers, completions, file-local diagnostics

**File**: `crates/lazuli_lsp/src/lib.rs`.

**Spec**: §"LSP" above. 8 new hovers; 6 new completion contexts;
2 file-local diagnostics mirroring `PLAN-FEATURE-UNDECLARED-001`
and `PLAN-QUOTA-MISSING-001`.

**Commit message**: `lsp: plan/gate hover + completion`.

### S5 — Inspect projection

**File**: `crates/lazuli_cli/src/main.rs` +
`crates/lazuli_cli/src/inspect.rs`.

**Spec**: `--expand=plans` + `--expand=gates` per §"Inspect
projection" above. JSON normalisation rules from same section.

**Commit message**: `inspect: --expand=plans and --expand=gates`.

### S6 — Codegen (Lazuli Go): catalog + gate prelude

**File**:
`crates/lazuli_codegen_go/src/emitter/plans.rs` (new) +
the existing per-callable emitter to inject the 3-line gate prelude.

**Spec**: §"Codegen" above. Snapshot tests against the
`examples/full-capsule/` fixture.

**Commit message**: `codegen: plan catalog + per-callable gate prelude`.

### S7 — Runtime contract (Lazuli Go)

**Files**:
- `runtime/go/lazuli/billing/plan.go`
- `runtime/go/lazuli/billing/gate.go`
- `runtime/go/lazuli/billing/usage.go`
- `runtime/go/lazuli/billing/usage_migration.sql`
- `runtime/go/lazuli/billing/billing_test.go`

**Spec**: §"Runtime contract" above. Wire-thin gate per CLAUDE.md
founding principle: every file ≤ 100 effective LOC, every
non-stdlib import named.

**Commit message**: `runtime: billing/plan + gate + usage`.

### S8 — Highlighting

**File**: `editors/vscode/syntaxes/lazuli.tmLanguage.json`.

**Spec**: §"Highlighting" above. Additive only.

**Commit message**: `highlight: plan/gate keywords`.

### S9 — Fixture exercise + memo

**Files**:
- `examples/full-capsule/app.lzi` — add catalog + anchor.
- `examples/full-capsule/full-capsule.lzi` — add 1 gated command.
- Re-run codegen; verify outputs land where expected and compile.
- Update `docs/proposals/production-readiness.md` row #9 to 🟢
  with the merge SHA.

**Commit message**: `examples: full-capsule plan/gate exercise`.

## Acceptance (cycle-level)

- [ ] `lazuli check examples/full-capsule` accepts the new
  catalog + anchor + gate syntax.
- [ ] `lazuli inspect --format=json --expand=plans --expand=gates
  examples/full-capsule` emits the shapes documented in
  §"Inspect projection".
- [ ] `lazuli doctor examples/full-capsule` emits the 5 diagnostics
  on their respective fixtures, zero diagnostics on the canonical
  fixture.
- [ ] `lazuli generate examples/full-capsule` produces
  `dist/go/plan/catalog.gen.go` + a 3-line gate prelude on the one
  gated callable, and the package compiles under
  `runtime/go/lazuli/billing`.
- [ ] `runtime/go/lazuli/billing/billing_test.go` exercises:
  feature-pass / feature-deny / quota-pass / quota-deny / trial
  revert / `unlimited` no-op / multi-tenant lookup.
- [ ] LSP regression tests cover the 8 hovers + 6 completion contexts.
- [ ] `runtime/go/lazuli/billing/` total effective LOC ≤ 250.

## Out of scope (deferred)

- **Stripe / MercadoPago / Pagar.me integration.** Provider
  webhooks, charge state machines, billing-portal redirects — all
  `@plugin/<provider>` work. The `SubscriptionStore` adapter
  contract is the only seam exposed.
- **Paywall UX / upgrade flow.** Frontend rendering of "402, upgrade
  here" is a Lazurite/distro concern, not Lazuli surface. The
  generated SDK includes the typed `ErrPlanFeatureForbidden` /
  `ErrPlanQuotaExceeded` shapes; UX layers on top.
- **Per-feature config carried by a feature.** `hand_mais
  { credit_lookup: true }` — v0.2 candidate; v0.1 features are
  ID-only.
- **`plan.tier: pro` membership tests at gate sites.** A gate
  testing "any plan named pro-or-better" implies plan tiers as
  ordered ranks. Lazuli has no `<` operator on identifiers; would
  require either tier-ranks (`plan pro tier 2`) or named groups
  (`plan_group paid_tiers ...`). Both are vocabulary extensions
  that need pilot pressure before promotion.
- **Quotas reset on non-monthly periods.** v0.1 hardcodes "period =
  one calendar month aligned to `subscriptions.started_at`". Daily
  / weekly / yearly are v0.2 — add a `limits <name> <int> per
  <period>` form when a real product asks.
- **Quotas as floats (`storage_gb_per_month 1.5`).** Floats invite
  rounding bugs; v0.1 is `u64`. Storage gates use `kb`/`mb`/`gb`
  size literals from `@cap.File`, not floating limits.
- **Aggregate / sub-account quotas.** "5 users × 100 queries each =
  500 org-level" is a multi-actor accounting model out of v0.1.
- **`gate behind` evaluating arbitrary predicates.** v0.1 supports
  only the two axes `plan.feature` and `plan.limit`. Cross-feature
  policy predicates already live under `policy @policy.*`; the gate
  is intentionally narrow.
- **Plan-change auditing & history.** Subscription state transitions
  are the runtime / `@plugin/<provider>` adapter's job; doctor and
  inspect do not project subscription history.
- **Locale-aware billing windows** (e.g. fiscal year vs. calendar
  year). Same v0.2 deferral as `@plugin/scalars-pt-BR`.

## Risks

| Risk | Mitigation |
|---|---|
| Catalog grows unwieldy on apps with 10+ plans. | The `<other_plan>.features` / `<other_plan>.limits` reuse is the v0.1 answer. `--expand=plans` provides the agent context pack; doctor's union catalog gives a single audit surface. If usable-size proves a real problem at pilot, a `plan_group <ident>` collector becomes a v0.2 candidate. |
| Gate fails closed on a transient subscription-store outage, breaking the entire app. | `ErrPlanLookupFailed` is the typed escape; runtime has a per-request memoization (1 lookup per ctx) + an adapter-level circuit breaker. Doctor warns when gate-protected callable has no `escape_route` declared, mirroring the existing `escape_route` convention. (Note: this is a v0.2 doctor warning candidate, not v0.1 — would create noise on the fixture today.) |
| `subscription_usage` table becomes a hot row under high QPS, blocking commands. | Wire-thin: the runtime uses the same pgx pool as everything else; the per-`(subscription, period, limit)` row contention is bounded by the gate count, not the request count. Postgres handles this row pattern under tens of thousands of QPS per row. If a product hits the ceiling, the adapter swap (`@plugin/<provider>-billing`) handles it; not a v0.1 concern. |
| `gate behind` and `policy @policy.*` semantics overlap visually — readers may not realize a `gate` failure is 402 vs. `policy` failure 403. | The hover docs explicitly call out the 402 vs. 403 distinction. Codegen comments name the status. The boundary is: `gate` = monetization; `policy` = authorization. Doctor `PLAN-GATE-RATE-LIMIT-COLLISION-001` covers the related visual overlap with `rate_limit`. |
| Trial-revert logic in the runtime conflicts with Stripe-side webhook events that *also* revert the plan. | The `SubscriptionStore` adapter is the single source of truth. Default postgres adapter watches `expires_at`; the Stripe/MP adapter watches webhook events. The Lazuli runtime's trial logic activates **only** for the default adapter — provider adapters set `plan_name` themselves and Lazuli respects it. Doctor cross-checks `trial` declaration vs. adapter capability via a v0.2 lint. |
| `gate quota` post-success increment fails (DB write error after handler succeeded). | Runtime logs but does not fail the request — the user got their work done. A periodic reconciliation job (delivered by the runtime alongside `usage.go`) heals counter drift via `SELECT count(*) FROM events WHERE ...` once per period. Doctor warns if `gate quota` exists on a callable without `audit default` — the audit log is the reconciliation source of truth. |
| Subscription anchor pointing to a resource without a `tenancy` field on a multi-tenant app silently routes everyone to the same subscription. | `PLAN-NO-SUBSCRIPTION-001` covers absence; a parallel `PLAN-SUBSCRIPTION-TENANCY-001` lint covers axis mismatch. Folded into S3 if v0.1 ships multi-tenant fixture; otherwise v0.2. |
| Feature catalog has typos that cross commits invisibly (`bulk_consult` vs `bulk-consult` vs `bulkConsult`). | The identifier regex is closed (`[a-z][a-z0-9_]*`). LSP completion + doctor cross-check are the safety net. The fixture exercise in S9 includes a deliberate typo case to verify diagnostics fire. |

## Companion docs to update

After cells land:
- `docs/architecture.md` — add `plan` bucket to the bucket
  inventory + name the runtime path.
- `docs/invariants.md` — add closed-grammar notes for `plan` /
  `gate` / `subscription resource`; declare the closed catalog rule
  for features/limits.
- `docs/proposals/production-readiness.md` row #9 flips ⬜ → 🟢
  with merge SHA.
- `editors/vscode/syntaxes/lazuli.tmLanguage.json` — done in S8.

## Open question — biggest unresolved design tension

**Q: Is the plan catalog package-wide (one per `app`), or workspace-wide
(one per `workspace.lzi`)?**

The strawman in this proposal is package-wide — every plan block
collapses into one `PlanCatalog` per app. This matches the
shape of `registry.capabilities` and `app.environments`.

But a workspace with multiple Lazuli apps (e.g. a public API and an
admin console) likely shares one billing model. If catalogs are
package-wide, the workspace has to declare the catalog twice and
keep them in sync (defeating the closed-grammar promise — the LLM
cold-read on either app omits half the picture). If catalogs are
workspace-wide, the single-app case has to look up through
`workspace.lzi` even when none exists, adding ceremony.

**Three options:**

1. **Package-wide only.** Workspace duplication is the author's
   problem; we add a `lazuli inspect --include=workspace --expand=plans`
   union view as a band-aid.
2. **Workspace-wide with package fallback.** When `workspace.lzi`
   exists and declares `plan ...`, every contained app reads from
   it; standalone apps keep their own. This is the natural answer
   for a real multi-app product but introduces a precedence rule
   (workspace > app) that resembles cascade.
3. **Workspace-only when `workspace.lzi` exists.** Force migration —
   if you have a workspace, plans live there. Cleaner closed grammar
   but breaks the "package-as-self-contained-truth" property for
   billing.

The proposal as written picks (1) — package-wide — to preserve
self-contained declarations. The risk is that production-grade apps
that grow into a workspace immediately hit the duplication problem,
which is the first observable cost of being wrong here.

A decision against (1) requires:
- A concrete fixture of a multi-app workspace with shared billing.
- An invariant statement of whether workspace declarations override
  package declarations or merge them (and what the conflict
  resolution rule is — Lazuli's no-cascade principle forbids
  partial diffs).
- Doctor diagnostics for the new failure modes.

**Recommendation:** ship v0.1 as package-wide; revisit at first
pilot evidence of pain. The bar is "≥ 1 real product asks for it
within 2 months of merging this proposal".

A secondary open question — much smaller — is whether
`gate behind plan.feature: <X>` and `gate quota plan.limit: <X>`
should be allowed on `policy` blocks themselves (as a more
declarative way to express "this whole policy category requires
the pro plan"). v0.1 forbids it; the gate lives on the callable.
Pilot evidence will tell.

## Grade-then-fix gate

This proposal must reach **≥ 8.5/10 with no dimension below 7**
via `lazuli-language-architect`. Hard blockers:

- **Boundary leak**: any vendor SaaS name (Stripe, MercadoPago,
  Pagar.me, Recurly, Chargebee) leaking into surface, IR, or
  runtime base — all such concerns are `@plugin/<provider>`
  adapters resolved via `registry.lzi`. The proposal as written
  names them only in §"Out of scope".
- **Wire violation**: any `runtime/go/lazuli/billing/` file growing
  past ~100 effective LOC or duplicating logic that the existing
  pgx pool / `lazuli.Ctx` already provides.
- **Vocabulary drift**: any new `@-namespace` (this proposal adds
  zero), any new closed-catalog keyword without LSP coverage, any
  introduction of cascade semantics on the catalog.
- **Polysemy**: `gate` as a word must not collide with workspace
  `gateway`, `auth_failed_redirect`, deploy gates, or any existing
  `gate` use. (Spot-check: `gate` is not currently a Lazuli keyword
  per `crates/lazuli_lsp/src/lib.rs` keyword tables — confirmed via
  grep before drafting.)
- **Split-kinds regression**: splitting `plan` into separate
  `feature_flag` / `quota` / `trial` kinds is explicitly rejected
  in §"Why one cohesive `plan`". A grading agent proposing the
  split as polish should re-read §1.

If any blocker survives v1, the proposal blocks at design time and
cells S1-S9 do not launch.
