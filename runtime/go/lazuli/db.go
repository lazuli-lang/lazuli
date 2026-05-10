package lazuli

import (
	"context"

	"github.com/jackc/pgx/v5"
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
// back otherwise. Used by `Command.Handle`.
func withTx(ctx context.Context, fn func(tx pgx.Tx) error) error {
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
