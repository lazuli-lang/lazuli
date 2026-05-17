# Scope Discipline — What Lazuli Owns vs What Apps Own

**Status**: canonical operating policy. Mirrored in `CLAUDE.md` / `AGENTS.md` for AI agents and in the project's contributor guidelines for humans.

**Audience**: language designers (proposals, IR, codegen), runtime maintainers, plugin authors, and downstream app authors.

**Date**: 2026-05-14.

## TL;DR

Lazuli aims to absorb **~80% of any production-grade backend** through generic primitives. The remaining **~20% is app-specific** and belongs in **handlers, escape hatches, or plugins** — not in the framework.

We are not chasing every demand of every downstream consumer. We will not absorb per-vendor adapters, per-domain scalars, per-country regulations, or per-product UX flows into the core. Those are an app's job.

This rule has teeth: any proposal that tries to make the framework conform to a single app's specifics is a **scope violation** and will be rejected (or kicked to `@plugin/<name>`).

The negative reference is **the framework lineage that preceded Lazuli** (template-driven full codegen that grew unbounded as every client demand was absorbed). Lazuli exists to NOT repeat that.

## The 80/20 boundary

### What the framework owns (the generic 80%)

Closed-catalog primitives shared across virtually every production-grade backend:

- **Resources** (data model, types, relations, soft-delete, retention, multi-tenancy).
- **Commands** (write effects, idempotency, audit, lifecycle transitions).
- **Queries** (list/lookup/sql, filters, pagination, cache, search).
- **Lifecycle** (state machines on resources, terminal states, invariants).
- **Jobs** (queued or scheduled, retry, backoff, idempotency).
- **Webhooks** (HMAC-verified inbound, envelope, dispatch).
- **Pollers** (async cursor loops, retry, terminal status).
- **Notifications** (channel-agnostic dispatch).
- **APIs** (typed HTTP routes, policy, rate_limit).
- **Reports** (CSV/XLSX export with signed URLs).
- **Agents** (LLM contract surface).
- **Storage** (object stores, signed URLs, blob upload).
- **Encryption at rest** (AES-256-GCM field-level cipher).
- **RBAC** (role/permission catalogs with closure-baked codegen).
- **Plans + gates** (subscription tiers, feature gates, quota gates).
- **Multi-tenancy** (tenant pinning, row scoping, audit trail).
- **i18n / translation**.
- **Migrations** (auto-emit DDL).
- **Observability** (tracing, logging, audit).
- **Auth** (session, password, MFA basics).

Anything in this catalog is **the framework's responsibility**. New entries enter only when they (a) generalize to ≥3 distinct app shapes and (b) survive the architect-grading rubric (≥ 8.5 / no dim < 7) via an architect-graded proposal in the operational archive.

### What apps own (the specific 20%)

Specifics that vary per app, per client, per industry, or per geography. Apps own these via the **five escape hatches** documented below.

Examples — **non-exhaustive**, listed only to make the boundary concrete:

- Per-vendor API integrations (any specific SaaS provider's HTTP client).
- Per-vendor webhook payload schemas + HMAC quirks.
- Per-vendor async-resolution quirks (gender flip, retry-after with custom headers, etc.).
- Per-country scalars (national ID formats, phone formats, address layouts, locale-specific currency parsing).
- Per-domain UI flows (specific paywall layouts, specific onboarding wizards).
- Per-business-rule logic (this company's commission table, this product's discount tree).
- Per-deploy-target configuration (Procfile shape, container build, k8s manifests).
- One-time port tooling (migrating from a specific legacy ORM).
- Custom analytics dashboards / report column expansions specific to one product.

If a downstream consumer wants any of the above, the answer is **"write a handler"** or **"author a plugin"** — not "extend the framework grammar."

### What plugins own (the shared-but-not-canonical middle)

Reusable code that's **broader than one app** but **not canonical enough for core**:

- `@plugin/<saas-provider>` adapters for paid commodities (payment processors, observability vendors, email providers, push providers, feature-flag managers).
- `@plugin/scalars-<locale>` kits (national ID / phone / address / currency parsers for one locale).
- `@plugin/<reusable-widget>` for cross-app frontend components (e.g., a tax-id input field, a signature pad).
- Canonical kit-packs that demonstrate a recurring pattern (e.g., a starter CRM block).

Plugins live in **separate repos** at `github.com/lazuli-lang/lazuli-plugin-<name>` (public) or under the consuming organization (private). See `docs/plugin-authoring.md`.

## The five escape hatches

When an app needs something the framework doesn't model, these five mechanisms cover it:

1. **`@fn.<name>` handlers**. Every command/query/job/webhook/poller surface accepts `@fn.<name>` references that lower to user-written Go in `features/<feature>/handlers/<name>.go`. This is the primary escape hatch. Examples: vendor API call, custom validation, derived field computation, multi-step orchestration.

2. **Opaque `handler "./path.go"` on `api` blocks**. For full-control endpoints that don't fit the typed `command`/`query` shape (e.g., a vendor's bespoke webhook callback with three nested polymorphic envelopes), declare `api foo / handler "./api/foo.go"`. The generated server mounts the route; the handler owns the body.

