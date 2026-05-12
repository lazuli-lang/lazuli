package migrations

import (
	"context"
	"errors"
	"testing"
	"testing/fstest"
)

func TestSeedLoaderLoadsSQLFilesInOrder(t *testing.T) {
	source := fstest.MapFS{
		"seeds/002_roles.sql":          &fstest.MapFile{Data: []byte("INSERT INTO roles VALUES ('admin');")},
		"seeds/001_users.sql":          &fstest.MapFile{Data: []byte("INSERT INTO users VALUES ('u1');")},
		"seeds/readme.md":              &fstest.MapFile{Data: []byte("ignored")},
		"seeds/nested/003_grants.sql":  &fstest.MapFile{Data: []byte("INSERT INTO grants VALUES ('admin', 'read');")},
		"seeds/nested/004_notes.sqlx":  &fstest.MapFile{Data: []byte("ignored")},
		"seeds/nested/005_upper.SQL":   &fstest.MapFile{Data: []byte("ignored")},
		"seeds/nested/006_empty.sql":   &fstest.MapFile{Data: nil},
		"seeds/nested/archive/keep.md": &fstest.MapFile{Data: []byte("ignored")},
	}
	executor := &recordingSQLExecutor{}

	applied, err := NewSeedLoader(source, "seeds", executor).Load(context.Background())
	if err != nil {
		t.Fatalf("Load returned %v", err)
	}

	if want := []string{
		"INSERT INTO users VALUES ('u1');",
		"INSERT INTO roles VALUES ('admin');",
		"INSERT INTO grants VALUES ('admin', 'read');",
		"",
	}; !equal(executor.sqls, want) {
		t.Fatalf("executed SQL = %v, want %v", executor.sqls, want)
	}
	if want := []AppliedSeed{
		{File: "seeds/001_users.sql", Bytes: len("INSERT INTO users VALUES ('u1');")},
		{File: "seeds/002_roles.sql", Bytes: len("INSERT INTO roles VALUES ('admin');")},
		{File: "seeds/nested/003_grants.sql", Bytes: len("INSERT INTO grants VALUES ('admin', 'read');")},
		{File: "seeds/nested/006_empty.sql", Bytes: 0},
	}; !equalAppliedSeeds(applied, want) {
		t.Fatalf("applied = %#v, want %#v", applied, want)
	}
}

func TestLoadSQLSeedsDiscoversRootAndNestedFiles(t *testing.T) {
	source := fstest.MapFS{
		"001_root.sql":             &fstest.MapFile{Data: []byte("SELECT 1;")},
		"002_root.txt":             &fstest.MapFile{Data: []byte("ignored")},
		"nested/003_seed.sql":      &fstest.MapFile{Data: []byte("SELECT 3;")},
		"nested/004_seed.down.sql": &fstest.MapFile{Data: []byte("SELECT 4;")},
	}
	executor := &recordingSQLExecutor{}

	applied, err := LoadSQLSeeds(context.Background(), source, "", executor)
	if err != nil {
		t.Fatalf("LoadSQLSeeds returned %v", err)
	}

	if want := []AppliedSeed{
		{File: "001_root.sql", Bytes: len("SELECT 1;")},
		{File: "nested/003_seed.sql", Bytes: len("SELECT 3;")},
		{File: "nested/004_seed.down.sql", Bytes: len("SELECT 4;")},
	}; !equalAppliedSeeds(applied, want) {
		t.Fatalf("applied = %#v, want %#v", applied, want)
	}
}

func TestSeedLoaderReturnsPartialResultsOnExecutionError(t *testing.T) {
	source := fstest.MapFS{
		"001_ok.sql":   &fstest.MapFile{Data: []byte("SELECT 1;")},
		"002_fail.sql": &fstest.MapFile{Data: []byte("SELECT 2;")},
		"003_skip.sql": &fstest.MapFile{Data: []byte("SELECT 3;")},
	}
	sentinel := errors.New("executor failed")
	executor := &recordingSQLExecutor{fail: sentinel, failN: 1}

	applied, err := NewSeedLoader(source, ".", executor).Load(context.Background())
	if !errors.Is(err, sentinel) {
		t.Fatalf("expected sentinel error, got %v", err)
	}
	if want := []AppliedSeed{
		{File: "001_ok.sql", Bytes: len("SELECT 1;")},
	}; !equalAppliedSeeds(applied, want) {
		t.Fatalf("applied = %#v, want %#v", applied, want)
	}
	if want := []string{"SELECT 1;"}; !equal(executor.sqls, want) {
		t.Fatalf("executed SQL = %v, want %v", executor.sqls, want)
	}
}

func TestSeedLoaderRespectsContextCancellation(t *testing.T) {
	source := fstest.MapFS{
		"001_first.sql":  &fstest.MapFile{Data: []byte("SELECT 1;")},
		"002_second.sql": &fstest.MapFile{Data: []byte("SELECT 2;")},
	}
	ctx, cancel := context.WithCancel(context.Background())
	executor := &cancelAfterFirstSeedExecutor{cancel: cancel}

	applied, err := NewSeedLoader(source, ".", executor).Load(ctx)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("expected context canceled, got %v", err)
	}
	if want := []AppliedSeed{
		{File: "001_first.sql", Bytes: len("SELECT 1;")},
	}; !equalAppliedSeeds(applied, want) {
		t.Fatalf("applied = %#v, want %#v", applied, want)
	}
	if want := []string{"SELECT 1;"}; !equal(executor.sqls, want) {
		t.Fatalf("executed SQL = %v, want %v", executor.sqls, want)
	}
}

func TestSeedLoaderValidatesRequiredInputs(t *testing.T) {
	source := fstest.MapFS{}
	executor := &recordingSQLExecutor{}

	if _, err := NewSeedLoader(nil, ".", executor).Load(context.Background()); !errors.Is(err, errNilMigrationFS) {
		t.Fatalf("nil FS error = %v", err)
	}
	if _, err := NewSeedLoader(source, ".", nil).Load(context.Background()); !errors.Is(err, errNilMigrationExecutor) {
		t.Fatalf("nil executor error = %v", err)
	}
	if _, err := NewSeedLoader(source, "../seeds", executor).Load(context.Background()); !errors.Is(err, errInvalidMigrationDir) {
		t.Fatalf("invalid dir error = %v", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := NewSeedLoader(source, ".", executor).Load(ctx); !errors.Is(err, context.Canceled) {
		t.Fatalf("canceled context error = %v", err)
	}
}

type cancelAfterFirstSeedExecutor struct {
	sqls   []string
	cancel context.CancelFunc
}

func (e *cancelAfterFirstSeedExecutor) Exec(_ context.Context, sql string) error {
	e.sqls = append(e.sqls, sql)
	if len(e.sqls) == 1 {
		e.cancel()
	}
	return nil
}

func equalAppliedSeeds(a, b []AppliedSeed) bool {
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
