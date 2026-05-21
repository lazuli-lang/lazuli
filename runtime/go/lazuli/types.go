// Package lazuli is the Lazuli runtime library. Generated code in `dist/go/`
// imports this package to declare resources, commands, queries, events, and
// other DSL constructs. The runtime executes those declarations.
package lazuli

import (
	"encoding/json"
	"time"
)

// ID is the canonical identifier type used by every resource.
type ID = int64

// Time is the canonical timestamp type.
type Time = time.Time

// Date is a calendar date with no time-of-day component. Wire format
// is RFC 3339 `YYYY-MM-DD`. Codegen emits this alias; convert to
// `time.Time` at the consumer boundary when arithmetic is needed.
type Date = string

// Semantic value aliases. Each captures the DSL `@semantic.<Type>`
// contract at the type level so resource structs document their
// intent; the validator pipeline (`extensions.validator <name>`) is
// responsible for runtime shape enforcement. Aliases (not new named
// types) keep pgx scan + json marshal trivial — they erase to their
// underlying primitive at compile time. Promotion to named types
// (e.g. for Money arithmetic safety) is deferred until a concrete
// regression makes it load-bearing.
type (
	// Email is a validated email address (RFC 5321 surface).
	Email = string
	// Phone is a validated E.164 phone number.
	Phone = string
	// URL is a validated absolute URL.
	URL = string
	// UUID is an RFC 4122 v4 identifier in canonical 36-char form.
	UUID = string
	// Currency is an ISO 4217 3-letter uppercase code.
	Currency = string
	// Money is a typed integer in minor units (cents, satoshi, etc.).
	// Resource fields combining `@semantic.Money` + `@semantic.Currency`
	// document the pair at the field site; arithmetic safety is the
	// caller's responsibility today.
	Money = int64
	// JSON is a raw JSON document. Aliased to json.RawMessage so that
	// HTTP request/response bodies carry the document inline (object /
	// array / scalar) rather than as a base64-encoded string — Go's
	// stdlib treats `[]byte` as base64 by default, which would force
	// callers to encode every JSON field twice. Use `encoding/json` to
	// decode into a typed shape at the consumer boundary.
	JSON = json.RawMessage
)

// Capability reference aliases. Each names the typed reference shape
// stored in resource columns declared with the corresponding
// `@cap.<Capability>` decorator. The opaque payload format
// (e.g. argon2id PHC string, AES-GCM ciphertext envelope) is owned by
// the adapter bound at boot via `@adapter.<name>`; codegen never
// inspects the payload. Aliases for v0 — promoted to typed wrappers
// when an adapter needs richer metadata than a string carries.
type (
	// Secret is a typed reference to a secret resolved at boot via
	// `@adapter.secrets`. The stored value is opaque to generated code.
	Secret = string
	// HashedRef references a hashed column (argon2id, bcrypt, etc.).
	// The stored value is the canonical PHC string emitted by the
	// configured `@cap.Hashed(algorithm: ...)` adapter; the runtime
	// never exposes the plaintext.
	HashedRef = string
	// EncryptedRef references an encrypted column. The stored value
	// is the AES-256-GCM ciphertext envelope (`nonce || ciphertext || tag`,
	// see runtime/go/lazuli/encryption/aes_gcm.go). Per-resource
	// `Encrypt<Resource>` / `Decrypt<Resource>` helpers emitted into
	// `<feature>/resource.gen.go` thread these bytes through
	// `encryption.ForCtx` at the SQL boundary. The corresponding DDL
	// column type is BYTEA (see migration_ddl.rs).
	EncryptedRef = []byte
	// TokenRef references a tokenised column (payment tokens, PII
	// vault handles, etc.). The token is opaque; the runtime resolves
	// to the underlying value via the configured `@adapter.tokenization`.
	TokenRef = string
)

// TenancyMode declares how a resource is scoped across tenants.
type TenancyMode int

const (
	TenancyNone TenancyMode = iota // no tenant scoping
	TenancyOrg                     // scoped to a single organisation per row
	TenancyTeam                    // scoped to a single team per row
	// TenancyCustom marks resources scoped to a custom axis declared in
	// the DSL (`tenancy workspace`, `tenancy project`, ...). The axis
	// name is not yet threaded through `Resource[T]`; the dispatcher
	// treats `TenancyCustom` as a structural marker until the runtime
	// gains a `TenancyAxis string` slot. Codegen still emits this
	// constant so the captured intent survives the round-trip.
	TenancyCustom
)

// `RateLimit` is declared in `rate_limit.go` per the
// `ir-rate-limit-env-aware` proposal — it carries `Default` + env
// overrides, with `Resolve()` picking the active limit per request.

// Duration is a Lazuli duration literal: "30s", "10m", "1h", "7d", "7y".
type Duration string

// AppCors is the lowered `app.cors` contract emitted by Go codegen.
// AllowOrigins is keyed by environment name (matches `app.environments`);
// the runtime middleware resolves the active set against ctx environment
// at request time. MaxAge is in seconds (codegen converts the DSL duration
// literal "1h" / "30 minutes" / "10s" into seconds).
type AppCors struct {
	AllowOrigins     map[string][]string
	AllowCredentials bool
	MaxAge           int64
}

// AppRoute is the lowered application route contract emitted by Go codegen.
type AppRoute struct {
	Name     string
	Path     string
	PathType string
	From     *string
}

// StringPtr returns a pointer to s for generated optional string fields.
func StringPtr(s string) *string {
	return &s
}

// RetentionSpec declares how long a resource retains rows after soft-delete
// before the runtime applies the terminal action.
type RetentionSpec struct {
	Window Duration
	Then   RetentionAction
}

// RetentionAction is the terminal action applied at the end of the window.
type RetentionAction int

const (
	RetentionDelete    RetentionAction = iota // hard-delete the row
	RetentionAnonymize                        // null PII fields, keep the row
	RetentionArchive                          // move to archive table/storage
)

// Retention is a builder helper for the canonical "Nu then action" form.
//
//	lazuli.Retention("7y").Then(lazuli.Anonymize)
type Retention Duration

// Then completes a retention spec.
func (d Retention) Then(action RetentionAction) RetentionSpec {
	return RetentionSpec{Window: Duration(d), Then: action}
}

// Convenience aliases that read as the DSL would.
const (
	Delete    = RetentionDelete
	Anonymize = RetentionAnonymize
	Archive   = RetentionArchive
)
