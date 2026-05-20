package lazuli

import (
	"context"
	"strings"
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
)

// TestAuditRowWrittenInsideTx asserts that the audit helper writes a
// lazuli_audit row through the supplied pgx tx.
func TestAuditRowWrittenInsideTx(t *testing.T) {
	ctx := &Ctx{
		Context: context.Background(),
		Actor:   ActorUser,
		Tenant:  &Tenant{OrgID: 42},
	}
	tx := &auditRowTxStub{}

	if err := writeAuditRow(ctx, tx, "test.x", "allowed"); err != nil {
		t.Fatalf("writeAuditRow returned error: %v", err)
	}
	if tx.execCalls != 1 {
		t.Fatalf("Exec calls = %d, want 1", tx.execCalls)
	}
	if !strings.Contains(tx.sql, "INSERT INTO lazuli_audit") {
		t.Fatalf("SQL = %q, want lazuli_audit insert", tx.sql)
	}
	if len(tx.args) != 4 {
		t.Fatalf("Exec arg count = %d, want 4", len(tx.args))
	}
	assertAuditRowArg(t, tx.args[0], "test.x", "command")
	assertAuditRowArg(t, tx.args[1], "user", "actor")
	assertAuditRowStringPtrArg(t, tx.args[2], "42", "tenant")
	assertAuditRowArg(t, tx.args[3], "allowed", "decision")
}

type auditRowTxStub struct {
	pgx.Tx
	execCalls int
	sql       string
	args      []any
	execErr   error
}

func (tx *auditRowTxStub) Exec(_ context.Context, sql string, arguments ...any) (pgconn.CommandTag, error) {
	tx.execCalls++
	tx.sql = sql
	tx.args = append([]any(nil), arguments...)
	return pgconn.NewCommandTag("INSERT 0 1"), tx.execErr
}

func assertAuditRowStringPtrArg(t *testing.T, got any, want, name string) {
	t.Helper()
	ptr, ok := got.(*string)
	if !ok {
		t.Fatalf("%s arg type = %T, want *string", name, got)
	}
	if *ptr != want {
		t.Fatalf("%s arg = %q, want %q", name, *ptr, want)
	}
}

func assertAuditRowArg[T comparable](t *testing.T, got any, want T, name string) {
	t.Helper()
	value, ok := got.(T)
	if !ok {
		t.Fatalf("%s arg type = %T, want %T", name, got, want)
	}
	if value != want {
		t.Fatalf("%s arg = %v, want %v", name, value, want)
	}
}
