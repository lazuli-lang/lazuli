// Package runtime carries the small set of runtime sentinels + constructors
// that codegen-emitted feature glue (`<feature>/guards.gen.go`, etc.) rejects
// with, kept out of the parent `lazuli` package so the emitted guard files can
// import a stable, dependency-light surface.
//
// Spec 0014 — referential guards (`restrict on_delete references <rel> via
// <fk> [error <CODE>] [where <pred>]`). A guard whose tenant-scoped,
// soft-delete-aware `EXISTS` finds a live reference rejects deletion with:
//
//   - the bare sentinel `ErrReferencedInUse` (no authored `error <CODE>`), or
//   - `NewReferencedInUseError("<CODE>")` (GAP-2: pinned per-guard domain code,
//     e.g. `CATEGORY_HAS_CUSTOMERS`, surfaced on the wire as the error code).
//
// Both wire to HTTP 409 (conflict): the resource cannot be deleted while a
// live reference exists.
package runtime

import (
	"errors"

	"lazuli.dev/runtime/lazuli"
)

// CodeReferencedInUse is the default wire code for a referential-guard
// rejection when the guard did not pin a custom `error <CODE>`.
const CodeReferencedInUse = "referenced_in_use"

// ErrReferencedInUse is the back-compat sentinel a referential guard rejects
// with when no per-guard `error <CODE>` was authored. Wires to HTTP 409.
//
// Callers may branch on it with errors.Is(err, runtime.ErrReferencedInUse);
// it also matches any NewReferencedInUseError(...) value (the domain-coded
// variant Is-es back to this sentinel), so a generic "is this a
// referenced-in-use rejection?" check keeps working regardless of whether a
// custom code was pinned.
var ErrReferencedInUse = &ReferencedInUseError{Code: CodeReferencedInUse}

// ReferencedInUseError is the typed referential-guard rejection. `Code` is the
// wire error code: the canonical CodeReferencedInUse for the bare sentinel, or
// a pilot-pinned domain code (e.g. "CATEGORY_HAS_CUSTOMERS") via
// NewReferencedInUseError.
type ReferencedInUseError struct {
	// Code is the stable wire error code clients branch on.
	Code string
}

// NewReferencedInUseError builds a referential-guard rejection carrying the
// pilot-pinned domain `code` (Spec 0014 GAP-2). An empty `code` falls back to
// the canonical CodeReferencedInUse so the constructor never emits a blank
// wire code.
func NewReferencedInUseError(code string) *ReferencedInUseError {
	if code == "" {
		code = CodeReferencedInUse
	}
	return &ReferencedInUseError{Code: code}
}

// Error implements the error interface.
func (e *ReferencedInUseError) Error() string { return e.Code }

// Is lets every ReferencedInUseError (including domain-coded variants) match
// the ErrReferencedInUse sentinel, so callers can do a generic
// errors.Is(err, runtime.ErrReferencedInUse) check.
func (e *ReferencedInUseError) Is(target error) bool {
	var other *ReferencedInUseError
	return errors.As(target, &other)
}

// As projects the rejection onto the canonical *lazuli.Error envelope (HTTP
// 409), carrying the pinned domain code as both the wire Code and the L1
// MessageKey so a feature `errors` block can localize it.
func (e *ReferencedInUseError) As(target any) bool {
	le, ok := target.(**lazuli.Error)
	if !ok {
		return false
	}
	code := e.Code
	if code == "" {
		code = CodeReferencedInUse
	}
	*le = &lazuli.Error{
		Status:     409,
		Code:       code,
		Message:    "resource is referenced and cannot be deleted",
		MessageKey: code,
		Base: lazuli.ErrorBase{
			Status:  409,
			Code:    code,
			Message: "resource is referenced and cannot be deleted",
			Cause:   e,
		},
	}
	return true
}
