package lazuli

import (
	"context"
	"errors"
	"strings"
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
)

type fakeTx struct {
	pgx.Tx
	lastSQL  string
	lastArgs []any
	err      error
	calls    int
}

func (f *fakeTx) Exec(_ context.Context, sql string, args ...any) (pgconn.CommandTag, error) {
	f.calls++
	f.lastSQL = sql
	f.lastArgs = args
	return pgconn.CommandTag{}, f.err
}

func TestRecordActivityRejectsEmptyTable(t *testing.T) {
	err := RecordActivity(context.Background(), &fakeTx{}, ActivityRow{
		ParentColumn: "issue_id", ParentID: "x", Field: "title",
	})
	if err == nil {
		t.Fatal("expected error on empty Table")
	}
}

func TestRecordActivityRejectsEmptyParentColumn(t *testing.T) {
	err := RecordActivity(context.Background(), &fakeTx{}, ActivityRow{
		Table: "issue_activities", ParentID: "x", Field: "title",
	})
	if err == nil {
		t.Fatal("expected error on empty ParentColumn")
	}
}

func TestRecordActivityRejectsEmptyField(t *testing.T) {
	err := RecordActivity(context.Background(), &fakeTx{}, ActivityRow{
		Table: "issue_activities", ParentColumn: "issue_id", ParentID: "x",
	})
	if err == nil {
		t.Fatal("expected error on empty Field")
	}
}

func TestRecordActivitySkipsOnUnchangedValue(t *testing.T) {
	tx := &fakeTx{}
	err := RecordActivity(context.Background(), tx, ActivityRow{
		Table:        "issue_activities",
		ParentColumn: "issue_id",
		ParentID:     "abc",
		ActorID:      "user-1",
		Field:        "title",
		OldValue:     "same",
		NewValue:     "same",
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if tx.calls != 0 {
		t.Fatalf("expected no SQL when old==new, got %d calls", tx.calls)
	}
}

func TestRecordActivityHappyPathNoTenancy(t *testing.T) {
	tx := &fakeTx{}
	err := RecordActivity(context.Background(), tx, ActivityRow{
		Table:        "issue_activities",
		ParentColumn: "issue_id",
		ParentID:     "issue-abc",
		ActorID:      "user-42",
		Field:        "title",
		OldValue:     "old title",
		NewValue:     "new title",
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if tx.calls != 1 {
		t.Fatalf("expected 1 INSERT, got %d", tx.calls)
	}
	if tx.lastSQL == "" || !strings.Contains(tx.lastSQL, "issue_activities") {
		t.Fatalf("SQL should reference table, got: %s", tx.lastSQL)
	}
	if strings.Contains(tx.lastSQL, "tenant") {
		t.Fatalf("no tenancy column expected, got: %s", tx.lastSQL)
	}
}

func TestRecordActivityHappyPathWithTenancy(t *testing.T) {
	tx := &fakeTx{}
	err := RecordActivity(context.Background(), tx, ActivityRow{
		Table:         "issue_activities",
		ParentColumn:  "issue_id",
		ParentID:      "issue-abc",
		ActorID:       "user-42",
		Field:         "title",
		OldValue:      "old",
		NewValue:      "new",
		TenancyColumn: "tenant_id",
		TenancyID:     "tenant-xyz",
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !strings.Contains(tx.lastSQL, "tenant_id") {
		t.Fatalf("SQL should include tenancy column, got: %s", tx.lastSQL)
	}
	if len(tx.lastArgs) != 7 {
		t.Fatalf("expected 7 args with tenancy, got %d", len(tx.lastArgs))
	}
}

func TestRecordActivityHandlesNilOldValue(t *testing.T) {
	tx := &fakeTx{}
	err := RecordActivity(context.Background(), tx, ActivityRow{
		Table:        "issue_activities",
		ParentColumn: "issue_id",
		ParentID:     "abc",
		ActorID:      "user-1",
		Field:        "title",
		OldValue:     nil,
		NewValue:     "first value",
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if tx.calls != 1 {
		t.Fatalf("expected INSERT for nil->value, got %d calls", tx.calls)
	}
}

func TestRecordActivityPropagatesTxError(t *testing.T) {
	tx := &fakeTx{err: errors.New("simulated db error")}
	err := RecordActivity(context.Background(), tx, ActivityRow{
		Table:        "issue_activities",
		ParentColumn: "issue_id",
		ParentID:     "abc",
		ActorID:      "user-1",
		Field:        "title",
		OldValue:     "a",
		NewValue:     "b",
	})
	if err == nil {
		t.Fatal("expected wrapped tx error")
	}
}

