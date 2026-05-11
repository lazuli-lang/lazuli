# Bucket Cycle: Cache (L0→L2)

**Status**: design proposal. Stages 3–9 of the `bucket=cache` pipeline.
Implementation deferred to a separate run with `mode=implement`.

**Audience**: language team (Lazuli core), runtime team (Drusa).

**Date**: 2026-05-11.

**Pilot bucket**: cache is the **first item of the second wave**
(`docs/roadmap.md` line 57). The four §0 pilot buckets (auth, storage,
jobs, observability) closed language-side on 2026-05-11; cache is the
canonical "horizontal expansion" the closed cycle was meant to unlock.
The roadmap §1.15 names cache as one line: *"`cache` kind explícito
(tags, namespace, coalesce, stale-while-revalidate, sliding TTL,
locks)"*. This proposal scopes that line against fixture evidence and
the Drusa runtime contract that already ships.

## Contexto

The canonical fixture authors **two cache-bucket constructs** today,
both at L0+doctor + spike-runtime:

1. **`cache key/ttl`** on a query
   (`examples/full-capsule/full-capsule.lzi:100-102`):
   ```
   query.list list
     ...
     cache
       key customer.list(params)
       ttl "5 minutes"
   ```
   File-local LSP doctor (`crates/lazuli_lsp/src/lib.rs:5425-5503`)
   warns when `key`/`ttl` missing or TTL malformed. No IR struct;
   `lazuli inspect` does not project it. The hand-built codegen spike
   (`crates/lazuli_codegen_spec/src/lib.rs:155-156`) carries
   `RuntimeCache { key, ttl }` and emits
   `lazuli.CacheSpec{ Key, TTL }` against
   `runtime/go/lazuli/cache.go`'s real LRU cache.

2. **`invalidates`** on a command, authored 4× in the fixture
   (`full-capsule.lzi:240, :259, :282, :299`):
   ```
   command create
     ...
     invalidates
       query.list
       query.global_search
   ```
   File-local LSP doctor (`crates/lazuli_lsp/src/lib.rs:5505-5545`)
   validates target shape against the closed set
   `<feature>.query.<name>`, `<feature>.query.*`, `query.<name>`,
   `query.*`. No IR field; spike codegen carries
   `RuntimeCommand.invalidates: Vec<String>` as opaque strings
   (`crates/lazuli_codegen_spec/src/lib.rs:100`).

The lowering route was decided in
`docs/proposals/bucket-cache-scope.md` (canonical input for this run):
**Route C** — preemptive IR + parser sub-recogniser inside the Tier 3
spine. Cache lives inside `query.*` and `invalidates` lives inside
`command`; neither parent is parsed yet by the canonical-indent slice
(Tier 4 territory). Route C adds typed IR + lowers from text-pattern
facts in the legacy pest pipeline, knowing Tier 4 will inherit the
sub-recognisers verbatim.

Scope is **the 5 children pilot-needed by the fixture and runtime
contract**: `key`, `ttl`, `tags` (new), `namespace` (new), and a typed
`invalidates` shape. The roadmap §1.15 speculative children
(`coalesce`, `stale_while_revalidate`, `sliding_ttl`, `locks`) are
dropped from this design and stay in `docs/roadmap.md` §1.15 as is
until pilot evidence surfaces.

The closed-cycle criterion (4 new doctor diagnostics, `--expand=cache`
projection, LSP hover/completion on 5 keywords, Go round-trip +
invalidate test) is the acceptance gate. This proposal specifies the
design for every stage of that gate so the implementation run is
mechanical.

## Baseline (Stages 1-2 inventory)

| Layer | Status | Anchor |
|---|---|---|
| Surface syntax `cache` block | authored 1× | `examples/full-capsule/full-capsule.lzi:100-102` |
| Surface syntax `invalidates` block | authored 4× | `full-capsule.lzi:240, :259, :282, :299` |
| Grammar (`docs/grammar.lzi.md`) | implicit (no formal EBNF for `cache`/`invalidates`) | gap |
| IR (`crates/lazuli_ir`) | **missing entirely** — no `QueryCache`, no `Command.invalidates` | `crates/lazuli_ir/src/lib.rs:500-521`, `:631-703` |
| Analyzer lowering | none (legacy pest pipeline drops both) | `crates/lazuli_syntax/src/parser.rs:965-1100` |
| Parser slice | not extended (Tier 4 territory) | `crates/lazuli_syntax/src/parser.rs:1147-1183` |
| LSP file-local diagnostics | **mature** — 6 cases covered | `crates/lazuli_lsp/src/lib.rs:5425-5594` |
| Doctor cross-feature | **none** (no IR to read) | n/a |
| Inspect projection | **none** — `lazuli inspect` exposes zero cache facts | confirmed via probe |
| Codegen | text-pattern via hand-built spike | `crates/lazuli_codegen_spec/src/lib.rs:155-156, :100`; `crates/lazuli_codegen_go/src/runtime.rs:253-258, :480-484` |
| Runtime (Drusa) | **functional** — 217 lines, LRU + TTL + per-tenant scoping + `invalidateQueries` + `CacheStats` | `runtime/go/lazuli/cache.go:1-217` |
| Highlighting | `cache | key | ttl | invalidates` colored via generic keyword scope | `editors/vscode/syntaxes/lazuli.tmLanguage.json` |
| Adapter slot | not in capability catalog (`object_storage`/`queue`/`mailer`/`event_bus` exist; `cache` missing) | `crates/lazuli_lsp/src/lib.rs:8670` |
| Capability `cache <name>` (registry) | **not declared** in fixture; not in closed catalog | `examples/full-capsule/registry.lzi:12-19` |

