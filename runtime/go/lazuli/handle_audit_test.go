package lazuli

import (
	"context"
	"strings"
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
)

// TestAuditRowWrittenInsideTx asserts that the audit helper writes a row into
// the canonical `audit_log` table through the supplied pgx tx, with the
// reconciled column set that matches the codegen-emitted DDL
// (org_id, actor_id, actor_kind, command_name, result_status, error_code,
// correlation_id). happened_at is omitted so the DDL DEFAULT NOW() fills it.
func TestAuditRowWrittenInsideTx(t *testing.T) {
	ctx := &Ctx{
		Context:   context.Background(),
		Actor:     ActorUser,
		User:      &User{ID: 7, OrgID: 42},
		Tenant:    &Tenant{OrgID: 42},
		RequestID: "req-abc",
	}
	tx := &auditRowTxStub{}

	if err := writeAuditRow(ctx, tx, "account.signup", "allowed"); err != nil {
		t.Fatalf("writeAuditRow returned error: %v", err)
	}
	if tx.execCalls != 1 {
		t.Fatalf("Exec calls = %d, want 1", tx.execCalls)
	}
	if !strings.Contains(tx.sql, "INSERT INTO audit_log") {
		t.Fatalf("SQL = %q, want audit_log insert", tx.sql)
	}
	for _, col := range []string{
		"org_id", "actor_id", "actor_kind", "command_name",
		"result_status", "error_code", "correlation_id",
	} {
		if !strings.Contains(tx.sql, col) {
			t.Fatalf("SQL = %q, missing column %q", tx.sql, col)
		}
	}
	// The runtime must NOT carry the legacy lazuli_audit shape forward.
	if strings.Contains(tx.sql, "lazuli_audit") {
		t.Fatalf("SQL = %q, must not target legacy lazuli_audit table", tx.sql)
	}
	if len(tx.args) != 7 {
		t.Fatalf("Exec arg count = %d, want 7", len(tx.args))
	}
	assertAuditInt64PtrArg(t, tx.args[0], 42, "org_id")
	assertAuditInt64PtrArg(t, tx.args[1], 7, "actor_id")
	assertAuditRowArg(t, tx.args[2], "user", "actor_kind")
	assertAuditRowArg(t, tx.args[3], "account.signup", "command_name")
	assertAuditRowArg(t, tx.args[4], "ok", "result_status")
	if tx.args[5] != nil {
		// error_code is *string(nil) on an ok row.
		if ptr, ok := tx.args[5].(*string); !ok || ptr != nil {
			t.Fatalf("error_code arg = %#v, want nil *string on ok row", tx.args[5])
		}
	}
	assertAuditStringPtrArg(t, tx.args[6], "req-abc", "correlation_id")
}

// TestAuditDecisionToResultStatus pins the decision→result_status mapping.
func TestAuditDecisionToResultStatus(t *testing.T) {
	cases := map[string]string{
		"allowed":   "ok",
		"allow":     "ok",
		"committed": "ok",
		"ok":        "ok",
		"denied":    "error",
		"deny":      "error",
		"error":     "error",
		"":          "error",
	}
	for in, want := range cases {
		if got := auditDecisionToResultStatus(in); got != want {
			t.Fatalf("auditDecisionToResultStatus(%q) = %q, want %q", in, got, want)
		}
	}
}

// TestAssembleAuditRecordSystemActor proves the system path: actor_kind is set
// (the column is NOT NULL) and actor_id is NULL.
func TestAssembleAuditRecordSystemActor(t *testing.T) {
	ctx := &Ctx{Context: context.Background(), Actor: ActorSystem, Tenant: &Tenant{OrgID: 9}}
	rec := assembleAuditRecord(ctx, "job.tick", "allowed")
	if rec.ActorKind != "system" {
		t.Fatalf("ActorKind = %q, want system", rec.ActorKind)
	}
	if rec.ActorID != nil {
		t.Fatalf("ActorID = %v, want nil for system actor", *rec.ActorID)
	}
	if rec.OrgID == nil || *rec.OrgID != 9 {
		t.Fatalf("OrgID = %v, want 9", rec.OrgID)
	}
	if rec.ResultStatus != "ok" {
		t.Fatalf("ResultStatus = %q, want ok", rec.ResultStatus)
	}
}

// TestAssembleAuditRecordAnonymousNoTenant proves the anonymous path: actor_id
// and org_id NULL, actor_kind 'anonymous', error result carries error_code.
func TestAssembleAuditRecordAnonymousNoTenant(t *testing.T) {
	ctx := &Ctx{Context: context.Background(), Actor: ActorAnonymous}
	rec := assembleAuditRecord(ctx, "account.signup", "denied")
	if rec.ActorKind != "anonymous" {
		t.Fatalf("ActorKind = %q, want anonymous", rec.ActorKind)
	}
	if rec.ActorID != nil {
		t.Fatalf("ActorID = %v, want nil for anonymous actor", *rec.ActorID)
	}
	if rec.OrgID != nil {
		t.Fatalf("OrgID = %v, want nil with no tenant", *rec.OrgID)
	}
	if rec.ResultStatus != "error" {
		t.Fatalf("ResultStatus = %q, want error", rec.ResultStatus)
	}
	if rec.ErrorCode == nil || *rec.ErrorCode != "denied" {
		t.Fatalf("ErrorCode = %v, want denied on error row", rec.ErrorCode)
	}
}

// TestAuditMaterializeRowWritesReconciledColumns asserts the materialize sink
// inserts the same audit shape (plus the append_only ledger's recorded_at)
// into the codegen-named OperationLog table.
func TestAuditMaterializeRowWritesReconciledColumns(t *testing.T) {
	ctx := &Ctx{Context: context.Background(), Actor: ActorUser, User: &User{ID: 3}, Tenant: &Tenant{OrgID: 1}}
	tx := &auditRowTxStub{}

	if err := writeAuditMaterializeRow(ctx, tx, "operation_log", "ops.record", "allowed"); err != nil {
		t.Fatalf("writeAuditMaterializeRow returned error: %v", err)
	}
	if !strings.Contains(tx.sql, `INSERT INTO "operation_log"`) {
		t.Fatalf("SQL = %q, want sanitized operation_log insert", tx.sql)
	}
	for _, col := range []string{
		"org_id", "actor_id", "actor_kind", "command_name",
		"result_status", "error_code", "correlation_id", "recorded_at",
	} {
		if !strings.Contains(tx.sql, col) {
			t.Fatalf("SQL = %q, missing column %q", tx.sql, col)
		}
	}
	if len(tx.args) != 8 {
		t.Fatalf("Exec arg count = %d, want 8 (audit shape + recorded_at)", len(tx.args))
	}
	assertAuditRowArg(t, tx.args[3], "ops.record", "command_name")
	assertAuditRowArg(t, tx.args[4], "ok", "result_status")
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

func assertAuditInt64PtrArg(t *testing.T, got any, want int64, name string) {
	t.Helper()
	ptr, ok := got.(*int64)
	if !ok {
		t.Fatalf("%s arg type = %T, want *int64", name, got)
	}
	if ptr == nil || *ptr != want {
		t.Fatalf("%s arg = %v, want %d", name, ptr, want)
	}
}

func assertAuditStringPtrArg(t *testing.T, got any, want, name string) {
	t.Helper()
	ptr, ok := got.(*string)
	if !ok {
		t.Fatalf("%s arg type = %T, want *string", name, got)
	}
	if ptr == nil || *ptr != want {
		t.Fatalf("%s arg = %v, want %q", name, ptr, want)
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
