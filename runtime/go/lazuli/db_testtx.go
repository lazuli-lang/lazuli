package lazuli

import (
	"context"
	"errors"
	"reflect"
)

var (
	errNilRollbackBeginFunc = errors.New("lazuli: nil rollback begin function")
	errNilRollbackFunc      = errors.New("lazuli: nil rollback function")
	errNilRollbackTx        = errors.New("lazuli: nil rollback transaction")
)

// Tx is the minimal transaction interface required by WithRollback.
type Tx interface {
	Commit(context.Context) error
	Rollback(context.Context) error
}

// WithRollback starts a transaction, runs fn, and always rolls the transaction
// back instead of committing it.
//
// It is intended for tests that need database changes isolated per case.
// beginFunc may wrap any database adapter that can return a Tx-compatible
// transaction. If fn or ctx fails and Rollback also fails, the returned error
// joins the primary error first and the rollback error second.
func WithRollback(ctx context.Context, beginFunc func(context.Context) (Tx, error), fn func(context.Context, Tx) error) error {
	if beginFunc == nil {
		return errNilRollbackBeginFunc
	}
	if fn == nil {
		return errNilRollbackFunc
	}
	if err := ctx.Err(); err != nil {
		return err
	}

	tx, err := beginFunc(ctx)
	if err != nil {
		return err
	}
	if isNilRollbackTx(tx) {
		return errNilRollbackTx
	}

	primaryErr := ctx.Err()
	if primaryErr == nil {
		primaryErr = fn(ctx, tx)
	}
	if primaryErr == nil {
		primaryErr = ctx.Err()
	}

	rollbackErr := tx.Rollback(ctx)
	return joinRollbackError(primaryErr, rollbackErr)
}

func joinRollbackError(primaryErr, rollbackErr error) error {
	if primaryErr == nil {
		return rollbackErr
	}
	if rollbackErr == nil {
		return primaryErr
	}
	return errors.Join(primaryErr, rollbackErr)
}

func isNilRollbackTx(tx Tx) bool {
	if tx == nil {
		return true
	}

	value := reflect.ValueOf(tx)
	switch value.Kind() {
	case reflect.Chan, reflect.Func, reflect.Interface, reflect.Map, reflect.Ptr, reflect.Slice:
		return value.IsNil()
	default:
		return false
	}
}
