package lazuli

import (
	"reflect"
	"testing"
)

func TestBuildDBBulkInsertSQLQuotesIdentifiers(t *testing.T) {
	statement, err := BuildDBBulkInsertSQL(DBBulkInsertOptions{
		Table:   "public.Order_Events",
		Columns: []string{"id", "select", "Created_At"},
		Rows: [][]any{
			{1, "created", "2026-05-12"},
		},
	})
	if err != nil {
		t.Fatalf("BuildDBBulkInsertSQL returned error: %v", err)
	}

	wantSQL := `INSERT INTO "public"."Order_Events" ("id", "select", "Created_At") VALUES ($1, $2, $3)`
	if statement.SQL != wantSQL {
		t.Fatalf("SQL = %q, want %q", statement.SQL, wantSQL)
	}
	assertDBBulkArgs(t, statement.Args, []any{1, "created", "2026-05-12"})
}

func TestBuildDBBulkInsertSQLBuildsMultiRowArgs(t *testing.T) {
	statement, err := BuildDBBulkInsertSQL(DBBulkInsertOptions{
		Table:   "users",
		Columns: []string{"id", "email"},
		Rows: [][]any{
			{1, "a@example.com"},
			{2, "b@example.com"},
			{3, nil},
		},
	})
	if err != nil {
		t.Fatalf("BuildDBBulkInsertSQL returned error: %v", err)
	}

	wantSQL := `INSERT INTO "users" ("id", "email") VALUES ($1, $2), ($3, $4), ($5, $6)`
	if statement.SQL != wantSQL {
		t.Fatalf("SQL = %q, want %q", statement.SQL, wantSQL)
	}
	assertDBBulkArgs(t, statement.Args, []any{1, "a@example.com", 2, "b@example.com", 3, nil})
}

func TestBuildDBBulkInsertSQLBuildsConflictUpdate(t *testing.T) {
	statement, err := BuildDBBulkInsertSQL(DBBulkInsertOptions{
		Table:   "app.users",
		Columns: []string{"id", "email", "name", "updated_at"},
		Rows: [][]any{
			{1, "a@example.com", "A", "now"},
			{2, "b@example.com", "B", "later"},
		},
		ConflictTarget:        []string{"email"},
		ConflictUpdateColumns: []string{"name", "updated_at"},
	})
	if err != nil {
		t.Fatalf("BuildDBBulkInsertSQL returned error: %v", err)
	}

	wantSQL := `INSERT INTO "app"."users" ("id", "email", "name", "updated_at") VALUES ($1, $2, $3, $4), ($5, $6, $7, $8) ON CONFLICT ("email") DO UPDATE SET "name" = EXCLUDED."name", "updated_at" = EXCLUDED."updated_at"`
	if statement.SQL != wantSQL {
		t.Fatalf("SQL = %q, want %q", statement.SQL, wantSQL)
	}
	assertDBBulkArgs(t, statement.Args, []any{1, "a@example.com", "A", "now", 2, "b@example.com", "B", "later"})
}

func TestBuildDBBulkInsertSQLBuildsConflictDoNothing(t *testing.T) {
	statement, err := BuildDBBulkInsertSQL(DBBulkInsertOptions{
		Table:   "users",
		Columns: []string{"email"},
		Rows: [][]any{
			{"a@example.com"},
		},
		ConflictTarget: []string{"email"},
	})
	if err != nil {
		t.Fatalf("BuildDBBulkInsertSQL returned error: %v", err)
	}

	wantSQL := `INSERT INTO "users" ("email") VALUES ($1) ON CONFLICT ("email") DO NOTHING`
	if statement.SQL != wantSQL {
		t.Fatalf("SQL = %q, want %q", statement.SQL, wantSQL)
	}
	assertDBBulkArgs(t, statement.Args, []any{"a@example.com"})
}

func TestBuildDBBulkInsertSQLRejectsInvalidInput(t *testing.T) {
	tests := []struct {
		name string
		opts DBBulkInsertOptions
	}{
		{
			name: "empty table",
			opts: DBBulkInsertOptions{
				Columns: []string{"id"},
				Rows:    [][]any{{1}},
			},
		},
		{
			name: "invalid table",
			opts: DBBulkInsertOptions{
				Table:   "public.users.extra",
				Columns: []string{"id"},
				Rows:    [][]any{{1}},
			},
		},
		{
			name: "empty columns",
			opts: DBBulkInsertOptions{
				Table: "users",
				Rows:  [][]any{{1}},
			},
		},
		{
			name: "invalid column",
			opts: DBBulkInsertOptions{
				Table:   "users",
				Columns: []string{"1id"},
				Rows:    [][]any{{1}},
			},
		},
		{
			name: "empty rows",
			opts: DBBulkInsertOptions{
				Table:   "users",
				Columns: []string{"id"},
			},
		},
		{
			name: "row width mismatch",
			opts: DBBulkInsertOptions{
				Table:   "users",
				Columns: []string{"id", "email"},
				Rows:    [][]any{{1}},
			},
		},
		{
			name: "conflict update without target",
			opts: DBBulkInsertOptions{
				Table:                 "users",
				Columns:               []string{"id", "email"},
				Rows:                  [][]any{{1, "a@example.com"}},
				ConflictUpdateColumns: []string{"email"},
			},
		},
		{
			name: "invalid conflict target",
			opts: DBBulkInsertOptions{
				Table:          "users",
				Columns:        []string{"id"},
				Rows:           [][]any{{1}},
				ConflictTarget: []string{"email-address"},
			},
		},
		{
			name: "invalid conflict update column",
			opts: DBBulkInsertOptions{
				Table:                 "users",
				Columns:               []string{"id", "email"},
				Rows:                  [][]any{{1, "a@example.com"}},
				ConflictTarget:        []string{"id"},
				ConflictUpdateColumns: []string{"email address"},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if statement, err := BuildDBBulkInsertSQL(tt.opts); err == nil {
				t.Fatalf("BuildDBBulkInsertSQL returned nil error with statement %#v", statement)
			}
		})
	}
}

func assertDBBulkArgs(t *testing.T, got, want []any) {
	t.Helper()
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("Args = %#v, want %#v", got, want)
	}
}
