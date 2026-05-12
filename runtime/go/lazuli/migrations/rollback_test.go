package migrations

import (
	"context"
	"errors"
	"testing"
	"testing/fstest"
)

func TestRollbackSQLMigrationsRollsBackLatestAppliedIDs(t *testing.T) {
	source := fstest.MapFS{
		"migrations/001_create_users.down.sql": &fstest.MapFile{Data: []byte("DROP TABLE users;")},
		"migrations/002_add_name.down.sql":     &fstest.MapFile{Data: []byte("ALTER TABLE users DROP COLUMN name;")},
		"migrations/003_seed_posts.down.sql":   &fstest.MapFile{Data: []byte("DELETE FROM posts;")},
		"migrations/004_future.sql":            &fstest.MapFile{Data: []byte("SELECT 4;")},
		"migrations/readme.md":                 &fstest.MapFile{Data: []byte("notes")},
	}
	executor := &recordingSQLExecutor{}

	summary, err := RollbackSQLMigrations(
		context.Background(),
		source,
		"migrations",
		executor,
		[]string{"001_create_users", "002_add_name", "003_seed_posts"},
		2,
	)
	if err != nil {
		t.Fatalf("RollbackSQLMigrations returned %v", err)
	}

	if want := []string{
		"DELETE FROM posts;",
		"ALTER TABLE users DROP COLUMN name;",
	}; !equal(executor.sqls, want) {
		t.Fatalf("executed SQL = %v, want %v", executor.sqls, want)
	}
	if summary.Discovered != 3 {
		t.Fatalf("Discovered = %d, want 3", summary.Discovered)
	}
	if want := []RolledBackMigration{
		{ID: "003_seed_posts", File: "migrations/003_seed_posts.down.sql", Bytes: len("DELETE FROM posts;")},
		{ID: "002_add_name", File: "migrations/002_add_name.down.sql", Bytes: len("ALTER TABLE users DROP COLUMN name;")},
	}; !equalRolledBack(summary.RolledBack, want) {
		t.Fatalf("RolledBack = %#v, want %#v", summary.RolledBack, want)
	}
}

func TestRollbackSQLMigrationsRollsBackExplicitIDs(t *testing.T) {
	source := fstest.MapFS{
		"migrations/001_create_users.down.sql": &fstest.MapFile{Data: []byte("DROP TABLE users;")},
		"migrations/002_add_name.down.sql":     &fstest.MapFile{Data: []byte("ALTER TABLE users DROP COLUMN name;")},
		"migrations/003_seed_posts.down.sql":   &fstest.MapFile{Data: []byte("DELETE FROM posts;")},
	}
	executor := &recordingSQLExecutor{}

	summary, err := RollbackSQLMigrations(
		context.Background(),
		source,
		"migrations",
		executor,
		[]string{"migrations/001_create_users.sql", "003_seed_posts"},
		0,
	)
	if err != nil {
		t.Fatalf("RollbackSQLMigrations returned %v", err)
	}

	if want := []string{
		"DELETE FROM posts;",
		"DROP TABLE users;",
	}; !equal(executor.sqls, want) {
		t.Fatalf("executed SQL = %v, want %v", executor.sqls, want)
	}
	if want := []RolledBackMigration{
		{ID: "003_seed_posts", File: "migrations/003_seed_posts.down.sql", Bytes: len("DELETE FROM posts;")},
		{ID: "001_create_users", File: "migrations/001_create_users.down.sql", Bytes: len("DROP TABLE users;")},
	}; !equalRolledBack(summary.RolledBack, want) {
		t.Fatalf("RolledBack = %#v, want %#v", summary.RolledBack, want)
	}
}

func TestRollbackSQLMigrationsUsesCountWithoutAppliedIDs(t *testing.T) {
	source := fstest.MapFS{
		"001_init.down.sql":   &fstest.MapFile{Data: []byte("SELECT -1;")},
		"002_middle.down.sql": &fstest.MapFile{Data: []byte("SELECT -2;")},
		"003_latest.down.sql": &fstest.MapFile{Data: []byte("SELECT -3;")},
	}
	executor := &recordingSQLExecutor{}

	summary, err := RollbackSQLMigrations(context.Background(), source, ".", executor, nil, 1)
	if err != nil {
		t.Fatalf("RollbackSQLMigrations returned %v", err)
	}

	if want := []string{"SELECT -3;"}; !equal(executor.sqls, want) {
		t.Fatalf("executed SQL = %v, want %v", executor.sqls, want)
	}
	if want := []RolledBackMigration{
		{ID: "003_latest", File: "003_latest.down.sql", Bytes: len("SELECT -3;")},
	}; !equalRolledBack(summary.RolledBack, want) {
		t.Fatalf("RolledBack = %#v, want %#v", summary.RolledBack, want)
	}
}

func TestRollbackSQLMigrationsReturnsPartialSummaryOnExecutionError(t *testing.T) {
	source := fstest.MapFS{
		"001_init.down.sql": &fstest.MapFile{Data: []byte("SELECT -1;")},
		"002_next.down.sql": &fstest.MapFile{Data: []byte("SELECT -2;")},
	}
	sentinel := errors.New("executor failed")
	executor := &recordingSQLExecutor{fail: sentinel, failN: 1}

	summary, err := RollbackSQLMigrations(
		context.Background(),
		source,
		".",
		executor,
		[]string{"001_init", "002_next"},
		0,
	)
	if !errors.Is(err, sentinel) {
		t.Fatalf("expected sentinel error, got %v", err)
	}
	if want := []RolledBackMigration{
		{ID: "002_next", File: "002_next.down.sql", Bytes: len("SELECT -2;")},
	}; !equalRolledBack(summary.RolledBack, want) {
		t.Fatalf("RolledBack = %#v, want %#v", summary.RolledBack, want)
	}
	if want := []string{"SELECT -2;"}; !equal(executor.sqls, want) {
		t.Fatalf("executed SQL = %v, want %v", executor.sqls, want)
	}
}

func TestRollbackSQLMigrationsValidatesInputsAndMissingDown(t *testing.T) {
	source := fstest.MapFS{
		"001_init.down.sql": &fstest.MapFile{Data: []byte("SELECT -1;")},
	}
	executor := &recordingSQLExecutor{}

	if _, err := RollbackSQLMigrations(context.Background(), nil, ".", executor, nil, 1); !errors.Is(err, errNilMigrationFS) {
		t.Fatalf("nil FS error = %v", err)
	}
	if _, err := RollbackSQLMigrations(context.Background(), source, ".", nil, nil, 1); !errors.Is(err, errNilMigrationExecutor) {
		t.Fatalf("nil executor error = %v", err)
	}
	if _, err := RollbackSQLMigrations(context.Background(), source, "../migrations", executor, nil, 1); !errors.Is(err, errInvalidMigrationDir) {
		t.Fatalf("invalid dir error = %v", err)
	}
	if _, err := RollbackSQLMigrations(context.Background(), source, ".", executor, nil, -1); !errors.Is(err, errInvalidRollbackCount) {
		t.Fatalf("negative count error = %v", err)
	}

	summary, err := RollbackSQLMigrations(context.Background(), source, ".", executor, []string{"002_missing"}, 0)
	if !errors.Is(err, ErrRollbackMigrationNotFound) {
		t.Fatalf("missing rollback error = %v", err)
	}
	if summary.Discovered != 1 {
		t.Fatalf("Discovered = %d, want 1", summary.Discovered)
	}
	if len(executor.sqls) != 0 {
		t.Fatalf("executed SQL = %v, want none", executor.sqls)
	}
}

func equalRolledBack(a, b []RolledBackMigration) bool {
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
