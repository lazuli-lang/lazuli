// Package cache carries the typed cache contract derived from the
// Lazuli IR. Cache bucket cycle — these types replace the legacy
// CacheSpec spike with a typed `QuerySpec`/`InvalidationTarget` shape
// mirroring `ir::QueryCache` / `ir::InvalidatesSpec`.
//
// Boundary discipline: this file declares the contract that codegen
// consumes; concrete LRU / Redis / Memcached / Valkey adapters
// implement `Backend` in sibling packages (`@runtime/local`,
// `@runtime/redis`, `@plugin/<publisher>/memcached`, etc.).
package cache

import (
	"context"
	"time"
)

// QuerySpec is the lowered `cache` block from a `query.list` /
// `query.sql` declaration. Mirrors `lazuli_ir::QueryCache`.
type QuerySpec struct {
	// Key is the authored `cache key <expr>` template. Stored verbatim;
	// the runtime always derives the effective cache key by prepending
	// `<feature>.query.<name>|<tenant>|<args hash>` and (when present)
	// the Namespace.
	Key string

	// TTL bounds how long a cached entry is served. Zero falls back to
	// the runtime default; negative disables expiry.
	TTL time.Duration

	// Tags are author-defined labels used by `invalidates tag:<label>`
	// for fan-out invalidation across queries.
	Tags []string

	// Namespace scopes the cache key beyond the default `<feature>.
	// query.<name>` to avoid collisions in workspace / pack deployments.
	Namespace string
}

// InvalidationTarget is the closed-catalog target shape lowered from
// `Command.invalidates`. The runtime walks each entry after a command
// succeeds.
type InvalidationTarget interface {
	isInvalidationTarget()
}

// QueryTarget invalidates by named query reference, optionally with
// args. `Feature` is empty for same-feature short-form references.
type QueryTarget struct {
	Feature string
	Name    string
	Args    string
}

func (QueryTarget) isInvalidationTarget() {}

// QueryWildcardTarget invalidates every query in a feature.
type QueryWildcardTarget struct {
	Feature string
}

func (QueryWildcardTarget) isInvalidationTarget() {}

// TagTarget invalidates every cached query whose Tags include the
// label.
type TagTarget struct {
	Label string
}

func (TagTarget) isInvalidationTarget() {}

// Constructor helpers used by generated code.
func Query(feature, name, args string) QueryTarget {
	return QueryTarget{Feature: feature, Name: name, Args: args}
}

func QueryWildcard(feature string) QueryWildcardTarget {
	return QueryWildcardTarget{Feature: feature}
}

func Tag(label string) TagTarget {
	return TagTarget{Label: label}
}

// Backend is the adapter contract. Concrete implementations live in
// `@runtime/local` (in-process LRU; default), `@runtime/redis`,
// `@plugin/<publisher>/memcached`, etc.
type Backend interface {
	Get(ctx context.Context, key string) (value []byte, hit bool, err error)
	Put(ctx context.Context, key string, value []byte, ttl time.Duration, tags []string) error
	InvalidateQueries(ctx context.Context, names []string) (int, error)
	InvalidateTags(ctx context.Context, labels []string) (int, error)
	Stats(ctx context.Context) (QueryStats, error)
}

// QueryStats is a per-instance snapshot of cache health.
type QueryStats struct {
	Entries uint64
	Hits    uint64
	Misses  uint64
	Evicts  uint64
}
