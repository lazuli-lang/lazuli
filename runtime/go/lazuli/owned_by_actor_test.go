package lazuli

import (
	"context"
	"errors"
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

func TestOwnedByActorMatchesOwner(t *testing.T) {
	var gotSQL string
	var gotArgs []any
	previous := ownedByActorQueryRow
	ownedByActorQueryRow = func(_ context.Context, sql string, args ...any) (pgx.Row, error) {
		gotSQL = sql
		gotArgs = append([]any(nil), args...)
		return ownedByActorRowStub{owner: 42}, nil
	}
	t.Cleanup(func() { ownedByActorQueryRow = previous })

	ok, err := OwnedByActor(&Ctx{User: &User{ID: 42}}, "booking", "host", 7)
	if err != nil {
		t.Fatalf("OwnedByActor() error = %v, want nil", err)
	}
	if !ok {
		t.Fatal("OwnedByActor() = false, want true")
	}
	if gotSQL != `SELECT "host" FROM "booking" WHERE id = $1` {
		t.Fatalf("SQL = %q", gotSQL)
	}
	if len(gotArgs) != 1 || gotArgs[0] != ID(7) {
		t.Fatalf("args = %#v, want [7]", gotArgs)
	}
}

func TestOwnedByActorReturnsFalseOnMismatch(t *testing.T) {
	previous := ownedByActorQueryRow
	ownedByActorQueryRow = func(context.Context, string, ...any) (pgx.Row, error) {
		return ownedByActorRowStub{owner: 9}, nil
	}
	t.Cleanup(func() { ownedByActorQueryRow = previous })

	ok, err := OwnedByActor(&Ctx{User: &User{ID: 42}}, "booking", "host", 7)
	if err != nil {
		t.Fatalf("OwnedByActor() error = %v, want nil", err)
	}
	if ok {
		t.Fatal("OwnedByActor() = true, want false")
	}
}

func TestOwnedByActorReturnsFalseOnMissingRow(t *testing.T) {
	previous := ownedByActorQueryRow
	ownedByActorQueryRow = func(context.Context, string, ...any) (pgx.Row, error) {
		return ownedByActorRowStub{err: pgx.ErrNoRows}, nil
	}
	t.Cleanup(func() { ownedByActorQueryRow = previous })

	ok, err := OwnedByActor(&Ctx{User: &User{ID: 42}}, "booking", "host", 7)
	if err != nil {
		t.Fatalf("OwnedByActor() error = %v, want nil", err)
	}
	if ok {
		t.Fatal("OwnedByActor() = true, want false")
	}
}

func TestOwnedByActorRejectsNilUserBeforeQuery(t *testing.T) {
	called := false
	previous := ownedByActorQueryRow
	ownedByActorQueryRow = func(context.Context, string, ...any) (pgx.Row, error) {
		called = true
		return ownedByActorRowStub{}, nil
	}
	t.Cleanup(func() { ownedByActorQueryRow = previous })

	ok, err := OwnedByActor(&Ctx{}, "booking", "host", 7)
	if err == nil || ok {
		t.Fatalf("OwnedByActor() = (%v, %v), want unauthenticated false", ok, err)
	}
	if called {
		t.Fatal("OwnedByActor() queried DB before auth guard")
	}
}

func TestOwnedByActorReturnsInternalOnNilDB(t *testing.T) {
	previousQuery := ownedByActorQueryRow
	previousDB := dbPool
	ownedByActorQueryRow = defaultOwnedByActorQueryRow
	SetDB((*pgxpool.Pool)(nil))
	t.Cleanup(func() {
		ownedByActorQueryRow = previousQuery
		dbPool = previousDB
	})

	_, err := OwnedByActor(&Ctx{User: &User{ID: 42}}, "booking", "host", 7)
	var le *Error
	if !errors.As(err, &le) || le.Code != CodeInternal {
		t.Fatalf("OwnedByActor() error = %v, want internal *Error", err)
	}
}

type ownedByActorRowStub struct {
	owner ID
	err   error
}

func (row ownedByActorRowStub) Scan(dest ...any) error {
	if row.err != nil {
		return row.err
	}
	*dest[0].(*ID) = row.owner
	return nil
}
