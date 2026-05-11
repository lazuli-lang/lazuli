# Bucket Cache — Scope Resolution (Pre-Cycle)

**Status**: blocker scope-out before running Stages 3-9 of
`/lazuli-bucket-cycle bucket=cache`.

**Audience**: language team (Lazuli core), Lazuli Go runtime team.

**Date**: 2026-05-11.

This document is the analogue of `bucket-auth-cycle.md`'s "auth lowering
scope" and `bucket-jobs-scope.md`'s "jobs/webhooks/event_groups inspect
projection" side-quests: surface authored in the fixture that does not
reach the IR / inspect projection layer. Until this is resolved, the
L1→L2 design for cache cannot land because codegen and runtime have
no typed shape to consume.

The cycle proposal (`bucket-cache-cycle.md`) is written **against the
scope this document defines** — not against the current zero-IR state
of `cache` / `invalidates`.

---

## What's broken (single fact)

The two cache-bucket constructs the canonical fixture authors —
`cache` (child of `query.*`) and `invalidates` (child of `command`) —
are **the last L0+doctor children of `feature` whose IR shape is zero**.

| Construct | Surface | Grammar | IR | Doctor/LSP | Codegen | Runtime | L-level |
|---|---|---|---|---|---|---|---|
| `cache` (child of `query.list`/`query.sql`) | yes (`examples/full-capsule/full-capsule.lzi:100-102`) | text-pattern only (`crates/lazuli_lsp/src/lib.rs:5425-5503`) | **missing IR struct entirely** (no `QueryCache` / `CachePolicy` in `crates/lazuli_ir/src/lib.rs`) | yes — `cache-contract` warns when `key`/`ttl` missing or malformed | text-pattern via `RuntimeQuery.cache: Option<RuntimeCache>` (`crates/lazuli_codegen_spec/src/lib.rs:155-156`) — **hand-built spike fixture, not IR-driven** | yes — `runtime/go/lazuli/cache.go` has functional LRU + TTL + per-tenant scoping + `invalidateQueries` | **L0+doctor + spike runtime** (no IR bridge) |
| `invalidates` (child of `command`) | yes (`full-capsule.lzi:240`, `:259`, `:282`, `:299`) | text-pattern only (`crates/lazuli_lsp/src/lib.rs:5505-5545`) | **missing field on `Command`** (`crates/lazuli_ir/src/lib.rs:500-521`) | yes — `cache-invalidation-contract` validates target shape (`<feature>.query.<name>`, `query.*`, etc.) | text-pattern via `RuntimeCommand.invalidates: Vec<String>` (`crates/lazuli_codegen_spec/src/lib.rs:100`) — **hand-built spike fixture** | yes — `cache.invalidateQueries` wired into command success path (`runtime/go/lazuli/cache.go:111-132`) | **L0+doctor + spike runtime** (no IR bridge) |

Probe:

```bash
cargo run -q -p lazuli_cli -- inspect examples/full-capsule/full-capsule.lzi --format=json \
  | python -c "import json,sys; d=json.load(sys.stdin); \
    [print(q.get('name')) for f in d['features'] for q in f.get('queries',[]) if q.get('cache') or q.get('invalidates')]"
```

returns nothing — neither field exists in the inspect projection.

Compare with the other Tier 3 children that did land (jobs / webhooks /
notifications / event_groups via Route C in commit `e89ff27` →
`299878e`): each one has a typed IR struct, a `parse_*` recogniser, a
doctor pass driven by IR, and an `--expand=*` projection. Cache has
none of those.

---

## Why `cache` and `invalidates` are still text-pattern

Three reasons, in order of weight:

### 1. They live inside `query.*` and `command` — both still text-pattern

`parse_feature_skeleton` (`crates/lazuli_syntax/src/parser.rs:1147-1183`)
covers `agent` (Cut A), `auth` (Phase L Tier 1, commit `e1d8521`),
`@cap.File` field typing (Phase L Tier 2, commit `f60f6bf`), and `job`
/ `webhook` / `notification` / `event_group` (Phase L Tier 3 Route C,
commits `e89ff27` → `299878e`). It does **not** cover `command`,
`resource`, `query`, or `record` — those still flow through the legacy
pest pipeline (`parse_command` at `crates/lazuli_syntax/src/parser.rs:965`,
`parse_query` at `:1006`).

