// Package vectorstore declares the runtime contract for `@plugin/<name>`
// adapters that provide vector storage and similarity search.
//
// The contract is intentionally minimal — wire-thin. Concrete adapters
// (Chroma, pgvector, Qdrant, Weaviate, ...) live in separate
// `@plugin/<name>` repos and register via `lazuli.RegisterAdapter`
// against this package's `VectorStore` interface.
//
// See `docs/proposals/bucket-vectorstore-cycle.md` §V1.
//
// EXPERIMENTAL: shape may grow additive fields before 1.0. Stable
// promotion gated on first pilot consumer (Pleiades v2.2).
package vectorstore

import (
	"context"
	"errors"
)

// VectorStore is the per-provider adapter surface. One adapter
// instance per `vector_store: VectorStore` registry binding.
//
// Implementations MUST be safe for concurrent use across goroutines.
type VectorStore interface {
	// Provider returns the adapter id, e.g. "@plugin/chromadb".
	Provider() string

	// Collection scopes operations to a named bucket. Adapters create
	// the collection lazily on first write if it does not exist.
	Collection(name string) Collection
}

// Collection is the scoped operation surface. One handle per workspace
// per logical bucket (e.g. one Collection per Pleiades workspace).
type Collection interface {
	// Add stores a batch of items. Each item carries a caller-supplied
	// ID, a Vector (the embedding), an optional Document (raw text the
	// vector represents — adapters that store text natively cache it),
	// and optional Metadata. Vector dimension MUST match the
	// collection's pinned dimension; mismatch returns
	// ErrDimensionMismatch.
	Add(ctx context.Context, items []Item) error

	// QueryByVector returns the top-K most similar items to the given
	// embedding. K = q.Limit. Optional filtering via q.Filter. Sorted
	// by descending similarity (Match.Score is implementation-defined
	// but the slice order is the meaningful contract).
	QueryByVector(ctx context.Context, vec []float32, q VectorQuery) ([]Match, error)

	// QueryByText embeds the text via the bound Embedder (resolved
	// from the registry) and delegates to QueryByVector. Convenience
	// wrapper for the common RAG pattern; adapters may pass through
	// to the upstream provider's text-query when one exists.
	QueryByText(ctx context.Context, text string, q VectorQuery) ([]Match, error)

	// Delete removes items by ID. IDs not present in the collection
	// are silently ignored — the operation is idempotent per ID.
	Delete(ctx context.Context, ids []string) error

	// Get fetches items by ID. Missing IDs are silently absent from
	// the returned slice; the caller intersects to detect missing.
	// Common RAG pattern: agent has the slug key but not the embedding
	// row.
	Get(ctx context.Context, ids []string) ([]Item, error)

	// Update re-stores items by ID, replacing vector + metadata in
	// place. Used for re-rank with feedback, metadata back-fill, or
	// re-embedding under a new model. Adapters that lack native update
	// MUST implement as atomic Delete + Add per ID; partial state on
	// failure is not allowed.
	Update(ctx context.Context, items []Item) error
}

// Item is the unit of insertion. ID is caller-supplied (workspace-scoped
// stable identifier — e.g. a Pleiades `item_version.id`).
type Item struct {
	ID       string
	Vector   []float32
	Document string
	Metadata map[string]any
}

// VectorQuery shapes a similarity search.
type VectorQuery struct {
	// Limit is the maximum number of matches to return (top-K).
	// Adapters MAY cap the effective limit; the cap is provider-
	// specific (Chroma: 100 by default; Qdrant: configurable).
	Limit int

	// Filter narrows the candidate set before similarity scoring.
	// Empty Filter = unfiltered.
	Filter VectorFilter
}

// VectorFilter is intentionally limited. Adapters that support richer
// filtering can expand via the `Raw` map (provider-specific escape),
// but the canonical Lazuli surface stays small to keep the cross-
// provider contract honest.
type VectorFilter struct {
	// Tags matches any of the listed tags against the item's
	// `Metadata["tags"]` slice. Provider-side mapping:
	// Chroma: `where {"tags": {"$in": [...]}}`.
	Tags []string

	// Equals matches `Metadata[key] == value` for each entry.
	// Provider-side mapping: Chroma: `where {key: value}`.
	Equals map[string]any

	// Raw is the provider-specific escape hatch. Adapters pass it
	// through to the upstream without interpretation. Use sparingly
	// — it pins the consumer to the adapter.
	Raw map[string]any
}

// Match is a single similarity-search result.
type Match struct {
	ID       string
	Score    float32
	Document string
	Metadata map[string]any
}

// Embedder generates vector embeddings from text. Separate adapter,
// separate registry binding. The `VectorStore` adapter MAY depend on
// the Embedder (for `QueryByText`); the Embedder NEVER depends on the
// VectorStore.
type Embedder interface {
	// Provider returns the adapter id, e.g. "@plugin/openai-embeddings".
	Provider() string

	// Embed converts a single text to a vector. Adapters declare
	// their canonical dimension via DimensionHint; callers should
	// not assume it.
	Embed(ctx context.Context, text string) ([]float32, error)

	// EmbedBatch is the bulk form. Adapters that lack native bulk
	// MUST implement via sequential Embed.
	EmbedBatch(ctx context.Context, texts []string) ([][]float32, error)

	// DimensionHint returns the vector dimension this adapter
	// produces (e.g. 1536 for OpenAI text-embedding-3-small). Returns
	// 0 if dimension is dynamic (in which case doctor
	// `VECTOR-DIM-001` cannot pre-validate alignment).
	DimensionHint() int
}

// Typed errors. Caller-actionable categories are discrete so handlers
// can act differently without parsing strings.
var (
	// ErrVectorStoreUnbound — generated code referenced
	// `vector_store: VectorStore` but no adapter is bound in the
	// registry.
	ErrVectorStoreUnbound = errors.New("vectorstore: no adapter bound in registry")

	// ErrEmbedderUnbound — same for the Embedder binding.
	ErrEmbedderUnbound = errors.New("vectorstore: no embedder bound in registry")

	// ErrCollectionNotFound — read-class operation against a
	// collection the adapter cannot resolve. Write-class operations
	// auto-create the collection; reads fail fast.
	ErrCollectionNotFound = errors.New("vectorstore: collection not found")

	// ErrDimensionMismatch — caller supplied a vector whose dimension
	// does not match the collection's pinned dimension. Non-transient;
	// caller must fix the embedder + collection alignment.
	ErrDimensionMismatch = errors.New("vectorstore: vector dimension does not match collection")

	// ErrVectorStoreUnauthorized — credentials rejected by the
	// upstream provider (bad token / expired key). Non-transient;
	// caller fix is config rotation. Adapters MUST return this on
	// 401/403-class responses.
	ErrVectorStoreUnauthorized = errors.New("vectorstore: provider rejected credentials")

	// ErrVectorStoreRateLimited — upstream applied a rate limit
	// (429 or equivalent). Transient; caller backoff + retry is
	// valid. Adapters MUST return this on 429-class responses.
	ErrVectorStoreRateLimited = errors.New("vectorstore: provider rate-limited the request")

	// ErrVectorStoreUnavailable — catch-all for transport/network/
	// process unreachability that is NOT auth or rate-limit. Maps
	// any other non-success class from the upstream provider.
	ErrVectorStoreUnavailable = errors.New("vectorstore: provider unreachable")
)
