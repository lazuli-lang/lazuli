package lazuli

import (
	"context"
	"encoding/json"
	"testing"
)

// TestQuerySQLDispatchesToRunSQL proves the dispatcher routes `query.sql`
// into RunSQL (the shared SQL execution path) rather than returning the
// historical 501 "not yet implemented" stub. We give the query a policy
// the anonymous actor cannot satisfy, so RunSQL's EvalPolicy gate rejects
// with a 403 BEFORE touching the DB pool — the 403 is positive proof we
// reached RunSQL (a 501 would mean we never left dispatch's stub arm).
func TestQuerySQLDispatchesToRunSQL(t *testing.T) {
	type args struct {
		OrgID int64 `json:"org_id"`
	}
	type row struct {
		ID int64 `json:"id"`
	}

	q := &Query[args, row]{
		Name: "dashboard.recent_jobs",
		Kind: QuerySQL,
		// Atom the anonymous actor never matches -> RunSQL.EvalPolicy 403.
		Policy:  Policy{Name: "@policy.view", Atoms: []PolicyAtom{{Namespace: "role", Name: "admin"}}},
		SQLText: "SELECT id FROM job WHERE org_id = $1",
		SQLMany: true,
		SQLArgs: func(a args) []any { return []any{a.OrgID} },
	}

	_, err := q.dispatch(&Ctx{Context: context.Background(), Actor: ActorAnonymous}, json.RawMessage(`{"org_id":1}`))
	if err == nil {
		t.Fatal("dispatch error = nil, want policy-denied from RunSQL")
	}
	le, ok := err.(*Error)
	if !ok {
		t.Fatalf("error type = %T, want *lazuli.Error", err)
	}
	if le.Status == 501 {
		t.Fatalf("dispatch returned 501 stub; QuerySQL must route to RunSQL. err = %v", le)
	}
	if le.Status != 403 {
		t.Fatalf("status = %d, want 403 (policy denied inside RunSQL). err = %v", le.Status, le)
	}
}

// TestQuerySQLRunSQLGuardAdmitsQuerySQL is a belt-and-suspenders check that
// RunSQL itself accepts the QuerySQL kind (the guard once admitted only
// QueryView).
func TestQuerySQLRunSQLGuardAdmitsQuerySQL(t *testing.T) {
	type args struct{}
	type row struct{ ID int64 }

	q := &Query[args, row]{
		Name:    "x.opaque",
		Kind:    QuerySQL,
		Policy:  Policy{Name: "@policy.view", Atoms: []PolicyAtom{{Namespace: "role", Name: "admin"}}},
		SQLText: "SELECT 1 AS id",
		SQLMany: true,
	}

	_, err := q.RunSQL(&Ctx{Context: context.Background(), Actor: ActorAnonymous}, args{})
	le, ok := err.(*Error)
	if !ok {
		t.Fatalf("error type = %T, want *lazuli.Error", err)
	}
	// Must be the policy gate (403), NOT the "RunSQL called on non-SQL
	// query" 500 guard.
	if le.Status == 500 {
		t.Fatalf("RunSQL rejected QuerySQL kind with 500 guard; err = %v", le)
	}
	if le.Status != 403 {
		t.Fatalf("status = %d, want 403; err = %v", le.Status, le)
	}
}
