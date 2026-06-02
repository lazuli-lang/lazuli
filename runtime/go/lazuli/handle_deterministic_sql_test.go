package lazuli

import (
	"context"
	"sort"
	"strings"
	"testing"
)

// P2 (overnight-2026-06-02/03-codegen.md): the SQL-clause builders
// (applyCreates / applyUpdates / applyDeletes) iterate the Bindings map to
// produce the INSERT column list, SET clause, and WHERE clause. Go
// randomizes map iteration, so before the fix a fixed payload shape
// produced a *different* column order run-to-run — unstable SQL text that
// fragments pg_stat_statements and blocks plan-cache reuse. These tests
// pin that the emitted SQL is byte-identical across repeated calls and
// that the column order is the stable (sorted) one.

func TestApplyCreatesEmitsDeterministicColumnOrder(t *testing.T) {
	type wideInput struct {
		Alpha   string
		Bravo   string
		Charlie string
		Delta   string
		Echo    string
		Foxtrot string
	}
	type row struct {
		ID      int64
		Alpha   string
		Bravo   string
		Charlie string
		Delta   string
		Echo    string
		Foxtrot string
	}

	resource := &Resource[row]{Name: "Wide", Tenancy: TenancyNone}
	// Many bound columns → many map permutations if iteration were random.
	eff := Creates(resource, Bindings{
		"foxtrot": FromInput("Foxtrot"),
		"alpha":   FromInput("Alpha"),
		"delta":   FromInput("Delta"),
		"bravo":   FromInput("Bravo"),
		"echo":    FromInput("Echo"),
		"charlie": FromInput("Charlie"),
	})
	input := wideInput{"a", "b", "c", "d", "e", "f"}

	var first string
	for i := 0; i < 64; i++ {
		ctx := &Ctx{Context: context.Background(), Actor: ActorUser}
		tx := &updatedAtCaptureTxStub{}
		_, _ = applyCreates[wideInput, row](ctx, tx, eff, input)
		if tx.querySQL == "" {
			t.Fatalf("iter %d: expected an INSERT, got no Query", i)
		}
		if i == 0 {
			first = tx.querySQL
			continue
		}
		if tx.querySQL != first {
			t.Fatalf("INSERT SQL not deterministic across runs:\nfirst: %s\ngot:   %s", first, tx.querySQL)
		}
	}

	// And the order is the stable sorted one.
	wantOrder := []string{"alpha", "bravo", "charlie", "delta", "echo", "foxtrot"}
	if !columnsAppearInOrder(first, wantOrder) {
		t.Fatalf("INSERT columns not in sorted order; want %v in:\n%s", wantOrder, first)
	}
}

func TestApplyUpdatesEmitsDeterministicSetAndWhereOrder(t *testing.T) {
	type wideInput struct {
		ID      int64
		Alpha   string
		Bravo   string
		Charlie string
	}
	type row struct {
		ID      int64
		Alpha   string
		Bravo   string
		Charlie string
	}

	resource := &Resource[row]{Name: "Wide", Tenancy: TenancyNone}
	eff := Updates(resource,
		Bindings{"id": FromInput("ID")},
		Bindings{
			"charlie": FromInput("Charlie"),
			"alpha":   FromInput("Alpha"),
			"bravo":   FromInput("Bravo"),
		},
	)
	input := wideInput{ID: 1, Alpha: "a", Bravo: "b", Charlie: "c"}

	var first string
	for i := 0; i < 64; i++ {
		ctx := &Ctx{Context: context.Background(), Actor: ActorUser}
		tx := &updatedAtCaptureTxStub{}
		_, _ = applyUpdates[wideInput, row](ctx, tx, eff, input)
		if tx.querySQL == "" {
			t.Fatalf("iter %d: expected an UPDATE, got no Query", i)
		}
		if i == 0 {
			first = tx.querySQL
			continue
		}
		if tx.querySQL != first {
			t.Fatalf("UPDATE SQL not deterministic across runs:\nfirst: %s\ngot:   %s", first, tx.querySQL)
		}
	}
	wantSet := []string{"alpha", "bravo", "charlie"}
	if !columnsAppearInOrder(first, wantSet) {
		t.Fatalf("SET columns not in sorted order; want %v in:\n%s", wantSet, first)
	}
}

func TestSortedBindKeysIsLexicographic(t *testing.T) {
	b := Bindings{
		"zulu":  FromInput("Z"),
		"alpha": FromInput("A"),
		"mike":  FromInput("M"),
	}
	got := sortedBindKeys(b)
	want := []string{"alpha", "mike", "zulu"}
	if len(got) != len(want) {
		t.Fatalf("len mismatch: got %v want %v", got, want)
	}
	if !sort.StringsAreSorted(got) {
		t.Fatalf("sortedBindKeys output not sorted: %v", got)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("order mismatch at %d: got %v want %v", i, got, want)
		}
	}
}

// columnsAppearInOrder reports whether each quoted column name in `cols`
// appears in `sql`, and in the same relative order as `cols`.
func columnsAppearInOrder(sql string, cols []string) bool {
	prev := -1
	for _, c := range cols {
		idx := strings.Index(sql, `"`+c+`"`)
		if idx < 0 || idx <= prev {
			return false
		}
		prev = idx
	}
	return true
}
