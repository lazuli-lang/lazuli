package auth

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"

	"lazuli.dev/runtime/lazuli"
)

type cleanupSessionDB struct {
	rows    map[string]cleanupStoredSession
	execSQL string
	execArg any
}

type cleanupStoredSession struct {
	expiresAt time.Time
}

func withCleanupSessionDB(t *testing.T) *cleanupSessionDB {
	t.Helper()
	db := &cleanupSessionDB{rows: make(map[string]cleanupStoredSession)}
	prev := sessionDBProvider
	sessionDBProvider = func() sessionDB { return db }
	t.Cleanup(func() { sessionDBProvider = prev })
	return db
}

func (db *cleanupSessionDB) Exec(_ context.Context, sql string, args ...any) (pgconn.CommandTag, error) {
	db.execSQL = sql
	if len(args) != 1 {
		return pgconn.CommandTag{}, fmt.Errorf("cleanup arg count = %d", len(args))
	}
	db.execArg = args[0]

	if !strings.HasPrefix(sql, `DELETE FROM "Session" WHERE expires_at <= $1`) {
		return pgconn.CommandTag{}, fmt.Errorf("unexpected cleanup SQL: %s", sql)
	}
	now, ok := args[0].(time.Time)
	if !ok {
		return pgconn.CommandTag{}, fmt.Errorf("cleanup now type = %T", args[0])
	}

	deleted := int64(0)
	for tokenHash, session := range db.rows {
		if !session.expiresAt.After(now) {
			delete(db.rows, tokenHash)
			deleted++
		}
	}
	return pgconn.NewCommandTag(fmt.Sprintf("DELETE %d", deleted)), nil
}

func (db *cleanupSessionDB) QueryRow(context.Context, string, ...any) pgx.Row {
	return cleanupUnexpectedRow{}
}

type cleanupUnexpectedRow struct{}

func (cleanupUnexpectedRow) Scan(...any) error {
	return errors.New("unexpected cleanup QueryRow")
}

func TestCleanupExpiredSessionsDeletesRowsAtOrBeforeNow(t *testing.T) {
	db := withCleanupSessionDB(t)
	now := time.Date(2026, 5, 12, 15, 0, 0, 0, time.UTC)
	db.rows["expired"] = cleanupStoredSession{expiresAt: now.Add(-time.Second)}
	db.rows["boundary"] = cleanupStoredSession{expiresAt: now}
	db.rows["active"] = cleanupStoredSession{expiresAt: now.Add(time.Second)}

	deleted, err := CleanupExpiredSessions(
		&lazuli.Ctx{Context: context.Background(), Now: now},
		SessionsContract{Resource: "Session"},
	)
	if err != nil {
		t.Fatalf("CleanupExpiredSessions returned error: %v", err)
	}
	if deleted != 2 {
		t.Fatalf("CleanupExpiredSessions deleted = %d, want 2", deleted)
	}
	if _, ok := db.rows["expired"]; ok {
		t.Fatal("expired session was not deleted")
	}
	if _, ok := db.rows["boundary"]; ok {
		t.Fatal("boundary-expired session was not deleted")
	}
	if _, ok := db.rows["active"]; !ok {
		t.Fatal("active session was deleted")
	}
	if db.execSQL != `DELETE FROM "Session" WHERE expires_at <= $1` {
		t.Fatalf("cleanup SQL = %q, want session expiry delete", db.execSQL)
	}
	if got, ok := db.execArg.(time.Time); !ok || !got.Equal(now) {
		t.Fatalf("cleanup now arg = %#v, want %v", db.execArg, now)
	}
}

func TestCleanupExpiredSessionsWithUsesCleanerContract(t *testing.T) {
	now := time.Date(2026, 5, 12, 16, 0, 0, 0, time.UTC)
	cleaner := &cleanupCleaner{deleted: 3}

	deleted, err := CleanupExpiredSessionsWith(context.Background(), cleaner, now)
	if err != nil {
		t.Fatalf("CleanupExpiredSessionsWith returned error: %v", err)
	}
	if deleted != 3 {
		t.Fatalf("CleanupExpiredSessionsWith deleted = %d, want 3", deleted)
	}
	if !cleaner.now.Equal(now) {
		t.Fatalf("cleaner now = %v, want %v", cleaner.now, now)
	}
}

