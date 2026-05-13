package lazuli

import "fmt"

// TenantError signals tenant boundary violation.
// EXPERIMENTAL: subject to change before 1.0.
type TenantError struct {
	Base     ErrorBase
	Axis     string // tenancy axis name, e.g. "org_id", "user_id"
	Expected string // tenant value the request was scoped to
	Actual   string // tenant value of the resource being accessed
}

// Error implements the error interface.
func (e *TenantError) Error() string {
	return fmt.Sprintf("tenant_mismatch: axis=%s expected=%s actual=%s", e.Axis, e.Expected, e.Actual)
}

// Unwrap returns the underlying cause.
func (e *TenantError) Unwrap() error { return e.Base.Cause }

// ErrorBase returns the shared typed error context.
func (e *TenantError) ErrorBase() ErrorBase { return e.Base }
