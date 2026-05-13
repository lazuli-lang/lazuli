package lazuli

import "fmt"

// PolicyError signals an authorization/policy denial.
// EXPERIMENTAL: subject to change before 1.0.
type PolicyError struct {
	Base     ErrorBase
	Rule     string // canonical rule name, e.g. "customer.create.role_admin_only"
	Subject  string // who tried (actor id)
	Resource string // what they tried to access (resource id)
	Tenant   string // tenant axis value
}

// Error implements the error interface.
func (e *PolicyError) Error() string {
	return fmt.Sprintf("policy_denied: rule=%s subject=%s resource=%s", e.Rule, e.Subject, e.Resource)
}

// Unwrap returns the underlying cause.
func (e *PolicyError) Unwrap() error { return e.Base.Cause }

// ErrorBase returns the shared typed error context.
func (e *PolicyError) ErrorBase() ErrorBase { return e.Base }
