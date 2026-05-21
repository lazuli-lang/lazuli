// Sibling test for cell RU1 (`runtime/go/lazuli/handle.go:applyUpdates`).
//
// RU1 gates the unconditional `"updated_at" = now()` SET-clause append on
// the resource's lifecycle-column declaration (the `Timestamps` flag, surfaced
// to the runtime as `resourceErased.HasColumn("updated_at")`). Resources
// without timestamps previously crashed with PG 42703 (undefined_column).
//
// This file locks the gate behaviour at the integration level: it drives
// the full `Command.Handle()` pipeline against an in-memory tx stub that
// records the SQL string the runtime hands to pgx. The assertion target
// is purely the SET clause shape — no live Postgres required.
package lazuli

import (
	"context"
	"errors"
	"strings"
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
)

// TestApplyUpdatesAppendsUpdatedAtWhenResourceHasTimestamps is the
// positive case for RU1: a resource that declares `Timestamps: true`
// (DSL `defaults.timestamps = true` or per-resource `timestamps on`)
// MUST receive the `"updated_at" = now()` SET clause. Removing the
// append unconditionally would regress every standard CRUD resource.
func TestApplyUpdatesAppendsUpdatedAtWhenResourceHasTimestamps(t *testing.T) {
	sql, _ := runApplyUpdatesCapture(t, true)

	if !strings.Contains(sql, `"updated_at" = now()`) {
		t.Fatalf(
			"expected SET clause to bump updated_at on a Timestamps=true resource;\nSQL = %s",
			sql,
		)
	}
}

// TestApplyUpdatesOmitsUpdatedAtWhenResourceLacksTimestamps is the
// regression guard for RU1's bug fix: a resource without `Timestamps`
// (DSL `defaults.timestamps = false` or per-resource `timestamps off`)
// MUST NOT receive the `updated_at` SET clause. Before RU1 the runtime
// appended it unconditionally; the generated UPDATE then referenced an
// undefined column and Postgres returned 42703.
//
// Asserting absence of the literal substring `updated_at` (rather than
// the quoted SET fragment) catches any future variant that smuggles
// the column back in via `"updated_at"`, ` updated_at `, etc.
func TestApplyUpdatesOmitsUpdatedAtWhenResourceLacksTimestamps(t *testing.T) {
	sql, _ := runApplyUpdatesCapture(t, false)

	if strings.Contains(sql, "updated_at") {
		t.Fatalf(
			"expected SET clause to omit updated_at on a Timestamps=false resource;\nSQL = %s",
			sql,
		)
	}
	// And the bind-side column still has to be there — the test would
	// be hollow if the whole SET clause disappeared.
	if !strings.Contains(sql, `"email"`) {
		t.Fatalf(
			"expected SET clause to still include the bound `email` column;\nSQL = %s",
			sql,
		)
	}
}

// runApplyUpdatesCapture drives a minimal `Command.Handle()` request
// against an SQL-capturing tx stub and returns the captured UPDATE
// statement plus its bound args. The shared helper keeps the two test
// cases above a one-line shape diff (Timestamps true vs false) so a
// future regression in either direction surfaces by name.
func runApplyUpdatesCapture(t *testing.T, timestamps bool) (string, []any) {
	t.Helper()

	type input struct {
		ID    int64
		Email string
	}
	type output struct {
		ID    int64
		Email string
	}

	resource := &Resource[output]{
		Name:       "SubjectUnderTest",
		Tenancy:    TenancyNone,
		Timestamps: timestamps,
	}
	cmd := &Command[input, output]{
		Name: "subject.save",
		Policy: Policy{Atoms: []PolicyAtom{
			{Namespace: "scope", Name: "public"},
		}},
		Effect: Updates(resource,
			Bindings{"id": FromInput("ID")},
			Bindings{"email": FromInput("Email")},
		),
	}

	tx := &updatedAtCaptureTxStub{}
	previous := runCommandTx
	runCommandTx = func(_ context.Context, fn func(pgx.Tx) error) error {
		return fn(tx)
	}
	t.Cleanup(func() { runCommandTx = previous })

	// The captured SQL is the load-bearing return. The downstream scan
	// fails (zero rows from the stub) and `applyUpdates` surfaces a 404
	// — that's fine, the test asserts on the SET clause we already
	// recorded, not on the scanned row.
	_, err := cmd.Handle(
		&Ctx{Context: context.Background(), Actor: ActorAnonymous},
		input{ID: 7, Email: "user@example.org"},
	)
	if err == nil {
		t.Fatalf("expected stub-backed UPDATE to surface no-row 404, got nil")
	}
	if tx.querySQL == "" {
		t.Fatalf("tx stub never received Query — applyUpdates pipeline aborted before SQL emit (err=%v)", err)
	}
	return tx.querySQL, tx.queryArgs
}

