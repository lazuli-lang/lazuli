package lazuli

import (
	"context"
	"errors"
	"fmt"
)

var (
	errNilDBBatchRunner       = errors.New("lazuli: nil db batch runner")
	errNilDBBatchRunnerFunc   = errors.New("lazuli: nil db batch runner function")
	errDBBatchNameRequired    = errors.New("lazuli: db batch statement Name is required")
	errInvalidDBBatchStmtKind = errors.New("lazuli: invalid db batch statement kind")
)

// DBBatchStatementKind names which database operation a DBBatchStatement runs.
type DBBatchStatementKind int

const (
	// DBBatchExec runs a statement that does not return rows.
	DBBatchExec DBBatchStatementKind = iota
	// DBBatchQuery runs a statement that returns rows or a query-shaped value.
	DBBatchQuery
)

func (k DBBatchStatementKind) String() string {
	switch k {
	case DBBatchExec:
		return "exec"
	case DBBatchQuery:
		return "query"
	default:
		return fmt.Sprintf("unknown(%d)", k)
	}
}

// DBBatchStatement describes one named SQL statement in a generated batch.
type DBBatchStatement struct {
	// Name identifies the statement in DBBatchResult and DBBatchError.
	Name string
	// Kind tells the runner which operation should execute the statement.
	Kind DBBatchStatementKind
	// SQL is the statement text passed to the runner.
	SQL string
	// Args are the bound arguments passed to the runner.
	Args []any
}

// DBBatchResult records one completed statement result.
type DBBatchResult struct {
	// Name is copied from the statement that produced Value.
	Name string
	// Value is the adapter-specific execution or query result.
	Value any
}

// DBBatchError wraps the first statement error with batch location details.
type DBBatchError struct {
	// Name is the failed statement name.
	Name string
	// Index is the failed statement's zero-based position in the batch.
	Index int
	// Kind is the failed statement kind.
	Kind DBBatchStatementKind
	// Err is the wrapped validation or runner error.
	Err error
}

func (e *DBBatchError) Error() string {
	if e == nil {
		return "<nil>"
	}
	if e.Name == "" {
		return fmt.Sprintf("lazuli: db batch %s statement at index %d failed: %v", e.Kind, e.Index, e.Err)
	}
	return fmt.Sprintf("lazuli: db batch %s statement %q at index %d failed: %v", e.Kind, e.Name, e.Index, e.Err)
}

// Unwrap returns the underlying validation or runner error.
func (e *DBBatchError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Err
}

// DBBatchRunner is the minimal adapter-neutral statement execution contract.
//
// Generated repositories can implement it directly or use DBBatchRunnerFunc to
// wrap pgx, database/sql, sqlc, or test fakes without this package depending on
// driver-specific result types.
type DBBatchRunner interface {
	RunDBBatchStatement(context.Context, DBBatchStatement) (any, error)
}

// DBBatchRunnerFunc adapts a function into a DBBatchRunner.
type DBBatchRunnerFunc func(context.Context, DBBatchStatement) (any, error)

// RunDBBatchStatement runs one batch statement.
func (f DBBatchRunnerFunc) RunDBBatchStatement(ctx context.Context, stmt DBBatchStatement) (any, error) {
	if f == nil {
		return nil, errNilDBBatchRunnerFunc
	}
	return f(ctx, stmt)
}

// DBBatch collects named statements and runs them sequentially.
//
// The zero value is ready to use.
type DBBatch struct {
	statements []DBBatchStatement
}

// NewDBBatch returns a batch initialized with statements.
func NewDBBatch(statements ...DBBatchStatement) DBBatch {
	var batch DBBatch
	for _, stmt := range statements {
		batch.Add(stmt)
	}
	return batch
}

// Add appends a statement to the batch.
func (b *DBBatch) Add(stmt DBBatchStatement) {
	b.statements = append(b.statements, cloneDBBatchStatement(stmt))
}

// Exec appends an exec statement to the batch and returns the batch.
func (b *DBBatch) Exec(name, sql string, args ...any) *DBBatch {
	b.Add(DBBatchStatement{
		Name: name,
		Kind: DBBatchExec,
		SQL:  sql,
		Args: args,
	})
	return b
}

// Query appends a query statement to the batch and returns the batch.
func (b *DBBatch) Query(name, sql string, args ...any) *DBBatch {
	b.Add(DBBatchStatement{
		Name: name,
		Kind: DBBatchQuery,
		SQL:  sql,
		Args: args,
	})
	return b
}

// Statements returns a copy of the collected statements.
func (b DBBatch) Statements() []DBBatchStatement {
	statements := make([]DBBatchStatement, len(b.statements))
	for i, stmt := range b.statements {
		statements[i] = cloneDBBatchStatement(stmt)
	}
	return statements
}

// Run executes the batch sequentially through runner.
//
// Results preserve statement names and input order. If a statement fails, Run
// returns results completed before that statement and a DBBatchError wrapping
// the failure; later statements are not executed.
func (b DBBatch) Run(ctx context.Context, runner DBBatchRunner) ([]DBBatchResult, error) {
	return RunDBBatch(ctx, runner, b.statements...)
}

// RunDBBatch executes statements sequentially through runner.
func RunDBBatch(ctx context.Context, runner DBBatchRunner, statements ...DBBatchStatement) ([]DBBatchResult, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	if runner == nil {
		return nil, errNilDBBatchRunner
	}

	results := make([]DBBatchResult, 0, len(statements))
	for i, stmt := range statements {
		stmt = cloneDBBatchStatement(stmt)
		if err := validateDBBatchStatement(stmt); err != nil {
			return results, &DBBatchError{Name: stmt.Name, Index: i, Kind: stmt.Kind, Err: err}
		}
		if err := ctx.Err(); err != nil {
			return results, &DBBatchError{Name: stmt.Name, Index: i, Kind: stmt.Kind, Err: err}
		}

		value, err := runner.RunDBBatchStatement(ctx, stmt)
		if err != nil {
			return results, &DBBatchError{Name: stmt.Name, Index: i, Kind: stmt.Kind, Err: err}
		}
		results = append(results, DBBatchResult{Name: stmt.Name, Value: value})
	}
	return results, nil
}

func validateDBBatchStatement(stmt DBBatchStatement) error {
	if stmt.Name == "" {
		return errDBBatchNameRequired
	}
	switch stmt.Kind {
	case DBBatchExec, DBBatchQuery:
		return nil
	default:
		return errInvalidDBBatchStmtKind
	}
}

func cloneDBBatchStatement(stmt DBBatchStatement) DBBatchStatement {
	if len(stmt.Args) == 0 {
		stmt.Args = nil
		return stmt
	}
	args := make([]any, len(stmt.Args))
	copy(args, stmt.Args)
	stmt.Args = args
	return stmt
}
