package lazuli

import "fmt"

// LibBugError signals an internal invariant violation in the Lazuli
// runtime itself. Surface MUST be SurfaceLibInternal when constructed.
// EXPERIMENTAL: subject to change before 1.0.
type LibBugError struct {
	Base      ErrorBase
	Component string // Go package path of the offending component, e.g. "lazuli.dev/runtime/lazuli/auth"
	Invariant string // human-readable description of the violated invariant
	IssueURL  string // pre-filled URL for reporting (e.g. github.com/.../issues/new?title=...)
}

// Error implements the error interface.
func (e *LibBugError) Error() string {
	return fmt.Sprintf("lib_bug: component=%s invariant=%s", e.Component, e.Invariant)
}

// Unwrap returns the underlying cause.
func (e *LibBugError) Unwrap() error { return e.Base.Cause }

// ErrorBase returns the shared typed error context.
func (e *LibBugError) ErrorBase() ErrorBase { return e.Base }
