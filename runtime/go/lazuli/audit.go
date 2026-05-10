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
}

// AuditDefault is the canonical "audit with default fields" marker.
var AuditDefault = &AuditSpec{Fields: nil}

// AuditNone explicitly opts out of auditing (rare; usually omit the audit
// child to mean the same thing).
var AuditNone *AuditSpec
