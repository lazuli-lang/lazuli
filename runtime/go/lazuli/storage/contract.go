// Package storage hosts the runtime side of the `@cap.File(...)`
// capability. The language-level shape lowered by
// `crates/lazuli_analyzer/src/lib.rs:parse_cap_file_type` mirrors
// the structs declared here so generated `*.gen.go` code references
// typed contract values rather than raw strings.
//
// Boundary discipline: this package never names a concrete provider
// (S3, GCS, MinIO, Azure Blob, R2). The `ObjectStore` interface is
// implemented by adapter packages (`@runtime/local`, `@runtime/s3`,
// `@plugin/...`) selected at boot from `registry.lzi`.
package storage

import (
	"errors"
	"time"
)

// FileContract is the lowered `@cap.File(max_size:...,accept:...,
// visibility:...,signed_ttl:...)` shape. Generated code instantiates
// one `FileContract` per `@cap.File(...)` site and passes it to the
// upload/signed-URL/fetch helpers.
type FileContract struct {
	// Resource is the `<Resource>` name when the site is a
	// resource field. Empty when the site is an api output.
	Resource string

	// Field is the `<field>` name when the site is a resource
	// field. Empty when the site is an api output.
	Field string

	// API is the `<api>` name when the site is an api output.
	// Empty when the site is a resource field.
	API string

	// MaxSize is the maximum upload size in bytes. The Lazuli IR
	// canonicalises the size literal (`25mb`) to a u64 byte
	// count (`26214400`) at lowering.
	MaxSize int64

	// Accept lists the permitted MIME types. `Family == "*"` /
	// `Subtype == "*"` are valid wildcard markers; the typed
	// helpers below match accordingly.
	Accept []MimeType

	// Visibility is the URL visibility contract. Required on
	// api outputs; defaults to `Private` on resource fields.
	Visibility FileVisibility

	// SignedTTL is the signed URL expiry duration. Non-zero
	// only when `Visibility == Signed`; `IssueSignedURL`
	// refuses any other combination.
	SignedTTL time.Duration
}

// MimeType mirrors `lazuli_ir::MimeType`. `Family` is the IANA
// top-level family (`text`, `image`, `application`, `audio`,
// `video`, `font`) or the wildcard `*`.
type MimeType struct {
	Family  string
	Subtype string
}

// String renders the MIME type as `family/subtype`.
func (m MimeType) String() string {
	return m.Family + "/" + m.Subtype
}

// Matches reports whether two MIME types intersect under the
// family/subtype wildcard rules.
func (m MimeType) Matches(other MimeType) bool {
	familyOK := m.Family == other.Family || m.Family == "*" || other.Family == "*"
	subtypeOK := m.Subtype == other.Subtype || m.Subtype == "*" || other.Subtype == "*"
	return familyOK && subtypeOK
}

// FileVisibility mirrors `lazuli_ir::FileVisibility`. The closed
// catalog is enforced at the language level; the runtime never
// invents new values.
type FileVisibility int

const (
	// VisibilityPrivate gates downloads behind the command's
	// policy. The runtime mounts a download endpoint that
	// re-checks the policy per request.
	VisibilityPrivate FileVisibility = iota
	// VisibilityPublic exposes an unguessable but un-gated URL.
	// Suitable for CDN-served static assets.
	VisibilityPublic
	// VisibilitySigned issues a time-limited signed URL whose
	// expiry is bound by `FileContract.SignedTTL`.
	VisibilitySigned
)

// String renders the visibility as the canonical lowercase token
// from the Lazuli source.
func (v FileVisibility) String() string {
	switch v {
	case VisibilityPublic:
		return "public"
	case VisibilityPrivate:
		return "private"
	case VisibilitySigned:
		return "signed"
	default:
		return "unknown"
	}
}

// Key is the opaque storage-side identifier returned by
// `Upload` and consumed by `IssueSignedURL` / `FetchPrivate`.
// Adapters choose the concrete shape (S3 object key, filesystem
// path, GCS object name); generated code never inspects it.
type Key string

// FileRef is the typed reference stored in a resource row for a
// field declared with `@cap.File(...)`. Codegen emits resource
// columns as `storage.FileRef` (pointer when the field is optional)
// so the row carries the storage `Key` plus the resolved upload
// metadata. The bound `ObjectStore` resolves the `Key` to bytes on
// demand; generated code never inspects the payload.
//
// `ContentType` is the MIME type recorded at upload after the
// runtime validated it against `FileContract.Accept`; `Size` is
// the byte count recorded at upload. Both are persisted alongside
// the `Key` because callers (download handlers, signed-URL
// builders) need them without a round-trip to the object store.
type FileRef struct {
	Key         Key
	ContentType string
	Size        int64
}

// Metadata carries the uploader-supplied side information that
// adapters may persist alongside the bytes (filename, declared
// content-type from the multipart frame, custom headers). The
// runtime validates `ContentType` against `FileContract.Accept`
// before invoking the adapter.
type Metadata struct {
	Filename    string
	ContentType string
	Size        int64
}

// Typed error sentinels. Each maps to a stable
// `lazuli.Error.Code` so transports can translate them into the
// right wire status without each adapter rewriting the mapping.
var (
	// ErrFileSizeExceeded is returned when an upload exceeds
	// `FileContract.MaxSize`. Maps to HTTP 413.
	ErrFileSizeExceeded = errors.New("lazuli/storage: file_size_exceeded")

	// ErrFileMimeRejected is returned when the multipart
	// `Content-Type` does not intersect with
	// `FileContract.Accept`. Maps to HTTP 415.
	ErrFileMimeRejected = errors.New("lazuli/storage: file_mime_rejected")

	// ErrFileNotFound is returned when a `Key` does not resolve
	// to a stored object. Maps to HTTP 404.
	ErrFileNotFound = errors.New("lazuli/storage: file_not_found")

	// ErrSignedURLExpired is returned when a signed URL is
	// followed after its TTL elapsed. Maps to HTTP 410.
	ErrSignedURLExpired = errors.New("lazuli/storage: signed_url_expired")

	// ErrVisibilityMismatch is returned when a helper is called
	// against the wrong `Visibility` (e.g. `IssueSignedURL` on a
	// `Private` contract). Compile-time prevented by the doctor
	// diagnostic `cap_file_visibility_signed_ttl_mismatch`; this
	// is the runtime safety net.
	ErrVisibilityMismatch = errors.New("lazuli/storage: visibility_mismatch")
)