3. **`query.sql` raw blocks**. For aggregations, joins, window functions, or any SQL the typed `query.list` / `query.lookup` shape doesn't cover, declare `query.sql foo / @file.foo.sql`. The framework wires the route and policy; the SQL is yours.

4. **`extends @anchor` / `slot`** view extensibility. Frontend cells, drawers, list columns, and detail panels can be extended by sibling features via `extends @anchor.<name> / slot <position> / block @client.<widget>`. App owns the widget; framework owns the slotting.

5. **User-owned `main.go`** (or `dist/go/main.go` overrides). Generated `main.go` is replaceable. For runtime topology decisions — which subset of jobs to register per worker process, custom middleware ordering, integration-test hooks — the app's `main.go` (or a wrapper around the generated entry point) is the contract surface.

Together these cover every specific the framework doesn't model. **There is no sixth escape hatch coming**; if a need can't be expressed via these five, the framework grammar likely needs to grow (and that goes through a proposal).

## Decision tree

When you encounter a gap (an app can't express what it needs):

```
Is it generic across ≥3 distinct app shapes?
├─ YES → propose framework adoption.
│   ├─ Architect-graded proposal (target ≥ 8.5).
│   ├─ Surface + IR + analyzer + doctor + codegen + runtime.
│   └─ Lands in core.
└─ NO → is it a commodity SaaS / locale kit / reusable widget?
    ├─ YES → ship as `@plugin/<name>` in a separate repo.
    │   └─ See `docs/plugin-authoring.md`.
    └─ NO → it's app-specific.
        └─ Use one of the five escape hatches.
```

The "≥3 distinct app shapes" gate is the most-cited rejection criterion. Pilot evidence from one product does not justify framework adoption.

## What's explicitly out of scope

The framework will NOT absorb (today, and as a matter of policy):

- Specific named SaaS providers (payments, email, observability, feature flags, maps). These belong in plugins.
- Specific national/locale identifiers (any country's tax id, phone format, address parser). These belong in `@plugin/scalars-<locale>`.
- Specific vendor webhook schemas, HMAC variations, or rate-limit quirks. These belong in handlers.
- Specific UI flows (paywall layouts, onboarding wizards, branded checkout). Apps own UX.
- Specific business-domain logic (commission tables, discount trees, fraud heuristics). Apps own domain logic.
- Specific deploy targets (Procfile, k8s manifest, container build). Apps own ops.
- Specific frontend libraries' API shapes (a particular component-library's button props). Plugins or apps own these.
- One-time legacy-port tooling (migrating from a specific predecessor ORM). One-off scripts, not framework features.
- **CRDT / collaborative document sync** (Y.js, Hocuspocus, Automerge, server-side operational transform). 3-OSS evidence (Plane Hocuspocus + ToolJet Yjs gateway + AppFlowy CRDT-block) **confirms the plugin boundary holds**, not that it should move. See "no CRDT in core" boundary affirmation below.

If you find yourself writing IR types or doctor rules named after a specific vendor, country, or product — you're outside the framework's scope. Stop.

## Boundary affirmation — no CRDT in core (2026-05-17)

The OSS Mirror Wave 1+2 audit (`lazuli-ops/docs/proposals/synth-oss-mirror-wave-1-2-2026-05-17.md` §Theme 2 + §3.E) surfaced three independent products implementing CRDT-style document sync:

| Product | Library | Audit reference |
|---|---|---|
| Plane | Hocuspocus (Y.js-based) | `apps/live/src/hocuspocus.ts` — audit §pattern #12 |
| ToolJet | Yjs gateway | `server/src/modules/events/yjs.gateway.ts` — audit §pattern #12 |
| AppFlowy | CRDT block + awareness | `flowy-document/` cluster — audit §patterns #5, #8, #11, #12, #18 |

Naive cross-pilot inference would read this as "3-OSS confirms Y.js belongs in core". The opposite is correct: **when three independent products use the same external library with the same plugin-adapter pattern, that is evidence the plugin boundary is correctly placed, not evidence the boundary should move.** Y.js is mature, vendor-stable, single-vendor, and the wire is well-known — exactly the shape `@plugin/*` was designed for.

**The rule (canonical from 2026-05-17 forward):**

> CRDT-style collaborative document sync (operational transform, conflict-free replicated data types, awareness/presence streams, doc-version vectors, undo/redo over replicated state) does NOT enter Lazuli core grammar. It lives in `@plugin/yjs`, `@plugin/hocuspocus`, `@plugin/automerge`, or product-specific plugin namespaces. The `channel <name>` primitive (`lazuli-ops/docs/proposals/bucket-realtime-scope.md`) is the closest the framework gets — typed messages over a tenant-scoped channel, NOT replicated document state.

This rule applies even when:
- A pilot ships CRDT functionality via plugin (the plugin is the answer; do not "graduate" it into core).
- A fourth, fifth, or Nth OSS product adds CRDT (the rule's premise is shape-stability, not popularity).
- Lazuli's own future tooling needs collaborative editing for its own DSL (the LSP/editor uses `@plugin/*` like any other consumer).

The boundary affirmation is a **deletion**, not a deferral. The previously-deferred `presence` / `coedit channel kind` / `doc_sync` entries in `bucket-realtime-scope.md` MVP are not "waiting for ≥3 confirmations"; they are **rejected on principle** and the 3-OSS evidence is the citation.

If a future proposal contradicts this rule, the proposal must cite this section, articulate why the wire-thin principle no longer applies, and re-grade `scope-discipline.md` itself — not merely add a sibling primitive.

## When the boundary moves

The boundary is not frozen. Things can graduate **into** the framework when:

1. **Pilot evidence shows generalization**: ≥3 distinct downstream apps independently produce the same workaround pattern, in shapes that compose. (One app's pressure is not enough; that's the most common mistake.)
2. **The workaround in escape hatches is visibly painful**: not "we could do it in a handler" but "every consumer is duplicating the same 200 LOC handler with the same bugs."
3. **A clean closed-catalog form exists**: the abstraction passes the cold-readability test (an LLM or a new contributor can author it from the surface alone, without reading framework internals).
4. **An architect-graded proposal lands**: the `<name>` proposal (operational archive) ≥ 8.5 with no dimension < 7. See `feedback_grade_before_commit.md`.

Conversely, things can be **demoted out** of the framework when:

- A primitive shipped on speculation collects zero real usage after two production-app pilots.
- A primitive turns out to encode one client's specifics in framework dress (this is the most dangerous case — visible only in retrospect; the audit is `git log` + namespace policy violations).

**Open invitation**: if you have evidence that something currently in an escape hatch is generic enough to belong in the framework, write a proposal. Don't add ad-hoc adoption. The architect-grading loop exists to keep the bar honest.

## The negative reference

The framework lineage that preceded Lazuli (template-driven full codegen) became unmaintainable specifically because **every downstream client's demand was absorbed into the template**. The codebase grew to embed every payment processor, every webhook flavor, every locale's tax id, every country's address format. Eventually the templates carried more vendor-specific code than generic infrastructure, and the maintenance cost exceeded the value.

Lazuli's discipline: **the templates carry the generic primitives; the wires carry external code; vendor specifics live in plugins or apps**. The wire-thin principle in `CLAUDE.md` is the operational version of this rule.

If a proposal feels like it's chasing a specific client's specifics — even ours, even a paying one — push back. The framework's value is in being generic. Surrendering that to absorb specifics is the failure mode that already killed one framework.

## Cross-references

- `CLAUDE.md` / `AGENTS.md` — operating manual; namespace policy + wire-thin discipline.
- `docs/design-principles.md` — Rule Zero ("Vocabulary Over Mechanism").
- `docs/invariants.md` — closed grammar/IR catalogs.
- `docs/plugin-authoring.md` — how to ship a plugin.
- the `production-readiness` proposal (operational archive) — meta-roadmap with status per gap (framework / plugin / handler).
- Per-feature proposals in the `*-vocab` proposal (operational archive) — examples of architect-graded scope decisions.
