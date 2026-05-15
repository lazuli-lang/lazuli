# Bucket Cycle: Vectorstore (L0→L2)

**Status**: design proposal — runtime contract + adapter binding
surface. No new DSL `kind`; vector ops are imperative runtime
helpers, not declarative.

**Revision**: v0.2 (2026-05-13). Applied all five polish items
from Architect's `grade-v1` (BLOCK @ 8.3 → target ≥ 9.0):
P1 (`VECTOR-RAW-001` doctor warning), P2 (collection dimension
source-of-truth pinned), P3 (`Count` dropped, `Update` + `Get`
added), P4 (facade renamed `lazuli.Vector` → `lazuli.Vectorstore`),
P5 (split `ErrVectorStoreUnavailable` into `Unauthorized` +
`RateLimited` + the catch-all). See grade report at
[LAZ-13](/LAZ/issues/LAZ-13#document-grade-v1) for the original
verdict.

**Audience**: Lazuli Go runtime team, plugin adapter authors
(`@plugin/chromadb`, `@plugin/pgvector`, `@plugin/qdrant`, etc.),
downstream product authors who need similarity search.

**Date**: 2026-05-13.

**Pilot bucket**: greenfield infrastructure cycle. Adds one new Go
runtime package (`runtime/go/lazuli/vectorstore/`) with a closed
interface, plus the doctor + integration-binding wiring (`registry`
block in `.lzi`) so authors pick a provider with one line.

**Companion**: `docs/proposals/bucket-mcp-cycle.md` (the MCP cycle
shipping in the same Phase B wave — Pleiades v2 will use both;
this bucket exposes vector ops, MCP exposes them across process
boundaries).

**First consumer**: Pleiades v2 (Phase C of the strategic pivot at
`~/.claude/projects/c--Users-lucas-lazuli/memory/project_strategic_pivot_2026-05-13.md`).
Pleiades v2 stores embeddings for `item.content` and queries
similarity for "find related items" + RAG context export.

**First adapter**: `@plugin/chromadb` shipping in
`lazuli-lang/lazuli-plugin-chromadb` as a wire of the Chroma Go
client. Adapter is ~80 LOC per the plugin-authoring template.

---

## Contexto

Pleiades v2 needs semantic search. Slug-based jump-to handles
addressing (you know the key); slug-based search handles
prefix/tag/full-text. Neither handles the "what items are *similar
in meaning* to this one" query — the RAG retrieval shape.

Vector stores solve that. The current Go ecosystem has several
mature options that all share the same operational surface:

| Provider | Shape | When |
|---|---|---|
| **Chroma** (`github.com/amikos-tech/chroma-go` or upstream) | Standalone process, HTTP API, Python-first lineage | First adapter we ship — clean Go client, OSS, low op overhead |
| **pgvector** (`github.com/pgvector/pgvector-go` extension on existing postgres) | Postgres extension, SQL-native | When the app already runs postgres and wants zero-extra-infra |
| **Qdrant** (`github.com/qdrant/go-client`) | Standalone, gRPC, production-grade | Larger scale, multi-tenant isolation needs |
| **Weaviate** (`github.com/weaviate/weaviate-go-client`) | Standalone, GraphQL, schema-first | Niche; only if a product asks |

All four share the same core surface: collections (named buckets),
add (embed + store), query (similarity search by vector or by
text), delete, count. Differences are config + transport. **That's
the wire-thin opportunity** — Lazuli defines the contract once,
adapters ship in separate `@plugin/<name>` repos, downstream apps
swap providers via the `registry` block.

The bucket lands **six cells (V1–V6)** that together yield:

- **A new `vectorstore.VectorStore` interface** in
  `runtime/go/lazuli/vectorstore/` (~60 LOC including types and
  typed errors).
- **An `Embedder` sub-interface** so embedding generation is a
  separate adapter (e.g. OpenAI, Cohere, local-model). Lazuli does
  not embed itself — embedding is provider work.
- **`registry` block wiring** so authors declare
  `vector_store @plugin/chromadb` once in `registry.lzi` and the
  framework injects the binding into any handler that calls
  `lazuli.Vectorstore.Collection("...").Query(...)`.
- **First adapter `@plugin/chromadb`** in a separate repo. ~80 LOC
  wire of the Chroma Go client.

There is **no new `.lzi` `kind`** for vectorstore. Vector ops are
imperative — embedding, querying, indexing — and don't decompose
into declarative authoring surface the way `notification` or
`feature.command` do. Authors call vectorstore from
`@fn.<name>` Go handlers, the same way they call `database` or
`storage`. Rule Zero check: vocabulary already exists
(`registry`/integrations + Go runtime helpers); we don't invent
new mechanism.

---

## Baseline (Stages 1-2 inventory)

| Surface | Today | Anchor | L-level |
|---|---|---|---|
| Vector storage in Lazuli runtime | none | `runtime/go/lazuli/` no `vectorstore/` package | **missing** |
| Embedder surface | none | n/a | **missing** |
| `registry` integration binding for vectorstore | not defined | `crates/lazuli_ir/src/lib.rs` Integrations | **missing** |
| Doctor `VECTOR-*` codes | none | `crates/lazuli_cli/src/doctor.rs` | **missing** |
| Example fixture `examples/vectorstore-smoke/` | does not exist | n/a | **missing** |
| `@plugin/chromadb` adapter repo | does not exist | n/a | **missing** |
| Pleiades v2 dependency on vector ops | not consumed yet | n/a | **future** |

**Cross-cutting fact**: additive widening. No existing surface
changes semantics; the bucket is purely greenfield.

---

## Surface design

### No new DSL `kind`

Vectorstore is bound via the existing `registry` integration
mechanism. Example:

```lzi
registry
  integrations
    vector_store: VectorStore
      adapter @plugin/chromadb
      environments development, staging, production
      credentials platform
        url env.CHROMA_URL
        token env.CHROMA_TOKEN

    embedder: Embedder
      adapter @plugin/openai-embeddings
      environments development, staging, production
      credentials platform
        api_key env.OPENAI_API_KEY
        model "text-embedding-3-small"
```

The shape mirrors `payment_gateway: PaymentGateway` and
`channel_dispatcher: ChannelDispatcher` already in use. Doctor
`VECTOR-REGISTRY-001` enforces that exactly one
`vector_store: VectorStore` binding exists per app (or zero if no
feature uses it).

### Author-side Go runtime call

From any `@fn.<name>` handler:

```go
package handlers

import (
    "context"
    "lazuli.dev/runtime/lazuli"
)

func SearchSlugs(ctx context.Context, query string, tags []string) ([]SlugMatch, error) {
    // Embed the query (Embedder adapter resolved from registry).
    embedding, err := lazuli.Vectorstore.Embedder.Embed(ctx, query)
    if err != nil { return nil, err }

    // Query the vector store (VectorStore adapter resolved from registry).
    results, err := lazuli.Vectorstore.Collection("slugs").
        QueryByVector(ctx, embedding, lazuli.VectorQuery{
            Limit:   20,
            Filter:  lazuli.VectorFilter{Tags: tags},
        })
    if err != nil { return nil, err }

    return mapToSlugMatches(results), nil
}
```

`lazuli.Vectorstore` is the bucket facade — a generated struct in
`dist/go/<app>/main.go` that holds the bound adapters resolved at
boot from the `registry` block. Same pattern as `lazuli.DB`,
`lazuli.Storage`, etc.

---

## L1 — Runtime contract

### `runtime/go/lazuli/vectorstore/contract.go` (~60 LOC)

```go
// Package vectorstore implements the runtime side of the Lazuli
// vector storage bucket. The language has no new `kind`; bindings
// flow through the `registry` integration mechanism. Concrete
// adapters (Chroma, pgvector, Qdrant, ...) sit in `@plugin/<name>`
// packages and bind via `@adapter.vector_store.*` resolution at
// boot.
package vectorstore

import (
    "context"
    "errors"
)

// VectorStore is the per-provider adapter surface.
type VectorStore interface {
    // Provider returns the adapter id, e.g. "@plugin/chromadb".
    Provider() string

    // Collection scopes operations to a named bucket. Adapters
    // create the collection lazily if it does not exist.
    Collection(name string) Collection
}

// Collection is the scoped operation surface.
type Collection interface {
    // Add stores a vector + metadata. ID is caller-supplied.
    Add(ctx context.Context, items []Item) error

    // QueryByVector returns the K most similar items to the given
    // embedding, optionally filtered.
    QueryByVector(ctx context.Context, vec []float32, q VectorQuery) ([]Match, error)

    // QueryByText embeds the text via the bound Embedder, then
    // QueryByVector. Convenience wrapper.
    QueryByText(ctx context.Context, text string, q VectorQuery) ([]Match, error)

    // Delete removes items by ID.
    Delete(ctx context.Context, ids []string) error

    // Get fetches items by ID. Missing IDs are silently absent
    // from the returned slice (caller-side intersection tells us
    // what was missing). Use for re-fetching items an agent
    // already references by ID — common RAG pattern when the agent
    // has the slug key but not the embedding row.
    Get(ctx context.Context, ids []string) ([]Item, error)

    // Update re-stores items by ID, replacing vector + metadata
    // in place. Used for re-rank with feedback, metadata
    // back-fill, or re-embedding under a new model. Adapters that
    // lack native update implement as Delete + Add inside a
    // transaction; semantics MUST be atomic per-ID.
    Update(ctx context.Context, items []Item) error
}

// NOTE on Count: v0.1 included `Count(ctx) (int64, error)` because
// Chroma/Qdrant expose it cheaply, but no consumer named in this
// proposal needs it (Pleiades v2 wants similarity, not counts).
// Dropped from V1. Re-add when a debug/admin dashboard concretely
// asks; the change is purely additive.

// Item is the unit of insertion.
type Item struct {
    ID        string
    Vector    []float32
    Document  string            // raw text the vector represents
    Metadata  map[string]any
}

// VectorQuery shapes a similarity search.
type VectorQuery struct {
    Limit  int
    Filter VectorFilter
}

// VectorFilter is intentionally limited — adapters that support
// richer filtering can expand via a backend-specific raw map, but
// the canonical Lazuli surface stays small.
type VectorFilter struct {
    Tags      []string                // any tag matches
    Equals    map[string]any          // key == value
    Raw       map[string]any          // provider-specific escape
}

// Match is a single similarity-search result.
type Match struct {
    ID       string
    Score    float32
    Document string
    Metadata map[string]any
}

// Embedder generates vector embeddings from text. Separate
// adapter, separate registry binding.
type Embedder interface {
    // Provider returns the adapter id, e.g. "@plugin/openai-embeddings".
    Provider() string

    // Embed converts text to a vector. Adapters declare their
    // canonical dimension via DimensionHint; callers should not
    // assume it.
    Embed(ctx context.Context, text string) ([]float32, error)

    // EmbedBatch is the bulk form. Adapters that lack native bulk
    // implement via sequential Embed.
    EmbedBatch(ctx context.Context, texts []string) ([][]float32, error)

    // DimensionHint returns the vector dimension this adapter
    // produces (e.g. 1536 for OpenAI text-embedding-3-small).
    // Returns 0 if dimension is dynamic.
    DimensionHint() int
}

// Typed errors. Caller-actionable categories are discrete so
// downstream handlers can act differently without parsing strings.
var (
    ErrVectorStoreUnbound      = errors.New("vectorstore: no adapter bound in registry")
    ErrEmbedderUnbound         = errors.New("vectorstore: no embedder bound in registry")
    ErrCollectionNotFound      = errors.New("vectorstore: collection not found")
    ErrDimensionMismatch       = errors.New("vectorstore: vector dimension does not match collection")
    // ErrVectorStoreUnauthorized — credentials rejected by the
    // upstream provider (bad token / expired key). Non-transient;
    // caller fix is config rotation. Adapters MUST return this on
    // 401/403-class responses.
    ErrVectorStoreUnauthorized = errors.New("vectorstore: provider rejected credentials")
    // ErrVectorStoreRateLimited — upstream applied a rate limit
    // (429 or equivalent). Transient; caller backoff + retry is
    // valid. Adapters MUST return this on 429-class responses;
    // optional per-adapter Retry-After header propagation lives
    // on a `RateLimitInfo` interface (deferred — see Out of scope).
    ErrVectorStoreRateLimited  = errors.New("vectorstore: provider rate-limited the request")
    // ErrVectorStoreUnavailable — catch-all for transport/network/
    // process unreachability that is NOT auth or rate-limit. Maps
    // any other non-success class from the upstream provider.
    ErrVectorStoreUnavailable  = errors.New("vectorstore: provider unreachable")
)
```

**Wire-thin acceptance**: this file is the entire `runtime/go/lazuli/vectorstore/`
package. Zero external imports beyond stdlib (`context`, `errors`).
Adapters wire upstream Go clients in their own `@plugin/<name>` repos.

---

## L2 — Codegen wiring

### Bucket facade in generated `main.go`

Codegen extends the existing facade emission. Today `dist/go/<app>/main.go`
emits `lazuli.DB`, `lazuli.Storage`, `lazuli.Notifications`, etc.
Add `lazuli.Vectorstore` when the registry binds a `vector_store`:

```go
// generated
import (
    "lazuli.dev/runtime/lazuli/vectorstore"
    chromadb "github.com/lazuli-lang/lazuli-plugin-chromadb"
    openaiembeddings "github.com/lazuli-lang/lazuli-plugin-openai-embeddings"
)

func init() {
    lazuli.Vectorstore.Store = chromadb.New(chromadb.Config{
        URL:   mustEnv("CHROMA_URL"),
        Token: mustEnv("CHROMA_TOKEN"),
    })
    lazuli.Vectorstore.Embedder = openaiembeddings.New(openaiembeddings.Config{
        APIKey: mustEnv("OPENAI_API_KEY"),
        Model:  "text-embedding-3-small",
    })
}
```

Codegen reads the resolved adapter import path from the
registry's plugin manifest (`Lazurite.toml [plugins]` map),
identical to how `payment_gateway` and `channel_dispatcher`
adapters resolve today.

---

## Cells (V1 – V6)

### V1 — Runtime contract: `runtime/go/lazuli/vectorstore/contract.go`

**File**: new file, single-file output.

**Spec**: the full §"L1 — Runtime contract" content above. Types +
typed errors. Zero external imports beyond stdlib.

**Tests**: trivial unit tests for default values and typed error
identity (`errors.Is(err, ErrVectorStoreUnbound)`).

**Wire-thin gate**: ≤ 110 effective LOC (raised from 80 in v0.1
to absorb the `Update` + `Get` methods and three discrete error
sentinels added per polish items P3 + P5); zero external imports.

**Commit message**: `vectorstore: runtime contract`.

### V2 — IR: `registry` integration accepts `vector_store: VectorStore`

**File**: `crates/lazuli_ir/src/lib.rs` registry integration enum.
The existing integration enum lives in
`crates/lazuli_cli/src/app_manifest.rs::IntegrationKind` (verified
during grade-v1, polish P2); the LSP consumer that surfaces hover
text lives in `crates/lazuli_lsp/src/lib.rs`. Both must update.

**Spec**: add `IntegrationKind::VectorStore` and
`IntegrationKind::Embedder` variants. Same shape as
`PaymentGateway` and `ChannelDispatcher`. The `VectorStore`
variant carries an optional `dimension: Option<u32>` field used
as the first-precedence source of truth by doctor's
`VECTOR-DIM-001` check (see V4). The `Embedder` variant has no
extra fields beyond the common ones. JSON serde + IR roundtrip
tests.

**Commit message**: `ir: vector_store + embedder integration kinds`.

### V3 — Codegen Go: bucket facade `lazuli.Vectorstore`

**File**: `crates/lazuli_codegen_go/src/emitter/facade.rs` (or
existing facade file).

**Spec**: when the registry binds `vector_store` and/or
`embedder`, emit `lazuli.Vectorstore.Store` / `lazuli.Vectorstore.Embedder`
fields in the generated facade. Reuse the existing adapter import
resolution from the plugin manifest.

**Tests**: snapshot test against an
`examples/vectorstore-smoke/` fixture; generated `main.go`
compiles under `go build` once the (yet-to-exist) chromadb
adapter is pinned in `go.mod`.

**Commit message**: `codegen: lazuli.Vectorstore facade for registry bindings`.

### V4 — Doctor: `VECTOR-*` codes

**File**: `crates/lazuli_cli/src/doctor.rs`.

**Codes**:

- `VECTOR-REGISTRY-001` *(error)* — if any handler calls
  `lazuli.Vectorstore.*` but no `vector_store: VectorStore` binding
  exists in registry, doctor errors with the handler location.

- `VECTOR-REGISTRY-002` *(error)* — same for `embedder: Embedder` when
  any handler calls `QueryByText` or `Embed*`.

- `VECTOR-DIM-001` *(warning)* — warns when `Embedder.DimensionHint()`
  disagrees with the **collection's declared dimension**. Source of
  truth (precedence top → bottom; first one found wins):
    1. `dimension <int>` field on the `vector_store: VectorStore`
       integration block in `registry.lzi` (new optional field
       added in V2; the canonical place when authors care).
    2. `[plugins."@plugin/<name>".vectorstore]` table in
       `Lazurite.toml` with key `dimension = <int>` (adapter-level
       default the plugin author ships).
    3. The bound `embedder`'s `DimensionHint()` (runtime probe;
       used only when neither of the above is declared).
  Mismatch is **always warning-only**, never error — the runtime
  call still succeeds when the upstream collection accepts the
  vector. Doctor's job is to surface the drift to the author at
  design time so they can pin the right dimension before a
  production write fails.

