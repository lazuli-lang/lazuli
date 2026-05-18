package events

import (
	"context"

	"github.com/jackc/pgx/v5/pgxpool"
)

// InboxDedup is the receiver-side idempotency contract for
// EVENT-OUTBOX §3.3. River (or any subscriber wrapper) calls
// `AlreadySeen` before invoking the user handler; on success it calls
// `MarkSeen` so a repeat dispatch (after a crashed pump or replayed
// outbox row) is a no-op.
type InboxDedup struct {
	db *pgxpool.Pool
}

// NewInboxDedup constructs an InboxDedup bound to a pgxpool.
func NewInboxDedup(db *pgxpool.Pool) *InboxDedup {
	return &InboxDedup{db: db}
}

// AlreadySeen reports whether the envelope ID has previously been
// recorded in `lazuli_inbox`.
func (d *InboxDedup) AlreadySeen(ctx context.Context, envelopeID string) (bool, error) {
	var exists bool
	err := d.db.QueryRow(ctx,
		`SELECT EXISTS(SELECT 1 FROM lazuli_inbox WHERE envelope_id = $1)`,
		envelopeID,
	).Scan(&exists)
	if err != nil {
		return false, err
	}
	return exists, nil
}

// MarkSeen records the envelope ID. Concurrent receivers race the
// INSERT; `ON CONFLICT DO NOTHING` makes the call idempotent.
func (d *InboxDedup) MarkSeen(ctx context.Context, envelopeID, eventName string) error {
	_, err := d.db.Exec(ctx,
		`INSERT INTO lazuli_inbox (envelope_id, event_name)
		 VALUES ($1, $2)
		 ON CONFLICT (envelope_id) DO NOTHING`,
		envelopeID, eventName,
	)
	return err
}
