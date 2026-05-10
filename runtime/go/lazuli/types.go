// Package lazuli is the Lazuli runtime library. Generated code in `dist/go/`
// imports this package to declare resources, commands, queries, events, and
// other DSL constructs. The runtime executes those declarations.
package lazuli

import "time"

// ID is the canonical identifier type used by every resource.
type ID = int64

// Time is the canonical timestamp type.
type Time = time.Time

// TenancyMode declares how a resource is scoped across tenants.
type TenancyMode int

const (
	TenancyNone TenancyMode = iota // no tenant scoping
	TenancyOrg                     // scoped to a single organisation per row
)

// RateLimit is a rate-limit policy declared at command/api/agent level.
// Format mirrors the DSL string ("30 per hour per ip", "20 per hour per user").
type RateLimit string

// Duration is a Lazuli duration literal: "30s", "10m", "1h", "7d", "7y".
type Duration string

// RetentionSpec declares how long a resource retains rows after soft-delete
// before the runtime applies the terminal action.
type RetentionSpec struct {
	Window Duration
	Then   RetentionAction
}

// RetentionAction is the terminal action applied at the end of the window.
type RetentionAction int

const (
	RetentionDelete    RetentionAction = iota // hard-delete the row
	RetentionAnonymize                        // null PII fields, keep the row
	RetentionArchive                          // move to archive table/storage
)

// Retention is a builder helper for the canonical "Nu then action" form.
//
//	lazuli.Retention("7y").Then(lazuli.Anonymize)
type Retention Duration

// Then completes a retention spec.
func (d Retention) Then(action RetentionAction) RetentionSpec {
	return RetentionSpec{Window: Duration(d), Then: action}
}

// Convenience aliases that read as the DSL would.
const (
	Delete    = RetentionDelete
	Anonymize = RetentionAnonymize
	Archive   = RetentionArchive
)