// updatedAtCaptureTxStub records the first `Query` call's SQL + args
// and returns a zero-row Rows so `pgx.CollectOneRow` reports
// `pgx.ErrNoRows`. That short-circuits the scan without needing a real
// row shape — the test only cares about the SQL the runtime emitted.
type updatedAtCaptureTxStub struct {
	querySQL  string
	queryArgs []any
}

func (tx *updatedAtCaptureTxStub) Begin(context.Context) (pgx.Tx, error) { return tx, nil }
func (tx *updatedAtCaptureTxStub) Commit(context.Context) error          { return nil }
func (tx *updatedAtCaptureTxStub) Rollback(context.Context) error        { return nil }
func (tx *updatedAtCaptureTxStub) CopyFrom(context.Context, pgx.Identifier, []string, pgx.CopyFromSource) (int64, error) {
	return 0, errors.New("unexpected CopyFrom")
}
func (tx *updatedAtCaptureTxStub) SendBatch(context.Context, *pgx.Batch) pgx.BatchResults {
	panic("unexpected SendBatch")
}
func (tx *updatedAtCaptureTxStub) LargeObjects() pgx.LargeObjects { panic("unexpected LargeObjects") }
func (tx *updatedAtCaptureTxStub) Prepare(context.Context, string, string) (*pgconn.StatementDescription, error) {
	return nil, errors.New("unexpected Prepare")
}
func (tx *updatedAtCaptureTxStub) Exec(context.Context, string, ...any) (pgconn.CommandTag, error) {
	return pgconn.CommandTag{}, errors.New("unexpected Exec")
}
func (tx *updatedAtCaptureTxStub) Query(_ context.Context, sql string, args ...any) (pgx.Rows, error) {
	tx.querySQL = sql
	tx.queryArgs = append([]any(nil), args...)
	return &emptyRowsStub{}, nil
}
func (tx *updatedAtCaptureTxStub) QueryRow(context.Context, string, ...any) pgx.Row {
	panic("unexpected QueryRow")
}
func (tx *updatedAtCaptureTxStub) Conn() *pgx.Conn { return nil }

// emptyRowsStub is the minimum `pgx.Rows` impl that satisfies
// `pgx.CollectOneRow`: `Next` returns false on the first call, `Err`
// returns nil, and the helper surfaces `pgx.ErrNoRows` — exactly what
// `applyUpdates` then maps to its 404 envelope.
type emptyRowsStub struct{}

func (*emptyRowsStub) Close()                                       {}
func (*emptyRowsStub) Err() error                                   { return nil }
func (*emptyRowsStub) CommandTag() pgconn.CommandTag                { return pgconn.CommandTag{} }
func (*emptyRowsStub) FieldDescriptions() []pgconn.FieldDescription { return nil }
func (*emptyRowsStub) Next() bool                                   { return false }
func (*emptyRowsStub) Scan(...any) error                            { return pgx.ErrNoRows }
func (*emptyRowsStub) Values() ([]any, error)                       { return nil, nil }
func (*emptyRowsStub) RawValues() [][]byte                          { return nil }
func (*emptyRowsStub) Conn() *pgx.Conn                              { return nil }
