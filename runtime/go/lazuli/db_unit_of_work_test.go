package lazuli

import (
	"context"
	"errors"
	"reflect"
	"testing"
)

func TestWithUnitOfWorkCommitsAndRunsAfterCommitCallbacksInOrder(t *testing.T) {
	ctx := context.WithValue(context.Background(), unitOfWorkTestContextKey{}, "ctx")
	tx := &unitOfWorkTxFake{}
	var beginCtx context.Context
	var fnCtx context.Context
	var fnUOW *UnitOfWork
	var callbackContexts []context.Context
	var calls []string

	err := WithUnitOfWork(ctx, func(got context.Context) (Tx, error) {
		beginCtx = got
		return tx, nil
	}, func(got context.Context, uow *UnitOfWork) error {
		fnCtx = got
		fnUOW = uow

		if got.Value(unitOfWorkTestContextKey{}) != "ctx" {
			t.Fatal("fn context did not preserve parent values")
		}
		if fromCtx, ok := UnitOfWorkFromContext(got); !ok || fromCtx != uow {
			t.Fatalf("UnitOfWorkFromContext = %v, %v; want unit of work, true", fromCtx, ok)
		}
		if fromCtx, ok := TxFromContext(got); !ok || fromCtx != tx {
			t.Fatalf("TxFromContext = %v, %v; want tx, true", fromCtx, ok)
		}
		if !RegisterAfterCommit(got, func(callbackCtx context.Context) error {
			callbackContexts = append(callbackContexts, callbackCtx)
			calls = append(calls, "after-commit-1")
			return nil
		}) {
			t.Fatal("RegisterAfterCommit returned false")
		}
		uow.AfterCommit(func(callbackCtx context.Context) error {
			callbackContexts = append(callbackContexts, callbackCtx)
			calls = append(calls, "after-commit-2")
			return nil
		})
		uow.AfterRollback(func(context.Context) error {
			calls = append(calls, "after-rollback")
			return nil
		})
		if RegisterAfterCommit(got, nil) {
			t.Fatal("RegisterAfterCommit returned true for nil callback")
		}
		return nil
	})

	if err != nil {
		t.Fatalf("WithUnitOfWork returned error: %v", err)
	}
	if beginCtx != ctx {
		t.Fatal("beginFunc received different context")
	}
	if fnCtx == nil {
		t.Fatal("fn was not called")
	}
	if fnUOW == nil || fnUOW.Tx() != tx {
		t.Fatalf("UnitOfWork.Tx() = %v, want tx", fnUOW.Tx())
	}
	assertUnitOfWorkTxCalls(t, tx, unitOfWorkTxCalls{commit: 1})
	if tx.commitContext != fnCtx {
		t.Fatal("Commit received different context")
	}
	assertStringSlice(t, calls, []string{"after-commit-1", "after-commit-2"})
	for _, callbackCtx := range callbackContexts {
		if callbackCtx != fnCtx {
			t.Fatal("callback received different context")
		}
	}
}

func TestWithUnitOfWorkRollsBackAndRunsAfterRollbackCallbacksInOrder(t *testing.T) {
	ctx := context.Background()
	tx := &unitOfWorkTxFake{}
	wantErr := errors.New("function failed")
	var calls []string

	err := WithUnitOfWork(ctx, func(context.Context) (Tx, error) {
		return tx, nil
	}, func(got context.Context, uow *UnitOfWork) error {
		uow.AfterCommit(func(context.Context) error {
			calls = append(calls, "after-commit")
			return nil
		})
		uow.AfterRollback(func(context.Context) error {
			calls = append(calls, "after-rollback-1")
			return nil
		})
		if !RegisterAfterRollback(got, func(context.Context) error {
			calls = append(calls, "after-rollback-2")
			return nil
		}) {
			t.Fatal("RegisterAfterRollback returned false")
		}
		if RegisterAfterRollback(got, nil) {
			t.Fatal("RegisterAfterRollback returned true for nil callback")
		}
		return wantErr
	})

	if !errors.Is(err, wantErr) {
		t.Fatalf("WithUnitOfWork error = %v, want %v", err, wantErr)
	}
	if err != wantErr {
		t.Fatalf("WithUnitOfWork error identity = %v, want %v", err, wantErr)
	}
	assertUnitOfWorkTxCalls(t, tx, unitOfWorkTxCalls{rollback: 1})
	assertStringSlice(t, calls, []string{"after-rollback-1", "after-rollback-2"})
}