- `VECTOR-RAW-001` *(warning, polish item P1)* — when any handler
  call passes a non-empty `VectorFilter.Raw` map (the
  provider-specific escape hatch), doctor walks the IR for those
  call sites and emits a warning citing file + line. The body
  names the bound adapter so the author can confirm the raw shape
  is provider-supported. Adapters MAY register a `RawSchema()`
  hint via the adapter manifest (`Lazurite.toml`
  `[plugins."@plugin/<name>".vectorstore.raw_schema]`) and doctor
  upgrades the warning to **error** if the keys in `Raw` do not
  validate against that schema. Closed-catalog forms (`Tags`,
  `Equals`) skip this check entirely. **Why warning by default**:
  `Raw` is a documented escape, not a violation; the design-time
  signal is "you left the closed surface — confirm you mean to."

**Commit message**: `doctor: VECTOR-* checks (P1+P2 from grade-v1)`.

### V5 — Fixture: `examples/vectorstore-smoke/`

**Files**:
- `examples/vectorstore-smoke/app.lzi` + `registry.lzi` binding a
  mock vector store (in-memory adapter shipped under the fixture
  itself, not a real plugin).
- `examples/vectorstore-smoke/features/search/search.lzi`
  exercising both `QueryByText` and `Add`.
- `examples/vectorstore-smoke/features/search/handlers/{add,search}.go`.

