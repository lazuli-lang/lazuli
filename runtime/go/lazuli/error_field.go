package lazuli

import "fmt"

// FieldError wraps validation failures with structural field context.
// Codegen-generated handlers wrap bucket sentinels (e.g.
// auth.ErrPasswordMismatch) into FieldError at the runtime boundary;
// bucket authors do not construct this type directly (see section 7.2 +
// CODEGEN-WRAP-001 lint).
//
// EXPERIMENTAL: shape may add fields before 1.0.
type FieldError struct {
	Base      ErrorBase
	Field     string      // "email", "password", ...
	Path      string      // dotted path, e.g. "input.identity.email"
	Reason    FieldReason // closed catalog enum
	InputType string      // "string" | "@semantic.Email" | "@cap.File" | ...
}

// FieldReason is a closed-catalog enum for validation failure causes.
// Sized uint8 for stable serialization.
//
// EXPERIMENTAL: subject to change before 1.0.
type FieldReason uint8

const (
	FieldReasonRequired      FieldReason = iota // missing required field
	FieldReasonInvalidFormat                    // syntactic format mismatch (email regex, etc.)
	FieldReasonOutOfRange                       // numeric or length out of allowed range
	FieldReasonMismatch                         // value present but does not match expected (password mismatch, token mismatch)
	FieldReasonUnknownEnum                      // closed-catalog value not in enum
)

// String returns FieldReason's stable wire spelling.
func (r FieldReason) String() string {
	switch r {
	case FieldReasonRequired:
		return "required"
	case FieldReasonInvalidFormat:
		return "invalid_format"
	case FieldReasonOutOfRange:
		return "out_of_range"
	case FieldReasonMismatch:
		return "mismatch"
	case FieldReasonUnknownEnum:
		return "unknown_enum"
	default:
		return "unknown"
	}
}

// Error implements the error interface.
func (e *FieldError) Error() string {
	return fmt.Sprintf("field_error: %s [%s] reason=%s", e.Field, e.Path, e.Reason)
}

// Unwrap returns the underlying cause.
func (e *FieldError) Unwrap() error { return e.Base.Cause }

// ErrorBase returns the shared typed error context.
func (e *FieldError) ErrorBase() ErrorBase { return e.Base }