func TestWithUnitOfWorkReturnsAfterCommitCallbackErrorsInOrder(t *testing.T) {
	ctx := context.Background()
	tx := &unitOfWorkTxFake{}
	firstErr := errors.New("after commit 1 failed")
	secondErr := errors.New("after commit 2 failed")
	var calls []string

	err := WithUnitOfWork(ctx, func(context.Context) (Tx, error) {
		return tx, nil
	}, func(_ context.Context, uow *UnitOfWork) error {
		uow.AfterCommit(nil)
		uow.AfterCommit(func(context.Context) error {
			calls = append(calls, "after-commit-1")
			return firstErr
		})
		uow.AfterCommit(func(context.Context) error {
			calls = append(calls, "after-commit-2")
			return secondErr
		})
		return nil
	})

	if !errors.Is(err, firstErr) {
		t.Fatalf("WithUnitOfWork error = %v, want first callback error", err)
	}
	if !errors.Is(err, secondErr) {
		t.Fatalf("WithUnitOfWork error = %v, want second callback error", err)
	}
	if got, want := err.Error(), "after commit 1 failed\nafter commit 2 failed"; got != want {
		t.Fatalf("WithUnitOfWork error string = %q, want %q", got, want)
	}
	assertUnitOfWorkTxCalls(t, tx, unitOfWorkTxCalls{commit: 1})
	assertStringSlice(t, calls, []string{"after-commit-1", "after-commit-2"})
}

func TestWithUnitOfWorkJoinsFunctionAndAfterRollbackErrorsInOrder(t *testing.T) {
	ctx := context.Background()
	tx := &unitOfWorkTxFake{}
	fnErr := errors.New("function failed")
	firstErr := errors.New("after rollback 1 failed")
	secondErr := errors.New("after rollback 2 failed")
	var calls []string

	err := WithUnitOfWork(ctx, func(context.Context) (Tx, error) {
		return tx, nil
	}, func(_ context.Context, uow *UnitOfWork) error {
		uow.AfterRollback(func(context.Context) error {
			calls = append(calls, "after-rollback-1")
			return firstErr
		})
		uow.AfterRollback(func(context.Context) error {
			calls = append(calls, "after-rollback-2")
			return secondErr
		})
		return fnErr
	})

	for _, want := range []error{fnErr, firstErr, secondErr} {
		if !errors.Is(err, want) {
			t.Fatalf("WithUnitOfWork error = %v, want %v", err, want)
		}
	}
	if got, want := err.Error(), "function failed\nafter rollback 1 failed\nafter rollback 2 failed"; got != want {
		t.Fatalf("WithUnitOfWork error string = %q, want %q", got, want)
	}
	assertUnitOfWorkTxCalls(t, tx, unitOfWorkTxCalls{rollback: 1})
	assertStringSlice(t, calls, []string{"after-rollback-1", "after-rollback-2"})
}

func TestWithUnitOfWorkSkipsAfterRollbackCallbacksWhenRollbackFails(t *testing.T) {
	ctx := context.Background()
	fnErr := errors.New("function failed")
	rollbackErr := errors.New("rollback failed")
	tx := &unitOfWorkTxFake{rollbackErr: rollbackErr}
	var calls []string

	err := WithUnitOfWork(ctx, func(context.Context) (Tx, error) {
		return tx, nil
	}, func(_ context.Context, uow *UnitOfWork) error {
		uow.AfterRollback(func(context.Context) error {
			calls = append(calls, "after-rollback")
			return nil
		})
		return fnErr
	})

	if !errors.Is(err, fnErr) {
		t.Fatalf("WithUnitOfWork error = %v, want function error", err)
	}
	if !errors.Is(err, rollbackErr) {
		t.Fatalf("WithUnitOfWork error = %v, want rollback error", err)
	}
	if got, want := err.Error(), "function failed\nrollback failed"; got != want {
		t.Fatalf("WithUnitOfWork error string = %q, want %q", got, want)
	}
	assertUnitOfWorkTxCalls(t, tx, unitOfWorkTxCalls{rollback: 1})
	assertStringSlice(t, calls, nil)
}

func TestWithUnitOfWorkCommitErrorRollsBackAndRunsAfterRollbackCallbacks(t *testing.T) {
	ctx := context.Background()
	commitErr := errors.New("commit failed")
	callbackErr := errors.New("after rollback failed")
	tx := &unitOfWorkTxFake{commitErr: commitErr}
	var calls []string

	err := WithUnitOfWork(ctx, func(context.Context) (Tx, error) {
		return tx, nil
	}, func(_ context.Context, uow *UnitOfWork) error {
		uow.AfterCommit(func(context.Context) error {
			calls = append(calls, "after-commit")
			return nil
		})
		uow.AfterRollback(func(context.Context) error {
			calls = append(calls, "after-rollback")
			return callbackErr
		})
		return nil
	})

	if !errors.Is(err, commitErr) {
		t.Fatalf("WithUnitOfWork error = %v, want commit error", err)
	}
	if !errors.Is(err, callbackErr) {
		t.Fatalf("WithUnitOfWork error = %v, want callback error", err)
	}
	if got, want := err.Error(), "commit failed\nafter rollback failed"; got != want {
		t.Fatalf("WithUnitOfWork error string = %q, want %q", got, want)
	}
	assertUnitOfWorkTxCalls(t, tx, unitOfWorkTxCalls{commit: 1, rollback: 1})
	assertStringSlice(t, calls, []string{"after-rollback"})
}

