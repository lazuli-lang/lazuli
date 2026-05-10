package lazuli

import "fmt"

// Error is the canonical error envelope returned to clients. It mirrors the
// DSL error contract (`errors` block + `error <Name> status <code>`).
//
// Status flows to HTTP status codes; Code is a stable string clients can
// branch on; Message is the human-facing explanation.
type Error struct {
	Status  int    // HTTP status; 0 lets the runtime choose by Code
	Code    string // stable identifier ("policy_denied", "validation_failed")
	Message string // human-readable
	Data    any    // optional structured payload (only when DSL `expose` permits)
}

// Error implements the error interface.
func (e *Error) Error() string {
	return fmt.Sprintf("lazuli/%s: %s", e.Code, e.Message)
}

// Common error codes used by the runtime itself. Generated commands or
// validators may produce additional codes from the DSL `errors` block.
const (
	CodePolicyDenied      = "policy_denied"
	CodeRateLimited       = "rate_limited"
	CodeValidationFailed  = "validation_failed"
	CodeNotFound          = "not_found"
	CodeTenantMismatch    = "tenant_mismatch"
	CodeInternal          = "internal"
	CodeBadRequest        = "bad_request"
	CodeMethodNotAllowed  = "method_not_allowed"
	CodeIntegrationError  = "integration_error"
)
