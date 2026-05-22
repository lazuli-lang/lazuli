// Package storage hosts the runtime side of the `@cap.File(...)`
// capability. This file declares the vendor-neutral `Provider`
// contract that `lazuli.ObjectStore("<binding>")` resolves at runtime.
//
// Spec: docs/proposals/hostpoint-complete-roadmap-2026-05-18.md §3.5.
// Boundary discipline: this file never names S3, MinIO, R2, Tigris,
// GCS, or any concrete provider. The S3-specific implementation lives
// in the `@plugin/object-store` adapter package; this contract is the
// stable seam handlers code against.
package storage

import (
	"context"
	"io"
	"iter"
	"time"
)

// Provider is the vendor-neutral object-store contract resolved at
// runtime via `lazuli.ObjectStore("<binding-name>")`. The binding name
// comes from `registry.lzi`'s `bindings.<name>: <Kind>` declaration;
// the concrete implementation behind the resolved adapter lives in a
// `@plugin/<name>` package (e.g. `@plugin/object-store` for the
// S3-compatible adapter).
//
// Implementations MUST be safe for concurrent use.
//
// `bucket` is the logical bucket label (e.g. the slot name from
// `registry.capabilities object_storage <slot>`) — adapters MAY map
// it to a concrete provider-side bucket via per-environment config.
// `key` is the opaque storage-side object identifier; adapters choose
// the concrete shape (S3 object key, filesystem path, GCS object
// name). Generated code never inspects the value.
type Provider interface {
	// PutObject streams `body` into the adapter's backing store
	// under `(bucket, key)`, tagging the object with `contentType`.
	// Implementations MUST be idempotent on the `(bucket, key)`
	// pair so retries do not duplicate state.
	PutObject(ctx context.Context, bucket, key, contentType string, body io.Reader) error

	// GetObject returns a readable stream for the stored object plus
	// the object's recorded `contentType`. Returns `ErrFileNotFound`
	// (from `contract.go`) if `(bucket, key)` is unknown.
	GetObject(ctx context.Context, bucket, key string) (io.ReadCloser, string, error)

	// DeleteObject removes the stored object. Returns
	// `ErrFileNotFound` if `(bucket, key)` is unknown.
	DeleteObject(ctx context.Context, bucket, key string) error

	// HeadObject returns metadata (size, content-type, last-modified)
	// without streaming the body. Used by post-PUT verification flows
	// (SEC-H7): the AutoPhoto confirm path probes the actual object
	// to reject clients that lied about size_bytes / content_type at
	// request time. Returns `ErrFileNotFound` if `(bucket, key)` is
	// unknown — callers should treat that as "client never finished
	// the PUT" rather than a server error.
	HeadObject(ctx context.Context, bucket, key string) (ObjectMeta, error)

	// ListPrefix yields `ObjectMeta` entries for every object whose
	// key shares the supplied `prefix` under `bucket`. Adapters MAY
	// page transparently; callers consume the iterator in order. A
	// non-nil error in the second iter slot terminates the stream.
	ListPrefix(ctx context.Context, bucket, prefix string) iter.Seq2[ObjectMeta, error]

	// PresignedURL mints a signed URL that the bearer can use to
	// `method`-access the object directly. `method` is one of "GET"
	// or "PUT" (closed catalog mirrors the typed-error catalog used
	// elsewhere in this package). `ttl` bounds the URL's validity;
	// adapters that cannot honour `ttl` return `ErrVisibilityMismatch`.
	PresignedURL(ctx context.Context, bucket, key string, ttl time.Duration, method string) (string, error)
}

// ObjectMeta carries the listing-side side information adapters
// surface via `Provider.ListPrefix`. The field set is intentionally
// small — callers needing more provider-specific metadata reach for
// `Provider.GetObject` (which streams the body alongside the
// recorded `contentType`).
type ObjectMeta struct {
	// Key is the opaque storage-side identifier under the parent
	// bucket. Generated code never inspects it.
	Key string

	// Size is the object size in bytes.
	Size int64

	// ContentType is the MIME type recorded at upload after the
	// runtime validated it against `FileContract.Accept`.
	ContentType string

	// LastModified is the adapter-reported wall-clock time of the
	// last write. Adapters that cannot report a modification time
	// return the zero `time.Time`.
	LastModified time.Time
}
