package lazuli

import (
	"errors"
	"testing"
)

// SECURITY (SEC-API-POLICY-NULL): the api dispatch surface must enforce
// the emitted `Policy` field identically to the command/query paths.
// Before the fix, `Api.Invoke` ran the handler without ever calling
// `EvalPolicy*`, so an `api ... policy @policy.authenticated` was
// reachable by an anonymous caller. These tests are the permanent
// regression guard.

const apiLeaked = "leaked"

func authenticatedApi() Api[struct{}, string] {
	return Api[struct{}, string]{
		Name:   "f.secret",
		Policy: Policy{Name: "@policy.authenticated", Atoms: []PolicyAtom{{Namespace: "scope", Name: "authenticated"}}},
		Handler: func(_ *Ctx, _ struct{}) (string, error) {
			return apiLeaked, nil
		},
	}
}

// TestApiInvokeDeniesAnonymousForAuthenticatedPolicy proves the fix: an
// anonymous request to an `@policy.authenticated` api is denied (403)
// and the handler never runs (no data leaks).
func TestApiInvokeDeniesAnonymousForAuthenticatedPolicy(t *testing.T) {
	api := authenticatedApi()
	out, err := api.Invoke(&Ctx{Actor: ActorAnonymous}, struct{}{})
	var le *Error
	if !errors.As(err, &le) {
		t.Fatalf("anonymous call to authenticated api must error, got out=%q err=%v", out, err)
	}
	if le.Status != 403 {
		t.Fatalf("anonymous call to authenticated api must be 403, got status %d", le.Status)
	}
	if out == apiLeaked {
		t.Fatalf("handler must not run for a denied policy; got leaked output")
	}
}

// TestApiInvokeAllowsAuthenticated proves the fix does not over-block: a
// request carrying a resolved user satisfies @policy.authenticated and
// the handler runs.
func TestApiInvokeAllowsAuthenticated(t *testing.T) {
	api := authenticatedApi()
	ctx := &Ctx{Actor: ActorUser, User: &User{ID: 1}}
	out, err := api.Invoke(ctx, struct{}{})
	if err != nil {
		t.Fatalf("authenticated call must succeed, got err=%v", err)
	}
	if out != apiLeaked {
		t.Fatalf("authenticated call must reach handler, got out=%q", out)
	}
}

// TestApiInvokePublicStillServesAnonymous proves @policy.public api
// endpoints remain reachable anonymously (no regression for intentional
// public endpoints).
func TestApiInvokePublicStillServesAnonymous(t *testing.T) {
	api := Api[struct{}, string]{
		Name:   "f.health",
		Policy: Policy{Name: "@policy.public", Atoms: []PolicyAtom{{Namespace: "scope", Name: "public"}}},
		Handler: func(_ *Ctx, _ struct{}) (string, error) {
			return "ok", nil
		},
	}
	out, err := api.Invoke(&Ctx{Actor: ActorAnonymous}, struct{}{})
	if err != nil {
		t.Fatalf("public api must serve anonymously, got err=%v", err)
	}
	if out != "ok" {
		t.Fatalf("public api output = %q, want ok", out)
	}
}

// TestApiInvokeFailsClosedOnEmptyPolicy proves the fail-closed posture:
// an api whose Policy carries no atoms (a codegen invariant violation)
// returns 500 and never serves the handler — it can never accidentally
// fall through to an unguarded handler.
func TestApiInvokeFailsClosedOnEmptyPolicy(t *testing.T) {
	api := Api[struct{}, string]{
		Name:    "f.misconfigured",
		Policy:  Policy{Name: "@policy.broken"}, // no atoms
		Handler: func(_ *Ctx, _ struct{}) (string, error) { return apiLeaked, nil },
	}
	out, err := api.Invoke(&Ctx{Actor: ActorUser, User: &User{ID: 1}}, struct{}{})
	var le *Error
	if !errors.As(err, &le) || le.Status != 500 {
		t.Fatalf("empty-policy api must fail closed with 500, got out=%q err=%v", out, err)
	}
	if out == apiLeaked {
		t.Fatalf("empty-policy api must not run handler")
	}
}

// TestApiInvokeDeniesRoleMismatch proves a role-restricted api denies a
// user lacking the role (mirrors the command/query role check).
func TestApiInvokeDeniesRoleMismatch(t *testing.T) {
	api := Api[struct{}, string]{
		Name:    "f.admin_only",
		Policy:  Policy{Name: "@policy.admin", Atoms: []PolicyAtom{{Namespace: "role", Name: "admin"}}},
		Handler: func(_ *Ctx, _ struct{}) (string, error) { return apiLeaked, nil },
	}
	// User present but without the admin role.
	out, err := api.Invoke(&Ctx{Actor: ActorUser, User: &User{ID: 1, Roles: []string{"member"}}}, struct{}{})
	var le *Error
	if !errors.As(err, &le) || le.Status != 403 {
		t.Fatalf("role-mismatch api must be 403, got out=%q err=%v", out, err)
	}
	if out == apiLeaked {
		t.Fatalf("handler must not run for a denied role policy")
	}
}
