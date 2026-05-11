// Audit emission. Consumes the existing `lazuli.AuditSpec` plus the
// new `AuditSpec.EmitTo` field from row 37. The language declares
// which fields are recorded and where they go; this package owns the
// post-commit emission and the adapter dispatch.
//
// Reserved streams (`audit_log`, `audit_stream`) resolve to default
// adapters provided by the runtime. Authored event_group names
// resolve to the feature's `event_group <name>` declaration.
//
// See `docs/proposals/bucket-observability-cycle.md` §3.3 §6.5.

package observability

import (
	"context"
	"errors"
	"time"
)

// AuditDestination names the resolved sink for an audit row. The
// runtime maps both reserved streams and authored event_groups to
// the same interface so the generated code is uniform.
type AuditDestination string

const (
	// AuditLog is the reserved default audit stream. Adapter:
	// `@runtime/audit_log` (typically Postgres table with a tight
	// schema).
	AuditLog AuditDestination = "audit_log"
	// AuditStream is the reserved high-throughput audit stream.
	// Adapter: `@runtime/audit_stream` (typically Kafka/NATS/SQS for
	// SIEM forwarding).
	AuditStream AuditDestination = "audit_stream"
)

// AuditRecord is the lowered shape of one audit event. The runtime
// builds one per audited command/job/webhook execution and forwards
// it to the destination resolved from `AuditSpec.EmitTo`.
type AuditRecord struct {
	// Command names the qualified operation (e.g.
	// `customer.reassign`).
	Command string
	// Actor names the @actor.* identity from `Ctx`.
	Actor string
	// Tenant is propagated from `Ctx` when the app declared
	// multi-tenancy.
	Tenant string
	// Decision is the policy outcome: `allowed` | `denied` |
	// `error`. Denied executions still emit an audit row when the
	// language attaches `audit` to the operation (the row records
	// the denial).
	Decision string
	// Fields is the named-fields list lowered from
	// `audit actor, target.id, input.owner_id`.
	Fields map[string]any
	// Destination is the resolved sink.
	Destination AuditDestination
	// RecordedAt is the moment after the transaction committed.
	RecordedAt time.Time
}

// Typed errors.
var (
	// ErrAuditEmitFailed wraps a downstream adapter failure. The
	// runtime forwards by default (logged + retried per adapter
	// contract); a strict mode (`registry.capabilities audit
	// strict`) returns this to the caller as a 500 envelope.
	ErrAuditEmitFailed = errors.New("lazuli/observability: audit_emit_failed")
)

// EmitAudit forwards a built `AuditRecord` to the destination's
// adapter. Generated command/job/webhook handlers call this in a
// post-commit hook (see `runtime/go/lazuli/handle.go:27 withTx`).
//
// Stub: full implementation (adapter resolution, strict-mode
// behaviour, retry policy) lands with the runtime team.
func EmitAudit(ctx context.Context, record AuditRecord) error {
	_ = ctx
	_ = record
	// TODO(runtime): dispatch to the resolved adapter; honor the
	// `strict_audit` capability when set.
	return nil
}

// BuildAuditRecord assembles the post-commit audit row. Generated
// code calls this with the resolved `actor` / `tenant` from `Ctx`
// and the declared fields from the language `audit ... emit_to <X>`
// slot. The runtime fills `RecordedAt` automatically.
func BuildAuditRecord(
	command string,
	actor string,
	tenant string,
	decision string,
	fields map[string]any,
	destination AuditDestination,
) AuditRecord {
	return AuditRecord{
		Command:     command,
		Actor:       actor,
		Tenant:      tenant,
		Decision:    decision,
		Fields:      fields,
		Destination: destination,
		RecordedAt:  time.Now(),
	}
}
