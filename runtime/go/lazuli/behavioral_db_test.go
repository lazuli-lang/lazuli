package lazuli

// Behavioral end-to-end tests that exercise the runtime against a REAL
// Postgres, closing the W3-2 gap from the overnight test-coverage audit
// (docs/audits/overnight-2026-06-02/07-test-coverage.md §2 B1/B2/B3/B4).
//
// The existing _test.go files for these four behaviors (update_builder,
// transition, binding_fn, query_sql_dispatch) assert composed SQL /
// dispatch routing against FAKES — a column-name drift, a JSONB encode
// failure, a scan-shape mismatch, or a registration drift would pass
// those green while the booted server breaks. These tests run the actual
// statements against a live schema and assert the real outcome (the row
// changed, the transition advanced + an illegal one was rejected, the
// @fn value was bound into the row, and query.sql returned the seeded
// rows respecting a tenant predicate).
//
// CI/laptop safety: if no Postgres is reachable the tests SKIP (never
// fail), so `go test ./...` stays green without a DB. Point
// LAZULI_TEST_DB at a database to make them run. Each test creates and
// drops its own uniquely-named tables, so they are isolation-safe and
// leave no residue.

import (
	"context"
	"errors"
	"fmt"
	"os"
	"sync/atomic"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// defaultTestDBURL is the local dev database discovered for this
// ecosystem. Overridable via LAZULI_TEST_DB so the same tests can be
// pointed at a CI Postgres service container.
const defaultTestDBURL = "postgres://hostpoint:hostpoint_dev_password@localhost:5432/hostpoint?sslmode=disable"

var behavioralTableSeq atomic.Uint64

// connectTestDB returns a live pool, or skips the test when no DB is
// reachable. Never fails the suite for an absent DB — that is the
// contract that keeps `go test ./...` green off a developer laptop.
func connectTestDB(t *testing.T) *pgxpool.Pool {
	t.Helper()
	url := defaultTestDBURL
	if v := os.Getenv("LAZULI_TEST_DB"); v != "" {
		url = v
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	pool, err := pgxpool.New(ctx, url)
	if err != nil {
		t.Skipf("no test DB reachable (LAZULI_TEST_DB=%q): %v", url, err)
	}
	if pingErr := pool.Ping(ctx); pingErr != nil {
		pool.Close()
		t.Skipf("test DB not answering (LAZULI_TEST_DB=%q): %v", url, pingErr)
	}
	t.Cleanup(pool.Close)
	return pool
}

// uniqueTable returns a collision-free table name for a single test so
// parallel runs and reruns never clash on a leftover table.
func uniqueTable(prefix string) string {
	return fmt.Sprintf("lzbeh_%s_%d", prefix, behavioralTableSeq.Add(1))
}

func dropTable(t *testing.T, pool *pgxpool.Pool, name string) {
	t.Helper()
	if _, err := pool.Exec(context.Background(), "DROP TABLE IF EXISTS "+name); err != nil {
		t.Logf("cleanup: drop %s: %v", name, err)
	}
}

// --- B1: update ... where, executed against a real schema ---------------

// TestUpdateBuilder_WhereExecutesAgainstRealDB proves the composed
// `UPDATE ... SET ... WHERE ...` from UpdateBuilder actually mutates the
// matching row and ONLY the matching row in Postgres. The fake-execer
// test (update_builder_test.go) only string-asserts the SQL; this catches
// a real where-arg / set-arg ordering bug (set args must bind AFTER where
// args), which a string assertion cannot prove is correct against pg's
// $N positional binding.
func TestUpdateBuilder_WhereExecutesAgainstRealDB(t *testing.T) {
	pool := connectTestDB(t)
	tbl := uniqueTable("upd")
	ctx := context.Background()

	if _, err := pool.Exec(ctx, fmt.Sprintf(
		`CREATE TABLE %s (id bigint primary key, org_id bigint not null, name text not null)`, tbl)); err != nil {
		t.Fatalf("create table: %v", err)
	}
	defer dropTable(t, pool, tbl)

	if _, err := pool.Exec(ctx,
		fmt.Sprintf(`INSERT INTO %s (id, org_id, name) VALUES (1, 10, 'alice'), (2, 10, 'bob'), (3, 20, 'carol')`, tbl)); err != nil {
		t.Fatalf("seed: %v", err)
	}

	newName := "alice-renamed"
	// Update only id=1 in org 10. Where args bind $1,$2; set arg binds $3.
	b := NewUpdate(tbl).
		Where("id = $1 AND org_id = $2", ID(1), ID(10)).
		SetIfNotNilString("name", &newName)
	if b.IsNoop() {
		t.Fatal("builder unexpectedly a noop")
	}
	tag, err := b.Exec(ctx, pool)
	if err != nil {
		t.Fatalf("Exec: %v", err)
	}
	if tag.RowsAffected() != 1 {
		t.Fatalf("RowsAffected = %d, want 1 (where-clause must scope to exactly one row)", tag.RowsAffected())
	}

	// The target row changed...
	var got string
	if err := pool.QueryRow(ctx, fmt.Sprintf(`SELECT name FROM %s WHERE id = 1`, tbl)).Scan(&got); err != nil {
		t.Fatalf("readback id=1: %v", err)
	}
	if got != newName {
		t.Fatalf("id=1 name = %q, want %q — set arg did not bind to the right column/row", got, newName)
	}
	// ...and the non-matching rows did NOT.
	var bob, carol string
	if err := pool.QueryRow(ctx, fmt.Sprintf(`SELECT name FROM %s WHERE id = 2`, tbl)).Scan(&bob); err != nil {
		t.Fatalf("readback id=2: %v", err)
	}
	if err := pool.QueryRow(ctx, fmt.Sprintf(`SELECT name FROM %s WHERE id = 3`, tbl)).Scan(&carol); err != nil {
		t.Fatalf("readback id=3: %v", err)
	}
	if bob != "bob" || carol != "carol" {
		t.Fatalf("non-matching rows changed: id2=%q id3=%q — where predicate leaked", bob, carol)
	}

	// A where that matches nothing affects zero rows (the readback/conflict
	// signal handlers rely on).
	noMatch, err := NewUpdate(tbl).
		Where("id = $1 AND org_id = $2", ID(1), ID(999)).
		SetIfNotNilString("name", &newName).
		Exec(ctx, pool)
	if err != nil {
		t.Fatalf("non-matching Exec: %v", err)
	}
	if noMatch.RowsAffected() != 0 {
		t.Fatalf("non-matching where RowsAffected = %d, want 0", noMatch.RowsAffected())
	}
}

// --- B2: lifecycle transition, persisted + illegal transition rejected --

// TestTransition_PersistsAndRejectsIllegalAgainstRealDB drives the real
// optimistic state transition: a legal transition advances and persists
// the column, and an illegal transition (wrong from-state) affects zero
// rows and surfaces the canonical 409 lifecycle-state-mismatch. The stub
// test (transition_test.go) proves the SQL string + the zero-rows mapping
// but never executes; a column drift (status vs state) or a precondition
// that pg evaluates differently than the stub would pass there and 500
// in production.
func TestTransition_PersistsAndRejectsIllegalAgainstRealDB(t *testing.T) {
	pool := connectTestDB(t)
	tbl := uniqueTable("booking")
	bgctx := context.Background()

	if _, err := pool.Exec(bgctx, fmt.Sprintf(
		`CREATE TABLE %s (id bigint primary key, status text not null)`, tbl)); err != nil {
		t.Fatalf("create table: %v", err)
	}
	defer dropTable(t, pool, tbl)
	if _, err := pool.Exec(bgctx, fmt.Sprintf(`INSERT INTO %s (id, status) VALUES (7, 'pending')`, tbl)); err != nil {
		t.Fatalf("seed: %v", err)
	}

	tx, err := pool.Begin(bgctx)
	if err != nil {
		t.Fatalf("begin: %v", err)
	}
	defer func() { _ = tx.Rollback(bgctx) }()

	lctx := &Ctx{Context: bgctx}

	// Legal transition pending -> confirmed.
	if err := Transition(lctx, tx, tbl, "status", "pending", "confirmed", "id", ID(7)); err != nil {
		t.Fatalf("legal transition: %v", err)
	}
	var status string
	if err := tx.QueryRow(bgctx, fmt.Sprintf(`SELECT status FROM %s WHERE id = 7`, tbl)).Scan(&status); err != nil {
		t.Fatalf("readback: %v", err)
	}
	if status != "confirmed" {
		t.Fatalf("status after legal transition = %q, want confirmed — transition did not persist", status)
	}

	// Illegal transition: from-state no longer matches (row is now
	// 'confirmed', not 'pending') -> zero rows -> 409 mismatch.
	illegal := Transition(lctx, tx, tbl, "status", "pending", "cancelled", "id", ID(7))
	if illegal == nil {
		t.Fatal("illegal transition returned nil, want lifecycle-state-mismatch")
	}
	var le *Error
	if !errors.As(illegal, &le) {
		t.Fatalf("illegal transition error type = %T, want *lazuli.Error", illegal)
	}
	if le.Status != 409 || le.Code != CodeLifecycleStateMismatch {
		t.Fatalf("illegal transition = status %d code %q, want 409 %q", le.Status, le.Code, CodeLifecycleStateMismatch)
	}
	// Confirm the illegal transition did NOT mutate the row.
	if err := tx.QueryRow(bgctx, fmt.Sprintf(`SELECT status FROM %s WHERE id = 7`, tbl)).Scan(&status); err != nil {
		t.Fatalf("readback after illegal: %v", err)
	}
	if status != "confirmed" {
		t.Fatalf("status after illegal transition = %q, want unchanged 'confirmed'", status)
	}
}

// --- B3: @fn binding, value actually bound into the INSERT --------------

// TestBindingFn_ResolvedValueBindsIntoRow proves a `@fn.<name>` binding
// resolves through the registry AND the produced value lands in the row
// the runtime writes. The registry test (binding_fn_test.go) proves
// round-trip + last-wins in memory; it never inserts. Here we register a
// binding fn, resolve it the way codegen wiring does, and write its
// output into a real column, then read it back — catching a registration-
// name drift (register "hash" / look up "Hash") or a value-boxing bug
// that the in-memory test would miss.
func TestBindingFn_ResolvedValueBindsIntoRow(t *testing.T) {
	pool := connectTestDB(t)
	tbl := uniqueTable("acct")
	ctx := context.Background()

	if _, err := pool.Exec(ctx, fmt.Sprintf(
		`CREATE TABLE %s (id bigint primary key, password_hash text not null)`, tbl)); err != nil {
		t.Fatalf("create table: %v", err)
	}
	defer dropTable(t, pool, tbl)

	const fnName = "lzbeh_hash_password"
	RegisterBindingFn(fnName, func(_ context.Context, args ...any) (any, error) {
		plain, _ := args[0].(string)
		return "hashed:" + plain, nil
	})

	// Resolve the fn the same way the generated binding wiring does, then
	// invoke it to produce the column value.
	fn, ok := resolveBindingFn(fnName)
	if !ok {
		t.Fatalf("resolveBindingFn(%q) = not found; registration drift", fnName)
	}
	bound, err := fn(&Ctx{Context: ctx}, "swordfish")
	if err != nil {
		t.Fatalf("binding fn invoke: %v", err)
	}
	boundStr, ok := bound.(string)
	if !ok {
		t.Fatalf("bound value type = %T, want string", bound)
	}
	if boundStr != "hashed:swordfish" {
		t.Fatalf("bound value = %q, want %q", boundStr, "hashed:swordfish")
	}

	// The bound value must actually persist into the INSERT.
	if _, err := pool.Exec(ctx,
		fmt.Sprintf(`INSERT INTO %s (id, password_hash) VALUES ($1, $2)`, tbl), ID(1), bound); err != nil {
		t.Fatalf("insert with bound value: %v", err)
	}
	var got string
	if err := pool.QueryRow(ctx, fmt.Sprintf(`SELECT password_hash FROM %s WHERE id = 1`, tbl)).Scan(&got); err != nil {
		t.Fatalf("readback: %v", err)
	}
	if got != "hashed:swordfish" {
		t.Fatalf("persisted hash = %q, want %q — @fn value did not bind into the row", got, "hashed:swordfish")
	}

	// A fn whose name is in neither registry resolves to (nil,false) —
	// fail-closed, never a silent wrong pick.
	if _, ok := resolveBindingFn("lzbeh_does_not_exist"); ok {
		t.Fatal("resolveBindingFn returned ok for an unregistered name")
	}
}

// --- B4: query.sql executes and returns the seeded rows -----------------

// TestRunSQL_ReturnsSeededRowsRespectingScope proves a `query.sql`
// actually executes against Postgres, decodes into the generated row
// shape, and respects its parameterized (tenant) predicate. The dispatch
// test proves routing reaches the policy gate; nothing proves the SQL
// runs + scans. A RETURNS-LIST shape mismatch or a tenant-scope arg that
// the builder drops would pass dispatch and break here.
func TestRunSQL_ReturnsSeededRowsRespectingScope(t *testing.T) {
	pool := connectTestDB(t)
	tbl := uniqueTable("job")
	ctx := context.Background()

	if _, err := pool.Exec(ctx, fmt.Sprintf(
		`CREATE TABLE %s (id bigint primary key, org_id bigint not null, title text not null)`, tbl)); err != nil {
		t.Fatalf("create table: %v", err)
	}
	defer dropTable(t, pool, tbl)
	if _, err := pool.Exec(ctx, fmt.Sprintf(
		`INSERT INTO %s (id, org_id, title) VALUES (1,10,'a'),(2,10,'b'),(3,20,'c')`, tbl)); err != nil {
		t.Fatalf("seed: %v", err)
	}

	// Point the global DB() at our pool for the duration of this test, then
	// restore. RunSQL reads from DB().
	prev := dbPool
	SetDB(pool)
	defer SetDB(prev)

	type qargs struct {
		OrgID int64 `json:"org_id"`
	}
	type jobRow struct {
		ID    int64  `db:"id"`
		OrgID int64  `db:"org_id"`
		Title string `db:"title"`
	}

	q := &Query[qargs, jobRow]{
		Name:    "dashboard.jobs_for_org",
		Kind:    QuerySQL,
		Policy:  Policy{Name: "@policy.view", Atoms: []PolicyAtom{{Namespace: "scope", Name: "public"}}},
		SQLText: fmt.Sprintf("SELECT id, org_id, title FROM %s WHERE org_id = $1 ORDER BY id", tbl),
		SQLMany: true,
		SQLArgs: func(a qargs) []any { return []any{a.OrgID} },
	}

	lctx := &Ctx{Context: ctx, Actor: ActorAnonymous}
	out, err := q.RunSQL(lctx, qargs{OrgID: 10})
	if err != nil {
		t.Fatalf("RunSQL: %v", err)
	}
	rows, ok := out.([]jobRow)
	if !ok {
		t.Fatalf("RunSQL result type = %T, want []jobRow", out)
	}
	if len(rows) != 2 {
		t.Fatalf("rows for org 10 = %d, want 2 (tenant predicate must scope) -> %+v", len(rows), rows)
	}
	if rows[0].ID != 1 || rows[1].ID != 2 {
		t.Fatalf("rows = %+v, want ids [1,2] in order", rows)
	}
	for _, r := range rows {
		if r.OrgID != 10 {
			t.Fatalf("row leaked across tenant: %+v", r)
		}
		if r.Title == "" {
			t.Fatalf("row title not decoded: %+v", r)
		}
	}

	// The other tenant returns its own row set, not org 10's.
	out2, err := q.RunSQL(lctx, qargs{OrgID: 20})
	if err != nil {
		t.Fatalf("RunSQL org 20: %v", err)
	}
	rows2 := out2.([]jobRow)
	if len(rows2) != 1 || rows2[0].ID != 3 {
		t.Fatalf("rows for org 20 = %+v, want exactly id 3", rows2)
	}
}
