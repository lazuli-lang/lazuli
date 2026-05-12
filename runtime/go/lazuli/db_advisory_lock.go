package lazuli

import (
	"context"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
)

// ExecQuerier is the subset of pgx transaction and pool APIs used by Lazuli DB
// helpers.
type ExecQuerier interface {
	Exec(context.Context, string, ...any) (pgconn.CommandTag, error)
	QueryRow(context.Context, string, ...any) pgx.Row
}

// WithAdvisoryLock acquires a transaction-scoped Postgres advisory lock before
// running fn.
func WithAdvisoryLock(ctx context.Context, tx ExecQuerier, key int64, fn func(context.Context) error) error {
	if _, err := tx.Exec(ctx, "SELECT pg_advisory_xact_lock($1)", key); err != nil {
		return err
	}
	return fn(ctx)
}