**Acceptance**: `lazuli check` green, `lazuli generate` emits a
compilable Go app, `go test` on the generated app verifies the
in-memory adapter roundtrips one add+query.

**Commit message**: `examples: vectorstore-smoke fixture`.

### V6 — Scaffold `@plugin/chromadb` adapter repo

**Repo**: `lazuli-lang/lazuli-plugin-chromadb`, scaffolded per
`docs/plugin-authoring.md` (separate from this proposal, lands as
a follow-up PR).

**Shape**: `adapter.go` wires `github.com/amikos-tech/chroma-go`
(or upstream Chroma client; verified at impl time) against the
`vectorstore.VectorStore` interface from V1. Implements
`Provider() string` returning `"@plugin/chromadb"`. Auto-registers
via `init()` against `@plugin/chromadb` adapter id.

**Acceptance**: adapter file is < 150 effective LOC. Imports are
exactly: `context`, `errors`, the Chroma client package, and the
Lazuli `vectorstore` contract.

**Commit message** (in plugin repo): `adapter: chromadb wire of
vectorstore.VectorStore`.

---

## Acceptance (cycle-level)

- `examples/vectorstore-smoke/` doctor-green and codegen-green
  (with in-memory adapter shipped in-fixture).
- `cargo check --all-targets` green.
- `go test ./lazuli/vectorstore/...` green.
- All other existing fixtures stay green — vectorstore bucket
  lands as **additive** widening.