**Cross-cutting facts**:

- The 4 `invalidates` sites in the fixture all target
  `customer.query.list` (3×) + `customer.query.global_search` (1×) +
  `customer.query.by_id` (2×). This is the exact pattern that **tags**
  solve: one tag (`customer-list`) replaces 4 explicit query names
  across 4 commands.
- The runtime already keys cache as `<query name>|<org id>|<args hash>`
  (`runtime/go/lazuli/cache.go:193-203`). The language declares zero
  of those three axes today — they are runtime-implicit. This is the
  correct boundary; it means typing the language axis (`tags`,
  `namespace`) lets doctor cross-check without touching runtime
  internals.
- The `customer.query.list` cache (`key customer.list(params)`,
  `ttl "5 minutes"`) is the **only** cached query in the fixture. The
  fixture's other 13 queries (`by_id`, `by_email`, `global_search`,
  `lifetime_value`, `churn_risk`, `list_tags`, etc.) are uncached.
  This proposal does not propose adding more cached queries to the
  fixture; Stage 3 extends the existing one to exercise the new typed
  axes.

## Linguagem (Stage 3)

Surface is canonical for `cache key/ttl/invalidates` — already
authored, already LSP-checked. Stage 3 is **documentation + two
additive decorators (`tags`, `namespace`) + typed-target refinement on
`invalidates`** to tighten the contract.

### Formal grammar (EBNF, draft for `docs/grammar.lzi.md`)

```ebnf
query_cache_block   = "cache" NEWLINE INDENT
                      query_cache_key
                      query_cache_ttl
                      [ query_cache_tags ]            (* new *)
                      [ query_cache_namespace ]       (* new *)
                      DEDENT ;

query_cache_key     = "key" key_expr NEWLINE ;
                      (* key_expr is a label or a function-style template,
                         e.g. `customer.list(params)` — adapter-parsed,
                         language preserves verbatim *)

query_cache_ttl     = "ttl" ( duration_literal | quoted_prose ) NEWLINE ;

query_cache_tags    = "tags" tag_label ( "," tag_label )* NEWLINE ;
                      (* one or more labels; labels are lowercase
                         identifiers, dash-separated allowed *)

query_cache_namespace
                    = "namespace" namespace_label NEWLINE ;
                      (* single label, lowercase identifier *)

duration_literal    = INTEGER ( "s" | "m" | "h" | "d" ) ;
                      (* closed unit catalog; mirrors @cap.Token.ttl
                         and auth.sessions.ttl literals *)

tag_label           = IDENT_LOWER ( "-" IDENT_LOWER )* ;
namespace_label     = IDENT_LOWER ( "-" IDENT_LOWER )* ;

command_invalidates_block
                    = "invalidates" NEWLINE INDENT
                      invalidation_target ( NEWLINE invalidation_target )*
                      DEDENT ;

invalidation_target = query_target | tag_target ;

query_target        = ( feature "." )? "query." query_name [ "(" arg_list ")" ]
                    | ( feature "." )? "query." "*" ;

tag_target          = "tag:" tag_label ;                (* new *)
```

### Slot inventory (required/optional + type + closed catalog)

| Slot | Required | Type | Closed catalog | Fixture anchor |
|---|---|---|---|---|
| `cache key <expr>` | yes (already LSP-warned; **upgrade to required for typed IR**) | label/template | no — adapter-parsed | `full-capsule.lzi:101` |
| `cache ttl <duration>` | yes (already LSP-warned; **upgrade to required**) | duration literal or quoted prose | `s`, `m`, `h`, `d` (matches `is_duration_literal` at `crates/lazuli_lsp/src/lib.rs:3035`); quoted prose accepted | `full-capsule.lzi:102` |
| `cache tags <label>[, <label>...]` | **new** — optional | one or more lowercase labels | no — labels are author-defined; doctor cross-checks references | not in fixture; Stage 3 adds to `customer.query.list` |
| `cache namespace <label>` | **new** — optional | single lowercase label | no — labels are author-defined; doctor warns on cross-feature collision | not in fixture; Stage 3 adds to `customer.query.list` |
| `invalidates <target>` (typed) | optional, repeatable | `InvalidationTarget` enum | `query.<name>`, `query.<name>(<args>)`, `query.*`, `<feature>.query.*`, `tag:<label>` | `full-capsule.lzi:240, :259, :282, :299` |

### Closed-catalog rationale

