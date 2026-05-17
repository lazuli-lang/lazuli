// Package lazuli — per-field activity-row emitter.
//
// Cell W1.4 from synth-oss-mirror-wave-1-2-2026-05-17.md §3.B row 2.
// 4-OSS evidence (Plane track_<X> + ToolJet app_history + n8n
// ExecutionEntity + OpenMetadata EntityRepository.post{Create,Update,
// Delete}). The OpenMetadata closure-on-shared-supertype is the
// architectural anchor.
//
// Wire-thin: this is a parameterized INSERT inside the command's own
// transaction. No diff computation, no event emit (the existing
// `emits` does that orthogonally), no template engine.
package lazuli

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"

	"github.com/jackc/pgx/v5"
)

// ActivityRow is the typed envelope codegen builds when a command's
// `audit` child lists ≥ 2 fields. One ActivityRow per (resource, field)
// pair where old_value != new_value. Codegen fills every field; the
// runtime emits the INSERT.
//
// Codegen lays the per-resource table name + parent FK column name as
// strings because they're known at compile time. Doctor diagnostics
// (proposal-pending VOCAB-ACTIVITY-001/002) validate the strings against
// the IR before codegen runs.
type ActivityRow struct {
	// Table is the activity table name (e.g. "issue_activities").
	Table string
	// ParentColumn is the FK column referencing the parent resource
	// (e.g. "issue_id", "user_id"). Closed at codegen time from the
	// resource's identifier field.
	ParentColumn string
	// ParentID is the value of the parent resource's PK for the row
	// that mutated.
	ParentID string
	// ActorID is the user (or system actor) that triggered the mutation.
	// Resolved from ctx.actor.id by the command handler upstream.
	ActorID string
	// Field is the resource field that changed. Must match the audit
	// child's listed field name verbatim.
	Field string
	// OldValue is the pre-mutation value, marshaled to JSON for the
	// jsonb column. nil for inserts.
	OldValue any
	// NewValue is the post-mutation value. nil for deletes.
	NewValue any
	// TenancyColumn is the tenancy FK column (e.g. "tenant_id",
	// "org_id"). Empty when the parent resource has no tenant_from.
	TenancyColumn string
	// TenancyID is the value of the tenancy FK. Empty when
	// TenancyColumn is empty.
	TenancyID string
}

// RecordActivity inserts one ActivityRow inside `tx`. Returns nil on
// success. Caller is the command's own handler — the activity write
// shares the command's transaction so atomic semantics hold (either
// both the field mutation AND the activity row commit, or neither).
//
// Skips silently when OldValue == NewValue (both serialized as JSON);
// codegen already guards on the pre-call site, but defense-in-depth
// avoids degenerate rows.
func RecordActivity(ctx context.Context, tx pgx.Tx, row ActivityRow) error {
	if row.Table == "" {
		return errors.New("lazuli: RecordActivity: empty Table")
	}
	if row.ParentColumn == "" {
		return errors.New("lazuli: RecordActivity: empty ParentColumn")
	}
	if row.ParentID == "" {
		return errors.New("lazuli: RecordActivity: empty ParentID")
	}
	if row.Field == "" {
		return errors.New("lazuli: RecordActivity: empty Field")
	}

	oldJSON, err := jsonOrNull(row.OldValue)
	if err != nil {
		return fmt.Errorf("lazuli: RecordActivity marshal old: %w", err)
	}
	newJSON, err := jsonOrNull(row.NewValue)
	if err != nil {
		return fmt.Errorf("lazuli: RecordActivity marshal new: %w", err)
	}
	if bytesEqual(oldJSON, newJSON) {
		return nil
	}

	id, err := newActivityID()
	if err != nil {
		return err
	}

	// Closed column set — codegen generates the table with exactly
	// these columns + optionally the tenancy column. INSERT shape is
	// stable across resources.
	if row.TenancyColumn == "" {
		_, err = tx.Exec(ctx,
			`INSERT INTO "`+row.Table+`" `+
				`(id, "`+row.ParentColumn+`", actor_id, field, old_value, new_value) `+
				`VALUES ($1, $2, $3, $4, $5, $6)`,
			id, row.ParentID, row.ActorID, row.Field, oldJSON, newJSON,
		)
	} else {
		_, err = tx.Exec(ctx,
			`INSERT INTO "`+row.Table+`" `+
				`(id, "`+row.ParentColumn+`", actor_id, field, old_value, new_value, "`+row.TenancyColumn+`") `+
				`VALUES ($1, $2, $3, $4, $5, $6, $7)`,
			id, row.ParentID, row.ActorID, row.Field, oldJSON, newJSON, row.TenancyID,
		)
	}
	if err != nil {
		return fmt.Errorf("lazuli: RecordActivity insert into %s: %w", row.Table, err)
	}
	return nil
}

// jsonOrNull marshals v to JSON bytes, or returns nil when v is nil
// (so the jsonb column stores SQL NULL rather than the string "null").
func jsonOrNull(v any) ([]byte, error) {
	if v == nil {
		return nil, nil
	}
	return json.Marshal(v)
}

func bytesEqual(a, b []byte) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

// newActivityID returns a 32-hex-char id. Cheaper than uuid v4 (no
// version bits) and codegen drops it into a `uuid` column via Postgres
// implicit cast.
func newActivityID() (string, error) {
	buf := make([]byte, 16)
	if _, err := rand.Read(buf); err != nil {
		return "", fmt.Errorf("lazuli: RecordActivity id: %w", err)
	}
	// Force uuid v4-ish bits so column-type checks pass.
	buf[6] = (buf[6] & 0x0f) | 0x40
	buf[8] = (buf[8] & 0x3f) | 0x80
	s := hex.EncodeToString(buf)
	// Canonical 8-4-4-4-12 layout.
	return s[0:8] + "-" + s[8:12] + "-" + s[12:16] + "-" + s[16:20] + "-" + s[20:32], nil
}
