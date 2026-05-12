package auth

import (
	"context"
	"errors"
	"log/slog"

	"github.com/jackc/pgx/v5"

	"lazuli.dev/runtime/lazuli"
)

const (
	// AuditActorUser is an audit row emitted for an authenticated user actor.
	AuditActorUser = "user"
	// AuditActorSystem is an audit row emitted by runtime/system work.
	AuditActorSystem = "system"
	// AuditActorService is an audit row emitted by a service integration.
	AuditActorService = "service"

	// AuditResultOK records a successful command execution.
	AuditResultOK = "ok"
	// AuditResultError records a failed command execution.
	AuditResultError = "error"
)

var errNilAuditTx = errors.New("auth: nil audit transaction")

// AuditEntry mirrors the `audit_log` table schema emitted by Go codegen.
type AuditEntry struct {
	OrgID          *int64
	ActorID        *int64
	ActorKind      string // "user" | "system" | "service"
	CommandName    string // "customer.create"
	TargetResource string // "Customer"
	TargetID       *int64
	Payload        []byte // JSON
	ResultStatus   string // "ok" | "error"
	ErrorCode      string // when ResultStatus=="error"
	CorrelationID  string // request id
}

// AuditFromCtx pre-fills actor, tenant, and request correlation fields from a
// Lazuli execution context.
func AuditFromCtx(ctx *lazuli.Ctx) AuditEntry {
	if ctx == nil {
		return AuditEntry{}
	}

	entry := AuditEntry{
		ActorKind:     string(ctx.Actor),
		CorrelationID: ctx.RequestID,
	}
	if entry.CorrelationID == "" && ctx.Context != nil {
		entry.CorrelationID = lazuli.RequestID(ctx.Context)
	}

	if ctx.Tenant != nil {
		entry.OrgID = auditIDPtr(ctx.Tenant.OrgID)
	} else if ctx.User != nil {
		entry.OrgID = auditIDPtr(ctx.User.OrgID)
	}

	switch {
	case ctx.Actor == lazuli.ActorUser || (ctx.Actor == "" && ctx.User != nil):
		entry.ActorKind = AuditActorUser
		if ctx.User != nil {
			entry.ActorID = auditIDPtr(ctx.User.ID)
		}
	case ctx.Actor == lazuli.ActorSystem:
		entry.ActorKind = AuditActorSystem
	case string(ctx.Actor) == AuditActorService:
		entry.ActorKind = AuditActorService
	}

	return entry
}

// WithCommand sets the qualified command name, e.g. "customer.create".
func (e AuditEntry) WithCommand(name string) AuditEntry {
	e.CommandName = name
	return e
}

// WithTarget sets the target resource and id affected by the command.
func (e AuditEntry) WithTarget(resource string, id lazuli.ID) AuditEntry {
	e.TargetResource = resource
	e.TargetID = auditIDPtr(id)
	return e
}

// WithTargetResource sets the target resource when no concrete id is known.
func (e AuditEntry) WithTargetResource(resource string) AuditEntry {
	e.TargetResource = resource
	return e
}

// WithTargetID sets the target id while preserving an existing resource name.
func (e AuditEntry) WithTargetID(id lazuli.ID) AuditEntry {
	e.TargetID = auditIDPtr(id)
	return e
}

// WithPayload sets the JSON payload persisted to audit_log.payload.
func (e AuditEntry) WithPayload(payload []byte) AuditEntry {
	e.Payload = payload
	return e
}

// Succeeded marks the audit result as a successful command execution.
func (e AuditEntry) Succeeded() AuditEntry {
	e.ResultStatus = AuditResultOK
	e.ErrorCode = ""
	return e
}

// Failed marks the audit result as a failed command execution.
func (e AuditEntry) Failed(errorCode string) AuditEntry {
	e.ResultStatus = AuditResultError
	e.ErrorCode = errorCode
	return e
}

// EmitAudit inserts an audit_log row using the bound pgx transaction.
//
// Audit failures do not break the command. They are logged via slog and never
// propagated to callers.
func EmitAudit(ctx context.Context, conn pgx.Tx, entry AuditEntry) {
	if ctx == nil {
		ctx = context.Background()
	}
	if conn == nil {
		slog.Warn("lazuli: audit emit failed", "err", errNilAuditTx, "command", entry.CommandName)
		return
	}

	const sql = `INSERT INTO audit_log (
		org_id, actor_id, actor_kind, command_name,
		target_resource, target_id, payload,
		result_status, error_code, correlation_id
	) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)`
	_, err := conn.Exec(ctx, sql,
		entry.OrgID, entry.ActorID, entry.ActorKind, entry.CommandName,
		entry.TargetResource, entry.TargetID, entry.Payload,
		entry.ResultStatus, entry.ErrorCode, entry.CorrelationID,
	)
	if err != nil {
		slog.Warn("lazuli: audit emit failed", "err", err, "command", entry.CommandName)
	}
}

func auditIDPtr(id lazuli.ID) *int64 {
	v := int64(id)
	return &v
}