- `ttl` units `{s, m, h, d}` already enforced by `is_duration_literal`
  (`crates/lazuli_lsp/src/lib.rs:3035`). The IR axis must share the
  catalog. The existing quoted-prose form (`ttl "5 minutes"`) is kept
  as an opt-out — adapters can parse it; doctor doesn't reject. This
  mirrors the auth `sessions ttl` precedent (`bucket-auth-cycle.md`
  Stage 3 table).
- `invalidates <target>` enum closes the set: a target is either a
  query reference (with optional args and wildcards) or a tag
  reference (`tag:<label>`). Anything else is a doctor error.
  Pre-existing LSP catalog (`crates/lazuli_lsp/src/lib.rs:5527-5542`)
  already lists the query-reference shapes; this proposal adds
  `tag:<label>` as the fifth.
- `tags` and `namespace` labels are not closed catalogs — they are
  author-defined identifiers. The closed-catalog discipline applies
  to *kinds* of decorator, not to label vocabulary.

### Example expansion in the fixture

Stage 3 extends `full-capsule.lzi:100-102` to exercise the new typed
axes:

```lazuli
      cache
        key customer.list(params)
        ttl 5m
        tags customer-list, customer-summary
        namespace customer
```

(TTL upgrades from quoted prose `"5 minutes"` to duration literal `5m`
to exercise the closed-catalog axis. Quoted prose stays accepted for
authors who prefer it.)

And extends `full-capsule.lzi:240` to exercise tag-based invalidation:

```lazuli
    emits customer_created from creates
    invalidates
      tag:customer-list
```

The two new decorators are **additive** — every existing `cache
key/ttl` without `tags`/`namespace` keeps parsing; every existing
`invalidates query.X` keeps resolving. The doctor diagnostic
`cache_tags_referenced_but_undeclared_diagnostics` (Stage 8) warns
when `invalidates tag:X` has no declarer.

### Capability slot in `registry.lzi`

Cache joins the existing capability catalog. Stage 3 extends
`examples/full-capsule/registry.lzi:12-19` to declare:

```lazuli
  capabilities
    database postgres
    queue background_jobs
    object_storage files
    mailer transactional
    event_bus internal
    cache shared        # new
    tracing optional
    integration crm
```

The capability binds at boot via `registry.lzi` adapter bindings
(today implicit; an adapter slot like `cache shared via @runtime/redis`
is a follow-up tied to row 40 below). Doctor adds a new diagnostic
`cache_capability_undeclared_diagnostics` when a feature uses `cache`
but the registry lacks a `cache` capability — mirroring `APP-CAP-001`
for `object_storage` (`crates/lazuli_cli/src/doctor.rs:1328-1336`).

## IR (Stage 4)

The IR shape needs one new struct on `Query` variants, one new enum on
`Command`, and additive fields. Recommended placement: next to
`ListQuery` / `SqlQuery` at `crates/lazuli_ir/src/lib.rs:647-703`, and
inside `Command` at `:500-521`.

### IR additions

```rust
// crates/lazuli_ir/src/lib.rs — additive

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryCache {
    /// `cache key <expr>` — opaque template/label preserved verbatim.
    /// Adapter parses; language stores the source string.
    pub key: String,
    /// `cache ttl <literal>` — typed duration with the authored literal
    /// preserved for inspect round-trip.
    pub ttl: CacheTtl,
    /// `cache tags <label>[, <label>...]` — zero or more lowercase
    /// labels. Used by `invalidates tag:<label>` for fan-out
    /// invalidation across queries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// `cache namespace <label>` — single label that scopes the cache
    /// key beyond the runtime's default `<query name>|<tenant>|<args>`
    /// key derivation. `None` falls back to the feature name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum CacheTtl {
    /// `ttl 5m` — typed duration literal. Closed unit catalog.
    Literal(CacheTtlLiteral),
    /// `ttl "5 minutes"` — quoted prose passed to the adapter as-is.
    /// Adapters may reject; language stores verbatim.
    Quoted(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheTtlLiteral {
    Seconds(u32),
    Minutes(u32),
    Hours(u32),
    Days(u32),
}

// Existing Query variants (ListQuery, SqlQuery) gain an additive field:
//   pub cache: Option<QueryCache>,
// Position: right after the `paginate: Option<u32>` field in ListQuery
// (crates/lazuli_ir/src/lib.rs:660) and before `previous_names` in
// SqlQuery so JSON ordering stays readable.
// LookupQuery (`:670`) does NOT gain a cache field — the fixture only
// authors cache on list/sql shapes; lookup caching is a runtime
// decision, not a language axis. (Pilot-needed if lookup caching
// surfaces as a contract.)

// Existing Command struct gains a typed invalidations vec:
//   pub invalidates: Vec<InvalidationTarget>,
// Position: between `emits: Vec<String>` and `tests: Option<TestBlock>`
// in `crates/lazuli_ir/src/lib.rs:514-516`.

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum InvalidationTarget {
    /// `query.<name>` (same feature), `<feature>.query.<name>`
    /// (cross-feature), with optional args.
    Query {
        /// `None` for same-feature short form; `Some(<feature>)` for
        /// fully-qualified targets.
        feature: Option<String>,
        name: String,
        /// `Some(<args>)` for `query.by_id(id: route.id)`-style
        /// targeted invalidation. Args preserved verbatim; doctor
        /// validates path references separately.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        args: Option<String>,
    },
    /// `query.*` (same feature) or `<feature>.query.*` (cross-feature).
    QueryWildcard { feature: Option<String> },
    /// `tag:<label>` — new in this cut. Fan-out invalidation across
    /// any query carrying the tag in its `cache tags` declaration.
    Tag { label: String },
}
```

