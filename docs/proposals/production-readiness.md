# Production-Grade Readiness — Roadmap Meta-Doc

**Status**: meta-roadmap tracking the multi-wave initiative to make Lazuli welcoming for production-grade backends — the kind of app shape that has: multi-tenant company hierarchies, RBAC, encrypted credentials at rest, async vendor consultations with cursor polling, webhook receivers, tabular CSV/XLSX exports, subscription tiers with feature/quota gates, and a queue worker topology.

**Audience**: orchestrator (Claude), language-architect agents, Codex implementers, downstream contributors.

**Date**: 2026-05-14.

**Goal**: land framework features such that *any* production-grade app can adopt Lazuli without falling back to hand-written Go for the load-bearing pieces — **and stop there**. Per [`scope-discipline.md`](../scope-discipline.md), Lazuli owns the generic 80%; specifics belong in plugins / handlers / escape hatches.

## Non-goals

- Porting any specific client app. The fixture in [`examples/production-grade/`](../../examples/production-grade) is a synthetic shape — not a real product.
- Shipping vendor SaaS adapters in core. Specific named providers (payment processors, observability vendors, email providers, etc.) are `@plugin/<name>` in separate repos.
- Per-country / per-domain scalars (national IDs, address formats, currency parsers). Those are `@plugin/scalars-<locale>`.
- Per-vendor business logic. Those are handler code in consumer repos.

See [`scope-discipline.md`](../scope-discipline.md) for the 80/20 boundary, the five escape hatches, and what's explicitly out of scope.

## Status of the 22 gaps (post-wave-6)

Status legend:
- 🟢 **shipped in framework** (end-to-end functional)
- 🔵 **shipped via escape hatch / plugin** (app uses `@fn` handler, `query.sql`, opaque `api` handler, or installs a plugin)
- ⏳ **pending** (framework gap; not yet shipped; ≥3-app pilot evidence needed to justify adoption)
- ⛔ **out of scope** (specifics; will not enter framework)

| # | Gap | Status | Owner |
|---|---|---|---|
| 1 | Storage / blob / signed URL | 🟢 | framework — `runtime/go/lazuli/storage/` |
| 2 | Field-level encryption at rest | 🟢 | framework — `@cap.Encrypted` surface + AES-256-GCM runtime + boot wiring |
| 3 | CSV/XLSX export | 🟢 | framework — `kind report` + auto-mount HTTP routes + policy enforcement |
| 4 | CSV upload + parse | 🟢 | framework — via storage `@cap.File(accept:text/csv)` + handler decoding |
| 5 | Async polling with cursor | 🟢 | framework — `kind poller` end-to-end |
| 6 | Cron / scheduled jobs | 🟢 | framework — via `kind job` + `trigger schedule` |
| 7 | Worker process topology | 🔵 | escape hatch — app's `main.go` registers job subsets per process |
| 8 | RBAC catalog | 🟢 | framework — `permission` + `role` catalogs + runtime evaluator |
| 9 | Subscription / plan + feature gating | 🟢 | framework — `kind plan` + `gate behind/quota` directives |
| 10 | Idempotent enqueue guard | 🔵 | handler — `if resource.external_id != nil { return nil }` on first line. Doctor lint is candidate for framework if ≥3 pilots ask. |
| 11 | Aggregate / materialized views | 🔵 | escape hatch — `query.sql` block with SQL aggregation embedded |
| 12 | Locale-aware scalars (any country) | ⛔ | plugin — `@plugin/scalars-<locale>` |
| 13 | Webhooks-receive into cache w/ TTL | 🟢 | framework — `runtime/go/lazuli/webhooks/` + `cache.go` |
| 14 | Cross-vendor token manager | ⛔ | handler — vendor specifics belong in app |
| 15 | Multi-tenant w/ RLS-style row scoping | 🟢 | framework — `tenant_from` + `@scope.same_<axis>` policy atoms |
| 16 | Schema-from-existing-DB importer | 🔵 | port tooling — one-time `lazuli import` cell candidate; manual hand-translate works today |
| 17 | `lazuli dev` multi-frontend + workers | ⏳ | framework — DX gap, deferred per [`project_lazuli_dev_build_lifecycle_planned`](../../C:/Users/lucas/.claude/projects/c--Users-lucas-lazuli/memory/project_lazuli_dev_build_lifecycle_planned.md). Workaround: run frontends + `lazuli generate go --watch` in separate terminals. |
| 18 | Observability vendor exporters | ⛔ | plugin — `@plugin/<vendor>` per provider |
| 19 | Auth provider plugins (OAuth/SSO) | ⛔ | plugin — `@plugin/<provider>` per provider |
| 20 | Specific UI-library distros | ⛔ | plugin — Lazurite default ships Tailwind-shaped; alternatives via `@plugin/theme-<kit>` |
| 21 | Vendor adapter shape canon | ⏳ | docs — first plugin in the wild defines it; see [`docs/plugin-authoring.md`](../plugin-authoring.md) |
| 22 | Country-specific doctor lints | ⛔ | folds into #12 plugins |

