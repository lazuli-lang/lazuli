package lazuli

import (
	"errors"
	"reflect"
	"testing"
)

func TestBuildDBOptimisticLockWhereBuildsPredicateAndArgs(t *testing.T) {
	got, err := BuildDBOptimisticLockWhere(DBOptimisticLockVersion{
		Column:   "lock_version",
		Expected: int64(4),
	}, "$7")
	if err != nil {
		t.Fatalf("BuildDBOptimisticLockWhere returned error: %v", err)
	}

	if got.SQL != `"lock_version" = $7` {
		t.Fatalf("SQL = %q, want version predicate", got.SQL)
	}
	if !reflect.DeepEqual(got.Args, []any{int64(4)}) {
		t.Fatalf("Args = %#v, want expected version", got.Args)
	}
}

func TestBuildDBOptimisticLockWhereUsesDefaultColumnAndAdapterPlaceholders(t *testing.T) {
	tests := []struct {
		name        string
		placeholder string
		wantSQL     string
	}{
		{name: "positional", placeholder: "?", wantSQL: `"version" = ?`},
		{name: "at name", placeholder: "@p1", wantSQL: `"version" = @p1`},
		{name: "colon name", placeholder: ":expected_version", wantSQL: `"version" = :expected_version`},
		{name: "trimmed", placeholder: "  $3  ", wantSQL: `"version" = $3`},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := BuildDBOptimisticLockWhere(DBOptimisticLockVersion{Expected: 12}, tt.placeholder)
			if err != nil {
				t.Fatalf("BuildDBOptimisticLockWhere returned error: %v", err)
			}
			if got.SQL != tt.wantSQL {
				t.Fatalf("SQL = %q, want %q", got.SQL, tt.wantSQL)
			}
			if !reflect.DeepEqual(got.Args, []any{12}) {
				t.Fatalf("Args = %#v, want expected version", got.Args)
			}
		})
	}
}

func TestBuildDBOptimisticLockNextVersionAssignmentBuildsAssignmentAndArgs(t *testing.T) {
	got, err := BuildDBOptimisticLockNextVersionAssignment(DBOptimisticLockVersion{
		Column: "row_version",
		Next:   int64(8),
	}, ":next_version")
	if err != nil {
		t.Fatalf("BuildDBOptimisticLockNextVersionAssignment returned error: %v", err)
	}

	if got.SQL != `"row_version" = :next_version` {
		t.Fatalf("SQL = %q, want next-version assignment", got.SQL)
	}
	if !reflect.DeepEqual(got.Args, []any{int64(8)}) {
		t.Fatalf("Args = %#v, want next version", got.Args)
	}
}

func TestDBOptimisticLockRejectsInvalidColumns(t *testing.T) {
	invalidColumns := []string{
		"1version",
		"version-id",
		"version id",
		"version.id",
		`version"`,
		"version;drop",
		"versaoé",
	}

	for _, column := range invalidColumns {
		t.Run(column, func(t *testing.T) {
			_, err := BuildDBOptimisticLockWhere(DBOptimisticLockVersion{
				Column:   column,
				Expected: 1,
			}, "$1")
			if !errors.Is(err, errInvalidDBOptimisticLockColumn) {
				t.Fatalf("BuildDBOptimisticLockWhere error = %v, want invalid column", err)
			}
		})
	}
}

func TestDBOptimisticLockRejectsInvalidPlaceholders(t *testing.T) {
	invalidPlaceholders := []string{
		"",
		"$",
		"@",
		":",
		"p1",
		"$1 $2",
		"$1;DROP",
		"${version}",
		"@p 1",
	}

	for _, placeholder := range invalidPlaceholders {
		t.Run(placeholder, func(t *testing.T) {
			_, err := BuildDBOptimisticLockNextVersionAssignment(DBOptimisticLockVersion{Next: 2}, placeholder)
			if !errors.Is(err, errInvalidDBOptimisticLockPlaceholder) {
				t.Fatalf("BuildDBOptimisticLockNextVersionAssignment error = %v, want invalid placeholder", err)
			}
		})
	}
}

func TestCheckDBOptimisticLockRowsAffected(t *testing.T) {
	if err := CheckDBOptimisticLockRowsAffected(1, "customer", ID(7)); err != nil {
		t.Fatalf("CheckDBOptimisticLockRowsAffected(1) returned error: %v", err)
	}
	if err := CheckDBOptimisticLockRowsAffected(3, "customer", ID(7)); err != nil {
		t.Fatalf("CheckDBOptimisticLockRowsAffected(3) returned error: %v", err)
	}

	err := CheckDBOptimisticLockRowsAffected(0, "customer", ID(7))
	if !errors.Is(err, ErrRepositoryConflict) {
		t.Fatalf("CheckDBOptimisticLockRowsAffected(0) error = %v, want repository conflict", err)
	}
	var conflict *RepositoryConflictError
	if !errors.As(err, &conflict) {
		t.Fatal("CheckDBOptimisticLockRowsAffected did not return RepositoryConflictError")
	}
	if conflict.Resource != "customer" || conflict.ID != ID(7) || conflict.Reason != DBOptimisticLockConflictReason {
		t.Fatalf("conflict = %#v, want resource/id/reason metadata", conflict)
	}
}

func TestCheckDBOptimisticLockRowsAffectedRejectsNegativeCounts(t *testing.T) {
	err := CheckDBOptimisticLockRowsAffected(-1, "customer", ID(7))
	if !errors.Is(err, errInvalidDBOptimisticLockRowsAffected) {
		t.Fatalf("CheckDBOptimisticLockRowsAffected(-1) error = %v, want invalid rows affected", err)
	}
}
