package lazuli

import (
	"context"
	"errors"
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
)

func TestTransitionAdvancesState(t *testing.T) {
	tx := &transitionTxStub{tag: pgconn.NewCommandTag("UPDATE 1")}

	err := Transition(&Ctx{}, tx, "booking", "status", "pending", "confirmed", "id", ID(7))
	if err != nil {
		t.Fatalf("Transition() error = %v, want nil", err)
	}
	if tx.sql != `UPDATE "booking" SET "status" = $3 WHERE "status" = $1 AND "id" = $2` {
		t.Fatalf("SQL = %q", tx.sql)
	}
	wantArgs := []any{"pending", ID(7), "confirmed"}
	if !sameArgs(tx.args, wantArgs) {
		t.Fatalf("args = %#v, want %#v", tx.args, wantArgs)
	}
}

func TestTransitionReturnsMismatchOnZeroRows(t *testing.T) {
	tx := &transitionTxStub{tag: pgconn.NewCommandTag("UPDATE 0")}

	err := Transition(&Ctx{}, tx, "booking", "status", "pending", "confirmed", "id", ID(7))
	var le *Error
	if !errors.As(err, &le) {
		t.Fatalf("Transition() error type = %T, want *Error", err)
	}
	if le.Status != 409 || le.Code != CodeLifecycleStateMismatch {
		t.Fatalf("Transition() = status %d code %q, want 409 %q", le.Status, le.Code, CodeLifecycleStateMismatch)
	}
}

func TestTransitionClassifiesExecError(t *testing.T) {
	tx := &transitionTxStub{err: errors.New("network down")}

	err := Transition(&Ctx{}, tx, "booking", "status", "pending", "confirmed", "id", ID(7))
	var le *Error
	if !errors.As(err, &le) || le.Code != CodeInternal {
		t.Fatalf("Transition() error = %v, want internal *Error", err)
	}
}

func TestTransitionRejectsNilTx(t *testing.T) {
	err := Transition(&Ctx{}, nil, "booking", "status", "pending", "confirmed", "id", ID(7))
	var le *Error
	if !errors.As(err, &le) || le.Code != CodeInternal {
		t.Fatalf("Transition(nil tx) = %v, want internal *Error", err)
	}
}

type transitionTxStub struct {
	pgx.Tx
	sql  string
	args []any
	tag  pgconn.CommandTag
	err  error
}

func (tx *transitionTxStub) Exec(_ context.Context, sql string, args ...any) (pgconn.CommandTag, error) {
	tx.sql = sql
	tx.args = append([]any(nil), args...)
	if tx.err != nil {
		return pgconn.CommandTag{}, tx.err
	}
	return tx.tag, nil
}

func sameArgs(got, want []any) bool {
	if len(got) != len(want) {
		return false
	}
	for i := range got {
		if got[i] != want[i] {
			return false
		}
	}
	return true
}
