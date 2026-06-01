package lazuli

import (
	"context"
	"errors"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/pgxpool"
)

// DBTX is the minimal query surface a referential guard (Spec 0014,
// `<feature>/guards.gen.go`) needs to run its pre-delete `EXISTS` probe.
// Both `*pgxpool.Pool` (the value `DB()` returns) and `pgx.Tx` satisfy it,
// so a guard can run against the process pool (the generated delete-command
// wiring's choice) or be threaded a transaction later without a signature
// change. The single method mirrors pgx's real `QueryRow` shape exactly —
// `(ctx, sql, args...) pgx.Row` — so the emitted guard body's
// `db.QueryRow(ctx, q, ...).Scan(&inUse)` compiles against the live handle.
type DBTX interface {
	QueryRow(ctx context.Context, sql string, args ...any) pgx.Row
}

// dbPool is the process-global Postgres pool. The runtime initialises it via
// `Boot(...)` at startup; commands and queries pull connections from here.
var dbPool *pgxpool.Pool

// NewPool opens a Postgres pool with Lazuli runtime defaults.
func NewPool(ctx context.Context, url string) (*pgxpool.Pool, error) {
	cfg, err := newPoolConfig(url)
	if err != nil {
		return nil, err
	}
	return pgxpool.NewWithConfig(ctx, cfg)
}

// MustNewPool opens a Postgres pool and panics if configuration fails.
// It is useful in main() boot paths.
func MustNewPool(ctx context.Context, url string) *pgxpool.Pool {
	pool, err := NewPool(ctx, url)
	if err != nil {
		panic(err)
	}
	return pool
}

func newPoolConfig(url string) (*pgxpool.Config, error) {
	cfg, err := pgxpool.ParseConfig(url)
	if err != nil {
		return nil, err
	}

	cfg.MaxConns = 25
	cfg.MinConns = 2
	cfg.MaxConnLifetime = 30 * time.Minute
	cfg.MaxConnIdleTime = 5 * time.Minute
	cfg.HealthCheckPeriod = time.Minute
	cfg.ConnConfig.DefaultQueryExecMode = pgx.QueryExecModeSimpleProtocol

	return cfg, nil
}

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