Row 24 of `docs/next-checklist.md` (Phase L, status **prerequisite —
Tier 4 outstanding**) names exactly this gap:

> Tier 4 remains: `parse_command` / `parse_resource` / `parse_query` /
> `parse_record` + lift `defaults.tenancy` from the slice + replace the
> `JobDeclarative.raw_*` carve-out with the shared declarative spine.

`cache` is a `query.*` child; `invalidates` is a `command` child.
Neither parent has a canonical-indent parser yet. Adding cache-only
parsing without lifting the query/command parents would mean a
**third** text-pattern fact family on top of the legacy pest tree —
the same anti-pattern the bucket-jobs-scope doc identifies in its
Route C analysis (`docs/proposals/phase-l-tier-3-job-effect-scope.md:355-360`).

### 2. The spike codegen already projects via a parallel hand-built struct

`crates/lazuli_codegen_spec/src/lib.rs:155-156` declares
`RuntimeCache { key, ttl }`; `crates/lazuli_codegen_go/src/runtime.rs:480-484`
emits `lazuli.CacheSpec{ Key, TTL }` against it. This is the
"runtime spike" path — a hand-built spec that runs the customer
fixture through codegen end-to-end **without going through `lazuli_ir`**.

That spike masked the IR gap because the cache path appeared to work
in dist/go output (it does, for the spike feature). The spike's TODO
is explicit (`crates/lazuli_codegen_spec/src/lib.rs:204-205`):

> Replaced in a future cut with `from_module(&lazuli_ir::Module)`.

When that cut lands (Tier 4 wires `parse_command` / `parse_query` and
the codegen consumes typed IR instead of the spike fixture), the
spike's `RuntimeCache` becomes dead code and `Module → RuntimeFeature`
needs a real `QueryCache` / `command.invalidates` IR axis to read.

### 3. The runtime has more contract than the language

`runtime/go/lazuli/cache.go` (217 lines, shipped in `main`) already
implements:

- LRU eviction with `capacity = 1024` (`:33`, `:52-61`).
- Per-tenant cache keying (`cacheKeyFor` at `:193-203`:
  `<query name>|<org id>|<args hash>`).
- TTL with `expiryAt` defaulting to 60s, negative for no expiry
  (`:179-188`).
- Stable `sha256(json(args))` argument hashing (`hashArgs` at
  `:209-216`).
- `invalidateQueries(names []string)` walking by query name
  (`:111-132`).
- `CacheStats` snapshot (`:135-154`).
- `FlushCache` test helper (`:168-169`).

The language only declares `key "<label>"` and `ttl "<duration>"`. The
runtime decisions about LRU capacity, tenant-keying, argument hashing,
stats, and invalidation cardinality are **not** language-visible. That
is the correct boundary — but it means doctor/LSP today can't
cross-check the runtime's behaviour because there's no typed axis to
cross-check against.

---

## Routes A / B / C

Three ways to close the cache lowering gap, all honouring the
language/runtime boundary:

### Route A — defer until Tier 4 closes (status quo)

Wait for Tier 4 to land `parse_command` and `parse_query`, then add
typed `Command.invalidates: Vec<InvalidationTarget>` and
`QueryCache` as part of the same migration. No isolated cache cut.

**Pros**: smallest scope; one consistent migration; no third
text-pattern fact family; respects the boundary that cache children
inherit from their parent's parser.

**Cons**: cache stays at L0+doctor + spike-runtime until Tier 4 lands.
Tier 4 has no scheduled ETA (`docs/next-checklist.md:60`); blocking
the cache cycle on it could be months. The Lazuli Go runtime contract for
cache (Redis adapter, stale-while-revalidate, coalesce) **does not
require** Tier 4 to ship — runtime-team can extend `cache.go`
independently against the existing `CacheSpec { TTL, Key }` shape.

### Route B — text-pattern fact extraction (the `CommandApprovalFact` shape)

Add `collect_query_cache_facts` + `collect_command_invalidation_facts`
in `crates/lazuli_cli/src/doctor.rs` next to the existing
`CommandApprovalFact` harvesters. Surface as
`registry_cache_defects: Vec<CacheFact>` and similar on `DoctorPackage`.
Inspect projection stays text-derived.

