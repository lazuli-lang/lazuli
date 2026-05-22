package lazuli

import (
	"context"
	"reflect"
	"strings"
	"testing"
)

// `conventions [crud]` synth `update_<resource>` binds every input
// slot via FromInputOptional. When the wire payload omits a field, the
// generated input struct's pointer field is nil and the runtime must
// SKIP the column entirely so the existing row value stays put. This
// test pins that contract for the partial-update path.
func TestApplyUpdatesSkipsFromInputOptionalNilColumns(t *testing.T) {
	type partialInput struct {
		ID     int64
		Name   *string
		Gender *string
	}
	type row struct {
		ID     int64
		Name   string
		Gender string
	}

	resource := &Resource[row]{
		Name:       "Traveler",
		Tenancy:    TenancyOrg,
		SoftDelete: true,
		Timestamps: true,
	}
	eff := Updates(resource,
		Bindings{"id": FromInput("ID")},
		Bindings{
			"name":   FromInputOptional("Name"),
			"gender": FromInputOptional("Gender"),
		},
	)
	ctx := &Ctx{
		Context: context.Background(),
		Actor:   ActorUser,
		Tenant:  &Tenant{OrgID: 42},
	}
	tx := &updatedAtCaptureTxStub{}

	// Wire payload sent only `gender`; `name` is unset (nil pointer)
	// and must drop out of the SET clause entirely.
	gender := "male"
	_, _ = applyUpdates[partialInput, row](ctx, tx, eff, partialInput{
		ID:     7,
		Name:   nil,
		Gender: &gender,
	})

	if tx.querySQL == "" {
		t.Fatalf("expected applyUpdates to issue an UPDATE — got no Query call")
	}
	if strings.Contains(tx.querySQL, `"name"`) {
		t.Fatalf("nil-valued FromInputOptional column must NOT appear in SET clause:\n%s", tx.querySQL)
	}
	if !strings.Contains(tx.querySQL, `"gender"`) {
		t.Fatalf("non-nil FromInputOptional column must appear in SET clause:\n%s", tx.querySQL)
	}
	// Args should carry the gender value + tenant + id (no name slot).
	wantArgs := []any{"male", ID(42), int64(7)}
	if !reflect.DeepEqual(tx.queryArgs, wantArgs) {
		t.Fatalf("unexpected bind args:\nwant: %#v\ngot:  %#v\nSQL:  %s",
			wantArgs, tx.queryArgs, tx.querySQL)
	}
}

// Mirror of the above for applyCreates: nil-valued FromInputOptional
// columns drop out so the column default applies. Non-nil values keep
// the same INSERT slot as a plain FromInput.
func TestApplyCreatesSkipsFromInputOptionalNilColumns(t *testing.T) {
	type partialInput struct {
		Name  *string
		Color *string
	}
	type row struct {
		ID    int64
		Name  string
		Color string
	}

	resource := &Resource[row]{
		Name:       "Widget",
		Tenancy:    TenancyOrg,
		Timestamps: true,
	}
	eff := Creates(resource,
		Bindings{
			"name":  FromInputOptional("Name"),
			"color": FromInputOptional("Color"),
		},
	)
	ctx := &Ctx{
		Context: context.Background(),
		Actor:   ActorUser,
		Tenant:  &Tenant{OrgID: 42},
	}
	tx := &updatedAtCaptureTxStub{}

	color := "indigo"
	_, _ = applyCreates[partialInput, row](ctx, tx, eff, partialInput{
		Name:  nil,
		Color: &color,
	})

	if tx.querySQL == "" {
		t.Fatalf("expected applyCreates to issue an INSERT — got no Query call")
	}
	if strings.Contains(tx.querySQL, `"name"`) {
		t.Fatalf("nil-valued FromInputOptional column must NOT appear in INSERT column list:\n%s", tx.querySQL)
	}
	if !strings.Contains(tx.querySQL, `"color"`) {
		t.Fatalf("non-nil FromInputOptional column must appear in INSERT column list:\n%s", tx.querySQL)
	}
}

// A wire value of zero (`0`, `""`, `false`) is meaningful — the
// pointer is non-nil even though the value is the type's zero value.
// `FromInputOptional` must distinguish "field absent" (nil pointer)
// from "field present, value=0" so explicit zeros still reach the DB.
func TestApplyUpdatesKeepsFromInputOptionalZeroValues(t *testing.T) {
	type partialInput struct {
		ID   int64
		Name *string
	}
	type row struct {
		ID   int64
		Name string
	}

	resource := &Resource[row]{
		Name:       "Traveler",
		Tenancy:    TenancyOrg,
		SoftDelete: true,
		Timestamps: true,
	}
	eff := Updates(resource,
		Bindings{"id": FromInput("ID")},
		Bindings{"name": FromInputOptional("Name")},
	)
	ctx := &Ctx{
		Context: context.Background(),
		Actor:   ActorUser,
		Tenant:  &Tenant{OrgID: 42},
	}
	tx := &updatedAtCaptureTxStub{}

	empty := ""
	_, _ = applyUpdates[partialInput, row](ctx, tx, eff, partialInput{
		ID:   7,
		Name: &empty,
	})

	if !strings.Contains(tx.querySQL, `"name" = $1`) {
		t.Fatalf("explicit empty string must still SET the column:\n%s", tx.querySQL)
	}
	wantArgs := []any{"", ID(42), int64(7)}
	if !reflect.DeepEqual(tx.queryArgs, wantArgs) {
		t.Fatalf("unexpected bind args:\nwant: %#v\ngot:  %#v", wantArgs, tx.queryArgs)
	}
}
