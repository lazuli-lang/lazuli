package lazuli

import "fmt"

// Error is the canonical runtime error value. It mirrors the DSL error
// contract (`errors` block + `error <Name> status <code>`) and is converted
// into RFC 9457 problem details for HTTP responses.
//
// Status flows to HTTP status codes; Code is a stable string clients can
// branch on; Message is the human-facing explanation.
type Error struct {
	Status  int    // HTTP status; 0 lets the runtime choose by Code
	Code    string // stable identifier ("policy_denied", "validation_failed")
	Message string // human-readable
	Data    any    // optional structured payload exposed as problem extension "data"
	Base    ErrorBase
}

// Error implements the error interface.
func (e *Error) Error() string {
	if e == nil {
		return "<nil>"
	}
	code := e.Code
	if code == "" {
		code = e.Base.Code
	}
	message := e.Message
	if message == "" {
		message = e.Base.Message
	}
	if message == "" {
		return fmt.Sprintf("lazuli/%s", code)
	}
	return fmt.Sprintf("lazuli/%s: %s", code, message)
}

// Unwrap exposes the source-aware envelope cause for errors.Is and errors.As.
func (e *Error) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Base.Cause
}

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