- Runtime `runtime/go/lazuli/vectorstore/` is ≤ 110 effective LOC
  (revised from 80 in v0.1 to absorb `Update` / `Get` + the three
  discrete error sentinels from polish P3/P5), zero external
  imports beyond stdlib.
- The `@plugin/chromadb` repo (separate) builds and its acceptance
  test (~1 round-trip add+query against a local Chroma instance)
  passes.

---

## Risks

| Risk | Mitigation |
|---|---|
| Multiple competing Chroma Go clients (community vs upstream) | V6's adapter pins one at scaffold time; if upstream ships an official client later, the adapter becomes a one-file rewrite (~150 LOC) without touching the Lazuli contract. |
| Adapter authors abuse `VectorFilter.Raw` to ship vendor-coupled queries | Two-layer defence (added v0.2 per polish P1): (1) `docs/plugin-authoring.md` rule that adapters document the `Raw` surface in their README; (2) **doctor code `VECTOR-RAW-001`** walks IR call sites with non-empty `Raw` and warns at design time. Adapters MAY register a `RawSchema()` via the plugin manifest, in which case doctor upgrades the warning to an error when keys do not validate. Closed-catalog forms (`Tags`, `Equals`) cover ≥ 90 % of real queries. |
| Embedder dimension mismatch across providers | `DimensionHint() int` is the runtime check. Doctor `VECTOR-DIM-001` warns at design time. **Source-of-truth pinned v0.2 per polish P2**: precedence is (1) `dimension <int>` on the `vector_store: VectorStore` registry block (canonical), (2) `[plugins."@plugin/<name>".vectorstore]` table in `Lazurite.toml` (adapter default), (3) `Embedder.DimensionHint()` (runtime fallback). |
| pgvector requires schema migrations | Out of scope for this bucket. `@plugin/pgvector` ships its migration recipe via the existing `migrations` bucket. The contract surface stays identical. |
| Embedding latency dominates request time | Out of scope for the contract. Embedder adapters handle batching + caching internally; observability bucket already exposes the timing via pprof labels (per `bucket-ai-debug-loop-cycle.md` D7). |
| Embeddings model drift (OpenAI deprecates a model) | Adapter authors version-pin in adapter config. Lazuli core does not pin. |

