package lazuli

import (
	"context"
	"encoding/json"
	"errors"
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
)

func TestCommandTransitionMismatchReturnsTypedError(t *testing.T) {
	type input struct {
		ID int64
	}
	type output struct {
		ID int64
	}

	resource := &Resource[output]{Name: "LifecycleThing"}
	cmd := &Command[input, output]{
		Name: "thing.advance",
		Policy: Policy{Atoms: []PolicyAtom{
			{Namespace: "scope", Name: "public"},
		}},
		Effect: Updates(resource,
			Bindings{"id": FromInput("ID")},
			Bindings{"name": FromConst("changed")},
		),
		Transitions: []TransitionAdvance{{From: "A", To: "B"}},
	}

	tx := &commandTransitionTxStub{lifecycleState: "X"}
	previous := runCommandTx
	runCommandTx = func(ctx context.Context, fn func(pgx.Tx) error) error {
		return fn(tx)
	}
	t.Cleanup(func() { runCommandTx = previous })

	_, err := cmd.dispatch(&Ctx{Context: context.Background(), Actor: ActorAnonymous}, json.RawMessage(`{"ID":123}`))
	if err == nil {
		t.Fatal("dispatch error = nil, want lifecycle mismatch")
	}
	if !errors.Is(err, ErrLifecycleStateMismatch) {
		t.Fatalf("errors.Is(err, ErrLifecycleStateMismatch) = false; err = %v", err)
	}
	var mismatch *LifecycleStateMismatchError
	if !errors.As(err, &mismatch) {
		t.Fatalf("error type = %T, want *LifecycleStateMismatchError", err)
	}
	if mismatch.Expected != "A" || mismatch.Actual != "X" {
		t.Fatalf("mismatch = {Expected:%q Actual:%q}, want {A X}", mismatch.Expected, mismatch.Actual)
	}
	if tx.queryRowSQL != `SELECT lifecycle_state FROM "lifecycle_thing" WHERE "id" = $1 FOR UPDATE` {
		t.Fatalf("guard SQL = %q", tx.queryRowSQL)
	}
	if len(tx.queryRowArgs) != 1 || tx.queryRowArgs[0] != int64(123) {
		t.Fatalf("guard args = %#v, want [123]", tx.queryRowArgs)
	}
	if tx.queryCalls != 0 || tx.execCalls != 0 {
		t.Fatalf("effect ran after mismatch: Query calls=%d Exec calls=%d", tx.queryCalls, tx.execCalls)
	}
}

type commandTransitionTxStub struct {
	lifecycleState string
	queryRowSQL    string
	queryRowArgs   []any
	queryCalls     int
	execCalls      int
}

func (tx *commandTransitionTxStub) Begin(context.Context) (pgx.Tx, error) { return tx, nil }
func (tx *commandTransitionTxStub) Commit(context.Context) error          { return nil }
func (tx *commandTransitionTxStub) Rollback(context.Context) error        { return nil }
func (tx *commandTransitionTxStub) CopyFrom(context.Context, pgx.Identifier, []string, pgx.CopyFromSource) (int64, error) {
	return 0, errors.New("unexpected CopyFrom")
}
func (tx *commandTransitionTxStub) SendBatch(context.Context, *pgx.Batch) pgx.BatchResults {
	panic("unexpected SendBatch")
}
func (tx *commandTransitionTxStub) LargeObjects() pgx.LargeObjects { panic("unexpected LargeObjects") }
func (tx *commandTransitionTxStub) Prepare(context.Context, string, string) (*pgconn.StatementDescription, error) {
	return nil, errors.New("unexpected Prepare")
}
func (tx *commandTransitionTxStub) Exec(context.Context, string, ...any) (pgconn.CommandTag, error) {
	tx.execCalls++
	return pgconn.CommandTag{}, errors.New("unexpected Exec")
}
func (tx *commandTransitionTxStub) Query(context.Context, string, ...any) (pgx.Rows, error) {
	tx.queryCalls++
	return nil, errors.New("unexpected Query")
}
func (tx *commandTransitionTxStub) QueryRow(_ context.Context, sql string, args ...any) pgx.Row {
	tx.queryRowSQL = sql
	tx.queryRowArgs = append([]any(nil), args...)
	return commandTransitionRowStub{state: tx.lifecycleState}
}
func (tx *commandTransitionTxStub) Conn() *pgx.Conn { return nil }

type commandTransitionRowStub struct {
	state string
}

func (row commandTransitionRowStub) Scan(dest ...any) error {
	*dest[0].(*string) = row.state
	return nil
}
