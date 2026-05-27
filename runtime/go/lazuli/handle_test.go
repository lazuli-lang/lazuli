// WHERE-clause shape tests for the new sourceCtxOwnedVia kind.
// Closes the relation-traversal arm of @scope.owner per the the canonical pilot
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

// quoteResourceTable lowercases + snake_cases the resource's authored
// PascalCase name so SELECT/INSERT/UPDATE/DELETE round-trip with the
// migration emit. The list/lookup SELECT path in `run.go` previously
// used the raw `quoteIdent(res.Name)` which produced quoted PascalCase
// — case-sensitive in Postgres and failing with 42P01 against the
// snake_case migrated tables. The 3 fixtures cover the shapes that
// surfaced during the the canonical pilot booking-flow validation:
//   - single-word resource (`User` → `user`)
//   - multi-word PascalCase (`UserSession` → `user_session`)
//   - 3+ words for completeness (`ServiceTransaction` → `service_transaction`).
func TestQuoteResourceTableSnakeCasesPascalName(t *testing.T) {
	t.Parallel()
	cases := []struct {
		in   string
		want string
	}{
		{in: "User", want: `"user"`},
		{in: "UserSession", want: `"user_session"`},
		{in: "ServiceTransaction", want: `"service_transaction"`},
	}
	for _, tc := range cases {
		got := quoteResourceTable(tc.in)
		if got != tc.want {
			t.Errorf("quoteResourceTable(%q) = %q, want %q", tc.in, got, tc.want)
		}
	}
}
