package lazuli

import (
	"context"
	"errors"
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/pgxpool"
)

var (
	_ ExecQuerier = pgx.Tx(nil)
	_ ExecQuerier = (*pgxpool.Pool)(nil)
)

func TestWithAdvisoryLockExecutesLockAndRunsFn(t *testing.T) {
	ctx := context.WithValue(context.Background(), advisoryLockTestContextKey{}, "ctx")
	tx := &advisoryLockTxFake{}

	var fnCalled bool
	err := WithAdvisoryLock(ctx, tx, 42, func(fnCtx context.Context) error {
		fnCalled = true
		if fnCtx != ctx {
			t.Fatal("fn received different context")
		}
		return nil
	})
	if err != nil {
		t.Fatalf("WithAdvisoryLock returned error: %v", err)
	}

	if !fnCalled {
		t.Fatal("fn was not called")
	}
	if tx.execContext != ctx {
		t.Fatal("Exec received different context")
	}
	if tx.execSQL != "SELECT pg_advisory_xact_lock($1)" {
		t.Fatalf("Exec SQL = %q, want advisory lock query", tx.execSQL)
	}
	if len(tx.execArgs) != 1 || tx.execArgs[0] != int64(42) {
		t.Fatalf("Exec args = %#v, want [42]", tx.execArgs)
	}
}

func TestWithAdvisoryLockReturnsLockErrorAndSkipsFn(t *testing.T) {
	lockErr := errors.New("lock failed")
	tx := &advisoryLockTxFake{execErr: lockErr}

	err := WithAdvisoryLock(context.Background(), tx, 42, func(context.Context) error {
		t.Fatal("fn was called after lock error")
		return nil
	})

	if !errors.Is(err, lockErr) {
		t.Fatalf("WithAdvisoryLock error = %v, want %v", err, lockErr)
	}
}

func TestWithAdvisoryLockReturnsFnError(t *testing.T) {
	fnErr := errors.New("fn failed")
	tx := &advisoryLockTxFake{}

	err := WithAdvisoryLock(context.Background(), tx, 42, func(context.Context) error {
		return fnErr
	})

	if !errors.Is(err, fnErr) {
		t.Fatalf("WithAdvisoryLock error = %v, want %v", err, fnErr)
	}
}

type advisoryLockTestContextKey struct{}

type advisoryLockTxFake struct {
	execContext context.Context
	execSQL     string
	execArgs    []any
	execErr     error
}

func (tx *advisoryLockTxFake) Exec(ctx context.Context, sql string, args ...any) (pgconn.CommandTag, error) {
	tx.execContext = ctx
	tx.execSQL = sql
	tx.execArgs = args
	return pgconn.CommandTag{}, tx.execErr
}

func (tx *advisoryLockTxFake) QueryRow(context.Context, string, ...any) pgx.Row {
	return advisoryLockRowFake{}
}

type advisoryLockRowFake struct{}

func (advisoryLockRowFake) Scan(...any) error {
	return nil
}
