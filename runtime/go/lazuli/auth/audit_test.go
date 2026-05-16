package auth

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"log/slog"
	"strings"
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"

	"lazuli.dev/runtime/lazuli"
)

func TestAuditFromCtxPrefillsUserTenantAndRequest(t *testing.T) {
	ctx := &lazuli.Ctx{
		Context:   context.Background(),
		Actor:     lazuli.ActorUser,
		User:      &lazuli.User{ID: 42, OrgID: 7},
		Tenant:    &lazuli.Tenant{OrgID: 99},
		RequestID: "req-123",
	}

	entry := AuditFromCtx(ctx)

	if got := auditPtrValue(t, entry.OrgID, "OrgID"); got != 99 {
		t.Fatalf("OrgID = %d, want 99", got)
	}
	if got := auditPtrValue(t, entry.ActorID, "ActorID"); got != 42 {
		t.Fatalf("ActorID = %d, want 42", got)
	}
	if entry.ActorKind != AuditActorUser {
		t.Fatalf("ActorKind = %q, want %q", entry.ActorKind, AuditActorUser)
	}
	if entry.CorrelationID != "req-123" {
		t.Fatalf("CorrelationID = %q, want req-123", entry.CorrelationID)
	}
}

func TestAuditFromCtxPrefillsSystemActor(t *testing.T) {
	ctx := &lazuli.Ctx{
		Context: context.Background(),
		Actor:   lazuli.ActorSystem,
		Tenant:  &lazuli.Tenant{OrgID: 12},
	}

	entry := AuditFromCtx(ctx)

	if got := auditPtrValue(t, entry.OrgID, "OrgID"); got != 12 {
		t.Fatalf("OrgID = %d, want 12", got)
	}
	if entry.ActorID != nil {
		t.Fatalf("ActorID = %v, want nil for system actor", *entry.ActorID)
	}
	if entry.ActorKind != AuditActorSystem {
		t.Fatalf("ActorKind = %q, want %q", entry.ActorKind, AuditActorSystem)
	}
}

func TestAuditEntryBuilders(t *testing.T) {
	payload := []byte(`{"name":"Ada"}`)

	entry := (AuditEntry{}).
		WithCommand("customer.create").
		WithTarget("customer", lazuli.ID(55)).
		WithPayload(payload).
		Succeeded()

	if entry.CommandName != "customer.create" {
		t.Fatalf("CommandName = %q, want customer.create", entry.CommandName)
	}
	if entry.TargetResource != "customer" {
		t.Fatalf("TargetResource = %q, want Customer", entry.TargetResource)
	}
	if got := auditPtrValue(t, entry.TargetID, "TargetID"); got != 55 {
		t.Fatalf("TargetID = %d, want 55", got)
	}
	if string(entry.Payload) != string(payload) {
		t.Fatalf("Payload = %q, want %q", entry.Payload, payload)
	}
	if entry.ResultStatus != AuditResultOK {
		t.Fatalf("ResultStatus = %q, want %q", entry.ResultStatus, AuditResultOK)
	}

	entry = entry.Failed("validation_failed")
	if entry.ResultStatus != AuditResultError {
		t.Fatalf("ResultStatus after Failed = %q, want %q", entry.ResultStatus, AuditResultError)
	}
	if entry.ErrorCode != "validation_failed" {
		t.Fatalf("ErrorCode = %q, want validation_failed", entry.ErrorCode)
	}
}

func TestEmitAuditInsertsAuditLogRow(t *testing.T) {
	entry := AuditEntry{
		OrgID:          auditIDPtr(10),
		ActorID:        auditIDPtr(20),
		ActorKind:      AuditActorUser,
		CommandName:    "customer.create",
		TargetResource: "customer",
		TargetID:       auditIDPtr(30),
		Payload:        []byte(`{"name":"Ada"}`),
		ResultStatus:   AuditResultOK,
		CorrelationID:  "req-456",
	}
	tx := &auditTxStub{}

	EmitAudit(context.Background(), tx, entry)

	if tx.execCalls != 1 {
		t.Fatalf("Exec calls = %d, want 1", tx.execCalls)
	}
	if !strings.Contains(tx.sql, "INSERT INTO audit_log") {
		t.Fatalf("SQL = %q, want audit_log insert", tx.sql)
	}
	if !strings.Contains(tx.sql, "correlation_id") {
		t.Fatalf("SQL = %q, want correlation_id column", tx.sql)
	}
	if len(tx.args) != 10 {
		t.Fatalf("Exec arg count = %d, want 10", len(tx.args))
	}
	assertAuditPtrArg(t, tx.args[0], 10, "org_id")
	assertAuditPtrArg(t, tx.args[1], 20, "actor_id")
	assertAuditArg(t, tx.args[2], AuditActorUser, "actor_kind")
	assertAuditArg(t, tx.args[3], "customer.create", "command_name")
	assertAuditArg(t, tx.args[4], "customer", "target_resource")
	assertAuditPtrArg(t, tx.args[5], 30, "target_id")
	if got, ok := tx.args[6].([]byte); !ok || string(got) != `{"name":"Ada"}` {
		t.Fatalf("payload arg = %#v, want JSON bytes", tx.args[6])
	}
	assertAuditArg(t, tx.args[7], AuditResultOK, "result_status")
	assertAuditArg(t, tx.args[8], "", "error_code")
	assertAuditArg(t, tx.args[9], "req-456", "correlation_id")
}

