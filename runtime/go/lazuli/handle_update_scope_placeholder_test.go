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

func TestApplyDeletesSoftDeleteUsesUniqueScopeAndWherePlaceholders(t *testing.T) {
	type input struct {
		ID int64
	}
	type output struct {
		ID int64
	}

	resource := &Resource[output]{
		Name:       "Traveler",
		Tenancy:    TenancyOrg,
		SoftDelete: true,
	}
	eff := Deletes(resource, Bindings{"id": FromInput("ID")})
	ctx := &Ctx{
		Context: context.Background(),
		Actor:   ActorUser,
		Tenant:  &Tenant{OrgID: 42},
	}
	tx := &updatedAtCaptureTxStub{}

	_, err := applyDeletes[input, output](ctx, tx, eff, input{ID: 7})
	if err == nil {
		t.Fatalf("expected stub-backed DELETE to surface no-row 404, got nil")
	}
	if tx.querySQL == "" {
		t.Fatalf("tx stub never received Query - applyDeletes aborted before SQL emit (err=%v)", err)
	}

	assertPlaceholdersUsedOnce(t, tx.querySQL, len(tx.queryArgs))

	wantArgs := []any{ID(42), int64(7)}
	if !reflect.DeepEqual(tx.queryArgs, wantArgs) {
		t.Fatalf("unexpected bind arg order:\nwant: %#v\ngot:  %#v\nSQL:  %s", wantArgs, tx.queryArgs, tx.querySQL)
	}

	for _, fragment := range []string{
		`UPDATE "traveler" SET "deleted_at" = now()`,
		`deleted_at IS NULL`,
		`org_id = $1`,
		`"id" = $2`,
		`RETURNING *`,
	} {
		if !strings.Contains(tx.querySQL, fragment) {
			t.Fatalf("expected SQL fragment %q in:\n%s", fragment, tx.querySQL)
		}
	}
}

func TestListAndLookupSelectsStartScopedPlaceholdersAtOne(t *testing.T) {
	type args struct {
		ID     int64
		Status string
	}
	type row struct {
		ID     int64
		Status string
	}

	resource := &Resource[row]{
		Name:       "ProjectTask",
		Tenancy:    TenancyOrg,
		SoftDelete: true,
	}
	ctx := &Ctx{
		Context: context.Background(),
		Actor:   ActorUser,
		Tenant:  &Tenant{OrgID: 42},
	}
	input := args{ID: 7, Status: "open"}

	list := &Query[args, row]{
		Name:     "tasks.list",
		Resource: resource,
		Kind:     QueryList,
		Filters: []FilterRule{
			{Column: "org_id", When: FromCtx("actor.org_id")},
			{Column: "status", When: FromInput("Status")},
		},
		Order:    []OrderClause{{Column: "id"}},
		Paginate: 25,
	}

	listSQL, listArgs, _, err := list.buildListSQL(ctx, input)
	if err != nil {
		t.Fatalf("buildListSQL returned error: %v", err)
	}
	assertPlaceholdersUsedOnce(t, listSQL, len(listArgs))
	if want := []any{ID(42), ID(42), "open"}; !reflect.DeepEqual(listArgs, want) {
		t.Fatalf("unexpected list bind args:\nwant: %#v\ngot:  %#v\nSQL:  %s", want, listArgs, listSQL)
	}
	for _, fragment := range []string{
		`SELECT * FROM "project_task"`,
		`deleted_at IS NULL`,
		`org_id = $1`,
		`"org_id" = $2`,
		`"status" = $3`,
		`ORDER BY "id" ASC`,
		`LIMIT 25`,
	} {
		if !strings.Contains(listSQL, fragment) {
			t.Fatalf("expected list SQL fragment %q in:\n%s", fragment, listSQL)
		}
	}

	lookup := &Query[args, row]{
		Name:     "tasks.lookup",
		Resource: resource,
		Kind:     QueryLookup,
		LookupBy: []LookupKey{
			{Column: "id", Source: FromInput("ID")},
			{Column: "status", Source: FromInput("Status")},
		},
	}

	lookupSQL, lookupArgs, _, err := lookup.buildLookupSQL(ctx, input)
	if err != nil {
		t.Fatalf("buildLookupSQL returned error: %v", err)
	}
	assertPlaceholdersUsedOnce(t, lookupSQL, len(lookupArgs))
	if want := []any{ID(42), int64(7), "open"}; !reflect.DeepEqual(lookupArgs, want) {
		t.Fatalf("unexpected lookup bind args:\nwant: %#v\ngot:  %#v\nSQL:  %s", want, lookupArgs, lookupSQL)
	}
	for _, fragment := range []string{
		`SELECT * FROM "project_task"`,
		`deleted_at IS NULL`,
		`org_id = $1`,
		`"id" = $2`,
		`"status" = $3`,
		`LIMIT 1`,
	} {
		if !strings.Contains(lookupSQL, fragment) {
			t.Fatalf("expected lookup SQL fragment %q in:\n%s", fragment, lookupSQL)
		}
	}
}

