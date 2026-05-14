package lazuli

import (
	"context"
	"testing"

	"github.com/jackc/pgx/v5"
)

// TestFromFnRegistrationAndResolution verifies that:
//  1. RegisterFn stores the handler under the given name.
//  2. FromFn builds a sourceFn Source whose name and args are recorded.
//  3. resolveFn calls the handler with the tx from context and resolved args.
func TestFromFnRegistrationAndResolution(t *testing.T) {
	const fnName = "test_fn_laz48"

	// Guard against leftover state if the test is run more than once in the
	// same process (e.g. -count=2).
	fnRegistry.Lock()
	delete(fnRegistry.m, fnName)
	fnRegistry.Unlock()

	var gotTx   pgx.Tx
	var gotArgs []any

	RegisterFn(fnName, func(ctx context.Context, tx pgx.Tx, args []any) (any, error) {
		gotTx = tx
		gotArgs = args
		return int64(42), nil
	})

	// Inject a sentinel tx via the context helper.
	ctx := &Ctx{Context: context.Background()}
	sentinelTx := &fakeTx{}
	ctx = withTxContext(ctx, sentinelTx)

	type input struct{ ItemId int64 } //nolint:revive // matches pascalCase("item_id")
	src := FromFn(fnName, FromInput("item_id"))

	val, err := resolveSource(ctx, src, input{ItemId: 7})
	if err != nil {
		t.Fatalf("resolveSource: %v", err)
	}
	if val != int64(42) {
		t.Errorf("expected 42, got %v", val)
	}
	if gotTx != sentinelTx {
		t.Error("handler did not receive the sentinel tx from context")
	}
	if len(gotArgs) != 1 || gotArgs[0] != int64(7) {
		t.Errorf("handler received wrong args: %v", gotArgs)
	}
}

// TestFromFnPanicOnDuplicate ensures RegisterFn panics on double registration.
func TestFromFnPanicOnDuplicate(t *testing.T) {
	const fnName = "test_fn_dup_laz48"

	fnRegistry.Lock()
	delete(fnRegistry.m, fnName)
	fnRegistry.Unlock()

	RegisterFn(fnName, func(_ context.Context, _ pgx.Tx, _ []any) (any, error) { return nil, nil })
	defer func() {
		if r := recover(); r == nil {
			t.Error("expected panic on duplicate RegisterFn, got none")
		}
	}()
	RegisterFn(fnName, func(_ context.Context, _ pgx.Tx, _ []any) (any, error) { return nil, nil })
}

// TestFromFnUnregistered verifies a 500 error when the fn name is unknown.
func TestFromFnUnregistered(t *testing.T) {
	ctx := &Ctx{Context: context.Background()}
	src := FromFn("definitely_not_registered_laz48")
	_, err := resolveSource(ctx, src, struct{}{})
	if err == nil {
		t.Fatal("expected error for unregistered fn, got nil")
	}
	lazErr, ok := err.(*Error)
	if !ok || lazErr.Status != 500 {
		t.Errorf("expected 500 Error, got %T %v", err, err)
	}
}

// TestWithTxContext checks that txFromContext round-trips the injected tx.
func TestWithTxContext(t *testing.T) {
	ctx := &Ctx{Context: context.Background()}
	if txFromContext(ctx) != nil {
		t.Error("expected nil tx before injection")
	}
	sentinel := &fakeTx{}
	ctx2 := withTxContext(ctx, sentinel)
	if txFromContext(ctx2) != sentinel {
		t.Error("txFromContext did not return injected sentinel")
	}
	// Original ctx must remain unaffected.
	if txFromContext(ctx) != nil {
		t.Error("withTxContext mutated original ctx")
	}
}

// fakeTx is a minimal pgx.Tx stub for use in unit tests. It satisfies the
// pgx.Tx interface without a real database connection.
type fakeTx struct{ pgx.Tx }
