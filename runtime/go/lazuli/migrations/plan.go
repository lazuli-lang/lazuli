// Planning surface. The language ships `lazuli plan --check <name>`
// today (snapshot-integrity validation); typed field-level diff lands
// once Phase L Tier 4 lifts `Resource.fields` into a diffable shape.
// This file declares the contract a future planner adapter satisfies
// so generated wiring doesn't churn when the planner ships.
package migrations

import "context"

// PlannerOutcome is the verdict of a plan run. The runtime emits this
// as a JSON document the adapter consumes; for now, only the snapshot
// integrity bit is populated.
type PlannerOutcome struct {
	// CheckpointName is the snapshot the planner ran against.
	CheckpointName string
	// SnapshotIntegrityOK is true when the pinned snapshot loaded
	// cleanly + its `lazuli_version` matched the analyzer.
	SnapshotIntegrityOK bool
	// SnapshotStale is true when the snapshot loaded but its version
	// lags the analyzer. Warning, not error.
	SnapshotStale bool
	// VersionExpected is the analyzer's expected `lazuli_version`.
	VersionExpected string
	// VersionFound is the `lazuli_version` recorded in the snapshot.
	VersionFound string
}

// Planner is the adapter contract for migration planning. The runtime
// binds an implementation at boot; the CLI's `lazuli plan --check`
// path also calls into a planner when the language-side checkpoint
// integrity check is not enough.
//
// For Route C, the in-process planner only validates checkpoint
// integrity. Typed field-level diff (rename/add/remove) lands in the
// Tier-4 follow-up cycle.
type Planner interface {
	Check(ctx context.Context, checkpoint Checkpoint) (PlannerOutcome, error)
}
