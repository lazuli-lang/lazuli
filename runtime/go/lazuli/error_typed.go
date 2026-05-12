package lazuli

import "fmt"

// Origin routes experimental typed errors to the owner responsible for fixing
// the failure.
//
// EXPERIMENTAL: subject to change before 1.0.
type Origin uint8

const (
	// OriginUserDSL means the user-authored Lazuli DSL or input caused the error.
	OriginUserDSL Origin = iota
	// OriginLibInternal means Lazuli runtime internals caused the error.
	OriginLibInternal
	// OriginCodegenBug means generated code caused the error.
	OriginCodegenBug
	// OriginAdapterRuntime means an external adapter/runtime boundary caused the error.
	OriginAdapterRuntime
)

// ErrorBase carries the shared envelope for experimental typed errors.
//
// EXPERIMENTAL: subject to change before 1.0.
type ErrorBase struct {
	Code    string
	Origin  Origin
	Status  int
	Message string
	Feature string
	Kind    string
	Op      string
	Source  string
	Cause   error
}

// FieldReason classifies why a field failed validation.
//
// EXPERIMENTAL: subject to change before 1.0.
type FieldReason uint8

const (
	// FieldReasonRequired means a required field was missing.
	FieldReasonRequired FieldReason = iota
	// FieldReasonInvalidFormat means the field value did not match its format.
	FieldReasonInvalidFormat
	// FieldReasonOutOfRange means the field value was outside the allowed range.
	FieldReasonOutOfRange
	// FieldReasonMismatch means related field values did not match.
	FieldReasonMismatch
	// FieldReasonUnknownEnum means the field value was not in the enum catalog.
	FieldReasonUnknownEnum
)

// FieldError reports a user-authored field validation failure.
//
// EXPERIMENTAL: subject to change before 1.0.
type FieldError struct {
	Base      ErrorBase
	Field     string
	Path      string
	Reason    FieldReason
	InputType string
}

// Error implements the error interface.
func (e *FieldError) Error() string {
	if e == nil {
		return "<nil>"
	}
	return typedErrorString("field_error", e.Base)
}

// Unwrap exposes the underlying cause for errors.Is and errors.As.
func (e *FieldError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Base.Cause
}

// PolicyError reports a user-authored policy failure.
//
// EXPERIMENTAL: subject to change before 1.0.
type PolicyError struct {
	Base     ErrorBase
	Rule     string
	Subject  string
	Resource string
	Tenant   string
}

// Error implements the error interface.
func (e *PolicyError) Error() string {
	if e == nil {
		return "<nil>"
	}
	return typedErrorString("policy_error", e.Base)
}

// Unwrap exposes the underlying cause for errors.Is and errors.As.
func (e *PolicyError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Base.Cause
}

// TenantError reports a user-authored tenant routing or isolation failure.
//
// EXPERIMENTAL: subject to change before 1.0.
type TenantError struct {
	Base     ErrorBase
	Axis     string
	Expected string
	Actual   string
}

// Error implements the error interface.
func (e *TenantError) Error() string {
	if e == nil {
		return "<nil>"
	}
	return typedErrorString("tenant_error", e.Base)
}

// Unwrap exposes the underlying cause for errors.Is and errors.As.
func (e *TenantError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Base.Cause
}

// AdapterError reports a failure at an external adapter/runtime boundary.
//
// EXPERIMENTAL: subject to change before 1.0.
type AdapterError struct {
	Base                ErrorBase
	Adapter             string
	Op                  string
	RetryBudgetConsumed int
	RetryBudgetMax      int
}

// Error implements the error interface.
func (e *AdapterError) Error() string {
	if e == nil {
		return "<nil>"
	}
	return typedErrorString("adapter_error", e.Base)
}

// Unwrap exposes the underlying cause for errors.Is and errors.As.
func (e *AdapterError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Base.Cause
}

// LibBugError reports a Lazuli runtime invariant failure.
//
// EXPERIMENTAL: subject to change before 1.0.
type LibBugError struct {
	Base      ErrorBase
	Component string
	Invariant string
	IssueURL  string
}

// Error implements the error interface.
func (e *LibBugError) Error() string {
	if e == nil {
		return "<nil>"
	}
	return typedErrorString("lib_bug_error", e.Base)
}

// Unwrap exposes the underlying cause for errors.Is and errors.As.
func (e *LibBugError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Base.Cause
}

func typedErrorString(kind string, base ErrorBase) string {
	code := base.Code
	if code == "" {
		code = kind
	}
	if base.Message == "" {
		return fmt.Sprintf("lazuli/%s", code)
	}
	return fmt.Sprintf("lazuli/%s: %s", code, base.Message)
}
