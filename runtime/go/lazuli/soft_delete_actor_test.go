package lazuli

import "testing"

// Spec 0015 — `soft_delete by` projects a `deleted_by` actor column. The
// runtime gates the `"deleted_by" = $actor` SET-clause append (in
// `applyDeletes`) on `Resource.SoftDeleteActor` via `HasColumn`. These
// tests pin the flag plumbing without a DB: a bare `soft_delete` resource
// must NOT report a `deleted_by` column (or the soft-delete UPDATE would
// reference a non-existent column → PG 42703), while a `soft_delete by`
// resource must.

func TestHasColumn_deleted_by_gated_on_actor_flag(t *testing.T) {
	bare := &resourceErased{Name: "customer", SoftDelete: true, SoftDeleteActor: false}
	if bare.HasColumn("deleted_by") {
		t.Fatalf("bare soft_delete must not expose deleted_by column")
	}
	if !bare.HasColumn("deleted_at") {
		t.Fatalf("soft_delete resource must expose deleted_at column")
	}

	actor := &resourceErased{Name: "customer", SoftDelete: true, SoftDeleteActor: true}
	if !actor.HasColumn("deleted_by") {
		t.Fatalf("soft_delete by must expose deleted_by column")
	}
}

func TestErased_propagates_soft_delete_actor(t *testing.T) {
	r := &Resource[struct{}]{Name: "customer", SoftDelete: true, SoftDeleteActor: true}
	e := r.erased()
	if !e.SoftDeleteActor {
		t.Fatalf("erased() must carry SoftDeleteActor into the runtime view")
	}
}
