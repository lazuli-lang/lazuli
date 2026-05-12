package lazuli

import (
	"errors"
	"reflect"
	"testing"
	"time"
)

func TestBuildAuditLogWhereBuildsStablePredicatesAndArgs(t *testing.T) {
	orgID := ID(10)
	actorID := ID(20)
	targetID := ID(30)
	from := time.Date(2026, 5, 10, 12, 0, 0, 0, time.UTC)
	to := time.Date(2026, 5, 11, 12, 0, 0, 0, time.UTC)

	got, err := BuildAuditLogWhere(AuditLogFilter{
		OrgID:          &orgID,
		ActorID:        &actorID,
		ActorKind:      " user ",
		CommandName:    "customer.create",
		TargetResource: "Customer",
		TargetID:       &targetID,
		ResultStatus:   "ok",
		ErrorCode:      " ",
		CorrelationID:  "req-123",
		CreatedAtFrom:  from,
		CreatedAtTo:    to,
	}, 3)
	if err != nil {
		t.Fatalf("BuildAuditLogWhere returned error: %v", err)
	}

	wantSQL := `"org_id" = $3 AND "actor_id" = $4 AND "actor_kind" = $5 AND "command_name" = $6 AND "target_resource" = $7 AND "target_id" = $8 AND "result_status" = $9 AND "correlation_id" = $10 AND "created_at" >= $11 AND "created_at" < $12`
	if got.SQL != wantSQL {
		t.Fatalf("SQL = %q, want %q", got.SQL, wantSQL)
	}
	wantArgs := []any{ID(10), ID(20), "user", "customer.create", "Customer", ID(30), "ok", "req-123", from, to}
	if !reflect.DeepEqual(got.Args, wantArgs) {
		t.Fatalf("Args = %#v, want %#v", got.Args, wantArgs)
	}
}

func TestBuildAuditLogWhereEmptyFilterReturnsEmptyFragment(t *testing.T) {
	got, err := BuildAuditLogWhere(AuditLogFilter{}, 0)
	if err != nil {
		t.Fatalf("BuildAuditLogWhere returned error: %v", err)
	}
	if got.SQL != "" || got.Args != nil {
		t.Fatalf("fragment = %#v, want empty", got)
	}
}

func TestBuildAuditLogWhereRejectsInvalidPlaceholderWhenFiltersArePresent(t *testing.T) {
	orgID := ID(10)
	_, err := BuildAuditLogWhere(AuditLogFilter{OrgID: &orgID}, 0)
	if !errors.Is(err, errInvalidAuditLogPlaceholder) {
		t.Fatalf("BuildAuditLogWhere error = %v, want %v", err, errInvalidAuditLogPlaceholder)
	}
}

func TestBuildAuditLogWhereRejectsInvalidTimeRange(t *testing.T) {
	from := time.Date(2026, 5, 12, 0, 0, 0, 0, time.UTC)
	to := time.Date(2026, 5, 11, 0, 0, 0, 0, time.UTC)

	_, err := BuildAuditLogWhere(AuditLogFilter{CreatedAtFrom: from, CreatedAtTo: to}, 1)
	if !errors.Is(err, errInvalidAuditLogTimeRange) {
		t.Fatalf("BuildAuditLogWhere error = %v, want %v", err, errInvalidAuditLogTimeRange)
	}
}

func TestBuildAuditLogOrderByDefaultsToStableNewestFirst(t *testing.T) {
	got, err := BuildAuditLogOrderBy(nil)
	if err != nil {
		t.Fatalf("BuildAuditLogOrderBy returned error: %v", err)
	}

	want := `ORDER BY "created_at" DESC, "id" DESC`
	if got != want {
		t.Fatalf("order = %q, want %q", got, want)
	}
}

func TestBuildAuditLogOrderByAppendsStableIDTieBreaker(t *testing.T) {
	got, err := BuildAuditLogOrderBy([]AuditLogOrder{
		{Column: AuditLogOrderCreatedAt},
	})
	if err != nil {
		t.Fatalf("BuildAuditLogOrderBy returned error: %v", err)
	}

	want := `ORDER BY "created_at" ASC, "id" ASC`
	if got != want {
		t.Fatalf("order = %q, want %q", got, want)
	}
}

func TestBuildAuditLogOrderByRejectsInvalidOrder(t *testing.T) {
	tests := []struct {
		name string
		in   []AuditLogOrder
		want error
	}{
		{
			name: "invalid column",
			in: []AuditLogOrder{
				{Column: AuditLogOrderColumn("actor_kind")},
			},
			want: errInvalidAuditLogOrderColumn,
		},
		{
			name: "duplicate column",
			in: []AuditLogOrder{
				{Column: AuditLogOrderCreatedAt},
				{Column: AuditLogOrderCreatedAt, Desc: true},
			},
			want: errDuplicateAuditLogOrder,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := BuildAuditLogOrderBy(tt.in)
			if !errors.Is(err, tt.want) {
				t.Fatalf("BuildAuditLogOrderBy error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestBuildAuditLogPaginationNormalizesAndBindsLimitOffset(t *testing.T) {
	got, err := BuildAuditLogPagination(
		PaginationInput{Limit: 500, Offset: 25},
		PaginationOptions{DefaultLimit: 50, MaxLimit: 100},
		7,
	)
	if err != nil {
		t.Fatalf("BuildAuditLogPagination returned error: %v", err)
	}

	if got.SQL != "LIMIT $7 OFFSET $8" {
		t.Fatalf("SQL = %q, want LIMIT/OFFSET placeholders", got.SQL)
	}
	if got.Page != (Page{Limit: 100, Offset: 25}) {
		t.Fatalf("Page = %#v, want clamped limit and offset", got.Page)
	}
	if !reflect.DeepEqual(got.Args, []any{100, 25}) {
		t.Fatalf("Args = %#v, want limit and offset", got.Args)
	}
}

func TestBuildAuditLogPaginationUsesCursorOffset(t *testing.T) {
	cursor, err := EncodePageCursor(PageCursor{Offset: 80})
	if err != nil {
		t.Fatalf("EncodePageCursor returned error: %v", err)
	}

	got, err := BuildAuditLogPagination(PaginationInput{Limit: 20, Offset: 40, Cursor: cursor}, PaginationOptions{}, 1)
	if err != nil {
		t.Fatalf("BuildAuditLogPagination returned error: %v", err)
	}
	if got.Page != (Page{Limit: 20, Offset: 80}) {
		t.Fatalf("Page = %#v, want cursor offset", got.Page)
	}
	if !reflect.DeepEqual(got.Args, []any{20, 80}) {
		t.Fatalf("Args = %#v, want cursor offset", got.Args)
	}
}

func TestBuildAuditLogPaginationRejectsInvalidInput(t *testing.T) {
	if _, err := BuildAuditLogPagination(PaginationInput{}, PaginationOptions{}, 0); !errors.Is(err, errInvalidAuditLogPlaceholder) {
		t.Fatalf("BuildAuditLogPagination placeholder error = %v, want %v", err, errInvalidAuditLogPlaceholder)
	}

	if _, err := BuildAuditLogPagination(PaginationInput{Limit: -1}, PaginationOptions{}, 1); !isBadRequest(err) {
		t.Fatalf("BuildAuditLogPagination negative limit error = %v, want bad request", err)
	}
}
