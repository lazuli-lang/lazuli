package lazuli

import (
	"errors"
)

// §4 backend helpers — replace the boilerplate at the top of every
// handler that needs an authenticated user or a role check. The
// payoff is two-fold:
//
//  1. LOC reduction: ~80 hostpoint handlers today open with
//     `if ctx.User == nil { return X{}, errors.New("not_authenticated") }`.
//     Each migration drops 3 lines and the stringly-typed error.
//
//  2. Typed-error correctness: raw `errors.New("not_authenticated")`
//     used to surface as HTTP 500 (the runtime can't categorize
//     opaque strings); via `ErrNotAuthenticated` it lands as 401
//     with a stable, translatable error code.
//
// See `docs/proposals/lazuli-wave-contract-over-boilerplate-2026-05-23.md`
// §4 for the full migration narrative.

// CodeNotAuthenticated is the stable error code for missing-user
// failures. HTTP-wire: 401.
const CodeNotAuthenticated = "not_authenticated"

// ErrNotAuthenticated is the typed sentinel returned by
// [RequireActor] when the context carries no authenticated user.
// HTTP-wire: 401 with code `not_authenticated`.
//
// Use `errors.Is(err, lazuli.ErrNotAuthenticated)` to branch on
// this specific failure in custom code paths.
var ErrNotAuthenticated = errors.New(CodeNotAuthenticated)

// ErrRoleRequired is the typed sentinel returned by [RequireRole]
// when the authenticated user lacks the required role.
// HTTP-wire: 403 with code `policy_denied`.
var ErrRoleRequired = errors.New("role_required")

// ErrResourceNotFound is the typed sentinel returned by [NotFoundError]
// for missing-row failures. HTTP-wire: 404 with code `not_found`.
var ErrResourceNotFound = errors.New("resource_not_found")

// ErrNotOwner is the typed sentinel returned by [NotOwnerError] when
// the actor doesn't own the targeted resource. HTTP-wire: 403 with
// code `policy_denied`.
var ErrNotOwner = errors.New("not_owner")

// RequireActor returns the authenticated user attached to ctx, or
// [ErrNotAuthenticated] when ctx carries no user. Replaces the
// boilerplate at the top of every authenticated handler:
//
//	user, err := lazuli.RequireActor(ctx)
//	if err != nil { return Result{}, err }
//	// use user.ID, user.OrgID, etc.
//
// The returned error is wrapped in a [*Error] envelope by the HTTP
// boundary (status 401, code `not_authenticated`) so the wire
// payload stays consistent across handlers.
func RequireActor(ctx *Ctx) (*User, error) {
	if ctx == nil || ctx.User == nil {
		return nil, notAuthenticatedError()
	}
	return ctx.User, nil
}

// RequireRole returns the authenticated user when they hold the
// named role, or [ErrRoleRequired] otherwise. Combines the
// not-authenticated check with the role gate in one call:
//
//	host, err := lazuli.RequireRole(ctx, "host")
//	if err != nil { return Result{}, err }
//
// Returns [ErrNotAuthenticated] when no user is attached (callers
// don't need a separate nil check). Role matching is case-
// sensitive — pass the canonical role name from your `.lzi`
// `@role.<name>` declarations.
func RequireRole(ctx *Ctx, role string) (*User, error) {
	user, err := RequireActor(ctx)
	if err != nil {
		return nil, err
	}
	for _, r := range user.Roles {
		if r == role {
			return user, nil
		}
	}
	return nil, roleRequiredError(role)
}

// HasRole reports whether the authenticated user holds the named
// role. Returns false (rather than erroring) when no user is
// attached — convenient for branching logic that needs to render
// differently for anonymous, host, and traveler actors without
// raising errors.
func HasRole(ctx *Ctx, role string) bool {
	if ctx == nil || ctx.User == nil {
		return false
	}
	for _, r := range ctx.User.Roles {
		if r == role {
			return true
		}
	}
	return false
}

func notAuthenticatedError() *Error {
	return &Error{
		Status:     401,
		Code:       CodeNotAuthenticated,
		Message:    "authentication required",
		MessageKey: CodeNotAuthenticated,
		Base: ErrorBase{
			Status:  401,
			Code:    CodeNotAuthenticated,
			Message: "authentication required",
			Surface: SurfaceUserDSL,
			Cause:   ErrNotAuthenticated,
		},
	}
}

// NotFoundError returns a typed 404 envelope for the named
// resource. Use in handlers after a lookup that returns
// `pgx.ErrNoRows`:
//
//	err := db.QueryRow(...).Scan(...)
//	if errors.Is(err, pgx.ErrNoRows) {
//	    return Result{}, lazuli.NotFoundError("property")
//	}
//
// HTTP-wire: 404, code=not_found, message="property not found".
func NotFoundError(resource string) *Error {
	msg := resource + " not found"
	return &Error{
		Status:     404,
		Code:       CodeNotFound,
		Message:    msg,
		MessageKey: CodeNotFound,
		Base: ErrorBase{
			Status:  404,
			Code:    CodeNotFound,
			Message: msg,
			Surface: SurfaceUserDSL,
			Cause:   ErrResourceNotFound,
		},
	}
}

// NotOwnerError returns a typed 403 envelope for ownership-check
// failures. Use when an authenticated actor tries to mutate a
// resource they don't own:
//
//	if ownerUserID != ctx.User.ID {
//	    return Result{}, lazuli.NotOwnerError("property")
//	}
//
// HTTP-wire: 403, code=policy_denied, message="not the property owner".
// Returns the same code as the policy gate so client-side error
// banners can treat them identically.
func NotOwnerError(resource string) *Error {
	msg := "not the " + resource + " owner"
	return &Error{
		Status:     403,
		Code:       CodePolicyDenied,
		Message:    msg,
		MessageKey: CodePolicyDenied,
		Base: ErrorBase{
			Status:  403,
			Code:    CodePolicyDenied,
			Message: msg,
			Surface: SurfaceUserDSL,
			Cause:   ErrNotOwner,
		},
	}
}

func roleRequiredError(role string) *Error {
	return &Error{
		Status:     403,
		Code:       CodePolicyDenied,
		Message:    "role required: " + role,
		MessageKey: CodePolicyDenied,
		Base: ErrorBase{
			Status:  403,
			Code:    CodePolicyDenied,
			Message: "role required: " + role,
			Surface: SurfaceUserDSL,
			Cause:   ErrRoleRequired,
		},
	}
}
