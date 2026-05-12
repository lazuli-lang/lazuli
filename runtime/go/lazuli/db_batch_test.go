package lazuli

import (
	"context"
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestDBBatchRunExecutesSequentiallyAndPreservesResultNames(t *testing.T) {
	ctx := t.Context()
	var batch DBBatch
	batch.Query("load_user", "select * from users where id = $1", 10).
		Exec("touch_user", "update users set seen_at = now() where id = $1", 10)
	runner := &dbBatchRunnerFake{
		values: map[string]any{
			"load_user":  []string{"ada"},
			"touch_user": "updated",
		},
	}

	results, err := batch.Run(ctx, runner)

	if err != nil {
		t.Fatalf("DBBatch.Run returned error: %v", err)
	}
	assertDBBatchCalls(t, runner.calls, []DBBatchStatement{
		{Name: "load_user", Kind: DBBatchQuery, SQL: "select * from users where id = $1", Args: []any{10}},
		{Name: "touch_user", Kind: DBBatchExec, SQL: "update users set seen_at = now() where id = $1", Args: []any{10}},
	})
	assertDBBatchResults(t, results, []DBBatchResult{
		{Name: "load_user", Value: []string{"ada"}},
		{Name: "touch_user", Value: "updated"},
	})
}

func TestDBBatchRunStopsOnFirstErrorWithStatementContext(t *testing.T) {
	wantErr := errors.New("exec failed")
	var batch DBBatch
	batch.Query("first", "select 1").
		Exec("second", "insert into logs default values").
		Query("third", "select 3")
	runner := &dbBatchRunnerFake{
		values:    map[string]any{"first": "ok"},
		errByName: map[string]error{"second": wantErr},
	}

	results, err := batch.Run(t.Context(), runner)

	if !errors.Is(err, wantErr) {
		t.Fatalf("DBBatch.Run error = %v, want wrapped %v", err, wantErr)
	}
	var batchErr *DBBatchError
	if !errors.As(err, &batchErr) {
		t.Fatalf("DBBatch.Run error = %T, want *DBBatchError", err)
	}
	if batchErr.Name != "second" || batchErr.Index != 1 || batchErr.Kind != DBBatchExec {
		t.Fatalf("DBBatchError = %#v, want second exec at index 1", batchErr)
	}
	if !strings.Contains(err.Error(), `"second"`) || !strings.Contains(err.Error(), "index 1") {
		t.Fatalf("DBBatch.Run error %q does not include statement context", err.Error())
	}
	assertDBBatchCalls(t, runner.calls, []DBBatchStatement{
		{Name: "first", Kind: DBBatchQuery, SQL: "select 1"},
		{Name: "second", Kind: DBBatchExec, SQL: "insert into logs default values"},
	})
	assertDBBatchResults(t, results, []DBBatchResult{{Name: "first", Value: "ok"}})
}

func TestDBBatchRunStopsWhenContextIsCanceled(t *testing.T) {
	ctx, cancel := context.WithCancel(t.Context())
	var batch DBBatch
	batch.Query("first", "select 1").
		Query("second", "select 2")
	runner := DBBatchRunnerFunc(func(context.Context, DBBatchStatement) (any, error) {
		cancel()
		return "ok", nil
	})

	results, err := batch.Run(ctx, runner)

	if !errors.Is(err, context.Canceled) {
		t.Fatalf("DBBatch.Run error = %v, want context.Canceled", err)
	}
	var batchErr *DBBatchError
	if !errors.As(err, &batchErr) {
		t.Fatalf("DBBatch.Run error = %T, want *DBBatchError", err)
	}
	if batchErr.Name != "second" || batchErr.Index != 1 {
		t.Fatalf("DBBatchError = %#v, want second at index 1", batchErr)
	}
	assertDBBatchResults(t, results, []DBBatchResult{{Name: "first", Value: "ok"}})
}

func TestDBBatchRunValidatesRunnerAndStatements(t *testing.T) {
	valid := DBBatchStatement{Name: "valid", Kind: DBBatchExec, SQL: "delete from logs"}

	if _, err := RunDBBatch(t.Context(), nil, valid); !errors.Is(err, errNilDBBatchRunner) {
		t.Fatalf("RunDBBatch nil runner error = %v, want %v", err, errNilDBBatchRunner)
	}

	var nilFunc DBBatchRunnerFunc
	if _, err := RunDBBatch(t.Context(), nilFunc, valid); !errors.Is(err, errNilDBBatchRunnerFunc) {
		t.Fatalf("RunDBBatch nil runner func error = %v, want %v", err, errNilDBBatchRunnerFunc)
	}

	neverCalled := DBBatchRunnerFunc(func(context.Context, DBBatchStatement) (any, error) {
		t.Fatal("runner should not be called for invalid statement")
		return nil, nil
	})
	if _, err := RunDBBatch(t.Context(), neverCalled, DBBatchStatement{Kind: DBBatchExec}); !errors.Is(err, errDBBatchNameRequired) {
		t.Fatalf("RunDBBatch missing name error = %v, want %v", err, errDBBatchNameRequired)
	}
	if _, err := RunDBBatch(t.Context(), neverCalled, DBBatchStatement{Name: "bad", Kind: DBBatchStatementKind(99)}); !errors.Is(err, errInvalidDBBatchStmtKind) {
		t.Fatalf("RunDBBatch invalid kind error = %v, want %v", err, errInvalidDBBatchStmtKind)
	}
}

func TestDBBatchCopiesStatementArgs(t *testing.T) {
	args := []any{"original"}
	batch := NewDBBatch(DBBatchStatement{
		Name: "copy_args",
		Kind: DBBatchExec,
		SQL:  "insert into audit(message) values ($1)",
		Args: args,
	})
	args[0] = "changed"

	statements := batch.Statements()
	if got := statements[0].Args[0]; got != "original" {
		t.Fatalf("Statements()[0].Args[0] = %q, want original", got)
	}
	statements[0].Args[0] = "changed again"
	if got := batch.Statements()[0].Args[0]; got != "original" {
		t.Fatalf("DBBatch retained mutated statement arg = %q, want original", got)
	}

	runner := &dbBatchRunnerFake{
		values:     map[string]any{"copy_args": "ok"},
		mutateArgs: true,
	}
	if _, err := batch.Run(t.Context(), runner); err != nil {
		t.Fatalf("DBBatch.Run returned error: %v", err)
	}
	if got := batch.Statements()[0].Args[0]; got != "original" {
		t.Fatalf("runner mutation changed batch arg = %q, want original", got)
	}
}

type dbBatchRunnerFake struct {
	calls      []DBBatchStatement
	values     map[string]any
	errByName  map[string]error
	mutateArgs bool
}

func (r *dbBatchRunnerFake) RunDBBatchStatement(_ context.Context, stmt DBBatchStatement) (any, error) {
	r.calls = append(r.calls, cloneDBBatchStatement(stmt))
	if r.mutateArgs && len(stmt.Args) > 0 {
		stmt.Args[0] = "mutated"
	}
	if err := r.errByName[stmt.Name]; err != nil {
		return nil, err
	}
	return r.values[stmt.Name], nil
}

func assertDBBatchCalls(t *testing.T, got, want []DBBatchStatement) {
	t.Helper()
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("calls = %#v, want %#v", got, want)
	}
}

func assertDBBatchResults(t *testing.T, got, want []DBBatchResult) {
	t.Helper()
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("results = %#v, want %#v", got, want)
	}
}
