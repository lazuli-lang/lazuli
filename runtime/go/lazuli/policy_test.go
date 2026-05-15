package lazuli

import (
	"errors"
	"testing"
)

// withRbacStub installs deterministic RBAC checkers for the duration of
// the test and restores the no-op defaults via t.Cleanup. The stub map
// is keyed by role name; permissions are flat lists.
func withRbacStub(t *testing.T, perms map[string][]string) {
	t.Helper()
	prevRole := rbacHasRole
	prevPerm := rbacHasPermission
	t.Cleanup(func() {
		rbacHasRole = prevRole
		rbacHasPermission = prevPerm
	})
	RegisterRbac(
		func(roleName, name string) bool { return roleName == name },
		func(roleName, perm string) bool {
			for _, p := range perms[roleName] {
				if p == perm {
					return true
				}
			}
			return false
		},
	)
}

func TestEvalPolicyEmptyAtomsIsInternalError(t *testing.T) {
	err := EvalPolicy(&Ctx{}, Policy{Name: "@policy.zero"})
	var le *Error
	if !errors.As(err, &le) {
		t.Fatalf("expected *Error, got %T: %v", err, err)
	}
	if le.Status != 500 {
		t.Fatalf("status = %d, want 500", le.Status)
	}
}

func TestEvalPolicyLegacyRoleAtomMatches(t *testing.T) {
	ctx := &Ctx{Actor: ActorUser, User: &User{ID: 1, Roles: []string{"admin"}}}
	p := Policy{Name: "@policy.legacy", Atoms: []PolicyAtom{
		{Namespace: "role", Name: "admin"},
	}}
	if err := EvalPolicy(ctx, p); err != nil {
		t.Fatalf("expected admin to satisfy @role.admin, got %v", err)
	}
}

func TestEvalPolicyLegacyRoleAtomDeniesWhenMissing(t *testing.T) {
	ctx := &Ctx{Actor: ActorUser, User: &User{ID: 1, Roles: []string{"viewer"}}}
	p := Policy{Name: "@policy.legacy", Atoms: []PolicyAtom{
		{Namespace: "role", Name: "admin"},
	}}
	err := EvalPolicy(ctx, p)
	var le *Error
	if !errors.As(err, &le) || le.Status != 403 {
		t.Fatalf("expected 403 policy_denied, got %v", err)
	}
}

func TestEvalPolicyHasRolePredicateMatchesWhenCtxCarriesRole(t *testing.T) {
	withRbacStub(t, nil)

	// "authenticated and has_role manager" — atoms emitted by codegen as:
	//   ( predicate.authenticated and rbac.role(manager) )
	atoms := []PolicyAtom{
		{Namespace: "predicate", Name: "("},
		{Namespace: "predicate", Name: "authenticated"},
		{Namespace: "predicate", Name: "and"},
		{Namespace: "rbac.role", Name: "manager"},
		{Namespace: "predicate", Name: ")"},
	}
	p := Policy{Name: "authenticated and has_role manager", Atoms: atoms}

	ok := Allow(&Ctx{Actor: ActorUser, User: &User{ID: 7, Roles: []string{"manager"}}}, p)
	if !ok {
		t.Fatalf("manager role should satisfy `has_role manager`")
	}

	denied := Allow(&Ctx{Actor: ActorUser, User: &User{ID: 7, Roles: []string{"viewer"}}}, p)
	if denied {
		t.Fatalf("viewer should not satisfy `has_role manager`")
	}

	anonDenied := Allow(&Ctx{Actor: ActorAnonymous}, p)
	if anonDenied {
		t.Fatalf("anonymous should not satisfy `authenticated and has_role manager`")
	}
}

func TestEvalPolicyHasPermissionPredicateConsultsRbacHook(t *testing.T) {
	withRbacStub(t, map[string][]string{
		"sales_manager": {"users:create", "users:read"},
		"viewer":        {"users:read"},
	})

	// "authenticated and has_permission users:create"
	atoms := []PolicyAtom{
		{Namespace: "predicate", Name: "("},
		{Namespace: "predicate", Name: "authenticated"},
		{Namespace: "predicate", Name: "and"},
		{Namespace: "rbac.permission", Name: "users:create"},
		{Namespace: "predicate", Name: ")"},
	}
	p := Policy{Name: "authenticated and has_permission users:create", Atoms: atoms}

	if !Allow(&Ctx{Actor: ActorUser, User: &User{ID: 9, Roles: []string{"sales_manager"}}}, p) {
		t.Fatalf("sales_manager closure includes users:create")
	}
	if Allow(&Ctx{Actor: ActorUser, User: &User{ID: 9, Roles: []string{"viewer"}}}, p) {
		t.Fatalf("viewer closure should not include users:create")
	}
	if Allow(&Ctx{Actor: ActorUser, User: &User{ID: 9, Roles: []string{"intern"}}}, p) {
		t.Fatalf("undeclared role should fail closed")
	}
}

func TestEvalPolicyNotPredicateInverts(t *testing.T) {
	withRbacStub(t, nil)

	// "not has_role banned" — single not-of-atom.
	atoms := []PolicyAtom{
		{Namespace: "predicate", Name: "not"},
		{Namespace: "rbac.role", Name: "banned"},
	}
	p := Policy{Name: "not has_role banned", Atoms: atoms}

	if !Allow(&Ctx{Actor: ActorUser, User: &User{ID: 1, Roles: []string{"viewer"}}}, p) {
		t.Fatalf("non-banned role should satisfy `not has_role banned`")
	}
	if Allow(&Ctx{Actor: ActorUser, User: &User{ID: 1, Roles: []string{"banned"}}}, p) {
		t.Fatalf("banned role should fail `not has_role banned`")
	}
}

func TestEvalPolicyOrPredicateAcceptsEitherBranch(t *testing.T) {
	withRbacStub(t, nil)

	// "has_role admin or has_role manager"
	atoms := []PolicyAtom{
		{Namespace: "predicate", Name: "("},
		{Namespace: "rbac.role", Name: "admin"},
		{Namespace: "predicate", Name: "or"},
		{Namespace: "rbac.role", Name: "manager"},
		{Namespace: "predicate", Name: ")"},
	}
	p := Policy{Name: "has_role admin or has_role manager", Atoms: atoms}

	for _, role := range []string{"admin", "manager"} {
		if !Allow(&Ctx{Actor: ActorUser, User: &User{ID: 1, Roles: []string{role}}}, p) {
			t.Fatalf("role %q should satisfy the OR", role)
		}
	}
	if Allow(&Ctx{Actor: ActorUser, User: &User{ID: 1, Roles: []string{"viewer"}}}, p) {
		t.Fatalf("viewer should not satisfy the OR")
	}
}

func TestRegisterRbacIgnoresNilCheckers(t *testing.T) {
	prevRole := rbacHasRole
	prevPerm := rbacHasPermission
	t.Cleanup(func() {
		rbacHasRole = prevRole
		rbacHasPermission = prevPerm
	})

	called := false
	RegisterRbac(func(_, _ string) bool { called = true; return true }, nil)
	if !rbacHasRole("a", "b") || !called {
		t.Fatalf("RegisterRbac should have installed the role checker")
	}
	if rbacHasPermission("a", "b") {
		t.Fatalf("nil perm checker should keep the fail-closed default")
	}
}