**Pros**: small upfront cost (~1 cell each); doctor immediately gains
cross-feature diagnostics (e.g. `invalidates query.X` resolves against
a query declared in another feature). Promotes cache from "LSP
file-local only" to "doctor cross-feature".

**Cons**: doubles down on the same anti-pattern Phase L is meant to
retire. Cache becomes a fourth text-pattern fact family after
`CommandApprovalFact`, the Tier 3 carve-outs, and the
`collect_feature_symbols` walker. Tier 4 will need to migrate **two**
walkers (cache + invalidates) plus the existing approval one.

### Route C — preemptive IR + parser sub-recogniser inside Tier 3 spine

Add `QueryCache` and `Command.invalidates` to the IR now, and add
`parse_query_cache` + `parse_command_invalidates` as **sub-recognisers**
that Tier 4 will inherit when `parse_command` / `parse_query` land.

Mechanically: add IR types in `crates/lazuli_ir/src/lib.rs` next to
`Query` and `Command`. Lower from text-pattern facts in the legacy
`parse_command` / `parse_query` (pest pipeline at
`crates/lazuli_syntax/src/parser.rs:965-1100`) — the same way
`AuthPassword.algorithm` lowered through the legacy pipeline before
Tier 1 canonical-indent lift (`docs/proposals/bucket-auth-cycle.md`
Stage 4 erratum).

This is the **exact pattern** Tier 3 (Route C) used for jobs:
declarative-body fields landed in IR (`JobDeclarative.raw_*`) with the
canonical parser doing partial work, knowing Tier 4 would fill the
gap. Result: cache becomes typed end-to-end at the IR / inspect /
codegen / runtime layer, **without** the parser slice having to grow
its `query` / `command` recognisers yet.

**Pros**:
- Cache becomes L1-typed today: inspect projects it, doctor cross-checks
  it against typed targets, codegen consumes typed IR (retiring the
  hand-built `RuntimeCache` spike).
- Lazuli Go adapter work (Redis, stale-while-revalidate, coalesce) gets a
  stable IR contract to bind to — no waiting on Tier 4.
- Tier 4 inherits the `parse_query_cache` / `parse_command_invalidates`
  sub-recognisers verbatim; no rework.
- Doctor diagnostics promote from text-pattern to typed at the same
  time (`cache-contract` → `cache_ttl_unit_invalid_diagnostics` etc.).

**Cons**:
- Lowering pass added to the legacy pest pipeline that Tier 4 will
  retire. Code lives for one release cycle in two places (legacy
  lower → IR + future canonical parser → IR). Mitigated by extracting
  the sub-recognisers into shared helpers.
- Slightly more upfront cost than Route B (~2 cells: IR types + legacy
  pest lowering).

### Comparison

| Axis | Route A (defer) | Route B (text-pattern facts) | Route C (preemptive IR) |
|---|---|---|---|
| Upfront cost | zero | ~1 cell each (2 walkers) | ~2 cells (IR + legacy pest lowering) |
| Doctor cross-checks possible | none (stays LSP file-local) | yes, on text-pattern walks | yes, on typed IR |
| Inspect projection | none | text-derived (brittle) | typed `--expand=cache` from IR |
| Codegen impact | spike stays | spike stays | IR-driven; retires spike when Tier 4 lands codegen wiring |
| Lazuli Go runtime unblocked | yes (existing `CacheSpec`) | yes | yes (stable typed contract to bind Redis adapter to) |
| Compat with Tier 4 | aligned (Tier 4 does everything) | misaligned (more walkers to migrate) | aligned (sub-recognisers extract cleanly into Tier 4) |
| Risk of redesign | none | medium — fact shape may diverge from typed IR | low — shape mirrors `Job.tenant_from` precedent (typed today, parser-lifted later) |
| Unblocks the cycle? | no (must wait for Tier 4) | partial (no inspect / codegen typing) | yes |

### Recommendation

**Route C.** Same reasoning as jobs Tier 3 (`bucket-jobs-scope.md`
selected the same route): the IR types lock in the contract, the
Lazuli Go runtime gets a stable shape to extend against, doctor and
inspect both promote uniformly, and Tier 4 inherits the
sub-recognisers without rework. Route A blocks the cycle for an
indefinite time; Route B accumulates Phase L debt for the same
upfront cost as proper typing.

Boundary discipline matches the precedents:

