package lazuli

import (
	"strings"
	"testing"
)

func TestBuildFTSQueryTenantScoped(t *testing.T) {
	ctx := &Ctx{Tenant: &Tenant{OrgID: 42}}
	sql, args, err := buildFTSQuery(ctx, "slugs", "hello world")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !strings.Contains(sql, `"slugs"`) {
		t.Errorf("expected quoted table name, got: %s", sql)
	}
	if !strings.Contains(sql, "org_id = $1") {
		t.Errorf("expected org_id clause at $1, got: %s", sql)
	}
	if !strings.Contains(sql, "fts @@ websearch_to_tsquery('simple', $2)") {
		t.Errorf("expected FTS clause at $2, got: %s", sql)
	}
	if !strings.Contains(sql, "ts_rank(fts, websearch_to_tsquery('simple', $2)) DESC") {
		t.Errorf("expected ts_rank order at $2, got: %s", sql)
	}
	if len(args) != 2 {
		t.Fatalf("expected 2 args, got %d", len(args))
	}
	if args[0] != ID(42) {
		t.Errorf("args[0] = %v, want 42", args[0])
	}
	if args[1] != "hello world" {
		t.Errorf("args[1] = %v, want 'hello world'", args[1])
	}
}

func TestBuildFTSQueryNoTenant(t *testing.T) {
	ctx := &Ctx{}
	sql, args, err := buildFTSQuery(ctx, "posts", "lazuli")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if strings.Contains(sql, "org_id") {
		t.Errorf("expected no tenant scope, got: %s", sql)
	}
	if !strings.Contains(sql, "fts @@ websearch_to_tsquery('simple', $1)") {
		t.Errorf("expected FTS at $1 when no tenant, got: %s", sql)
	}
	if len(args) != 1 {
		t.Fatalf("expected 1 arg, got %d", len(args))
	}
	if args[0] != "lazuli" {
		t.Errorf("args[0] = %v, want 'lazuli'", args[0])
	}
}

func TestBuildFTSQueryValidation(t *testing.T) {
	ctx := &Ctx{Tenant: &Tenant{OrgID: 1}}

	_, _, err := buildFTSQuery(ctx, "posts", "")
	if err == nil {
		t.Fatal("expected error for empty query, got nil")
	}

	long := strings.Repeat("a", maxFTSQueryLen+1)
	_, _, err = buildFTSQuery(ctx, "posts", long)
	if err == nil {
		t.Fatal("expected error for oversized query, got nil")
	}

	atLimit := strings.Repeat("b", maxFTSQueryLen)
	_, _, err = buildFTSQuery(ctx, "posts", atLimit)
	if err != nil {
		t.Fatalf("expected no error at limit, got: %v", err)
	}
}