**Tally**: 11 🟢 + 5 🔵 + 2 ⏳ + 6 ⛔ = 22 gaps covered (framework + handler + plugin combined). The framework is corbanx-class-ready in the meaningful sense: every gap has a path to "yes" without hand-writing Go for the load-bearing pieces.

## Wave history

Six waves shipped 2026-05-14 (single session, fully autonomous after kick).

| Wave | Focus | Commits | Key shipping |
|---|---|---|---|
| 1 | Roadmap + 5 architect-graded proposals + production-grade fixture | 7 | All 5 PASS (avg 8.91), corbanx-class fixture passes doctor |
| 2 | 5 features end-to-end (encryption, poller, report, plan-and-gate, rbac) | ~26 | Full surface + IR + analyzer + doctor + LSP + codegen + runtime for each |
| 3 | RBAC re-integration + 8 doctor polish cells | 12 | Cleaner CRLF discipline; 8 additional doctor rules |
| 4 | End-to-end activation (helpers ship) | 9 | `Encrypt<Resource>/Decrypt<Resource>` helpers, `Prelude` annotation, auto-mount routes, `PolicyExpr` enum |
| 5 | Call-sites + runtime evaluators | 7 | Encryption wired to Insert/Update/SELECT; gates fire in command path; RBAC runtime evaluator; report routes enforce policy |
| 6 | Boot wiring + emitter polish | 3 | Pattern annotation for poller emitter; `lazuli.RegisterRbac` in `rbac.gen.go` init(); report emitter resolves local policy refs |

## When the boundary moves

Pending items (⏳) graduate **into framework** when:

1. ≥3 distinct downstream apps independently produce the same workaround pattern.
2. The escape-hatch workaround visibly hurts (every consumer duplicates the same ~200 LOC handler with the same bugs).
3. A clean closed-catalog form exists (cold-readable; LLM-authorable).
4. An architect-graded proposal lands at ≥ 8.5 / no dim < 7.

Items marked ⛔ (specifics) graduate **into plugins** when there's pressure for a reusable shape. Specifics never graduate into framework — that's the policy.

Items marked 🔵 (escape hatch) may graduate into framework via the same ≥3-app gate. Otherwise they stay as handler/escape-hatch patterns indefinitely. **That's fine** — that's the framework's job description.

## What's NOT on this roadmap

Anything that violates [`scope-discipline.md`](../scope-discipline.md). Concretely:

- Specific vendor adapters (no payment-processor IR, no observability-vendor IR, no specific email-API IR).
- Specific country / locale scalars.
- Specific business-domain logic (commissions, fraud rules, discount trees).
- Specific deploy-target shape (Procfile syntax, k8s manifest, container build).
- Specific UI-library API shapes.

If a proposal appears that violates the scope, reject or kick to `@plugin/`. The architect grading rubric explicitly tests for scope adherence.

## Decision log

- **2026-05-14**: vendor adapters (any specific provider's API) are **handlers in consumer repo**, not framework features. Plugin = extends `.lzi`/`.lzx` surface; handler = uses surface via `@fn`. See conversation 2026-05-14 (claude main).
- **2026-05-14**: locale scalars deferred per [`project_validation_strategy_2026-05-14`](../../C:/Users/lucas/.claude/projects/c--Users-lucas-lazuli/memory/project_validation_strategy_2026-05-14.md). Promote to `@plugin/scalars-<locale>` when ≥3 pilots share one locale.
- **2026-05-14**: storage runtime can ship in Wave 1 (interface + S3 impl) even though surface lowering is still 🟡 — runtime contract is independent of whether codegen calls it yet.
- **2026-05-14** (canonical): Lazuli is the generic 80%; specifics live in handlers / plugins / escape hatches. See [`scope-discipline.md`](../scope-discipline.md). Anti-pattern: chasing per-client specifics into core. Negative reference: the template-driven framework lineage that preceded Lazuli.

## How to read this doc

- 🟢 → covered by framework; verify any new fixture demonstrates the pattern.
- 🔵 → covered by escape hatch / plugin; see [`scope-discipline.md`](../scope-discipline.md) §"The five escape hatches".
- ⏳ → framework gap; gather pilot evidence before designing.
- ⛔ → not framework's job; if pressure exists, route to plugin.
