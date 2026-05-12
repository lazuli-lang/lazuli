package auth

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5/pgconn"

	"lazuli.dev/runtime/lazuli"
)

// ExpiredSessionCleaner is the provider-neutral contract for deleting expired
// persisted sessions. Adapters can back this with Postgres, Redis, or another
// session store while tests use small fakes.
type ExpiredSessionCleaner interface {
	CleanupExpiredSessions(ctx context.Context, now time.Time) (int64, error)
}

// SessionCleanupExecutor is the minimal SQL executor needed by
// SessionTableCleanup.
type SessionCleanupExecutor interface {
	Exec(context.Context, string, ...any) (pgconn.CommandTag, error)
}

// SessionTableCleanup deletes expired rows from the generated session table.
type SessionTableCleanup struct {
	Resource string
	DB       SessionCleanupExecutor
}

var errSessionCleanupMissing = errors.New("auth: session cleanup missing")

// CleanupExpiredSessions deletes sessions whose expires_at is at or before now.
func (c SessionTableCleanup) CleanupExpiredSessions(ctx context.Context, now time.Time) (int64, error) {
	if err := sessionStoreContextErr(ctx); err != nil {
		return 0, err
	}
	if c.DB == nil {
		return 0, errSessionCleanupMissing
	}
	if now.IsZero() {
		now = time.Now()
	}

	sql := fmt.Sprintf(
		"DELETE FROM %s WHERE expires_at <= $1",
		quoteSessionIdent(c.Resource),
	)
	tag, err := c.DB.Exec(ctx, sql, now)
	if err != nil {
		return 0, err
	}
	return tag.RowsAffected(), nil
}

// CleanupExpiredSessions removes expired sessions for the generated
// SessionsContract using the default runtime DB binding.
func CleanupExpiredSessions(ctx *lazuli.Ctx, contract SessionsContract) (int64, error) {
	cleaner := SessionTableCleanup{
		Resource: contract.Resource,
		DB:       sessionDBProvider(),
	}
	return cleaner.CleanupExpiredSessions(ctxOrBackground(ctx), sessionNow(ctx))
}

// CleanupExpiredSessionsWith runs cleanup through an adapter-supplied cleaner.
func CleanupExpiredSessionsWith(ctx context.Context, cleaner ExpiredSessionCleaner, now time.Time) (int64, error) {
	if err := sessionStoreContextErr(ctx); err != nil {
		return 0, err
	}
	if cleaner == nil {
		return 0, errSessionCleanupMissing
	}
	return cleaner.CleanupExpiredSessions(ctx, now)
}

// SessionLifecycleAuditKind names auth/session lifecycle events recorded in
// audit_log.
type SessionLifecycleAuditKind string

const (
	SessionAuditIssued      SessionLifecycleAuditKind = "auth.session.issued"
	SessionAuditResolved    SessionLifecycleAuditKind = "auth.session.resolved"
	SessionAuditInvalidated SessionLifecycleAuditKind = "auth.session.invalidated"
	SessionAuditExpired     SessionLifecycleAuditKind = "auth.session.expired"
	SessionAuditPruned      SessionLifecycleAuditKind = "auth.session.pruned"
)

// SessionLifecycleAuditEvent carries the non-secret facts recorded for a
// session lifecycle audit row. Tokens and token hashes are intentionally not
// part of the contract.
type SessionLifecycleAuditEvent struct {
	Kind        SessionLifecycleAuditKind
	Resource    string
	UserID      lazuli.ID
	ExpiresAt   time.Time
	PrunedCount int64
	ErrorCode   string
	Details     map[string]any
}

// SessionLifecycleAuditRecorder is the minimal sink used by
// RecordSessionLifecycleAudit.
type SessionLifecycleAuditRecorder interface {
	RecordSessionLifecycleAudit(ctx context.Context, entry AuditEntry) error
}

var errSessionAuditKindMissing = errors.New("auth: session lifecycle audit kind missing")

// BuildSessionLifecycleAuditEntry converts a lifecycle event into the generic
// audit_log row shape.
func BuildSessionLifecycleAuditEntry(ctx *lazuli.Ctx, event SessionLifecycleAuditEvent) (AuditEntry, error) {
	if event.Kind == "" {
		return AuditEntry{}, errSessionAuditKindMissing
	}

	resource := event.Resource
	if resource == "" {
		resource = "Session"
	}

	payload := map[string]any{
		"event":    string(event.Kind),
		"resource": resource,
	}
	if event.UserID != 0 {
		payload["user_id"] = int64(event.UserID)
	}
	if !event.ExpiresAt.IsZero() {
		payload["expires_at"] = event.ExpiresAt.UTC().Format(time.RFC3339Nano)
	}
	if event.Kind == SessionAuditPruned || event.PrunedCount != 0 {
		payload["pruned_count"] = event.PrunedCount
	}
	if len(event.Details) > 0 {
		payload["details"] = cloneSessionAttrs(event.Details)
	}

	encoded, err := json.Marshal(payload)
	if err != nil {
		return AuditEntry{}, err
	}

	entry := AuditFromCtx(ctx).
		WithCommand(string(event.Kind)).
		WithTargetResource(resource).
		WithPayload(encoded).
		Succeeded()
	if event.ErrorCode != "" {
		entry = entry.Failed(event.ErrorCode)
	}

	applySessionAuditActor(&entry, event)
	return entry, nil
}

// RecordSessionLifecycleAudit builds and writes a session lifecycle audit row.
// A nil recorder is treated as audit disabled.
func RecordSessionLifecycleAudit(
	ctx *lazuli.Ctx,
	recorder SessionLifecycleAuditRecorder,
	event SessionLifecycleAuditEvent,
) error {
	if recorder == nil {
		return nil
	}
	entry, err := BuildSessionLifecycleAuditEntry(ctx, event)
	if err != nil {
		return err
	}
	return recorder.RecordSessionLifecycleAudit(ctxOrBackground(ctx), entry)
}

func applySessionAuditActor(entry *AuditEntry, event SessionLifecycleAuditEvent) {
	if entry.ActorKind == "" {
		if event.UserID != 0 && event.Kind != SessionAuditPruned {
			entry.ActorKind = AuditActorUser
			entry.ActorID = auditIDPtr(event.UserID)
			return
		}
		entry.ActorKind = AuditActorSystem
		return
	}
	if entry.ActorKind == AuditActorUser && entry.ActorID == nil && event.UserID != 0 {
		entry.ActorID = auditIDPtr(event.UserID)
	}
}