func TestApplyUpdatesMultiSetMultiWhereKeepsScopePlaceholderAfterSets(t *testing.T) {
	type input struct {
		ID     int64
		Status string
		A      string
		B      string
		C      string
		D      string
	}
	type output struct {
		ID     int64
		Status string
	}

	resource := &Resource[output]{
		Name:    "WorkItem",
		Tenancy: TenancyOrg,
	}
	eff := Updates(resource,
		Bindings{
			"id":     FromInput("ID"),
			"status": FromInput("Status"),
		},
		Bindings{
			"a": FromInput("A"),
			"b": FromInput("B"),
			"c": FromInput("C"),
			"d": FromInput("D"),
		},
	)
	ctx := &Ctx{
		Context: context.Background(),
		Actor:   ActorUser,
		Tenant:  &Tenant{OrgID: 42},
	}
	tx := &updatedAtCaptureTxStub{}

	_, err := applyUpdates[input, output](ctx, tx, eff, input{
		ID:     7,
		Status: "ready",
		A:      "alpha",
		B:      "bravo",
		C:      "charlie",
		D:      "delta",
	})
	if err == nil {
		t.Fatalf("expected stub-backed UPDATE to surface no-row 404, got nil")
	}
	if tx.querySQL == "" {
		t.Fatalf("tx stub never received Query - applyUpdates aborted before SQL emit (err=%v)", err)
	}

	assertPlaceholdersUsedOnce(t, tx.querySQL, len(tx.queryArgs))
	if len(tx.queryArgs) != 7 {
		t.Fatalf("expected 7 bind args, got %d:\nargs: %#v\nSQL:  %s", len(tx.queryArgs), tx.queryArgs, tx.querySQL)
	}
	if tx.queryArgs[4] != ID(42) {
		t.Fatalf("expected tenant bind at arg 5, got %#v:\nargs: %#v\nSQL:  %s", tx.queryArgs[4], tx.queryArgs, tx.querySQL)
	}
	if !strings.Contains(tx.querySQL, `org_id = $5`) {
		t.Fatalf("expected tenant scope to use $5 after four SET binds:\n%s", tx.querySQL)
	}
	for _, col := range []string{"a", "b", "c", "d"} {
		assertColumnPlaceholderInRange(t, tx.querySQL, col, 1, 4)
	}
	for _, col := range []string{"id", "status"} {
		assertColumnPlaceholderInRange(t, tx.querySQL, col, 6, 7)
	}
}

func TestApplyUpdatesTenancyNoneStartsPlaceholdersAtOne(t *testing.T) {
	type input struct {
		ID   int64
		Name string
	}
	type output struct {
		ID   int64
		Name string
	}

	resource := &Resource[output]{
		Name:    "GlobalSetting",
		Tenancy: TenancyNone,
	}
	eff := Updates(resource,
		Bindings{"id": FromInput("ID")},
		Bindings{"name": FromInput("Name")},
	)
	ctx := &Ctx{
		Context: context.Background(),
		Actor:   ActorAnonymous,
	}
	tx := &updatedAtCaptureTxStub{}

	_, err := applyUpdates[input, output](ctx, tx, eff, input{
		ID:   11,
		Name: "support_email",
	})
	if err == nil {
		t.Fatalf("expected stub-backed UPDATE to surface no-row 404, got nil")
	}
	if tx.querySQL == "" {
		t.Fatalf("tx stub never received Query - applyUpdates aborted before SQL emit (err=%v)", err)
	}

	assertPlaceholdersUsedOnce(t, tx.querySQL, len(tx.queryArgs))
	wantArgs := []any{"support_email", int64(11)}
	if !reflect.DeepEqual(tx.queryArgs, wantArgs) {
		t.Fatalf("unexpected bind arg order:\nwant: %#v\ngot:  %#v\nSQL:  %s", wantArgs, tx.queryArgs, tx.querySQL)
	}
	for _, fragment := range []string{
		`UPDATE "global_setting" SET "name" = $1`,
		`WHERE "id" = $2`,
		`RETURNING *`,
	} {
		if !strings.Contains(tx.querySQL, fragment) {
			t.Fatalf("expected SQL fragment %q in:\n%s", fragment, tx.querySQL)
		}
	}
	if strings.Contains(tx.querySQL, "org_id") {
		t.Fatalf("TenancyNone update must not bind tenant scope:\n%s", tx.querySQL)
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

func assertColumnPlaceholderInRange(t *testing.T, sql, column string, min, max int) {
	t.Helper()

	pattern := regexp.MustCompile(regexp.QuoteMeta(quoteIdent(column)) + ` = \$(\d+)`)
	match := pattern.FindStringSubmatch(sql)
	if len(match) != 2 {
		t.Fatalf("expected column %q placeholder in SQL:\n%s", column, sql)
	}
	idx, err := strconv.Atoi(match[1])
	if err != nil {
		t.Fatalf("invalid placeholder index for column %q: %v", column, err)
	}
	if idx < min || idx > max {
		t.Fatalf("expected column %q placeholder in $%d..$%d, got $%d in SQL:\n%s", column, min, max, idx, sql)
	}
}
