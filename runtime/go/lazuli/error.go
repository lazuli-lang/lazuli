package lazuli

import "fmt"

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
	Message string // human-readable
	Data    any    // Deprecated: transitional structured payload from the v1 flat envelope.
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
const (
	CodePolicyDenied     = "policy_denied"
	CodeRateLimited      = "rate_limited"
	CodeValidationFailed = "validation_failed"
	CodeNotFound         = "not_found"
	CodeTenantMismatch   = "tenant_mismatch"
	CodeInternal         = "internal"
	CodeBadRequest       = "bad_request"
	CodeMethodNotAllowed = "method_not_allowed"
	CodeIntegrationError = "integration_error"
)