### Surface → IR mapping

| Surface | IR field | Notes |
|---|---|---|
| `cache key <expr>` on `query.list` | `ListQuery.cache = Some(QueryCache { key, ... })` | additive; existing queries without `cache` keep `cache = None`. |
| `cache ttl 5m` | `QueryCache.ttl = CacheTtl::Literal(CacheTtlLiteral::Minutes(5))` | typed via closed-catalog parse. |
| `cache ttl "5 minutes"` | `QueryCache.ttl = CacheTtl::Quoted("5 minutes".to_owned())` | preserved verbatim; adapter parses. |
| `cache tags X, Y` | `QueryCache.tags = vec!["X", "Y"]` | comma-separated lowercase identifiers. |
| `cache namespace ns` | `QueryCache.namespace = Some("ns")` | single identifier; `None` when omitted. |
| `invalidates query.list` | `Command.invalidates.push(InvalidationTarget::Query { feature: None, name: "list", args: None })` | short form. |
| `invalidates customer.query.by_id(id: route.id)` | `InvalidationTarget::Query { feature: Some("customer"), name: "by_id", args: Some("id: route.id") }` | args preserved as raw string; doctor walks path. |
| `invalidates query.*` | `InvalidationTarget::QueryWildcard { feature: None }` | feature-local wildcard. |
| `invalidates tag:customer-list` | `InvalidationTarget::Tag { label: "customer-list" }` | new shape. |

### Inspect JSON shape (`lazuli inspect --format=json --expand=cache`)

New top-level `--expand=cache` flag in `ExpandSet`
(`crates/lazuli_cli/src/main.rs:95-148`). Projection:

```json
{
  "features": [
    {
      "name": "customer",
      "cache": {
        "queries": [
          {
            "query": "customer.query.list",
            "cache": {
              "key": "customer.list(params)",
              "ttl": { "kind": "Literal", "value": { "Minutes": 5 } },
              "tags": ["customer-list", "customer-summary"],
              "namespace": "customer"
            },
            "origin": "examples/full-capsule/full-capsule.lzi:100"
          }
        ],
        "invalidations": [
          {
            "command": "customer.command.create_lead",
            "targets": [
              { "kind": "Tag", "value": { "label": "customer-list" } }
            ],
            "origin": "examples/full-capsule/full-capsule.lzi:240"
          },
          {
            "command": "customer.command.reassign",
            "targets": [
              { "kind": "Query", "value": { "feature": null, "name": "list", "args": null } },
              { "kind": "Query", "value": { "feature": null, "name": "by_id", "args": "id: route.id" } }
            ],
            "origin": "examples/full-capsule/full-capsule.lzi:282"
          }
        ]
      }
    }
  ]
}
```

Normalisation rules:

- `cache.tags` is always an array even with a single tag (mirrors
  `oauth` / `accept` projections from auth/storage).
- `cache.namespace` is `null` when omitted.
- `invalidations` aggregates per command for easy LLM reading; each
  entry carries an `origin` so consumers can jump to source.
- Features without any cache use have the `cache` key omitted entirely
  (mirrors the `agent` / `storage` / `jobs` conventions).
- Without `--expand=cache` the `cache` key is omitted (mirrors all
  existing expand projections).

### Cross-refs the analyzer must register

| Edge | Source field | Target | Resolution scope |
|---|---|---|---|
| `cache` site ↔ `registry.capabilities.cache` | any query carrying `QueryCache` | the registry must declare a `cache <name>` capability (mirrors `object_storage` precedent) | package-wide; `cache_capability_undeclared_diagnostics` |
| `invalidates query.<name>` ↔ `Query` declaration | `InvalidationTarget::Query` | a `Query` with the matching name exists in the same feature (when `feature` is `None`) or the named feature (when `Some`) | feature-local or cross-feature |
| `invalidates tag:<label>` ↔ `cache tags` declaration | `InvalidationTarget::Tag` | at least one `QueryCache` in any feature declares the tag in its `tags` vec | package-wide |
| `cache namespace` cross-feature collision | `QueryCache.namespace` | two queries in different features sharing the same namespace warn (likely unintentional aliasing) | package-wide |

The cross-ref shape mirrors auth/storage/jobs precedents (feature-local
then package-wide fallback).

## Codegen (Stage 5)

One new generated file per feature consuming `cache` / `invalidates`.
Output is skeletal — Drusa supplies the runtime — and follows the
existing `dist/go/customer/customer.gen.go` style.

### `dist/go/customer/cache.gen.go`

Generated when a feature's queries declare `cache` or its commands
declare `invalidates`.

