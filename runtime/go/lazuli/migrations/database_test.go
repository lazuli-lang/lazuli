package migrations

import (
	"errors"
	"testing"
)

func TestBuildDatabaseLifecycleSQL(t *testing.T) {
	createSQL, err := BuildCreateDatabaseSQL("lazuli_test")
	if err != nil {
		t.Fatalf("BuildCreateDatabaseSQL returned %v", err)
	}
	if want := `CREATE DATABASE "lazuli_test";`; createSQL != want {
		t.Fatalf("create SQL = %q, want %q", createSQL, want)
	}

	dropSQL, err := BuildDropDatabaseSQL("lazuli_test", DropDatabaseOptions{IfExists: true})
	if err != nil {
		t.Fatalf("BuildDropDatabaseSQL returned %v", err)
	}
	if want := `DROP DATABASE IF EXISTS "lazuli_test";`; dropSQL != want {
		t.Fatalf("drop SQL = %q, want %q", dropSQL, want)
	}

	truncateSQL, err := BuildTruncateTablesSQL(TruncateTablesOptions{
		Tables: []TableName{
			{Schema: "app", Name: "users"},
			{Schema: "audit", Name: "events"},
			{Name: "queue_jobs"},
		},
		RestartIdentity: true,
		Cascade:         true,
	})
	if err != nil {
		t.Fatalf("BuildTruncateTablesSQL returned %v", err)
	}
	want := `TRUNCATE TABLE "app"."users", "audit"."events", "queue_jobs" RESTART IDENTITY CASCADE;`
	if truncateSQL != want {
		t.Fatalf("truncate SQL = %q, want %q", truncateSQL, want)
	}
}

func TestBuildDatabaseLifecycleSQLAllowsStrictIdentifiers(t *testing.T) {
	for _, name := range []string{"_", "_test", "Lazuli_2"} {
		t.Run(name, func(t *testing.T) {
			if _, err := BuildCreateDatabaseSQL(name); err != nil {
				t.Fatalf("BuildCreateDatabaseSQL(%q) returned %v", name, err)
			}
		})
	}

	statement, err := BuildTruncateTablesSQL(TruncateTablesOptions{
		Tables: []TableName{
			{Schema: "Tenant_1", Name: "_events"},
			{Name: "Users_2"},
		},
	})
	if err != nil {
		t.Fatalf("BuildTruncateTablesSQL returned %v", err)
	}
	want := `TRUNCATE TABLE "Tenant_1"."_events", "Users_2";`
	if statement != want {
		t.Fatalf("statement = %q, want %q", statement, want)
	}
}

func TestBuildDatabaseLifecycleSQLRejectsInvalidIdentifiers(t *testing.T) {
	tests := []struct {
		name string
		run  func() error
	}{
		{
			name: "empty database",
			run: func() error {
				_, err := BuildCreateDatabaseSQL("")
				return err
			},
		},
		{
			name: "database starts with digit",
			run: func() error {
				_, err := BuildDropDatabaseSQL("1test", DropDatabaseOptions{})
				return err
			},
		},
		{
			name: "database punctuation",
			run: func() error {
				_, err := BuildCreateDatabaseSQL("lazuli-test")
				return err
			},
		},
		{
			name: "database injection",
			run: func() error {
				_, err := BuildDropDatabaseSQL(`test"; DROP DATABASE prod; --`, DropDatabaseOptions{IfExists: true})
				return err
			},
		},
		{
			name: "table schema",
			run: func() error {
				_, err := BuildTruncateTablesSQL(TruncateTablesOptions{
					Tables: []TableName{{Schema: "bad-schema", Name: "users"}},
				})
				return err
			},
		},
		{
			name: "table name",
			run: func() error {
				_, err := BuildTruncateTablesSQL(TruncateTablesOptions{
					Tables: []TableName{{Schema: "app", Name: "users;drop"}},
				})
				return err
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := tt.run(); !errors.Is(err, ErrInvalidSQLIdentifier) {
				t.Fatalf("error = %v, want %v", err, ErrInvalidSQLIdentifier)
			}
		})
	}
}

func TestBuildTruncateTablesSQLRejectsNoTables(t *testing.T) {
	if statement, err := BuildTruncateTablesSQL(TruncateTablesOptions{}); !errors.Is(err, ErrNoTruncateTables) {
		t.Fatalf("BuildTruncateTablesSQL returned statement %q and error %v, want %v", statement, err, ErrNoTruncateTables)
	}
}
