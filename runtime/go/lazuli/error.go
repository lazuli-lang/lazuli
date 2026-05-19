package lazuli

import (
	"context"
	"fmt"
)

// Error is the canonical error envelope returned to clients. It mirrors the
// DSL error contract (`errors` block + `error <Name> status <code>`).
//
// Status flows to HTTP status codes; Code is a stable string clients can
// branch on; Message is the human-facing explanation.
//
// EXPERIMENTAL: typed error hierarchy may grow additive variants before 1.0.
// Stable promotion gated on first pilot consumer doing errors.As(err,
// &lazuli.FieldError{}) in production code. See docs/release-policy.md
// Stability tiers.
//
// Child types carry ErrorBase as a named field, not an embed (per ADR-1).
type Error struct {
	Base ErrorBase

	Status  int    // HTTP status; 0 lets the runtime choose by Code
	Code    string // stable identifier ("policy_denied", "validation_failed")
	Message string // human-readable; literal English fallback (built-in last-resort)
	Data    any    // Deprecated: transitional structured payload from the v1 flat envelope.

	// MessageKey is the resolved translation key for this error envelope,
	// fully-qualified at the producer site (e.g. "account.choose_role_signin_required").
	// Codegen emits the per-command/policy override into this slot before the
	// HTTP boundary; the runtime resolver reads it as the L1 lookup. Empty
	// when the producer wants the resolver to fall through to feature/built-in
	// layers. See IR Error-Vocab proposal §5.1.
	//
	// EXPERIMENTAL: additive; existing fields untouched.
	MessageKey string
	// MessageArgs carries optional ICU/strings.Replacer arguments for
	// parameterized keys (e.g. {"duration": "30s"}). v1 leaves rendering
	// engine to the i18n adapter; v2 may add structured args.
	MessageArgs map[string]any
}

// ErrorBase carries the structural context shared by all typed errors.
// Codegen-generated handlers populate Capsule/Feature/Kind/Op/Source
// automatically from the context's SourceTag (D5).
//
// EXPERIMENTAL: subject to change before 1.0.
type ErrorBase struct {
	Code    string  // canonical code, e.g. "field_invalid"; backward-compatible with v1 Error.Code
	Surface Surface // discriminator: who is responsible (see Surface consts)
	Status  int     // HTTP status; 0 = derived from Code via canonical mapping
	Message string  // human-readable
	Capsule string  // app capsule name, e.g. "crm"
	Feature string  // feature name within the capsule, e.g. "customer"
	Kind    string  // "command" | "query" | "job" | "webhook" | "notification"
	Op      string  // op name within the kind, e.g. "create_customer"
	Source  string  // ".lzi:line:col" - stripped per app.observability.error_source
	Cause   error   // wrapped underlying error; participates in errors.Is/As chain
}

// ErrorBaseFromContext constructs an ErrorBase, populating the
// Capsule/Feature/Kind/Op/Source fields from any SourceTag attached to ctx via
// WithSource. Codegen-emitted error wrap helpers call this.
//
// If ctx has no SourceTag, the fields are left empty. If base already carries
// values, caller-provided values win.
func ErrorBaseFromContext(ctx context.Context, base ErrorBase) ErrorBase {
	tag := SourceTagFromContext(ctx)
	if base.Capsule == "" {
		base.Capsule = tag.Capsule
	}
	if base.Feature == "" {
		base.Feature = tag.Feature
	}
	if base.Kind == "" {
		base.Kind = tag.Kind
	}
	if base.Op == "" {
		base.Op = tag.Op
	}
	if base.Source == "" {
		base.Source = tag.Source
	}
	return base
}

// Surface routes the AI debug response:
//   - UserDSL: read .lzi, debug user-side logic
//   - LibInternal: file issue against Lazuli core
//   - CodegenBug: route to codegen-go owner
//   - AdapterRuntime: escalate to the adapter author
//
// Closed 4-variant enum per ADR-2. Sized uint8.
//
// EXPERIMENTAL: subject to change before 1.0.
type Surface uint8

const (
	SurfaceUserDSL Surface = iota
	SurfaceLibInternal
	SurfaceCodegenBug
	SurfaceAdapterRuntime
)

// String returns Surface's stable wire spelling.
func (s Surface) String() string {
	switch s {
	case SurfaceUserDSL:
		return "user_dsl"
	case SurfaceLibInternal:
		return "lib_internal"
	case SurfaceCodegenBug:
		return "codegen_bug"
	case SurfaceAdapterRuntime:
		return "adapter_runtime"
	default:
		return "unknown"
	}
}

// Error implements the error interface.
func (e *Error) Error() string {
	code := e.Base.Code
	message := e.Base.Message
	if code == "" {
		code = e.Code
	}
	if message == "" {
		message = e.Message
	}
	if message != "" {
		return fmt.Sprintf("%s: %s", code, message)
	}
	return code
}

// Unwrap returns the typed envelope's underlying cause.
func (e *Error) Unwrap() error { return e.Base.Cause }

// Common error codes used by the runtime itself. Generated commands or
// validators may produce additional codes from the DSL `errors` block.
//
// DB-INTEGRITY: the four `*_violation` codes are emitted by `classifyDBError`
// (handle_db_errors.go) when a pgx INSERT/UPDATE/DELETE surfaces a Postgres
// constraint violation. They replace the legacy `code:"internal"` + raw
// SQLSTATE wire string with a stable, localizable envelope.
const (
	CodePolicyDenied        = "policy_denied"
	CodeRateLimited         = "rate_limited"
	CodeValidationFailed    = "validation_failed"
	CodeNotFound            = "not_found"
	CodeTenantMismatch      = "tenant_mismatch"
	CodeInternal            = "internal"
	CodeBadRequest          = "bad_request"
	CodeMethodNotAllowed    = "method_not_allowed"
	CodeIntegrationError    = "integration_error"
	CodeUniqueViolation     = "unique_violation"
	CodeForeignKeyViolation = "foreign_key_violation"
	CodeNotNullViolation    = "not_null_violation"
	CodeCheckViolation      = "check_violation"
)
