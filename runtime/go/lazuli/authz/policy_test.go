package authz

import "testing"

func TestPolicyAllowsRoleGraphPermission(t *testing.T) {
	t.Parallel()

	graph := mustRoleGraph(t,
		Role{Name: "viewer", Permissions: []Permission{PermissionFor("customers", "read")}},
		Role{Name: "editor", Inherits: []string{"viewer"}},
	)
	policy := Policy{Roles: graph}

	request := Request{
		Subject:  Subject{Roles: []string{" editor "}},
		Resource: " customers ",
		Action:   " read ",
	}
	result := policy.Evaluate(request)
	if !result.Allowed {
		t.Fatalf("Evaluate() allowed = false, want true: %#v", result)
	}
	if result.RuleIndex != -1 {
		t.Fatalf("Evaluate() rule index = %d, want -1 for RBAC fallback", result.RuleIndex)
	}
	if !policy.Allow(request) {
		t.Fatal("Allow() = false, want true")
	}

	request.Action = "delete"
	if result := policy.Evaluate(request); result.Allowed {
		t.Fatalf("Evaluate(delete) allowed = true, want false: %#v", result)
	}
}

func TestPolicyRulesAreEvaluatedInOrder(t *testing.T) {
	t.Parallel()

	graph := mustRoleGraph(t,
		Role{Name: "admin", Permissions: []Permission{PermissionFor("customers", "delete")}},
		Role{Name: "suspended"},
	)
	request := Request{
		Subject:  Subject{Roles: []string{"admin", "suspended"}},
		Resource: "customers",
		Action:   "delete",
	}

	denyFirst := Policy{
		Roles: graph,
		Rules: []Rule{
			{Effect: EffectDeny, Resource: "customers", Action: "delete", Roles: []string{"suspended"}, Reason: "suspended users cannot delete customers"},
			{Effect: EffectAllow, Resource: "customers", Action: "delete", Roles: []string{"admin"}},
		},
	}
	result := denyFirst.Evaluate(request)
	if result.Allowed {
		t.Fatalf("deny-first Evaluate() allowed = true, want false: %#v", result)
	}
	if result.RuleIndex != 0 || result.Effect != EffectDeny {
		t.Fatalf("deny-first Evaluate() = %#v, want rule 0 deny", result)
	}
	if result.Reason != "suspended users cannot delete customers" {
		t.Fatalf("deny-first reason = %q", result.Reason)
	}

	allowFirst := Policy{
		Roles: graph,
		Rules: []Rule{
			{Effect: EffectAllow, Resource: "customers", Action: "delete", Roles: []string{"admin"}},
			{Effect: EffectDeny, Resource: "customers", Action: "delete", Roles: []string{"suspended"}},
		},
	}
	result = allowFirst.Evaluate(request)
	if !result.Allowed {
		t.Fatalf("allow-first Evaluate() allowed = false, want true: %#v", result)
	}
	if result.RuleIndex != 0 || result.Effect != EffectAllow {
		t.Fatalf("allow-first Evaluate() = %#v, want rule 0 allow", result)
	}
}

func TestPolicyRulesUseInheritedSubjectRoles(t *testing.T) {
	t.Parallel()

	graph := mustRoleGraph(t,
		Role{Name: "editor"},
		Role{Name: "admin", Inherits: []string{"editor"}},
	)
	policy := Policy{
		Roles: graph,
		Rules: []Rule{
			{Effect: EffectAllow, Resource: "customers", Action: "archive", Roles: []string{"editor"}},
		},
	}

	result := policy.Evaluate(Request{
		Subject:  Subject{Roles: []string{"admin"}},
		Resource: "customers",
		Action:   "archive",
	})
	if !result.Allowed {
		t.Fatalf("Evaluate() allowed = false, want true for inherited editor rule: %#v", result)
	}
}

func TestPolicyOwnerAndSelfRules(t *testing.T) {
	t.Parallel()

	policy := Policy{
		Rules: []Rule{
			{Effect: EffectAllow, Resource: "customers", Action: "update", Owner: true},
			{Effect: EffectAllow, Resource: "users", Action: "read", Self: true},
		},
	}

	ownerRequest := Request{
		Subject:  Subject{ID: int64(42)},
		Resource: "customers",
		Action:   "update",
		OwnerID:  int64(42),
	}
	if result := policy.Evaluate(ownerRequest); !result.Allowed {
		t.Fatalf("owner Evaluate() allowed = false, want true: %#v", result)
	}

	ownerRequest.OwnerID = int64(7)
	if result := policy.Evaluate(ownerRequest); result.Allowed {
		t.Fatalf("non-owner Evaluate() allowed = true, want false: %#v", result)
	}

	selfRequest := Request{
		Subject:    Subject{ID: int64(42)},
		Resource:   "users",
		ResourceID: int64(42),
		Action:     "read",
	}
	if result := policy.Evaluate(selfRequest); !result.Allowed {
		t.Fatalf("self Evaluate() allowed = false, want true: %#v", result)
	}

	selfRequest.ResourceID = int64(7)
	if result := policy.Evaluate(selfRequest); result.Allowed {
		t.Fatalf("not-self Evaluate() allowed = true, want false: %#v", result)
	}
}

func TestPolicyInvalidRuleEffectFailsClosed(t *testing.T) {
	t.Parallel()

	policy := Policy{
		Rules: []Rule{
			{Effect: Effect("permit"), Resource: "customers", Action: "read"},
		},
	}

	result := policy.Evaluate(Request{Resource: "customers", Action: "read"})
	if result.Allowed {
		t.Fatalf("Evaluate() allowed = true, want false for invalid effect: %#v", result)
	}
	if result.RuleIndex != 0 || result.Effect != EffectDeny {
		t.Fatalf("Evaluate() = %#v, want rule 0 deny", result)
	}
}
