package migrations

import (
	"context"
	"errors"
	"testing"
	"testing/fstest"
)

type recordingSQLExecutor struct {
	sqls  []string
	fail  error
	failN int
}

func (e *recordingSQLExecutor) Exec(_ context.Context, sql string) error {
	if e.fail != nil && len(e.sqls) == e.failN {
		return e.fail
	}
	e.sqls = append(e.sqls, sql)
	return nil
}

func TestRunnerAppliesForwardSQLFilesInOrder(t *testing.T) {
	source := fstest.MapFS{
		"migrations/002_add_users.sql":      &fstest.MapFile{Data: []byte("ALTER TABLE users ADD COLUMN name text;")},
		"migrations/001_create_users.sql":   &fstest.MapFile{Data: []byte("CREATE TABLE users (id text primary key);")},
		"migrations/003_add_users.down.sql": &fstest.MapFile{Data: []byte("DROP TABLE users;")},
		"migrations/readme.md":              &fstest.MapFile{Data: []byte("notes")},
	}
	executor := &recordingSQLExecutor{}

	applied, err := NewRunner(source, "migrations", executor).Apply(context.Background())
	if err != nil {
		t.Fatalf("Apply returned %v", err)
	}

	if want := []string{
		"CREATE TABLE users (id text primary key);",
		"ALTER TABLE users ADD COLUMN name text;",
	}; !equal(executor.sqls, want) {
		t.Fatalf("executed SQL = %v, want %v", executor.sqls, want)
	}
	if want := []AppliedMigration{
		{File: "migrations/001_create_users.sql", Bytes: len("CREATE TABLE users (id text primary key);")},
		{File: "migrations/002_add_users.sql", Bytes: len("ALTER TABLE users ADD COLUMN name text;")},
	}; !equalApplied(applied, want) {
		t.Fatalf("applied = %#v, want %#v", applied, want)
	}
}

func TestRunnerDiscoversNestedSQLFiles(t *testing.T) {
	source := fstest.MapFS{
		"001_root.sql":                   &fstest.MapFile{Data: []byte("SELECT 1;")},
		"tenants/002_tenant.sql":         &fstest.MapFile{Data: []byte("SELECT 2;")},
		"tenants/002_tenant.down.sql":    &fstest.MapFile{Data: []byte("SELECT -2;")},
		"tenants/archive/003_seed.sql":   &fstest.MapFile{Data: []byte("SELECT 3;")},
		"tenants/archive/003_seed.txt":   &fstest.MapFile{Data: []byte("ignored")},
		"tenants/archive/004_patch.SQL":  &fstest.MapFile{Data: []byte("ignored")},
		"tenants/archive/005_patch.sqlx": &fstest.MapFile{Data: []byte("ignored")},
	}
	executor := &recordingSQLExecutor{}

	applied, err := ApplySQLMigrations(context.Background(), source, "", executor)
	if err != nil {
		t.Fatalf("ApplySQLMigrations returned %v", err)
	}

	if want := []AppliedMigration{
		{File: "001_root.sql", Bytes: len("SELECT 1;")},
		{File: "tenants/002_tenant.sql", Bytes: len("SELECT 2;")},
		{File: "tenants/archive/003_seed.sql", Bytes: len("SELECT 3;")},
	}; !equalApplied(applied, want) {
		t.Fatalf("applied = %#v, want %#v", applied, want)
	}
}

func TestRunnerReturnsPartialResultsOnExecutionError(t *testing.T) {
	source := fstest.MapFS{
		"001_ok.sql":   &fstest.MapFile{Data: []byte("SELECT 1;")},
		"002_fail.sql": &fstest.MapFile{Data: []byte("SELECT 2;")},
		"003_skip.sql": &fstest.MapFile{Data: []byte("SELECT 3;")},
	}
	sentinel := errors.New("executor failed")
	executor := &recordingSQLExecutor{fail: sentinel, failN: 1}

	applied, err := NewRunner(source, ".", executor).Apply(context.Background())
	if !errors.Is(err, sentinel) {
		t.Fatalf("expected sentinel error, got %v", err)
	}
	if want := []AppliedMigration{
		{File: "001_ok.sql", Bytes: len("SELECT 1;")},
	}; !equalApplied(applied, want) {
		t.Fatalf("applied = %#v, want %#v", applied, want)
	}
	if want := []string{"SELECT 1;"}; !equal(executor.sqls, want) {
		t.Fatalf("executed SQL = %v, want %v", executor.sqls, want)
	}
}

func TestRunnerValidatesRequiredInputs(t *testing.T) {
	source := fstest.MapFS{}
	executor := &recordingSQLExecutor{}

	if _, err := NewRunner(nil, ".", executor).Apply(context.Background()); !errors.Is(err, errNilMigrationFS) {
		t.Fatalf("nil FS error = %v", err)
	}
	if _, err := NewRunner(source, ".", nil).Apply(context.Background()); !errors.Is(err, errNilMigrationExecutor) {
		t.Fatalf("nil executor error = %v", err)
	}
	if _, err := NewRunner(source, "../migrations", executor).Apply(context.Background()); !errors.Is(err, errInvalidMigrationDir) {
		t.Fatalf("invalid dir error = %v", err)
	}
}

func equalApplied(a, b []AppliedMigration) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
