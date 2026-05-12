package lazuli

import (
	"context"
	"errors"
	"testing"
)

func TestWithRollbackRollsBackOnSuccess(t *testing.T) {
	ctx := context.WithValue(context.Background(), rollbackTestContextKey{}, "ctx")
	tx := &rollbackTxFake{}
	var beginCtx context.Context
	var fnCtx context.Context
	var fnTx Tx

	err := WithRollback(ctx, func(got context.Context) (Tx, error) {
		beginCtx = got
		return tx, nil
	}, func(got context.Context, gotTx Tx) error {
		fnCtx = got
		fnTx = gotTx
		return nil
	})

	if err != nil {
		t.Fatalf("WithRollback returned error: %v", err)
	}
	if beginCtx != ctx {
		t.Fatal("beginFunc received different context")
	}
	if fnCtx != ctx {
		t.Fatal("fn received different context")
	}
	if fnTx != tx {
		t.Fatal("fn received different transaction")
	}
	assertRollbackTxCalls(t, tx, rollbackTxCalls{rollback: 1})
	if tx.rollbackContext != ctx {
		t.Fatal("Rollback received different context")
	}
}

func TestWithRollbackRollsBackAndReturnsFunctionError(t *testing.T) {
	ctx := context.Background()
	tx := &rollbackTxFake{}
	wantErr := errors.New("function failed")

	err := WithRollback(ctx, func(context.Context) (Tx, error) {
		return tx, nil
	}, func(context.Context, Tx) error {
		return wantErr
	})

	if !errors.Is(err, wantErr) {
		t.Fatalf("WithRollback error = %v, want %v", err, wantErr)
	}
	if err != wantErr {
		t.Fatalf("WithRollback error identity = %v, want %v", err, wantErr)
	}
	assertRollbackTxCalls(t, tx, rollbackTxCalls{rollback: 1})
}

func TestWithRollbackReturnsRollbackErrorOnSuccess(t *testing.T) {
	ctx := context.Background()
	wantErr := errors.New("rollback failed")
	tx := &rollbackTxFake{rollbackErr: wantErr}

	err := WithRollback(ctx, func(context.Context) (Tx, error) {
		return tx, nil
	}, func(context.Context, Tx) error {
		return nil
	})

	if !errors.Is(err, wantErr) {
		t.Fatalf("WithRollback error = %v, want %v", err, wantErr)
	}
	assertRollbackTxCalls(t, tx, rollbackTxCalls{rollback: 1})
}

func TestWithRollbackJoinsFunctionAndRollbackErrors(t *testing.T) {
	ctx := context.Background()
	fnErr := errors.New("function failed")
	rollbackErr := errors.New("rollback failed")
	tx := &rollbackTxFake{rollbackErr: rollbackErr}

	err := WithRollback(ctx, func(context.Context) (Tx, error) {
		return tx, nil
	}, func(context.Context, Tx) error {
		return fnErr
	})

	if !errors.Is(err, fnErr) {
		t.Fatalf("WithRollback error = %v, want function error", err)
	}
	if !errors.Is(err, rollbackErr) {
		t.Fatalf("WithRollback error = %v, want rollback error", err)
	}
	if got, want := err.Error(), "function failed\nrollback failed"; got != want {
		t.Fatalf("WithRollback error string = %q, want %q", got, want)
	}
	assertRollbackTxCalls(t, tx, rollbackTxCalls{rollback: 1})
}

func TestWithRollbackReturnsBeginError(t *testing.T) {
	ctx := context.Background()
	wantErr := errors.New("begin failed")
	fnCalled := false

	err := WithRollback(ctx, func(context.Context) (Tx, error) {
		return nil, wantErr
	}, func(context.Context, Tx) error {
		fnCalled = true
		return nil
	})

	if !errors.Is(err, wantErr) {
		t.Fatalf("WithRollback error = %v, want %v", err, wantErr)
	}
	if fnCalled {
		t.Fatal("fn was called after begin error")
	}
}