- **Lazuli core** owns: typed `QueryCache { key, ttl, tags?, namespace?,
  coalesce?, stale_while_revalidate?, sliding_ttl?, locks? }` IR;
  `Command.invalidates: Vec<InvalidationTarget>` IR; doctor
  cross-checks; LSP closed-catalog hovers; `--expand=cache` inspect
  projection.
- **Lazuli Go runtime** owns: LRU implementation, Redis adapter, TTL
  parsing, stale-while-revalidate execution, coalesce locking,
  per-tenant key derivation, stats counters. The language declares
  the contract; the runtime executes it.
- **Adapters** own: concrete provider mechanics (Redis client config,
  Memcached driver, Valkey cluster topology) bound through
  `registry.lzi` capability `cache <name>` slots.

---

## Pilot-needed vs Speculative

The roadmap §1.15 lists six cache decorators in one sentence: *"cache
kind explícito (tags, namespace, coalesce, stale-while-revalidate,
sliding TTL, locks)"*. Classified against fixture evidence and pilot
gating:

### PILOT-NEEDED — exercised by the canonical fixture today

| Construct | Fixture evidence | Justification |
|---|---|---|
| `cache key "<label>"` | `full-capsule.lzi:101` | Already authored; needs IR lowering. Today it's just a hint to LSP. |
| `cache ttl "<duration>"` | `full-capsule.lzi:102` | Already authored; needs IR lowering + closed-catalog unit enforcement (`s`, `m`, `h`, `d`). |
| `invalidates <target>` | `full-capsule.lzi:240, :259, :282, :299` | Already authored 4×; targets include `query.list`, `query.global_search`, `query.by_id(id: route.id)`. Needs typed `InvalidationTarget` enum. |

These three are the **scope** of Stage 3 design — six already authored
use sites covering the canonical shape. Stage 3 tightens contracts
(closed catalogs, typed targets, cross-checks), not invents new
decorators.

### PILOT-NEEDED — extension implied by runtime contract

| Construct | Why pilot-needed |
|---|---|
| `cache tags <label>[, <label>...]` | The runtime today invalidates by **query name** only (`cache.invalidateQueries`). For multi-query invalidation patterns (e.g. invalidating "all caches tagged `customer-list`" across features), tags are the canonical primitive. The fixture has 4 invalidate sites that target the same `customer.query.list` from 4 different commands — exactly the pattern tags solve. Promoted to Stage 3. |
| `cache namespace <label>` | Today the runtime keys cache as `<query name>|<org id>|<args hash>`. The query name alone is a flat namespace; multi-app deployments (workspace.lzi) and pack-based features need explicit namespacing to avoid collisions. The fixture's `customer.query.list` vs `customer_tags.query.list_tags` would collide without namespace discipline if names ever shorten. Promoted to Stage 3. |

### SPECULATIVE — defer until a real pilot exercises them

| Construct | Status | Why defer |
|---|---|---|
| `cache coalesce <bool|"<duration>">` | Not in fixture. | Cache stampede protection (request coalescing under high concurrency) is a real production primitive but requires pilot evidence on what shape the language should declare. Open questions: per-key vs per-query coalesce window, max-wait policy, error propagation under coalesce. Today's runtime has no coalesce; the language shouldn't invent shape ahead of runtime support + pilot pressure. |
| `cache stale_while_revalidate "<duration>"` | Not in fixture. | SWR (RFC 5861) is well-understood at the HTTP layer but Lazuli's cache is query-result, not HTTP-cache. Pilot needed: a product where serving slightly-stale data while a background refresh runs is materially better than the current "TTL miss = block on refresh" path. The decorator shape and the `agent` / `query` interaction (does the agent see fresh or stale?) need pilot pressure to settle. |
| `cache sliding_ttl <bool>` | Not in fixture. | Sliding TTL (every access extends expiry) is a real cache pattern but it changes the cache's semantic from "data freshness" to "access recency". Pilot needed: a product authoring a cache where sliding semantics is materially better than a fixed TTL. Without pilot, this is one more knob nobody asked for. |
| `cache locks <strategy>` | Not in fixture. | Distributed cache locking (acquire-on-miss, hold during populate, release on success) is conceptually adjacent to `coalesce` but operates at a different layer. Pilot needed: a product that has both Redis-backed cache and a populate operation expensive enough to warrant locking. Today's in-memory cache doesn't need locks. |
| `cache kind` as top-level construct (vs. decorator on query) | Roadmap §1.15 suggests "kind explícito". | Speculation: would a top-level `cache <name>` kind (separate from `query <name> cache ...`) ever be needed? Use cases would be fragment cache, model cache, view cache — none of which exist in the fixture (no `view`/`fragment`/`partial` primitives). Defer until the runtime grows those primitives. |
| HTTP cache as a kind | Roadmap §2.10 lists "HTTP cache" as DF (runtime). | Boundary: HTTP cache (ETag, conditional GET, Cache-Control) is **runtime mechanics**, not language. The language declares the contract via existing `api` decorators (`rate_limit`, future `cache_control` decorator) — not as a cache kind. |

