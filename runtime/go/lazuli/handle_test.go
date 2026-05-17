// WHERE-clause shape tests for the new sourceCtxOwnedVia kind.
// Closes the relation-traversal arm of @scope.owner per the hostpoint
// Phase 4 capability audit (2026-05-17).
package lazuli

import (
	"strings"
	"testing"
)

func TestWhereConditionFragmentDirectColumn(t *testing.T) {
	t.Parallel()
	src := FromCtx("user.id")
	got := whereConditionFragment("user_id", src, 2)
	want := `"user_id" = $2`
	if got != want {
		t.Fatalf("direct ctx: got %q, want %q", got, want)
	}
}

func TestWhereConditionFragmentOwnedViaRelation(t *testing.T) {
	t.Parallel()
	src := FromCtxOwnedVia("host", "user_id", "user.id")
	got := whereConditionFragment("host", src, 3)
	want := `"host" IN (SELECT id FROM "host" WHERE "user_id" = $3)`
	if got != want {
		t.Fatalf("owned-via relation: got %q, want %q", got, want)
	}
}

func TestWhereConditionFragmentRefusesUnsafeIdent(t *testing.T) {
	t.Parallel()
	defer func() {
		if r := recover(); r == nil {
			t.Fatalf("expected panic on suspicious identifier")
		} else {
			msg := r.(string)
			if !strings.Contains(msg, "refusing to quote") {
				t.Fatalf("unexpected panic message: %s", msg)
			}
		}
	}()
	src := FromCtxOwnedVia("ho;st", "user_id", "user.id")
	_ = whereConditionFragment("host", src, 1)
}