func TestWithUnitOfWorkRejectsNilInputsAndCanceledContext(t *testing.T) {
	ctx := context.Background()

	if err := WithUnitOfWork(ctx, nil, func(context.Context, *UnitOfWork) error { return nil }); !errors.Is(err, errNilUnitOfWorkBeginFunc) {
		t.Fatalf("nil beginFunc error = %v, want %v", err, errNilUnitOfWorkBeginFunc)
	}
	if err := WithUnitOfWork(ctx, func(context.Context) (Tx, error) { return &unitOfWorkTxFake{}, nil }, nil); !errors.Is(err, errNilUnitOfWorkFunc) {
		t.Fatalf("nil fn error = %v, want %v", err, errNilUnitOfWorkFunc)
	}
	if err := WithUnitOfWork(ctx, func(context.Context) (Tx, error) { return nil, nil }, func(context.Context, *UnitOfWork) error { return nil }); !errors.Is(err, errNilUnitOfWorkTx) {
		t.Fatalf("nil tx error = %v, want %v", err, errNilUnitOfWorkTx)
	}

	var nilTx *unitOfWorkTxFake
	if err := WithUnitOfWork(ctx, func(context.Context) (Tx, error) { return nilTx, nil }, func(context.Context, *UnitOfWork) error { return nil }); !errors.Is(err, errNilUnitOfWorkTx) {
		t.Fatalf("typed nil tx error = %v, want %v", err, errNilUnitOfWorkTx)
	}

	canceledCtx, cancel := context.WithCancel(context.Background())
	cancel()
	beginCalled := false
	err := WithUnitOfWork(canceledCtx, func(context.Context) (Tx, error) {
		beginCalled = true
		return &unitOfWorkTxFake{}, nil
	}, func(context.Context, *UnitOfWork) error {
		t.Fatal("fn was called after context cancellation")
		return nil
	})

	if !errors.Is(err, context.Canceled) {
		t.Fatalf("WithUnitOfWork error = %v, want %v", err, context.Canceled)
	}
	if beginCalled {
		t.Fatal("beginFunc was called after context cancellation")
	}
}

func TestUnitOfWorkContextHelpersHandleMissingValues(t *testing.T) {
	if got, ok := UnitOfWorkFromContext(nil); ok || got != nil {
		t.Fatalf("UnitOfWorkFromContext(nil) = %v, %v; want nil, false", got, ok)
	}
	if got, ok := TxFromContext(nil); ok || got != nil {
		t.Fatalf("TxFromContext(nil) = %v, %v; want nil, false", got, ok)
	}
	if RegisterAfterCommit(context.Background(), func(context.Context) error { return nil }) {
		t.Fatal("RegisterAfterCommit returned true without UnitOfWork")
	}
	if RegisterAfterRollback(context.Background(), func(context.Context) error { return nil }) {
		t.Fatal("RegisterAfterRollback returned true without UnitOfWork")
	}

	ctx := ContextWithUnitOfWork(nil, nil)
	if _, ok := UnitOfWorkFromContext(ctx); ok {
		t.Fatal("ContextWithUnitOfWork stored nil UnitOfWork")
	}
	ctx = ContextWithTx(nil, nil)
	if _, ok := TxFromContext(ctx); ok {
		t.Fatal("ContextWithTx stored nil Tx")
	}
}

type unitOfWorkTestContextKey struct{}

type unitOfWorkTxCalls struct {
	commit   int
	rollback int
}

type unitOfWorkTxFake struct {
	commitErr       error
	rollbackErr     error
	commitCalls     int
	rollbackCalls   int
	commitContext   context.Context
	rollbackContext context.Context
}

func (tx *unitOfWorkTxFake) Commit(ctx context.Context) error {
	tx.commitCalls++
	tx.commitContext = ctx
	return tx.commitErr
}

func (tx *unitOfWorkTxFake) Rollback(ctx context.Context) error {
	tx.rollbackCalls++
	tx.rollbackContext = ctx
	return tx.rollbackErr
}

func assertUnitOfWorkTxCalls(t *testing.T, tx *unitOfWorkTxFake, want unitOfWorkTxCalls) {
	t.Helper()
	got := unitOfWorkTxCalls{commit: tx.commitCalls, rollback: tx.rollbackCalls}
	if got != want {
		t.Fatalf("tx calls = %+v, want %+v", got, want)
	}
}

func assertStringSlice(t *testing.T, got, want []string) {
	t.Helper()
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("calls = %#v, want %#v", got, want)
	}
}