The pilot-needed subset (the 3 already in fixture + 2 implied by
runtime contract = 5 children) is the scope of Stage 3. Speculative
additions wait for pilot evidence.

---

## Closed-cycle criterion for the cache bucket

Adapted from `docs/roadmap.md:44-53` (8-item §0 checklist) to the
specific shape of the cache bucket:

- [ ] **Fixture authors the full surface.** The canonical fixture
  exercises `cache key/ttl` + `invalidates` (`full-capsule.lzi:100-102,
  :240, :259, :282, :299`). Already true. Stage 3 design adds `tags`
  and `namespace` to **one** query (probably `customer.query.list`) to
  exercise the new typed axes, plus extends `invalidates` to reference
  a tag.
- [ ] **`lazuli check` accepts the syntax.** Already true for current
  shape; new `tags` / `namespace` decorators land as additive (no
  breaking changes to existing `cache key/ttl`).
- [ ] **`lazuli inspect --expand=cache` projects the IR.** New
  projection; required deliverable. Uses the new `QueryCache` IR
  struct + `Command.invalidates: Vec<InvalidationTarget>`.
- [ ] **`lazuli doctor` carries ≥3 cross-feature diagnostics for cache.**
  Concrete proposals (final list lives in `bucket-cache-cycle.md`
  §Doctor/LSP):
  - `cache_ttl_unit_invalid_diagnostics` — TTL literal must use closed
    unit catalog (`s`, `m`, `h`, `d`) or quoted prose.
  - `cache_invalidates_target_unresolved_diagnostics` — `invalidates
    query.X` must resolve to a query in the same feature, or
    `<feature>.query.X` cross-feature.
  - `cache_tags_referenced_but_undeclared_diagnostics` — `invalidates
    tag:X` must match a `cache tags X` on at least one query (any
    feature).
  - `cache_namespace_collision_diagnostics` — two queries with the same
    `namespace <label>` from different features warn (likely
    unintentional cross-feature aliasing).
- [ ] **`lazuli generate` produces Go that compiles.** Codegen for
  `dist/go/<feature>/<query>.gen.go` carrying typed `CachePolicy`
  values consumed by `runtime/go/lazuli/cache.go`. Runtime-team
  parallel deliverable; the spike `RuntimeCache` retires.
- [ ] **Lazuli Go executes end-to-end cache + invalidate.** Runtime-team
  deliverable. Outside language scope, but the runtime contract
  already exists (`cache.go`). Stage 3 design adds the **adapter
  interface** for Redis (and the secondary Memcached/Valkey adapters)
  so the runtime can swap backends.
- [ ] **`eval`/test coverage.** Go integration test
  (`runtime/go/lazuli/cache_test.go`) exercising round-trip cache hit
  + miss + invalidate + tag-based invalidate + per-tenant isolation +
  TTL expiry via `testing/synctest`. Doctor fixture tests for the 4+
  diagnostics.
- [ ] **LSP hover/completion on cache children.** Today the LSP carries
  shape-only diagnostics (`crates/lazuli_lsp/src/lib.rs:5425-5556`).
  Hover + completion on `cache <child>` keywords + closed-catalog
  completion for `ttl` units and `invalidates` target shapes.

The first four items are language-team Stage 3 deliverables. Items 5-6
are runtime-team. Item 7 is shared. Item 8 is language-team but small
(LSP catalog extension, mirroring the auth/storage hovers).

---

## Recommendation

1. **Take Route C** (preemptive IR + parser sub-recogniser inside Tier
   3 spine). Estimated scope: ~2 cells of IR + lowering, mechanical
   because the sub-recogniser shape mirrors the Tier 3 precedents
   (`TenantFromSpec`, `VerifySpec`, `FanoutSpec` all land typed today,
   with the parser slice planning to inherit them at Tier 4).