func TestEmitAuditLogsFailureWithoutPanic(t *testing.T) {
	var logBuf bytes.Buffer
	oldLogger := slog.Default()
	slog.SetDefault(slog.New(slog.NewJSONHandler(&logBuf, nil)))
	t.Cleanup(func() {
		slog.SetDefault(oldLogger)
	})
	tx := &auditTxStub{execErr: errors.New("insert failed")}

	EmitAudit(context.Background(), tx, AuditEntry{CommandName: "customer.create"})

	var logEntry map[string]any
	if err := json.Unmarshal(bytes.TrimSpace(logBuf.Bytes()), &logEntry); err != nil {
		t.Fatalf("log JSON decode error: %v; log = %q", err, logBuf.String())
	}
	if logEntry["msg"] != "lazuli: audit emit failed" {
		t.Fatalf("log msg = %v, want lazuli: audit emit failed", logEntry["msg"])
	}
	if logEntry["err"] != "insert failed" {
		t.Fatalf("log err = %v, want insert failed", logEntry["err"])
	}
	if logEntry["command"] != "customer.create" {
		t.Fatalf("log command = %v, want customer.create", logEntry["command"])
	}
}

type auditTxStub struct {
	execCalls int
	sql       string
	args      []any
	execErr   error
}

func (tx *auditTxStub) Begin(context.Context) (pgx.Tx, error) {
	return tx, nil
}

func (tx *auditTxStub) Commit(context.Context) error {
	return nil
}

func (tx *auditTxStub) Rollback(context.Context) error {
	return nil
}

func (tx *auditTxStub) CopyFrom(context.Context, pgx.Identifier, []string, pgx.CopyFromSource) (int64, error) {
	return 0, nil
}

func (tx *auditTxStub) SendBatch(context.Context, *pgx.Batch) pgx.BatchResults {
	return nil
}

func (tx *auditTxStub) LargeObjects() pgx.LargeObjects {
	return pgx.LargeObjects{}
}

func (tx *auditTxStub) Prepare(context.Context, string, string) (*pgconn.StatementDescription, error) {
	return nil, nil
}

func (tx *auditTxStub) Exec(_ context.Context, sql string, arguments ...any) (pgconn.CommandTag, error) {
	tx.execCalls++
	tx.sql = sql
	tx.args = append([]any(nil), arguments...)
	return pgconn.NewCommandTag("INSERT 0 1"), tx.execErr
}

func (tx *auditTxStub) Query(context.Context, string, ...any) (pgx.Rows, error) {
	return nil, nil
}

func (tx *auditTxStub) QueryRow(context.Context, string, ...any) pgx.Row {
	return nil
}

func (tx *auditTxStub) Conn() *pgx.Conn {
	return nil
}

func auditPtrValue(t *testing.T, got *int64, name string) int64 {
	t.Helper()
	if got == nil {
		t.Fatalf("%s = nil, want pointer", name)
	}
	return *got
}

func assertAuditPtrArg(t *testing.T, got any, want int64, name string) {
	t.Helper()
	ptr, ok := got.(*int64)
	if !ok {
		t.Fatalf("%s arg type = %T, want *int64", name, got)
	}
	if *ptr != want {
		t.Fatalf("%s arg = %d, want %d", name, *ptr, want)
	}
}

func assertAuditArg[T comparable](t *testing.T, got any, want T, name string) {
	t.Helper()
	value, ok := got.(T)
	if !ok {
		t.Fatalf("%s arg type = %T, want %T", name, got, want)
	}
	if value != want {
		t.Fatalf("%s arg = %v, want %v", name, value, want)
	}
}