func TestWithRollbackPropagatesCanceledContextBeforeBegin(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	beginCalled := false

	err := WithRollback(ctx, func(context.Context) (Tx, error) {
		beginCalled = true
		return &rollbackTxFake{}, nil
	}, func(context.Context, Tx) error {
		t.Fatal("fn was called after context cancellation")
		return nil
	})

	if !errors.Is(err, context.Canceled) {
		t.Fatalf("WithRollback error = %v, want %v", err, context.Canceled)
	}
	if beginCalled {
		t.Fatal("beginFunc was called after context cancellation")
	}
}

func TestWithRollbackPropagatesCanceledContextAfterFunction(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	tx := &rollbackTxFake{}

	err := WithRollback(ctx, func(context.Context) (Tx, error) {
		return tx, nil
	}, func(context.Context, Tx) error {
		cancel()
		return nil
	})

	if !errors.Is(err, context.Canceled) {
		t.Fatalf("WithRollback error = %v, want %v", err, context.Canceled)
	}
	assertRollbackTxCalls(t, tx, rollbackTxCalls{rollback: 1})
}

func TestWithRollbackJoinsContextAndRollbackErrors(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	rollbackErr := errors.New("rollback failed")
	tx := &rollbackTxFake{rollbackErr: rollbackErr}

	err := WithRollback(ctx, func(context.Context) (Tx, error) {
		return tx, nil
	}, func(context.Context, Tx) error {
		cancel()
		return nil
	})

	if !errors.Is(err, context.Canceled) {
		t.Fatalf("WithRollback error = %v, want context cancellation", err)
	}
	if !errors.Is(err, rollbackErr) {
		t.Fatalf("WithRollback error = %v, want rollback error", err)
	}
	if got, want := err.Error(), "context canceled\nrollback failed"; got != want {
		t.Fatalf("WithRollback error string = %q, want %q", got, want)
	}
	assertRollbackTxCalls(t, tx, rollbackTxCalls{rollback: 1})
}

func TestWithRollbackRejectsNilInputs(t *testing.T) {
	ctx := context.Background()

	if err := WithRollback(ctx, nil, func(context.Context, Tx) error { return nil }); !errors.Is(err, errNilRollbackBeginFunc) {
		t.Fatalf("nil beginFunc error = %v, want %v", err, errNilRollbackBeginFunc)
	}
	if err := WithRollback(ctx, func(context.Context) (Tx, error) { return &rollbackTxFake{}, nil }, nil); !errors.Is(err, errNilRollbackFunc) {
		t.Fatalf("nil fn error = %v, want %v", err, errNilRollbackFunc)
	}
	if err := WithRollback(ctx, func(context.Context) (Tx, error) { return nil, nil }, func(context.Context, Tx) error { return nil }); !errors.Is(err, errNilRollbackTx) {
		t.Fatalf("nil tx error = %v, want %v", err, errNilRollbackTx)
	}
}

type rollbackTestContextKey struct{}

type rollbackTxCalls struct {
	commit   int
	rollback int
}

type rollbackTxFake struct {
	rollbackErr     error
	commitCalls     int
	rollbackCalls   int
	commitContext   context.Context
	rollbackContext context.Context
}

func (tx *rollbackTxFake) Commit(ctx context.Context) error {
	tx.commitCalls++
	tx.commitContext = ctx
	return errors.New("commit should not be called")
}

func (tx *rollbackTxFake) Rollback(ctx context.Context) error {
	tx.rollbackCalls++
	tx.rollbackContext = ctx
	return tx.rollbackErr
}

func assertRollbackTxCalls(t *testing.T, tx *rollbackTxFake, want rollbackTxCalls) {
	t.Helper()
	got := rollbackTxCalls{commit: tx.commitCalls, rollback: tx.rollbackCalls}
	if got != want {
		t.Fatalf("tx calls = %+v, want %+v", got, want)
	}
}
