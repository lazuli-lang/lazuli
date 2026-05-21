package lazuli

import (
	"errors"
	"testing"
)

func TestBaseScopeConditionsFailsOnNilTenantTenancyOrg(t *testing.T) {
	ctx := &Ctx{Actor: ActorUser, Tenant: nil}
	resource := &resourceErased{Name: "Test", Tenancy: TenancyOrg}

	_, _, err := baseScopeConditions(ctx, resource, 1)
	if err == nil {
		t.Fatal("expected ErrTenantRequired for nil-Tenant on TenancyOrg")
	}
	if !errors.Is(err, ErrTenantRequired) {
		t.Fatalf("expected ErrTenantRequired; got %v", err)
	}
}

func TestBaseScopeConditionsAllowsTenancyNoneNilTenant(t *testing.T) {
	ctx := &Ctx{Actor: ActorAnonymous, Tenant: nil}
	resource := &resourceErased{Name: "Public", Tenancy: TenancyNone}

	preds, values, err := baseScopeConditions(ctx, resource, 1)
	if err != nil {
		t.Fatalf("TenancyNone+nil-Tenant should succeed; got %v", err)
	}
	if len(preds) != 0 {
		t.Fatalf("TenancyNone should not add predicates; got %d", len(preds))
	}
	if len(values) != 0 {
		t.Fatalf("TenancyNone should not add values; got %d", len(values))
	}
}

func TestBaseScopeConditionsAddsOrgPredicate(t *testing.T) {
	tenant := &Tenant{OrgID: 42}
	ctx := &Ctx{Actor: ActorUser, Tenant: tenant}
	resource := &resourceErased{Name: "Customer", Tenancy: TenancyOrg}

	preds, values, err := baseScopeConditions(ctx, resource, 1)
	if err != nil {
		t.Fatalf("unexpected err: %v", err)
	}
	if len(preds) != 1 {
		t.Fatalf("expected 1 predicate; got %d", len(preds))
	}
	if len(values) != 1 || values[0] != ID(42) {
		t.Fatalf("expected org_id value 42; got %v", values)
	}
}
