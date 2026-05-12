package lazuli

import (
	"context"
	"errors"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/pgxpool"
)

// dbPool is the process-global Postgres pool. The runtime initialises it via
// `Boot(...)` at startup; commands and queries pull connections from here.
var dbPool *pgxpool.Pool

// DB returns the active connection pool. Panics if `Boot` has not been
// called — generated code only reaches DB through the runtime, so a missing
// pool is a programming error, not a user error.
func DB() *pgxpool.Pool {
	if dbPool == nil {
		panic("lazuli: DB pool not initialised; call lazuli.Boot first")
	}
	return dbPool
}

// SetDB installs a connection pool. Used by `Boot` and by tests.
func SetDB(pool *pgxpool.Pool) { dbPool = pool }

// withTx runs fn inside a Postgres transaction. Commits on nil error, rolls
// back otherwise. Serialization failures and deadlocks are retried briefly.
// Used by `Command.Handle`.
func withTx(ctx context.Context, fn func(tx pgx.Tx) error) error {
	const maxAttempts = 3

	var lastErr error
	for attempt := 0; attempt < maxAttempts; attempt++ {
		err := runTxAttempt(ctx, fn)
		if err == nil {
			return nil
		}
		if !isRetryablePostgresError(err) {
			return err
		}

		lastErr = err
		if attempt < maxAttempts-1 {
			if err := sleepBeforeRetry(ctx, attempt); err != nil {
				return err
			}
		}
	}

	return lastErr
}

func runTxAttempt(ctx context.Context, fn func(tx pgx.Tx) error) error {
	tx, err := dbPool.Begin(ctx)
	if err != nil {
		return err
	}
	defer func() { _ = tx.Rollback(ctx) }()

	if err := fn(tx); err != nil {
		return err
	}
	return tx.Commit(ctx)
}

func isRetryablePostgresError(err error) bool {
	var pgErr *pgconn.PgError
	if !errors.As(err, &pgErr) {
		return false
	}
	return pgErr.Code == "40001" || pgErr.Code == "40P01"
}

func sleepBeforeRetry(ctx context.Context, attempt int) error {
	timer := time.NewTimer(time.Duration(1<<attempt) * 10 * time.Millisecond)
	defer timer.Stop()

	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-timer.C:
		return nil
	}
}
