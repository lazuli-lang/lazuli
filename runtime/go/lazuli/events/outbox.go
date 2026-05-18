// Package events ships the EVENT-OUTBOX §3.3 transactional-outbox
// wire: a row pump that drains `lazuli_outbox` and a sibling inbox
// dedup helper for receivers.
//
// Wire-thin discipline: the pump is a SELECT / Dispatch / UPDATE loop
// over pgx/v5. Tx-side outbox INSERTs are written by the generated
// command handler in the same transaction as the resource mutation;
// this file owns only the dispatch side.
package events

import (
	"context"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// OutboxRow is one undispatched row read from `lazuli_outbox`.
type OutboxRow struct {
	ID         int64
	EnvelopeID string
	EventName  string
	Payload    []byte
	CreatedAt  time.Time
}

// Dispatcher is the runtime hook called per row by PumpOnce. The
// concrete implementation wraps the river adapter (or any in-process
// bus); a typical Dispatch returns nil after the event reaches its
// transport. Errors leave the row undispatched for the next tick.
type Dispatcher interface {
	Dispatch(ctx context.Context, row OutboxRow) error
}

// PumpOnce drains up to `limit` undispatched rows from `lazuli_outbox`,
// dispatches each via `d`, and marks dispatched on success. Returns the
// number of rows successfully dispatched (and committed) on this pass.
func PumpOnce(
	ctx context.Context,
	db *pgxpool.Pool,
	d Dispatcher,
	limit int,
) (int, error) {
	if limit <= 0 {
		limit = 100
	}
	rows, err := db.Query(ctx, `
		SELECT id, envelope_id, event_name, payload, created_at
		FROM lazuli_outbox
		WHERE dispatched_at IS NULL
		ORDER BY id
		LIMIT $1`, limit)
	if err != nil {
		return 0, err
	}
	var batch []OutboxRow
	for rows.Next() {
		var r OutboxRow
		if err := rows.Scan(&r.ID, &r.EnvelopeID, &r.EventName, &r.Payload, &r.CreatedAt); err != nil {
			rows.Close()
			return 0, err
		}
		batch = append(batch, r)
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return 0, err
	}
	dispatched := 0
	for _, r := range batch {
		if err := d.Dispatch(ctx, r); err != nil {
			continue
		}
		if _, err := db.Exec(ctx,
			`UPDATE lazuli_outbox SET dispatched_at = now() WHERE id = $1`, r.ID); err != nil {
			return dispatched, err
		}
		dispatched++
	}
	return dispatched, nil
}

// StartPump ticks PumpOnce on `interval` until the returned cancel is
// invoked (or `ctx` is cancelled). Errors are dropped here; the caller
// installs a logging hook by wrapping `d`.
func StartPump(
	ctx context.Context,
	db *pgxpool.Pool,
	d Dispatcher,
	interval time.Duration,
) func() {
	if interval <= 0 {
		interval = 5 * time.Second
	}
	pumpCtx, cancel := context.WithCancel(ctx)
	go func() {
		t := time.NewTicker(interval)
		defer t.Stop()
		for {
			select {
			case <-pumpCtx.Done():
				return
			case <-t.C:
				_, _ = PumpOnce(pumpCtx, db, d, 100)
			}
		}
	}()
	return cancel
}