2. **Scope Stage 3 design to the PILOT-NEEDED subset only.** Five
   children — `key`, `ttl`, `tags`, `namespace`, `invalidates` (typed) —
   three already in the fixture + two implied by the runtime contract.
   Stage 3's job is to tighten the contract (closed catalogs, typed
   targets, cross-checks, doctor diagnostics, LSP hover), not invent
   coalesce/SWR/sliding/locks ahead of pilot evidence.
3. **Defer SPECULATIVE additions** until the bucket cycle surfaces real
   pilot pressure. `coalesce`, `stale_while_revalidate`, `sliding_ttl`,
   `locks`, top-level `cache` kind, HTTP cache as kind — all stay in
   roadmap §1.15 as is. This proposal does **not** promote them.
4. **Run Stage 3 with the closed-cycle criterion above as the
   acceptance gate.** Anything that doesn't shrink the gate counts as
   speculative and goes to backlog.
5. **Update `docs/next-checklist.md` row 24** (`Phase L`) only after
   Route C lands, to record that `cache` + `invalidates` join `auth` /
   `@cap.File` / `job` / `webhook` / `notification` / `event_group` in
   the Tier 3 IR sub-recogniser inheritance. Do not edit row 24 as
   part of this scope doc.
6. **Retire `RuntimeCache` from `crates/lazuli_codegen_spec/src/lib.rs`**
   when codegen consumes the typed `QueryCache` IR (parallel runtime-
   team work; signalled by the cycle's row 3 in `next-checklist.md`).

When Route C is implemented, Stage 3 (design-language) runs on the
shipped substrate and produces a focused proposal covering at most:

- 4-5 doctor diagnostics named in the closed-cycle criterion.
- `--expand=cache` projection.
- The Redis adapter contract (interface signature, not Redis client
  details — those live in `@runtime/redis`).
- A `testing/synctest` round-trip + invalidate Go integration test.

Stage 4 (Lazuli Go codegen) then has a stable IR JSON to consume.

---

## Output: tracked rows for `next-checklist.md`

These rows are **suggestions** for the cycle proposal to land; this
scope doc does not modify the checklist itself.

```
| 38 | Cache bucket cycle Route C — preemptive IR for `cache` + `invalidates` | planned | Add `QueryCache { key, ttl, tags, namespace }` to `crates/lazuli_ir/src/lib.rs` next to `ListQuery`/`SqlQuery`. Add `Command.invalidates: Vec<InvalidationTarget>` (typed enum: `Query { feature, name, args? }` / `QueryWildcard { feature }` / `Tag { label }`). Lower from text-pattern facts in legacy `parse_command`/`parse_query` (pest pipeline at `crates/lazuli_syntax/src/parser.rs:965-1100`). Retire `RuntimeCache` from `crates/lazuli_codegen_spec/src/lib.rs:155-156` when codegen consumes typed IR. See `docs/proposals/bucket-cache-cycle.md` §Linguagem/§IR + `docs/proposals/bucket-cache-scope.md`. |
| 39 | Cache bucket cycle — 4 doctor diagnostics + LSP coverage | planned | `cache_ttl_unit_invalid` (promotion of `cache-contract`), `cache_invalidates_target_unresolved`, `cache_tags_referenced_but_undeclared`, `cache_namespace_collision`. LSP hovers for 5 keywords (`cache`, `key`, `ttl`, `tags`, `namespace`) + closed-catalog completions for TTL units. Depends on row 38. See `docs/proposals/bucket-cache-cycle.md` §Doctor/LSP. |
| 40 | Cache bucket cycle — Lazuli Go runtime + Redis adapter + integration test | planned | Extend `runtime/go/lazuli/cache.go` to support `tags`/`namespace` axes. New `runtime/go/lazuli/cache/redis.go` stub for Redis adapter contract (real client owned by `@runtime/redis` adapter). `runtime/go/lazuli/cache_test.go` `testing/synctest` round-trip + invalidate + tag-based invalidate + per-tenant isolation + TTL expiry. The runtime team owns production Redis pack. Depends on row 38. See `docs/proposals/bucket-cache-cycle.md` §Runtime/§Evals. |
```