```go
// path: dist/go/customer/cache.gen.go
// Code generated by Lazuli. DO NOT EDIT.
package customer

import (
    "time"

    "github.com/lazuli/runtime/go/lazuli"
    "github.com/lazuli/runtime/go/lazuli/cache"
)

// ListCacheSpec is the lowered `cache` block on customer.query.list
// (examples/full-capsule/full-capsule.lzi:100-102).
var ListCacheSpec = cache.QuerySpec{
    Key:       "customer.list(params)",
    TTL:       5 * time.Minute,
    Tags:      []string{"customer-list", "customer-summary"},
    Namespace: "customer",
}

// CreateLeadInvalidations is the lowered `invalidates` block on
// customer.command.create_lead (full-capsule.lzi:240).
var CreateLeadInvalidations = []cache.InvalidationTarget{
    cache.TagTarget("customer-list"),
}

// ReassignInvalidations is the lowered `invalidates` block on
// customer.command.reassign (full-capsule.lzi:282).
var ReassignInvalidations = []cache.InvalidationTarget{
    cache.QueryTarget("customer", "list", ""),
    cache.QueryTarget("customer", "by_id", "id: route.id"),
}
```

### Types reused from `runtime/go/lazuli`

- `lazuli.Ctx` (`runtime/go/lazuli/ctx.go`) — request context, actor, tenant.
- `cache.QuerySpec` — new typed struct (Stage 6) replacing the spike's
  `lazuli.CacheSpec`.
- `cache.InvalidationTarget` — new typed interface (Stage 6).
- `cache.TagTarget`, `cache.QueryTarget` — constructors keeping author
  intent recoverable from generated code.

Boundary discipline: codegen never names `Redis`, `Memcached`, or
`Valkey`. The generated code references
`runtime/go/lazuli/cache` capabilities only; provider selection is
adapter-level (`@runtime/redis` / `@plugin/<publisher>/memcached` /
`@adapter.<local>` resolved from `registry.lzi`).

### Spike retirement plan

`crates/lazuli_codegen_spec/src/lib.rs:155-156` (`RuntimeCache`) and
`crates/lazuli_codegen_go/src/runtime.rs:480-484` (CacheSpec emit) get
retired in the same commit that wires `Module → RuntimeFeature` cache
projection. Hand-built fixture goes away; codegen reads from
`lazuli_ir::Module.features[].queries[].cache`. Migration is
mechanical because the IR shape is a superset of the spike's shape.

## Runtime (Stage 6)

The `runtime/go/lazuli/cache.go` already implements LRU + TTL +
per-tenant scoping + invalidate-by-name (217 lines). Stage 6 work is
**three additive capabilities**, none of which break the existing
API:

### `runtime/go/lazuli/cache/contract.go`

- **Capability**: declare typed `QuerySpec`, `InvalidationTarget`,
  `TagTarget`, `QueryTarget` types consumed by every generated file
  helper. Centralises the language-derived shape.
- **Lifecycle**: stateless types.
- **Config**: none.
- **Dependency**: none (stdlib types only).
- **Typed errors**: none (defined by consumers).