---

## Out of scope (deferred)

- **`Count(ctx) (int64, error)`** on `Collection`. Removed from V1
  in v0.2 — no consumer in the proposal needs it. Re-add when a
  debug/admin dashboard concretely asks; additive only, contract
  stays stable.
- **`RateLimitInfo` interface** for propagating Retry-After hints
  from `ErrVectorStoreRateLimited`. Useful for client backoff
  intelligence; defer until a real provider exposes it through
  the adapter (Chroma free tier does not rate-limit;
  OpenAI-embeddings rate limit is a separate concern on the
  `Embedder` side).
- **Hybrid search** (vector + keyword in one query). Adapters can
  expose via `VectorFilter.Raw`; canonical surface stays vector-only.
- **Cross-collection queries**. One `Collection(name)` at a time.
- **Bulk re-embedding workflows**. Cron job pattern using the
  existing `job` bucket; not a vectorstore feature.
- **Vector index management** (HNSW tuning, etc.). Provider-side
  concern; adapters expose tuning via config only.
- **Multi-tenant collection partitioning**. The `app.lzi` tenancy
  axis flows through `VectorFilter.Equals` (key = `tenant_id`);
  no separate primitive needed.
- **Local-only embedding adapters** (gguf models on-device). When a
  product asks, ship as `@plugin/<name>` adapter; contract
  unchanged.

