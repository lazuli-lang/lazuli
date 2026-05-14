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
	// Add stores items (vector + metadata). ID is caller-supplied.
	Add(ctx context.Context, items []Item) error

	// QueryByVector returns the K most similar items to the given
	// embedding, optionally filtered.
	QueryByVector(ctx context.Context, vec []float32, q VectorQuery) ([]Match, error)

	// QueryByText embeds the text via the bound Embedder, then
	// calls QueryByVector. Convenience wrapper.
	QueryByText(ctx context.Context, text string, q VectorQuery) ([]Match, error)

	// Delete removes items by ID.
	Delete(ctx context.Context, ids []string) error

	// Get fetches items by ID. Missing IDs are silently absent
	// from the returned slice; use set intersection to detect gaps.
	Get(ctx context.Context, ids []string) ([]Item, error)

	// Update re-stores items by ID, replacing vector + metadata in
	// place. Adapters that lack native update implement as Delete +
	// Add; semantics MUST be atomic per-ID.
	Update(ctx context.Context, items []Item) error
}

// Item is the unit of insertion.
type Item struct {
	ID       string
	Vector   []float32
	Document string         // raw text the vector represents
	Metadata map[string]any
}

// VectorQuery shapes a similarity search. Limit 0 means "use the
// adapter's own default".
type VectorQuery struct {
	Limit  int
	Filter VectorFilter
}

// VectorFilter is intentionally limited. Adapters that support
// richer filtering may expose provider-specific options via Raw,
// but the canonical Lazuli surface stays small (VECTOR-RAW-001).
type VectorFilter struct {
	Tags   []string       // any-of match on item tags
	Equals map[string]any // exact key==value metadata match
	Raw    map[string]any // provider-specific escape hatch
}

// Match is a single similarity-search result.
type Match struct {
	ID       string
	Score    float32
	Document string
	Metadata map[string]any
}

// Embedder generates vector embeddings from text. It is a separate
// adapter with its own registry binding.
type Embedder interface {
	// Provider returns the adapter id, e.g. "@plugin/openai-embeddings".
	Provider() string

	// Embed converts a single text to a vector.
	Embed(ctx context.Context, text string) ([]float32, error)

	// EmbedBatch is the bulk form. Adapters that lack native bulk
	// implement via sequential Embed calls.
	EmbedBatch(ctx context.Context, texts []string) ([][]float32, error)

	// DimensionHint returns the vector dimension this adapter
	// produces (e.g. 1536 for text-embedding-3-small). Returns 0
	// if the dimension is dynamic or unknown at startup.
	DimensionHint() int
}

// Typed errors. Caller-actionable categories are discrete so
// downstream handlers can branch without parsing strings.
var (
	ErrVectorStoreUnbound = errors.New("vectorstore: no adapter bound in registry")
	ErrEmbedderUnbound    = errors.New("vectorstore: no embedder bound in registry")
	ErrCollectionNotFound = errors.New("vectorstore: collection not found")
	ErrDimensionMismatch  = errors.New("vectorstore: vector dimension does not match collection")

	// ErrVectorStoreUnauthorized — credentials rejected by the upstream
	// provider (bad token / expired key). Non-transient; fix is config
	// rotation. Adapters MUST return this on 401/403-class responses.
	ErrVectorStoreUnauthorized = errors.New("vectorstore: provider rejected credentials")

	// ErrVectorStoreRateLimited — upstream applied a rate limit (429 or
	// equivalent). Transient; backoff + retry is valid.
	ErrVectorStoreRateLimited = errors.New("vectorstore: provider rate-limited the request")

	// ErrVectorStoreUnavailable — catch-all for transport/network/process
	// unreachability that is NOT auth or rate-limit.
	ErrVectorStoreUnavailable = errors.New("vectorstore: provider unreachable")
)
