package lazuli

import (
	"context"
	"errors"
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
)

func TestPartialUpdateBuildsSafeSetClause(t *testing.T) {
	var nickname *string
	tx := &partialUpdateTxStub{tag: pgconn.NewCommandTag("UPDATE 1")}

	err := PartialUpdate(&Ctx{}, tx, "profile", 7, map[string]any{
		"age":      0,
		"name":     "Ada",
		"nickname": nickname,
		"photo":    nil,
	})
	if err != nil {
		t.Fatalf("PartialUpdate() error = %v, want nil", err)
	}
	wantSQL := `UPDATE "profile" SET "age" = $1, "name" = $2 WHERE "id" = $3`
	if tx.sql != wantSQL {
		t.Fatalf("SQL = %q, want %q", tx.sql, wantSQL)
	}
	wantArgs := []any{0, "Ada", ID(7)}
	if !sameArgs(tx.args, wantArgs) {
		t.Fatalf("args = %#v, want %#v", tx.args, wantArgs)
	}
}

func TestPartialUpdateReturnsNotFoundOnZeroRows(t *testing.T) {
	tx := &partialUpdateTxStub{tag: pgconn.NewCommandTag("UPDATE 0")}

	err := PartialUpdate(&Ctx{}, tx, "profile", 7, map[string]any{"name": "Ada"})
	var le *Error
	if !errors.As(err, &le) {
		t.Fatalf("PartialUpdate() error type = %T, want *Error", err)
	}
	if le.Status != 404 || le.Code != CodeNotFound {
		t.Fatalf("PartialUpdate() = status %d code %q, want 404 %q", le.Status, le.Code, CodeNotFound)
	}
}

func TestPartialUpdateNoopsWhenAllFieldsNil(t *testing.T) {
	var nickname *string
	tx := &partialUpdateTxStub{tag: pgconn.NewCommandTag("UPDATE 1")}

	err := PartialUpdate(&Ctx{}, tx, "profile", 7, map[string]any{"nickname": nickname, "photo": nil})
	if err != nil {
		t.Fatalf("PartialUpdate() error = %v, want nil", err)
	}
	if tx.sql != "" {
		t.Fatalf("PartialUpdate() executed SQL %q, want no-op", tx.sql)
	}
}

func TestPartialUpdateRejectsNilTx(t *testing.T) {
	err := PartialUpdate(&Ctx{}, nil, "profile", 7, map[string]any{"name": "Ada"})
	var le *Error
	if !errors.As(err, &le) || le.Code != CodeInternal {
		t.Fatalf("PartialUpdate(nil tx) = %v, want internal *Error", err)
	}
}

type partialUpdateTxStub struct {
	pgx.Tx
	sql  string
	args []any
	tag  pgconn.CommandTag
	err  error
}

func (tx *partialUpdateTxStub) Exec(_ context.Context, sql string, args ...any) (pgconn.CommandTag, error) {
	tx.sql = sql
	tx.args = append([]any(nil), args...)
	if tx.err != nil {
		return pgconn.CommandTag{}, tx.err
	}
	return tx.tag, nil
}
