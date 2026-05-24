package lazuli

import (
	"context"
	"errors"
	"strings"
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
)

func TestUpdateBuilder_NoopWhenNoSet(t *testing.T) {
	b := NewUpdate("property").Where("id = $1", int64(1))
	if !b.IsNoop() {
		t.Error("expected IsNoop true with no SET fragments")
	}
	sql, args := b.SQL()
	if sql != "" {
		t.Errorf("expected empty SQL, got %q", sql)
	}
	if args != nil {
		t.Errorf("expected nil args, got %v", args)
	}
}

func TestUpdateBuilder_ComposesSQL(t *testing.T) {
	name := "New Name"
	city := "Salvador"
	b := NewUpdate("property").
		Where("id = $1 AND org_id = $2", int64(42), int64(7)).
		SetIfNotNilString("name", &name).
		SetIfNotNilString("city", &city)

	sql, args := b.SQL()
	wantPrefix := `UPDATE "property" SET name = $3, city = $4 WHERE id = $1 AND org_id = $2`
	if sql != wantPrefix {
		t.Errorf("sql:\n  got  %q\n  want %q", sql, wantPrefix)
	}
	if len(args) != 4 {
		t.Fatalf("expected 4 args, got %d: %v", len(args), args)
	}
	if args[0].(int64) != 42 || args[1].(int64) != 7 {
		t.Errorf("where args: %v", args[:2])
	}
	if args[2].(string) != name || args[3].(string) != city {
		t.Errorf("set args: %v", args[2:])
	}
}

func TestUpdateBuilder_NilPointerSkipped(t *testing.T) {
	var nilName *string
	city := "Recife"
	b := NewUpdate("property").
		Where("id = $1", int64(1)).
		SetIfNotNilString("name", nilName).
		SetIfNotNilString("city", &city)

	sql, args := b.SQL()
	if strings.Contains(sql, "name") {
		t.Errorf("expected name skipped, got %q", sql)
	}
	if !strings.Contains(sql, "city = $2") {
		t.Errorf("expected city = $2 (after WHERE $1), got %q", sql)
	}
	if len(args) != 2 {
		t.Fatalf("expected 2 args, got %d", len(args))
	}
}

func TestUpdateBuilder_SetIfNotNilGeneric(t *testing.T) {
	v := 99
	b := NewUpdate("property").
		Where("id = $1", int64(1)).
		SetIfNotNil("count", &v)
	sql, args := b.SQL()
	if !strings.Contains(sql, "count = $2") {
		t.Errorf("expected count fragment, got %q", sql)
	}
	if len(args) != 2 {
		t.Fatalf("expected 2 args, got %d", len(args))
	}
	if args[1].(int) != 99 {
		t.Errorf("expected dereferenced 99, got %v", args[1])
	}
}

func TestUpdateBuilder_SetIfNotNilGeneric_NilSkipped(t *testing.T) {
	var nilPtr *int
	b := NewUpdate("property").
		Where("id = $1", int64(1)).
		SetIfNotNil("count", nilPtr)
	if !b.IsNoop() {
		t.Error("typed nil should be detected and produce no SET")
	}
}

func TestUpdateBuilder_SetRawAndReturning(t *testing.T) {
	name := "X"
	b := NewUpdate("property").
		Where("id = $1", int64(1)).
		SetIfNotNilString("name", &name).
		SetRaw("updated_at = now()").
		Returning("id, updated_at")
	sql, _ := b.SQL()
	if !strings.Contains(sql, "name = $2") {
		t.Errorf("expected name fragment, got %q", sql)
	}
	if !strings.Contains(sql, "updated_at = now()") {
		t.Errorf("expected raw fragment, got %q", sql)
	}
	if !strings.HasSuffix(sql, "RETURNING id, updated_at") {
		t.Errorf("expected returning suffix, got %q", sql)
	}
}

// fakeExecer captures calls to Exec for assertion.
type fakeExecer struct {
	sql  string
	args []any
}

func (f *fakeExecer) Exec(_ context.Context, sql string, args ...any) (pgconn.CommandTag, error) {
	f.sql = sql
	f.args = args
	return pgconn.NewCommandTag("UPDATE 1"), nil
}

func TestUpdateBuilder_ExecNoopReturnsSentinel(t *testing.T) {
	b := NewUpdate("property").Where("id = $1", int64(1))
	_, err := b.Exec(context.Background(), &fakeExecer{})
	if !errors.Is(err, ErrUpdateNoChanges) {
		t.Errorf("expected ErrUpdateNoChanges, got %v", err)
	}
}

func TestUpdateBuilder_ExecRunsThroughExecer(t *testing.T) {
	name := "Y"
	fake := &fakeExecer{}
	b := NewUpdate("property").
		Where("id = $1", int64(7)).
		SetIfNotNilString("name", &name)
	tag, err := b.Exec(context.Background(), fake)
	if err != nil {
		t.Fatalf("unexpected err: %v", err)
	}
	if tag.String() != "UPDATE 1" {
		t.Errorf("expected tag 'UPDATE 1', got %q", tag.String())
	}
	if !strings.Contains(fake.sql, "name = $2") {
		t.Errorf("sql: %q", fake.sql)
	}
	if len(fake.args) != 2 || fake.args[1].(string) != name {
		t.Errorf("args: %v", fake.args)
	}
}

// Compile-time interface satisfaction — pgxpool.Pool, pgx.Conn,
// pgx.Tx all satisfy the minimal updateExecer/updateQuerier shapes.
// The static assertion catches drift if pgx ever changes signatures.
var (
	_ updateExecer  = (*pgx.Conn)(nil)
	_ updateQuerier = (*pgx.Conn)(nil)
)
