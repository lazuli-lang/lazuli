package lazuli

import (
	"context"
	"reflect"
	"regexp"
	"strconv"
	"strings"
	"testing"
)

func TestApplyUpdatesOffsetsBaseScopePlaceholdersAfterSetBindings(t *testing.T) {
	type input struct {
		ID      int64
		Vehicle string
	}
	type output struct {
		ID      int64
		Vehicle string
	}

	resource := &Resource[output]{
		Name:       "Traveler",
		Tenancy:    TenancyOrg,
		SoftDelete: true,
		Timestamps: true,
	}
	eff := Updates(resource,
		Bindings{"id": FromInput("ID")},
		Bindings{"vehicle": FromInput("Vehicle")},
	)
	ctx := &Ctx{
		Context: context.Background(),
		Actor:   ActorUser,
		Tenant:  &Tenant{OrgID: 42},
	}
	tx := &updatedAtCaptureTxStub{}

	_, err := applyUpdates[input, output](ctx, tx, eff, input{
		ID:      7,
		Vehicle: "sedan",
	})
	if err == nil {
		t.Fatalf("expected stub-backed UPDATE to surface no-row 404, got nil")
	}
	if tx.querySQL == "" {
		t.Fatalf("tx stub never received Query - applyUpdates aborted before SQL emit (err=%v)", err)
	}

	assertPlaceholdersUsedOnce(t, tx.querySQL, len(tx.queryArgs))

	wantArgs := []any{"sedan", ID(42), int64(7)}
	if !reflect.DeepEqual(tx.queryArgs, wantArgs) {
		t.Fatalf("unexpected bind arg order:\nwant: %#v\ngot:  %#v\nSQL:  %s", wantArgs, tx.queryArgs, tx.querySQL)
	}

	for _, fragment := range []string{
		`"vehicle" = $1`,
		`org_id = $2`,
		`deleted_at IS NULL`,
		`"id" = $3`,
		`"updated_at" = now()`,
	} {
		if !strings.Contains(tx.querySQL, fragment) {
			t.Fatalf("expected SQL fragment %q in:\n%s", fragment, tx.querySQL)
		}
	}
}

func assertPlaceholdersUsedOnce(t *testing.T, sql string, argCount int) {
	t.Helper()

	matches := regexp.MustCompile(`\$(\d+)`).FindAllStringSubmatch(sql, -1)
	if len(matches) != argCount {
		t.Fatalf("expected exactly %d placeholders, got %d in SQL:\n%s", argCount, len(matches), sql)
	}

	counts := make(map[string]int, argCount)
	for _, match := range matches {
		counts[match[1]]++
	}
	for i := 1; i <= argCount; i++ {
		key := strconv.Itoa(i)
		if counts[key] != 1 {
			t.Fatalf("expected placeholder $%d exactly once, got %d in SQL:\n%s", i, counts[key], sql)
		}
	}
}