This replaces the legacy `lazuli.CacheSpec` (217-line file's `:13-24`)
once codegen retires the spike. During migration, `cache.QuerySpec`
and `lazuli.CacheSpec` coexist; the legacy alias points at the new
struct.

### `runtime/go/lazuli/cache/tags.go`

- **Capability**: extend `cache` from "invalidate by query name" to
  "invalidate by tag". Adds a second index keyed by tag label →
  set of cache entries. Each entry's metadata gains a `tags` slice.
- **Lifecycle**: per-cache-instance state alongside the existing
  `queryCache` global. New API:
  - `(c *cache) invalidateTags(labels []string) int` — drops every
    entry whose tags intersect the provided labels.
  - `(c *cache) putWithTags(...)` — extended put that records tags.
- **Config**: reads `QuerySpec.Tags` at put time.
- **Dependency**: none.
- **Typed errors**: none.

### `runtime/go/lazuli/cache/adapter.go`

- **Capability**: adapter interface. The current in-memory cache is the
  default; Redis (primary) and Memcached/Valkey (secondary) are
  pluggable.
- **Lifecycle**: process-global; bound at boot from `registry.lzi`
  capability `cache <name>` slot.
- **Adapter contract**:

```go
// runtime/go/lazuli/cache/adapter.go (NOT codegen-generated)
type Backend interface {
    Get(ctx context.Context, key string) (value []byte, hit bool, err error)
    Put(ctx context.Context, key string, value []byte, ttl time.Duration, tags []string) error
    InvalidateQueries(ctx context.Context, names []string) (int, error)
    InvalidateTags(ctx context.Context, labels []string) (int, error)
    Stats(ctx context.Context) (QueryStats, error)
}
```

Adapter packages (`@runtime/local` — in-process default, mirrors
today's `cache.go`; `@runtime/redis`; `@plugin/<publisher>/memcached`;
`@plugin/<publisher>/valkey`) implement this interface. Lazuli core
never names any of them.

### Boundary discipline reminder

The language declares the cache contract (`key`, `ttl`, `tags`,
`namespace`, `invalidates`). The runtime owns:

- LRU capacity tuning (`runtime/go/lazuli/cache.go:33` — hardcoded 1024
  today; future runtime config).
- Cache key derivation (`cacheKeyFor` at `:193-203` — adds `namespace`
  to the key when present; otherwise feature name).
- Argument hashing (`hashArgs` at `:209-216`).
- Stats counters (`CacheStats` at `:135-154`).
- Adapter wiring + connection pool management for Redis/Memcached/Valkey.
- Stampede protection, SWR, sliding TTL, locks — all **runtime
  concerns** that may surface as language decorators **only** if pilot
  evidence demands it. Until then they live in adapter config.

## Evals/Testes (Stage 7)

### Doctor fixture — ttl unit invalid

`crates/lazuli_cli/tests/fixtures/cache/ttl_unit_invalid.lzi`:

```lzi
feature x_cache
  domain
    resource Customer
      id: ID required
  query.list list
    cache
      key x.list(params)
      ttl 5 weeks
```

Asserts that doctor emits **exactly one**
`cache_ttl_unit_invalid_diagnostics` at the `ttl 5 weeks` line.

### Doctor fixture — invalidates target unresolved

`crates/lazuli_cli/tests/fixtures/cache/invalidates_unresolved.lzi`:
authors a command with `invalidates query.bogus` where no `query.bogus`
exists. Asserts `cache_invalidates_target_unresolved_diagnostics` fires.

### Doctor fixture — tag referenced but undeclared

`crates/lazuli_cli/tests/fixtures/cache/tag_undeclared.lzi`: authors a
command with `invalidates tag:phantom` where no query declares
`cache tags phantom`. Asserts
`cache_tags_referenced_but_undeclared_diagnostics` fires.

### Doctor fixture — namespace collision

`crates/lazuli_cli/tests/fixtures/cache/namespace_collision.lzi`:
authors two features each with a `cache namespace shared` on a query.
Asserts `cache_namespace_collision_diagnostics` fires as a warning
(not error — cross-feature aliasing can be intentional).

### Go integration test — round-trip + invalidate

`runtime/go/lazuli/cache/cache_test.go` using `testing/synctest`:

```go
// Behaviour:
// 1. Bind cache capability to @runtime/local (in-process).
// 2. Put two entries with TTL=5m, one tagged "customer-list".
// 3. Read both back; assert hits + correct values.
// 4. InvalidateTags(["customer-list"]); assert tagged entry gone, untagged
//    entry still present.
// 5. Advance synthetic clock past TTL; assert remaining entry expires.
// 6. Verify per-tenant isolation: same query name + different org id
//    produce distinct cache slots.
```

Uses `testing/synctest` for the TTL expiry step (mirrors the storage
bucket test at `runtime/go/lazuli/storage/storage_test.go`).

### LSP test — hover + completion on cache children

`crates/lazuli_lsp/tests/cache.rs`:

- Hover on `cache` keyword shows the contract summary.
- Hover on `ttl` shows the closed unit catalog (`s | m | h | d`) and
  the lowered duration for the authored literal.
- Hover on `tags` shows the cross-cutting "labels are referenced by
  `invalidates tag:<label>`" hint.
- Hover on `namespace` shows the "scopes cache keys across features"
  hint.
- Completion at column after `ttl 5` offers `s`, `m`, `h`, `d`.
- Completion at column after `invalidates `+newline offers
  `query.<name>`, `query.*`, `<feature>.query.<name>`,
  `<feature>.query.*`, `tag:<label>`.

### Inspect contract test

`crates/lazuli_cli/tests/inspect_cache.rs`: runs
`lazuli inspect --format=json --expand=cache examples/full-capsule`
and asserts the `cache` projection matches the JSON shape in Stage 4
(typed `cache` per query, typed `invalidations` per command,
normalisation rules, omission of features without cache).

## Doctor/LSP (Stage 8)

### Diagnostic table

| Code | Severity | Message | Trigger | Test fixture |
|---|---|---|---|---|
| `cache_ttl_unit_invalid_diagnostics` (upgraded from text-pattern `cache-contract`) | error | "`cache ttl <literal>` must use a duration with unit `s`, `m`, `h`, or `d`, or quoted prose. Found `<X>`." | typed lowering rejects the literal | `ttl_unit_invalid.lzi` above |
| `cache_invalidates_target_unresolved_diagnostics` | error | "`invalidates <target>` does not resolve: `<reason>`. Reason ∈ { `query <X> not found in feature <F>`, `feature <F> not found`, `tag:<X> declared by no query` }." | `InvalidationTarget` fails resolution | `invalidates_unresolved.lzi` above |
| `cache_tags_referenced_but_undeclared_diagnostics` | error | "`invalidates tag:<X>` references a tag declared by no `cache tags` block in any feature. Either declare the tag on at least one query or change the invalidation target." | `InvalidationTarget::Tag` resolution fails package-wide | `tag_undeclared.lzi` above |
| `cache_namespace_collision_diagnostics` | warning | "`cache namespace <X>` is declared by queries in `<F1>` and `<F2>`. Cross-feature namespace aliasing is unusual; rename one to avoid accidental cache-key collisions." | two `QueryCache.namespace` in different features share a label | `namespace_collision.lzi` above |
| `cache_capability_undeclared_diagnostics` | error | "`cache` block requires a `cache <name>` capability in `registry.lzi` but none is declared. Add `cache <name>` to `registry.capabilities`." | any query carries `QueryCache` but `app/registry.capabilities` has no `cache` entry | minimal fixture removing `cache shared` from registry |

All five codes register under `is_security_enforcement_code`-style
catalog (`crates/lazuli_lsp/src/lib.rs:9585`) — except they aren't
security; they belong to a new sibling registration
`is_cache_contract_code` (mirrors the existing `cache-contract` /
`cache-invalidation-contract` codes that the new typed diagnostics
replace).

The two legacy text-pattern codes
(`cache-contract`/`cache-invalidation-contract` at
`crates/lazuli_lsp/src/lib.rs:5470, :5511, :5540, :5575, :5591`) stay
as **file-local LSP** fallback for pre-typed source (e.g. a freshly
authored query where the parser hasn't lowered yet). Once Route C
lowering lands, doctor reads typed IR; LSP retains the text-pattern
walker so live editing still gives feedback.

### Diagnostic anchors (where to add)

- `cache_ttl_unit_invalid_diagnostics` — typed promotion of the
  existing `crates/lazuli_lsp/src/lib.rs:5492-5500` warning. Runs at
  both LSP (file-local) and doctor (cross-feature, reads
  `QueryCache.ttl`).
- `cache_invalidates_target_unresolved_diagnostics` — new pass in
  `crates/lazuli_cli/src/doctor.rs` next to the existing approval/auth
  cross-checks. Resolves `InvalidationTarget::Query` against
  `Feature.queries` (and other features for fully-qualified targets).
- `cache_tags_referenced_but_undeclared_diagnostics` — same pass,
  package-wide tag index built from every feature's
  `QueryCache.tags`.
- `cache_namespace_collision_diagnostics` — same pass, package-wide
  namespace index. Severity is warning, not error: cross-feature
  aliasing is unusual but not invariant-breaking.
- `cache_capability_undeclared_diagnostics` — same pass; new sibling
  of `APP-CAP-001` (`crates/lazuli_cli/src/doctor.rs:1328-1336`).

### LSP hovers (new entries)

Add to `KEYWORD_HOVER` in `crates/lazuli_lsp/src/lib.rs:11741` (which
already has terse one-liners for `cache` and `invalidates`):

| Keyword | Hover summary |
|---|---|
| `cache` | "Query cache contract: `key <expr>` + `ttl <duration>` (+ optional `tags`, `namespace`). Used by the runtime to memoize query results per tenant. Requires a `cache <name>` capability in `registry.lzi`." |
| `key` (in `cache` context) | "Cache key template. Stored verbatim and surfaced in logs / cache stats; the runtime always prepends `<feature>.query.<name>|<tenant>|<args hash>`." |
| `ttl` (in `cache` context) | "Cache time-to-live. Closed unit catalog: `s`, `m`, `h`, `d`. Quoted prose (`\"5 minutes\"`) also accepted; adapters parse it." |
| `tags` (in `cache` context) | "Cache tags: comma-separated labels used by `invalidates tag:<label>` for fan-out invalidation across queries. Labels are author-defined lowercase identifiers." |
| `namespace` (in `cache` context) | "Cache namespace label. Scopes the cache key beyond the default `<feature>.query.<name>` to avoid collisions in workspace / pack deployments. One namespace per query." |
| `invalidates` | "Command invalidation contract: list of cache targets to invalidate after the command succeeds. Targets: `query.<name>`, `query.*`, `<feature>.query.<name>`, `<feature>.query.*`, `tag:<label>`." |

Closed-catalog completions to add:

- `ttl <int>` (in `cache` context) → `s`, `m`, `h`, `d`.
- `invalidates `+newline → `query.<name>`, `query.*`,
  `<feature>.query.<name>`, `<feature>.query.*`, `tag:<label>`.

### Namespaces (`is_allowed_reference_namespace`)

No new `@<namespace>` required. Cache references are positional
(`query.<name>` / `tag:<label>`), not namespaced via `@`. The closed
namespace catalog at `crates/lazuli_lsp/src/lib.rs:2062-2064` stays as
is (`@role`, `@scope`, `@actor`, `@policy`, `@semantic`, `@cap`,
`@pii`, `@key`, `@fn`, `@hook`, `@validator`, `@adapter`, `@client`,
`@query_modifier`, `@anchor`, `@llm`, `@tool`, `@trace`).

### Highlighting (`editors/vscode/syntaxes/lazuli.tmLanguage.json`)

`cache | key | ttl | tags | namespace | invalidates` already covered
by the generic keyword scope; add `tags` and `namespace` explicitly so
the cache context surfaces them with the same emphasis as `ttl` and
`key`. The `tag:` prefix in `invalidates tag:customer-list` hits the
existing operator/punctuation scope.

### Capability kind closed catalog

Add `cache` to the capability-kind closed set at
`crates/lazuli_lsp/src/lib.rs:8670` (currently `database` /
`object_storage` / `queue` / `mailer` / `event_bus` / `tracing` /
`integration`). Mirrors the storage bucket precedent — once the IR
projects the capability, the catalog must list it.

## Critério de "ciclo fechado"

- [ ] Fixture exercises typed `cache key/ttl/tags/namespace` on
  `customer.query.list` and tag-based `invalidates tag:customer-list`
  on at least one command (Stage 3 extends `full-capsule.lzi` per the
  inline examples above).
- [ ] `lazuli check examples/full-capsule` accepts the syntax after
  Route C lands (no regression on existing pre-typed `cache key/ttl`
  / `invalidates query.*` declarations — additive only).
- [ ] `lazuli inspect --format=json --expand=cache examples/full-capsule`
  shows the IR shape described in Stage 4 for `customer`.
- [ ] `lazuli doctor` emits the 5 named diagnostics on the matching
  fixtures.
- [ ] `lazuli generate` produces `dist/go/customer/cache.gen.go` that
  compiles under `runtime/go/lazuli/cache`. The hand-built
  `RuntimeCache` spike retires in the same commit.
- [ ] Drusa runs round-trip cache hit + miss + tag-based invalidate +
  per-tenant isolation end-to-end (runtime-team deliverable).
- [ ] `runtime/go/lazuli/cache/cache_test.go` synctest test passes for
  round-trip + TTL expiry + tag invalidation + tenant isolation.
- [ ] LSP hovers + completion cover the 6 keywords + 2 closed
  catalogs from Stage 8.

## Próximo passo

Human approval of this proposal **and** the scope proposal
(`docs/proposals/bucket-cache-scope.md`) + a separate `mode=implement`
run that lands Route C: add `QueryCache` / `CacheTtl` /
`CacheTtlLiteral` / `InvalidationTarget` to
`crates/lazuli_ir/src/lib.rs` next to `ListQuery` / `SqlQuery` /
`Command`, lower from text-pattern facts in the legacy pest pipeline
(`crates/lazuli_syntax/src/parser.rs:965-1100`), add `ExpandSet.cache`
(`crates/lazuli_cli/src/main.rs:95-148`), retire `RuntimeCache` from
`crates/lazuli_codegen_spec/src/lib.rs`, and ship the five doctor
diagnostics + LSP entries. Drusa team owns
`runtime/go/lazuli/cache/{contract,tags,adapter}.go` and the Redis
adapter in `@runtime/redis` in parallel.

When Tier 4 lands `parse_query` / `parse_command`, the legacy lowering
retires and the `parse_query_cache` / `parse_command_invalidates`
sub-recognisers extract cleanly into the canonical-indent slice (same
pattern Tier 3 used for `JobDeclarative.raw_*`).

## Rows sugeridas para `docs/next-checklist.md`

Three additions, formatted to match the existing table:

```
| 38 | Cache bucket cycle Route C — preemptive IR for `cache` + `invalidates` | planned | Add `QueryCache { key, ttl, tags, namespace }` to `crates/lazuli_ir/src/lib.rs` next to `ListQuery`/`SqlQuery`. Add `Command.invalidates: Vec<InvalidationTarget>` typed enum (`Query { feature, name, args? }` / `QueryWildcard { feature }` / `Tag { label }`). Lower from text-pattern facts in legacy `parse_command`/`parse_query` (pest pipeline at `crates/lazuli_syntax/src/parser.rs:965-1100`). New `--expand=cache` projection. Retire `RuntimeCache` spike from `crates/lazuli_codegen_spec/src/lib.rs:155-156` when codegen consumes typed IR. See `docs/proposals/bucket-cache-cycle.md` §Linguagem/§IR + `docs/proposals/bucket-cache-scope.md`. |
| 39 | Cache bucket cycle — 5 doctor diagnostics + LSP coverage | planned | `cache_ttl_unit_invalid` (typed promotion of `cache-contract`), `cache_invalidates_target_unresolved`, `cache_tags_referenced_but_undeclared`, `cache_namespace_collision` (warning), `cache_capability_undeclared`. LSP hovers for 6 keywords (`cache`, `key`, `ttl`, `tags`, `namespace`, `invalidates`) + closed-catalog completions for TTL units and invalidation targets. Capability kind closed set extended with `cache`. Depends on row 38. See `docs/proposals/bucket-cache-cycle.md` §Doctor/LSP. |
| 40 | Cache bucket cycle — Drusa runtime + Redis adapter contract + integration test | planned | Extend `runtime/go/lazuli/cache.go` into `runtime/go/lazuli/cache/{contract,tags,adapter}.go` carrying typed `QuerySpec`/`InvalidationTarget`/`Backend` interface. `Backend` interface enables Redis (primary, `@runtime/redis`) / Memcached/Valkey (secondary) adapter packs. `runtime/go/lazuli/cache/cache_test.go` `testing/synctest` round-trip + tag invalidate + per-tenant isolation + TTL expiry. Drusa-team owns production Redis adapter. Depends on row 38. See `docs/proposals/bucket-cache-cycle.md` §Runtime/§Evals. |
```
