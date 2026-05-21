package lazuli

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
)

// internalServerError returns a safe 500 envelope and logs the underlying
// `err` server-side via slog. Use this for any infrastructure failure (DB
// connection refused, encrypt/decrypt fault, JSON marshal failure, etc.) so
// the wire payload never carries internal detail like SQL strings, pgx
// connection URIs, or stack fragments. The `what` string is the stable
// user-facing message the wire emits ("lookup query failed"); the detailed
// `err` goes only to slog so ops can correlate via the request ID. Matches
// the unmapped-error branch in `writeError` (http.go) so both code paths
// converge on the same hide-internal contract.
func internalServerError(err error, what string) *Error {
	slog.Error("lazuli runtime: "+what, "err", err)
	return &Error{Status: 500, Code: CodeInternal, Message: what}
}

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
	CodePolicyDenied           = "policy_denied"
	CodeRateLimited            = "rate_limited"
	CodeValidationFailed       = "validation_failed"
	CodeNotFound               = "not_found"
	CodeTenantMismatch         = "tenant_mismatch"
	CodeInternal               = "internal"
	CodeBadRequest             = "bad_request"
	CodeMethodNotAllowed       = "method_not_allowed"
	CodeIntegrationError       = "integration_error"
	CodeTokenExpired           = "token_expired"
	CodeRefreshInvalid         = "refresh_invalid"
	CodeRefreshRevoked         = "refresh_revoked"
	CodeTheftDetected          = "theft_detected"
	CodeUniqueViolation        = "unique_violation"
	CodeForeignKeyViolation    = "foreign_key_violation"
	CodeNotNullViolation       = "not_null_violation"
	CodeCheckViolation         = "check_violation"
	CodeLifecycleStateMismatch = "lifecycle_state_mismatch"
)

// ErrTenantRequired is returned when a TenancyOrg resource is accessed
// without an attached *Tenant context. Previously this silently returned
// cross-org rows (SEC-H2 / STRIDE T7.T3-T4).
var ErrTenantRequired = errors.New("lazuli: tenant required for this resource")

// ErrRateLimited is returned when a command-level rate_limit bucket is
// exhausted. HTTP-wire: 429.
var ErrRateLimited = rateLimitedError{}

type rateLimitedError struct{}

func (rateLimitedError) Error() string { return CodeRateLimited }

func (rateLimitedError) As(target any) bool {
	le, ok := target.(**Error)
	if !ok {
		return false
	}
	*le = &Error{
		Status:     429,
		Code:       CodeRateLimited,
		Message:    "rate limit exceeded",
		MessageKey: CodeRateLimited,
		Base: ErrorBase{
			Status:  429,
			Code:    CodeRateLimited,
			Message: "rate limit exceeded",
			Cause:   ErrRateLimited,
		},
	}
	return true
}

func tenantRequiredError() error {
	return &Error{
		Status:  500,
		Code:    CodeInternal,
		Message: "internal server error",
		Base: ErrorBase{
			Status:  500,
			Code:    CodeInternal,
			Message: "internal server error",
			Surface: SurfaceLibInternal,
			Cause:   ErrTenantRequired,
		},
	}
}

// ErrLifecycleStateMismatch is returned when a Command with Transitions
// is dispatched but the resource's current lifecycle_state doesn't equal
// the chain's pre-condition (Transitions[0].From). HTTP-wire: 409.
var ErrLifecycleStateMismatch = errors.New("lifecycle_state_mismatch")

type LifecycleStateMismatchError struct {
	Expected    string
	Actual      string
	Transitions []string // names of the chain (for the JSON envelope)
}

func (e *LifecycleStateMismatchError) Error() string {
	return fmt.Sprintf("lifecycle_state mismatch: expected %q, got %q", e.Expected, e.Actual)
}

func (e *LifecycleStateMismatchError) Is(target error) bool {
	return target == ErrLifecycleStateMismatch
}

func (e *LifecycleStateMismatchError) As(target any) bool {
	le, ok := target.(**Error)
	if !ok {
		return false
	}
	*le = &Error{
		Status:  409,
		Code:    CodeLifecycleStateMismatch,
		Message: e.Error(),
		Data: map[string]any{
			"expected_state": e.Expected,
			"actual_state":   e.Actual,
			"transitions":    e.Transitions,
		},
		Base: ErrorBase{
			Status:  409,
			Code:    CodeLifecycleStateMismatch,
			Message: e.Error(),
			Cause:   ErrLifecycleStateMismatch,
		},
	}
	return true
}
