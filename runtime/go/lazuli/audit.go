package lazuli

// AuditSpec declares whether and how an operation is audited. Mirrors the
// DSL `audit` child of commands/queries/jobs/webhooks.
//
//	audit                  -> AuditDefault
//	audit none             -> AuditNone
//	audit actor, target.id -> &AuditSpec{Fields: []string{"actor", "target.id"}}
type AuditSpec struct {
	// Fields names what the runtime records on every successful execution.
	// Empty means "default fields" (actor, target identity, input subset).
	// A nil pointer to AuditSpec means "no audit".
	Fields []string

	// MaterializeTable is the snake-cased table name of an append_only
	// OperationLog the audit record is also written to (GAP-AUDIT-01,
	// `audit materialize @feature.<f>.<R>`). Empty means audit emits the
	// event/lazuli_audit row only — the default. When set, the runtime
	// inserts the SAME assembled audit record into this table inside the
	// command tx, in addition to the lazuli_audit row (one record, two
	// sinks). The target resource is verified append_only at compile time
	// by doctor AUDIT-MATERIALIZE-TARGET-001.
	MaterializeTable string
}

// AuditDefault is the canonical "audit with default fields" marker.
var AuditDefault = &AuditSpec{Fields: nil}

// AuditNone explicitly opts out of auditing (rare; usually omit the audit
// child to mean the same thing).
var AuditNone *AuditSpec
