package lazuli

import (
	"context"
	"errors"
)

var (
	errNilUnitOfWorkBeginFunc = errors.New("lazuli: nil unit of work begin function")
	errNilUnitOfWorkFunc      = errors.New("lazuli: nil unit of work function")
	errNilUnitOfWorkTx        = errors.New("lazuli: nil unit of work transaction")
)

type unitOfWorkContextKey struct{}
type txContextKey struct{}

// UnitOfWorkCallback runs after a unit of work commits or rolls back.
type UnitOfWorkCallback func(context.Context) error

// UnitOfWork tracks the active transaction plus callbacks that must run after
// its commit or rollback completes.
type UnitOfWork struct {
	tx            Tx
	afterCommit   []UnitOfWorkCallback
	afterRollback []UnitOfWorkCallback
}

// NewUnitOfWork wraps tx with callback registration.
func NewUnitOfWork(tx Tx) *UnitOfWork {
	return &UnitOfWork{tx: tx}
}

// Tx returns the transaction wrapped by uow.
func (uow *UnitOfWork) Tx() Tx {
	if uow == nil {
		return nil
	}
	return uow.tx
}

// AfterCommit registers fn to run after the transaction commits.
//
// Callbacks run in registration order. Nil callbacks are ignored.
func (uow *UnitOfWork) AfterCommit(fn UnitOfWorkCallback) {
	if uow == nil || fn == nil {
		return
	}
	uow.afterCommit = append(uow.afterCommit, fn)
}

// AfterRollback registers fn to run after the transaction rolls back.
//
// Callbacks run in registration order. Nil callbacks are ignored.
func (uow *UnitOfWork) AfterRollback(fn UnitOfWorkCallback) {
	if uow == nil || fn == nil {
		return
	}
	uow.afterRollback = append(uow.afterRollback, fn)
}

// WithUnitOfWork starts a transaction, runs fn with a context carrying the
// transaction and UnitOfWork, then commits on nil error or rolls back otherwise.
//
// After-commit callbacks run only after Commit succeeds. After-rollback
// callbacks run only after Rollback succeeds. Callback errors are joined in
// registration order after the transaction or function error that led to them.
func WithUnitOfWork(ctx context.Context, beginFunc func(context.Context) (Tx, error), fn func(context.Context, *UnitOfWork) error) error {
	if beginFunc == nil {
		return errNilUnitOfWorkBeginFunc
	}
	if fn == nil {
		return errNilUnitOfWorkFunc
	}
	if ctx == nil {
		ctx = context.Background()
	}
	if err := ctx.Err(); err != nil {
		return err
	}

	tx, err := beginFunc(ctx)
	if err != nil {
		return err
	}
	if isNilRollbackTx(tx) {
		return errNilUnitOfWorkTx
	}

	uow := NewUnitOfWork(tx)
	runCtx := ContextWithUnitOfWork(ContextWithTx(ctx, tx), uow)

	primaryErr := runCtx.Err()
	if primaryErr == nil {
		primaryErr = fn(runCtx, uow)
	}
	if primaryErr == nil {
		primaryErr = runCtx.Err()
	}

	if primaryErr != nil {
		return uow.rollback(runCtx, primaryErr)
	}
	return uow.commit(runCtx)
}

// ContextWithUnitOfWork returns a context carrying uow.
func ContextWithUnitOfWork(ctx context.Context, uow *UnitOfWork) context.Context {
	if ctx == nil {
		ctx = context.Background()
	}
	if uow == nil {
		return ctx
	}
	return context.WithValue(ctx, unitOfWorkContextKey{}, uow)
}

// UnitOfWorkFromContext returns the UnitOfWork attached to ctx.
func UnitOfWorkFromContext(ctx context.Context) (*UnitOfWork, bool) {
	if ctx == nil {
		return nil, false
	}
	uow, ok := ctx.Value(unitOfWorkContextKey{}).(*UnitOfWork)
	return uow, ok && uow != nil
}

// ContextWithTx returns a context carrying tx.
func ContextWithTx(ctx context.Context, tx Tx) context.Context {
	if ctx == nil {
		ctx = context.Background()
	}
	if isNilRollbackTx(tx) {
		return ctx
	}
	return context.WithValue(ctx, txContextKey{}, tx)
}

// TxFromContext returns the transaction attached to ctx.
func TxFromContext(ctx context.Context) (Tx, bool) {
	if ctx == nil {
		return nil, false
	}
	tx, ok := ctx.Value(txContextKey{}).(Tx)
	if !ok || isNilRollbackTx(tx) {
		return nil, false
	}
	return tx, true
}

// RegisterAfterCommit registers fn on the UnitOfWork attached to ctx.
func RegisterAfterCommit(ctx context.Context, fn UnitOfWorkCallback) bool {
	uow, ok := UnitOfWorkFromContext(ctx)
	if !ok || fn == nil {
		return false
	}
	uow.AfterCommit(fn)
	return true
}

// RegisterAfterRollback registers fn on the UnitOfWork attached to ctx.
func RegisterAfterRollback(ctx context.Context, fn UnitOfWorkCallback) bool {
	uow, ok := UnitOfWorkFromContext(ctx)
	if !ok || fn == nil {
		return false
	}
	uow.AfterRollback(fn)
	return true
}

func (uow *UnitOfWork) commit(ctx context.Context) error {
	if err := uow.tx.Commit(ctx); err != nil {
		return uow.rollback(ctx, err)
	}
	return runUnitOfWorkCallbacks(ctx, uow.afterCommit)
}

func (uow *UnitOfWork) rollback(ctx context.Context, primaryErr error) error {
	if err := uow.tx.Rollback(ctx); err != nil {
		return joinUnitOfWorkErrors(primaryErr, err)
	}
	return joinUnitOfWorkErrors(primaryErr, runUnitOfWorkCallbacks(ctx, uow.afterRollback))
}

func runUnitOfWorkCallbacks(ctx context.Context, callbacks []UnitOfWorkCallback) error {
	callbacks = append([]UnitOfWorkCallback(nil), callbacks...)

	errs := make([]error, 0, len(callbacks))
	for _, callback := range callbacks {
		if callback == nil {
			continue
		}
		if err := callback(ctx); err != nil {
			errs = append(errs, err)
		}
	}
	return joinUnitOfWorkErrors(errs...)
}

func joinUnitOfWorkErrors(errs ...error) error {
	joined := make([]error, 0, len(errs))
	for _, err := range errs {
		if err != nil {
			joined = append(joined, err)
		}
	}

	switch len(joined) {
	case 0:
		return nil
	case 1:
		return joined[0]
	default:
		return errors.Join(joined...)
	}
}