---

## Companion docs to update

After this proposal grades-then-fixes through to PASS, the
implementing cells must touch:

- `docs/architecture.md` — add vectorstore bucket to the runtime
  inventory.
- `docs/invariants.md` — add `VectorStore` / `Embedder`
  integration kinds to the registry catalog.
- `docs/plugin-authoring.md` — add a §"Vectorstore adapter" with
  the standard repo shape (Go server + optional web/mobile
  subdirs not applicable; vectorstore is server-only).
- `runtime/go/lazuli/notifications/contract.go` — no change.
- `runtime/go/lazuli/mcp/contract.go` (landing in parallel via
  `bucket-mcp-cycle.md`) — add a code example of calling
  `lazuli.Vectorstore.Collection("slugs").QueryByText(...)` from a
  Pleiades-style MCP tool handler, so the two buckets cross-link
  documentation.

---

## Grade-then-fix gate

Same as `bucket-mcp-cycle.md`: target ≥ 9.0/10 via
`lazuli-language-architect`, hard-block at < 8.5 or any dimension
< 7. Blockers:

- **Wire violation**: any file in `runtime/go/lazuli/vectorstore/`
  > 100 effective LOC with zero external imports. The whole point
  is the contract; adapters do the work.
- **Vendor coupling in core**: any reference to a specific provider
  (chromadb, qdrant, etc.) inside the runtime package. Providers
  ship in `@plugin/<name>` repos.
- **Vocabulary drift**: introducing a new `kind` keyword,
  `@-namespace`, or DSL surface for vector ops. This bucket
  deliberately stays imperative.
- **Cross-bucket leak**: any code in `runtime/go/lazuli/vectorstore/`
  that imports another `runtime/go/lazuli/<bucket>/` package
  (other than the Lazuli error envelope). The contract is
  self-contained.

If any blocker survives v1, the proposal blocks at design time and
the cells V1–V5 do not launch (V6 is a separate repo and can
proceed only after V1 is graded PASS, since the adapter consumes
the contract).
