package lazuli

import (
	"strings"
	"testing"
)

// Auto-tenant filter contract (review Camada 2 audit, 2026-05-15).
// `baseScopeConditions` is the single chokepoint that injects tenant
// scoping + soft-delete filtering into every query/update/delete path
// in `runtime/go/lazuli/handle.go` and `run.go`. Without these
// regression tests, a refactor of `baseScopeConditions` could silently
// remove tenant scoping and produce cross-tenant data leaks under the
// `TenancyOrg` model. The clauses below pin the four-way matrix:
//
//   { Tenancy=Org | None } × { SoftDelete=true | false }
//
// plus the corner case of `Tenancy=Org` with a nil ctx.Tenant (e.g.
// admin / system actor calling a tenant-scoped query — the runtime
// degrades to "no tenant filter" rather than crashing; doctor catches
// the case at the DSL level via `scope override`).

func TestBaseScopeConditions_org_tenancy_with_tenant_emits_org_id_filter(t *testing.T) {
	res := &resourceErased{Name: "customer", Tenancy: TenancyOrg, SoftDelete: false}
	ctx := &Ctx{Tenant: &Tenant{OrgID: 42}}

	conds, values := baseScopeConditions(ctx, res)

	joined := strings.Join(conds, " AND ")
	if !strings.Contains(joined, "org_id = $1") {
		t.Fatalf("expected `org_id = $1` clause; got: %q", joined)
	}
	if len(values) != 1 || values[0] != ID(42) {
		t.Fatalf("expected tenant org_id value 42; got: %v", values)
	}
}

func TestBaseScopeConditions_org_tenancy_without_tenant_skips_filter(t *testing.T) {
	res := &resourceErased{Name: "customer", Tenancy: TenancyOrg, SoftDelete: false}
	ctx := &Ctx{Tenant: nil}

	conds, values := baseScopeConditions(ctx, res)

	if len(conds) != 0 {
		t.Fatalf("nil tenant on Org resource must skip filter; got: %v", conds)
	}
	if len(values) != 0 {
		t.Fatalf("nil tenant must produce no values; got: %v", values)
	}
}

func TestBaseScopeConditions_none_tenancy_skips_org_filter(t *testing.T) {
	res := &resourceErased{Name: "settings", Tenancy: TenancyNone, SoftDelete: false}
	ctx := &Ctx{Tenant: &Tenant{OrgID: 42}}

	conds, values := baseScopeConditions(ctx, res)

	if len(conds) != 0 {
		t.Fatalf("TenancyNone must skip filter even with tenant set; got: %v", conds)
	}
	if len(values) != 0 {
		t.Fatalf("TenancyNone must produce no values; got: %v", values)
	}
}

func TestBaseScopeConditions_soft_delete_emits_deleted_at_filter(t *testing.T) {
	res := &resourceErased{Name: "customer", Tenancy: TenancyNone, SoftDelete: true}
	ctx := &Ctx{}

	conds, _ := baseScopeConditions(ctx, res)

	joined := strings.Join(conds, " AND ")
	if !strings.Contains(joined, "deleted_at IS NULL") {
		t.Fatalf("SoftDelete resource must filter live rows; got: %q", joined)
	}
}

func TestBaseScopeConditions_soft_delete_and_org_tenancy_compose(t *testing.T) {
	res := &resourceErased{Name: "customer", Tenancy: TenancyOrg, SoftDelete: true}
	ctx := &Ctx{Tenant: &Tenant{OrgID: 7}}

	conds, values := baseScopeConditions(ctx, res)
	joined := strings.Join(conds, " AND ")

	if !strings.Contains(joined, "deleted_at IS NULL") {
		t.Fatalf("soft-delete clause must be present; got: %q", joined)
	}
	if !strings.Contains(joined, "org_id = $1") {
		t.Fatalf("tenant clause must be present; got: %q", joined)
	}
	if len(values) != 1 || values[0] != ID(7) {
		t.Fatalf("tenant value must be 7; got: %v", values)
	}
}
