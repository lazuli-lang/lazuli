package lazuli

import (
	"context"
	"strings"
	"testing"
)

// applyCreates must auto-set `updated_at = $N` (bound to now()) on
// INSERT for any resource whose `Timestamps: true` predicate fires,
// mirroring the UPDATE-path bump. The DDL emits `updated_at TIMESTAMPTZ
// NOT NULL` for these resources; without the runtime auto-bind the
// INSERT short-circuits with `null value in column "updated_at"
// violates not-null constraint` for every `creates <Resource>` block
// that doesn't manually bind the column (i.e. essentially all of them
// — authors don't repeat `updated_at = ctx.now` per `creates`).
func TestApplyCreatesAutoBindsUpdatedAtWhenResourceHasColumn(t *testing.T) {
	type input struct {
		Email string
	}
	type row struct {
		ID    int64
		Email string
	}

	resource := &Resource[row]{
		Name:       "User",
		Tenancy:    TenancyOrg,
		Timestamps: true,
	}
	eff := Creates(resource, Bindings{
		"email": FromInput("Email"),
	})
	ctx := &Ctx{
		Context: context.Background(),
		Actor:   ActorUser,
		Tenant:  &Tenant{OrgID: 42},
	}
	tx := &updatedAtCaptureTxStub{}

	_, _ = applyCreates[input, row](ctx, tx, eff, input{Email: "probe@example.com"})

	if tx.querySQL == "" {
		t.Fatalf("expected applyCreates to issue an INSERT — got no Query call")
	}
	if !strings.Contains(tx.querySQL, `"updated_at"`) {
		t.Fatalf("applyCreates must auto-bind `updated_at` for Timestamps:true resources:\n%s", tx.querySQL)
	}
}

// Symmetric to the above: when the author EXPLICITLY binds `updated_at`
// in the `creates` clause (rare — usually a backfill / data-import
// command), the runtime must respect that binding and not double-bind
// the column (which would emit invalid SQL like
// `(... "updated_at", "updated_at" ...) VALUES (... $5, $6 ...)`).
func TestApplyCreatesDoesNotDoubleBindUpdatedAtWhenAlreadyBound(t *testing.T) {
	type input struct {
		Email     string
		UpdatedAt string
	}
	type row struct {
		ID        int64
		Email     string
		UpdatedAt string
	}

	resource := &Resource[row]{
		Name:       "User",
		Tenancy:    TenancyOrg,
		Timestamps: true,
	}
	eff := Creates(resource, Bindings{
		"email":      FromInput("Email"),
		"updated_at": FromInput("UpdatedAt"),
	})
	ctx := &Ctx{
		Context: context.Background(),
		Actor:   ActorUser,
		Tenant:  &Tenant{OrgID: 42},
	}
	tx := &updatedAtCaptureTxStub{}

	_, _ = applyCreates[input, row](ctx, tx, eff, input{
		Email:     "probe@example.com",
		UpdatedAt: "2026-05-28T13:00:00Z",
	})

	if tx.querySQL == "" {
		t.Fatalf("expected applyCreates to issue an INSERT — got no Query call")
	}
	occurrences := strings.Count(tx.querySQL, `"updated_at"`)
	if occurrences != 1 {
		t.Fatalf("expected exactly 1 `\"updated_at\"` reference (author-bound), got %d:\n%s", occurrences, tx.querySQL)
	}
}

// When the resource opts out of timestamps (no `defaults timestamps`
// and no explicit `created_at`/`updated_at` fields), the runtime must
// NOT inject the column — the DDL won't have it and an INSERT
// referencing the column would raise PG 42703 `column "updated_at"
// does not exist`. This pins the negative path of the auto-bind.
func TestApplyCreatesSkipsUpdatedAtWhenResourceHasNoTimestamps(t *testing.T) {
	type input struct {
		Name string
	}
	type row struct {
		ID   int64
		Name string
	}

	resource := &Resource[row]{
		Name:       "Org",
		Tenancy:    TenancyNone,
		Timestamps: false,
	}
	eff := Creates(resource, Bindings{
		"name": FromInput("Name"),
	})
	ctx := &Ctx{
		Context: context.Background(),
		Actor:   ActorUser,
	}
	tx := &updatedAtCaptureTxStub{}

	_, _ = applyCreates[input, row](ctx, tx, eff, input{Name: "Probe"})

	if tx.querySQL == "" {
		t.Fatalf("expected applyCreates to issue an INSERT — got no Query call")
	}
	if strings.Contains(tx.querySQL, `"updated_at"`) {
		t.Fatalf("applyCreates must NOT inject `updated_at` for Timestamps:false resources:\n%s", tx.querySQL)
	}
}
