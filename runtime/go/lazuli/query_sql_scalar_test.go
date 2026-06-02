package lazuli

import (
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
)

// fakeRows is a minimal in-memory pgx.Rows used to exercise the row
// collectors that RunSQL selects between (pgx.RowTo for scalars,
// pgx.RowToStructByName for structs) without a live Postgres pool.
//
// It supports the surface the collectors touch: Next / FieldDescriptions /
// Scan / Err / Close. Each row is a []any of column values; columns names the
// field descriptions (used by RowToStructByName for by-name mapping).
type fakeRows struct {
	columns []string
	data    [][]any
	pos     int // 0 = before first row
	closed  bool
}

func (r *fakeRows) Close()                        { r.closed = true }
func (r *fakeRows) Err() error                    { return nil }
func (r *fakeRows) CommandTag() pgconn.CommandTag { return pgconn.CommandTag{} }
func (r *fakeRows) Conn() *pgx.Conn               { return nil }

func (r *fakeRows) FieldDescriptions() []pgconn.FieldDescription {
	fds := make([]pgconn.FieldDescription, len(r.columns))
	for i, name := range r.columns {
		fds[i] = pgconn.FieldDescription{Name: name}
	}
	return fds
}

func (r *fakeRows) Next() bool {
	if r.pos >= len(r.data) {
		return false
	}
	r.pos++
	return true
}

func (r *fakeRows) Scan(dest ...any) error {
	row := r.data[r.pos-1]
	for i := range dest {
		switch d := dest[i].(type) {
		case *int64:
			*d = row[i].(int64)
		case *string:
			*d = row[i].(string)
		case *bool:
			*d = row[i].(bool)
		case *any:
			*d = row[i]
		default:
			// pgx.RowToStructByName passes *struct-field pointers; route
			// by concrete element type for the shapes this test uses.
			if p, ok := dest[i].(*int64); ok {
				*p = row[i].(int64)
			}
		}
	}
	return nil
}

func (r *fakeRows) Values() ([]any, error) { return r.data[r.pos-1], nil }
func (r *fakeRows) RawValues() [][]byte    { return nil }

// TestSQLReturnIsScalar pins the scalar-vs-struct classifier RunSQL uses to
// pick its row collector. Scalars (numbers/strings/bools, pointer-wrapped or
// not, plus time.Time) must classify as scalar; record/resource structs must
// not.
func TestSQLReturnIsScalar(t *testing.T) {
	if !sqlReturnIsScalar[int64]() {
		t.Error("int64 should be scalar")
	}
	if !sqlReturnIsScalar[string]() {
		t.Error("string should be scalar")
	}
	if !sqlReturnIsScalar[bool]() {
		t.Error("bool should be scalar")
	}
	if !sqlReturnIsScalar[*int64]() {
		t.Error("*int64 should be scalar")
	}
	type row struct {
		ID   int64  `db:"id"`
		Name string `db:"name"`
	}
	if sqlReturnIsScalar[row]() {
		t.Error("struct row should NOT be scalar")
	}
}

// TestScalarRowCollectorNoPanic is the regression for W4-2
// QUERY-SQL-SCALAR-PANIC: a single-column int64 result must scan via
// pgx.RowTo without the `reflect: NumField of non-struct type` panic that
// pgx.RowToStructByName raises on a non-struct destination.
func TestScalarRowCollectorNoPanic(t *testing.T) {
	// BEFORE: this is what RunSQL did unconditionally and it panicked.
	t.Run("RowToStructByName_panics_on_scalar", func(t *testing.T) {
		defer func() {
			if recover() == nil {
				t.Fatal("expected panic from RowToStructByName on scalar dest")
			}
		}()
		rows := &fakeRows{columns: []string{"count"}, data: [][]any{{int64(42)}}}
		_, _ = pgx.CollectOneRow(rows, pgx.RowToStructByName[int64])
	})

	// AFTER: the scalar collector RunSQL now selects scans cleanly.
	t.Run("RowTo_single_scalar", func(t *testing.T) {
		rows := &fakeRows{columns: []string{"count"}, data: [][]any{{int64(42)}}}
		got, err := pgx.CollectOneRow(rows, pgx.RowTo[int64])
		if err != nil {
			t.Fatalf("CollectOneRow err = %v", err)
		}
		if got != 42 {
			t.Fatalf("got %d, want 42", got)
		}
	})

	// list-of-scalar (query.sql ... returns int64[] / SQLMany=true).
	t.Run("RowTo_many_scalars", func(t *testing.T) {
		rows := &fakeRows{
			columns: []string{"id"},
			data:    [][]any{{int64(1)}, {int64(2)}, {int64(3)}},
		}
		got, err := pgx.CollectRows(rows, pgx.RowTo[int64])
		if err != nil {
			t.Fatalf("CollectRows err = %v", err)
		}
		if len(got) != 3 || got[0] != 1 || got[2] != 3 {
			t.Fatalf("got %v, want [1 2 3]", got)
		}
	})
}

// TestStructRowCollectorStillWorks proves the struct path RunSQL preserves is
// unchanged: a multi-column result maps by column name into a struct.
func TestStructRowCollectorStillWorks(t *testing.T) {
	type jobRow struct {
		ID   int64  `db:"id"`
		Name string `db:"name"`
	}
	rows := &fakeRows{
		columns: []string{"id", "name"},
		data:    [][]any{{int64(7), "alpha"}},
	}
	got, err := pgx.CollectOneRow(rows, pgx.RowToStructByName[jobRow])
	if err != nil {
		t.Fatalf("CollectOneRow err = %v", err)
	}
	if got.ID != 7 || got.Name != "alpha" {
		t.Fatalf("got %+v, want {7 alpha}", got)
	}
}