func TestCleanupExpiredSessionsWithReturnsContextErrors(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	cleaner := &cleanupCleaner{}
	if _, err := CleanupExpiredSessionsWith(ctx, cleaner, time.Now()); !errors.Is(err, context.Canceled) {
		t.Fatalf("CleanupExpiredSessionsWith canceled error = %v, want context.Canceled", err)
	}
	if cleaner.calls != 0 {
		t.Fatalf("cleaner calls = %d, want 0 after context cancellation", cleaner.calls)
	}
}

func TestBuildSessionLifecycleAuditEntry(t *testing.T) {
	expiresAt := time.Date(2026, 5, 13, 15, 0, 0, 0, time.UTC)
	ctx := &lazuli.Ctx{
		Context:   context.Background(),
		Actor:     lazuli.ActorUser,
		User:      &lazuli.User{ID: 42, OrgID: 7},
		Tenant:    &lazuli.Tenant{OrgID: 9},
		RequestID: "req-session",
	}

	entry, err := BuildSessionLifecycleAuditEntry(ctx, SessionLifecycleAuditEvent{
		Kind:      SessionAuditIssued,
		Resource:  "CustomerSession",
		UserID:    lazuli.ID(42),
		ExpiresAt: expiresAt,
		Details: map[string]any{
			"provider": "password",
		},
	})
	if err != nil {
		t.Fatalf("BuildSessionLifecycleAuditEntry returned error: %v", err)
	}

	if entry.CommandName != string(SessionAuditIssued) {
		t.Fatalf("CommandName = %q, want %q", entry.CommandName, SessionAuditIssued)
	}
	if entry.TargetResource != "CustomerSession" {
		t.Fatalf("TargetResource = %q, want CustomerSession", entry.TargetResource)
	}
	if entry.ResultStatus != AuditResultOK {
		t.Fatalf("ResultStatus = %q, want ok", entry.ResultStatus)
	}
	if entry.CorrelationID != "req-session" {
		t.Fatalf("CorrelationID = %q, want req-session", entry.CorrelationID)
	}
	if got := auditPtrValue(t, entry.OrgID, "OrgID"); got != 9 {
		t.Fatalf("OrgID = %d, want 9", got)
	}
	if got := auditPtrValue(t, entry.ActorID, "ActorID"); got != 42 {
		t.Fatalf("ActorID = %d, want 42", got)
	}

	var payload map[string]any
	if err := json.Unmarshal(entry.Payload, &payload); err != nil {
		t.Fatalf("payload JSON decode error: %v", err)
	}
	if payload["event"] != string(SessionAuditIssued) {
		t.Fatalf("payload event = %v, want %s", payload["event"], SessionAuditIssued)
	}
	if payload["resource"] != "CustomerSession" {
		t.Fatalf("payload resource = %v, want CustomerSession", payload["resource"])
	}
	if payload["user_id"] != float64(42) {
		t.Fatalf("payload user_id = %v, want 42", payload["user_id"])
	}
	if payload["expires_at"] != expiresAt.Format(time.RFC3339Nano) {
		t.Fatalf("payload expires_at = %v, want %s", payload["expires_at"], expiresAt.Format(time.RFC3339Nano))
	}
	details, ok := payload["details"].(map[string]any)
	if !ok {
		t.Fatalf("payload details = %#v, want object", payload["details"])
	}
	if details["provider"] != "password" {
		t.Fatalf("details provider = %v, want password", details["provider"])
	}
}

