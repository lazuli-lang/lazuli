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

// TestEvalPolicyDenyAtomAlwaysDenies asserts the fail-closed atom emitted by
// codegen for an UNRESOLVABLE policy reference (cross-feature
// PolicyRef::External, or an unknown @policy.<name>) can never be satisfied —
// not even by a superuser-like ctx (admin role + system actor + tenant). This
// is the runtime half of the POLICY-REF-UNRESOLVED fix: a command guarded by an
// unresolvable policy is denied (403), never silently allowed.
func TestEvalPolicyDenyAtomAlwaysDenies(t *testing.T) {
	withRbacStub(t, map[string][]string{"admin": {"everything:*:*"}})
	ctx := &Ctx{
		Actor:  ActorSystem,
		User:   &User{ID: 1, Roles: []string{"admin"}},
		Tenant: &Tenant{},
	}
	p := Policy{
		Name:  "accounts.policy.restricted",
		Atoms: []PolicyAtom{{Namespace: "predicate", Name: "deny"}},
	}
	err := EvalPolicy(ctx, p)
	var le *Error
	if !errors.As(err, &le) {
		t.Fatalf("expected *Error from deny atom, got %T: %v", err, err)
	}
	if le.Status != 403 {
		t.Fatalf("deny atom must yield 403 (fail closed), got status %d", le.Status)
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

// RBAC-OR-001 — the codegen emits a named policy category with 2+ role
// atoms as `( role.ADMIN or role.MANAGER )`. The user model is
// single-role, so the previously-emitted `and` joiner could never be
// satisfied (every such screen 403'd). This proves the runtime admits a
// caller holding ONLY ONE of the two listed roles — the exact pauta
// `@policy.view` / list_customers repro (admin@pauta.local, role=ADMIN).
func TestEvalPolicyNamedRoleListOrAdmitsSingleRoleHolder(t *testing.T) {
	// Exact shape `format_local_policy` now emits for
	//   view: @role.ADMIN, @role.MANAGER
	atoms := []PolicyAtom{
		{Namespace: "predicate", Name: "("},
		{Namespace: "role", Name: "ADMIN"},
		{Namespace: "predicate", Name: "or"},
		{Namespace: "role", Name: "MANAGER"},
		{Namespace: "predicate", Name: ")"},
	}
	p := Policy{Name: "@policy.view", Atoms: atoms}

	// A user holding ONLY ADMIN must be admitted.
	admin := &Ctx{Actor: ActorUser, User: &User{ID: 3, Roles: []string{"ADMIN"}}}
	if err := EvalPolicy(admin, p); err != nil {
		t.Fatalf("single-role ADMIN must satisfy (ADMIN or MANAGER), got %v", err)
	}
	// A user holding ONLY MANAGER must also be admitted.
	mgr := &Ctx{Actor: ActorUser, User: &User{ID: 4, Roles: []string{"MANAGER"}}}
	if err := EvalPolicy(mgr, p); err != nil {
		t.Fatalf("single-role MANAGER must satisfy (ADMIN or MANAGER), got %v", err)
	}
	// A user holding neither is denied.
	other := &Ctx{Actor: ActorUser, User: &User{ID: 5, Roles: []string{"VIEWER"}}}
	err := EvalPolicy(other, p)
	var le *Error
	if !errors.As(err, &le) || le.Status != 403 {
		t.Fatalf("VIEWER must be denied by (ADMIN or MANAGER), got %v", err)
	}
}

// --- GAP-09: input-value-predicate policy atoms -----------------------

type whenInput struct {
	Scope string
	Count int64
}

func TestEvalPolicyInputGuardedAtomGrantsWhenPredicateHolds(t *testing.T) {
	// create: @role.admin when input.scope = "production"
	p := Policy{Name: "@policy.create", Atoms: []PolicyAtom{
		{Namespace: "role", Name: "admin", When: &PolicyWhen{Path: "input.scope", Op: "=", Value: "production"}},
	}}
	ctx := &Ctx{Actor: ActorUser, User: &User{ID: 1, Roles: []string{"admin"}}}

	if err := EvalPolicyInput(ctx, p, whenInput{Scope: "production"}); err != nil {
		t.Fatalf("admin in production scope should pass, got %v", err)
	}
}

func TestEvalPolicyInputGuardedAtomSkippedWhenPredicateFails(t *testing.T) {
	// Two mutually-exclusive predicate-gated atoms:
	//   @role.admin   when input.scope = "production"
	//   @role.manager when input.scope = "media"
	p := Policy{Name: "@policy.create", Atoms: []PolicyAtom{
		{Namespace: "role", Name: "admin", When: &PolicyWhen{Path: "input.scope", Op: "=", Value: "production"}},
		{Namespace: "role", Name: "manager", When: &PolicyWhen{Path: "input.scope", Op: "=", Value: "media"}},
	}}

	// manager actor + media scope → manager atom applies, admin atom skipped.
	mgr := &Ctx{Actor: ActorUser, User: &User{ID: 2, Roles: []string{"manager"}}}
	if err := EvalPolicyInput(mgr, p, whenInput{Scope: "media"}); err != nil {
		t.Fatalf("manager in media scope should pass, got %v", err)
	}

	// manager actor + production scope → manager atom skipped (wrong scope),
	// admin atom requires admin role manager lacks → 403.
	err := EvalPolicyInput(mgr, p, whenInput{Scope: "production"})
	var le *Error
	if !errors.As(err, &le) || le.Status != 403 {
		t.Fatalf("manager in production scope should be denied, got %v", err)
	}
}

func TestEvalPolicyInputGuardSkippedWithNilInputFailsClosed(t *testing.T) {
	// EvalPolicy (input-less) cannot test the guard → atom fails closed.
	p := Policy{Name: "@policy.create", Atoms: []PolicyAtom{
		{Namespace: "role", Name: "admin", When: &PolicyWhen{Path: "input.scope", Op: "=", Value: "production"}},
	}}
	ctx := &Ctx{Actor: ActorUser, User: &User{ID: 1, Roles: []string{"admin"}}}
	if err := EvalPolicy(ctx, p); err == nil {
		t.Fatalf("guarded atom with nil input must fail closed")
	}
}

func TestPolicyWhenOrderedComparison(t *testing.T) {
	w := &PolicyWhen{Path: "input.count", Op: ">", Value: int64(5)}
	if !w.holds(whenInput{Count: 10}) {
		t.Fatalf("10 > 5 should hold")
	}
	if w.holds(whenInput{Count: 3}) {
		t.Fatalf("3 > 5 should not hold")
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
