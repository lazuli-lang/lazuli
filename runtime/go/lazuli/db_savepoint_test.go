package lazuli

import (
	"context"
	"errors"
	"reflect"
	"testing"

	"github.com/jackc/pgx/v5/pgconn"
)

func TestWithSavepointReleasesOnSuccess(t *testing.T) {
	ctx := context.Background()
	tx := &savepointExecFake{}
	called := false

	err := WithSavepoint(ctx, tx, "sp_1", func(got context.Context) error {
		if got != ctx {
			t.Fatal("WithSavepoint passed a different context to fn")
		}
		called = true
		return nil
	})

	if err != nil {
		t.Fatalf("WithSavepoint returned error: %v", err)
	}
	if !called {
		t.Fatal("WithSavepoint did not call fn")
	}
	assertSavepointStatements(t, tx.statements, []string{
		"SAVEPOINT sp_1",
		"RELEASE SAVEPOINT sp_1",
	})
}

func TestWithSavepointRollsBackAndReturnsFunctionError(t *testing.T) {
	ctx := context.Background()
	tx := &savepointExecFake{}
	wantErr := errors.New("function failed")

	err := WithSavepoint(ctx, tx, "_retry", func(context.Context) error {
		return wantErr
	})

	if !errors.Is(err, wantErr) {
		t.Fatalf("WithSavepoint error = %v, want %v", err, wantErr)
	}
	assertSavepointStatements(t, tx.statements, []string{
		"SAVEPOINT _retry",
		"ROLLBACK TO SAVEPOINT _retry",
	})
}

func TestWithSavepointReturnsSavepointError(t *testing.T) {
	ctx := context.Background()
	wantErr := errors.New("savepoint failed")
	tx := &savepointExecFake{
		errByStatement: map[string]error{
			"SAVEPOINT sp": wantErr,
		},
	}
	called := false

	err := WithSavepoint(ctx, tx, "sp", func(context.Context) error {
		called = true
		return nil
	})

	if !errors.Is(err, wantErr) {
		t.Fatalf("WithSavepoint error = %v, want %v", err, wantErr)
	}
	if called {
		t.Fatal("WithSavepoint called fn after SAVEPOINT failed")
	}
	assertSavepointStatements(t, tx.statements, []string{"SAVEPOINT sp"})
}

func TestWithSavepointReturnsReleaseError(t *testing.T) {
	ctx := context.Background()
	wantErr := errors.New("release failed")
	tx := &savepointExecFake{
		errByStatement: map[string]error{
			"RELEASE SAVEPOINT sp": wantErr,
		},
	}

	err := WithSavepoint(ctx, tx, "sp", func(context.Context) error {
		return nil
	})

	if !errors.Is(err, wantErr) {
		t.Fatalf("WithSavepoint error = %v, want %v", err, wantErr)
	}
	assertSavepointStatements(t, tx.statements, []string{
		"SAVEPOINT sp",
		"RELEASE SAVEPOINT sp",
	})
}

func TestWithSavepointRejectsInvalidNames(t *testing.T) {
	ctx := context.Background()
	invalidNames := []string{
		"",
		"1sp",
		"sp-name",
		"sp name",
		"sp.name",
		"sp$name",
		"sp;drop",
		"sp\"name",
		"sp\nname",
		"spé",
	}

	for _, name := range invalidNames {
		t.Run(name, func(t *testing.T) {
			tx := &savepointExecFake{}
			called := false

			err := WithSavepoint(ctx, tx, name, func(context.Context) error {
				called = true
				return nil
			})

			if err == nil {
				t.Fatal("WithSavepoint returned nil error for invalid name")
			}
			if called {
				t.Fatal("WithSavepoint called fn for invalid name")
			}
			assertSavepointStatements(t, tx.statements, nil)
		})
	}
}

func TestWithSavepointAllowsStrictIdentifierNames(t *testing.T) {
	ctx := context.Background()
	names := []string{"_", "_1", "sp1", "SP_2", "a_b_C3"}

	for _, name := range names {
		t.Run(name, func(t *testing.T) {
			tx := &savepointExecFake{}

			err := WithSavepoint(ctx, tx, name, func(context.Context) error {
				return nil
			})

			if err != nil {
				t.Fatalf("WithSavepoint returned error: %v", err)
			}
			assertSavepointStatements(t, tx.statements, []string{
				"SAVEPOINT " + name,
				"RELEASE SAVEPOINT " + name,
			})
		})
	}
}

type savepointExecFake struct {
	statements     []string
	errByStatement map[string]error
}

func (tx *savepointExecFake) Exec(_ context.Context, sql string, args ...any) (pgconn.CommandTag, error) {
	if len(args) != 0 {
		panic("savepointExecFake received unexpected arguments")
	}
	tx.statements = append(tx.statements, sql)
	return pgconn.CommandTag{}, tx.errByStatement[sql]
}

func assertSavepointStatements(t *testing.T, got, want []string) {
	t.Helper()
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("statements = %#v, want %#v", got, want)
	}
}