func TestBuildSessionLifecycleAuditEntryPrunedDefaultsToSystemActor(t *testing.T) {
	entry, err := BuildSessionLifecycleAuditEntry(nil, SessionLifecycleAuditEvent{
		Kind:        SessionAuditPruned,
		Resource:    "CustomerSession",
		PrunedCount: 4,
	})
	if err != nil {
		t.Fatalf("BuildSessionLifecycleAuditEntry returned error: %v", err)
	}
	if entry.ActorKind != AuditActorSystem {
		t.Fatalf("ActorKind = %q, want system", entry.ActorKind)
	}
	if entry.ActorID != nil {
		t.Fatalf("ActorID = %v, want nil for system cleanup", *entry.ActorID)
	}

	var payload map[string]any
	if err := json.Unmarshal(entry.Payload, &payload); err != nil {
		t.Fatalf("payload JSON decode error: %v", err)
	}
	if payload["pruned_count"] != float64(4) {
		t.Fatalf("payload pruned_count = %v, want 4", payload["pruned_count"])
	}
}

func TestBuildSessionLifecycleAuditEntryFailureStatus(t *testing.T) {
	entry, err := BuildSessionLifecycleAuditEntry(nil, SessionLifecycleAuditEvent{
		Kind:      SessionAuditExpired,
		UserID:    lazuli.ID(42),
		ErrorCode: "auth.session_expired",
	})
	if err != nil {
		t.Fatalf("BuildSessionLifecycleAuditEntry returned error: %v", err)
	}
	if entry.ResultStatus != AuditResultError {
		t.Fatalf("ResultStatus = %q, want error", entry.ResultStatus)
	}
	if entry.ErrorCode != "auth.session_expired" {
		t.Fatalf("ErrorCode = %q, want auth.session_expired", entry.ErrorCode)
	}
	if entry.ActorKind != AuditActorUser {
		t.Fatalf("ActorKind = %q, want user", entry.ActorKind)
	}
	if got := auditPtrValue(t, entry.ActorID, "ActorID"); got != 42 {
		t.Fatalf("ActorID = %d, want 42", got)
	}
}

func TestRecordSessionLifecycleAuditUsesRecorder(t *testing.T) {
	recorder := &sessionLifecycleRecorder{}
	ctx := &lazuli.Ctx{Context: context.Background(), RequestID: "req-recorder"}

	err := RecordSessionLifecycleAudit(ctx, recorder, SessionLifecycleAuditEvent{
		Kind:     SessionAuditInvalidated,
		Resource: "CustomerSession",
		UserID:   lazuli.ID(7),
	})
	if err != nil {
		t.Fatalf("RecordSessionLifecycleAudit returned error: %v", err)
	}
	if recorder.calls != 1 {
		t.Fatalf("recorder calls = %d, want 1", recorder.calls)
	}
	if recorder.entry.CommandName != string(SessionAuditInvalidated) {
		t.Fatalf("recorded CommandName = %q, want %q", recorder.entry.CommandName, SessionAuditInvalidated)
	}
	if recorder.entry.CorrelationID != "req-recorder" {
		t.Fatalf("recorded CorrelationID = %q, want req-recorder", recorder.entry.CorrelationID)
	}
}

func TestRecordSessionLifecycleAuditPropagatesRecorderError(t *testing.T) {
	recorderErr := errors.New("audit unavailable")
	recorder := &sessionLifecycleRecorder{err: recorderErr}

	err := RecordSessionLifecycleAudit(nil, recorder, SessionLifecycleAuditEvent{Kind: SessionAuditResolved})
	if !errors.Is(err, recorderErr) {
		t.Fatalf("RecordSessionLifecycleAudit error = %v, want recorder error", err)
	}
}

type cleanupCleaner struct {
	calls   int
	now     time.Time
	deleted int64
	err     error
}

func (c *cleanupCleaner) CleanupExpiredSessions(_ context.Context, now time.Time) (int64, error) {
	c.calls++
	c.now = now
	return c.deleted, c.err
}

type sessionLifecycleRecorder struct {
	calls int
	entry AuditEntry
	err   error
}

func (r *sessionLifecycleRecorder) RecordSessionLifecycleAudit(_ context.Context, entry AuditEntry) error {
	r.calls++
	r.entry = entry
	return r.err
}
