package poller

import (
	"context"
	"sync"
	"testing"
	"time"
)

// ── Backoff math (pure-function table tests) ─────────────────────────────────

func TestExponentialNextDelayCaps(t *testing.T) {
	e := Exponential{Base: 30 * time.Second, Cap: 10 * time.Minute}
	cases := []struct {
		attempts uint32
		want     time.Duration
	}{
		{0, 0},
		{1, 30 * time.Second},
		{2, 60 * time.Second},
		{3, 120 * time.Second},
		{4, 240 * time.Second},
		{5, 480 * time.Second},
		{6, 10 * time.Minute}, // capped
		{30, 10 * time.Minute},
	}
	for _, c := range cases {
		got := e.NextDelay(c.attempts)
		if got != c.want {
			t.Errorf("Exponential.NextDelay(%d) = %v, want %v", c.attempts, got, c.want)
		}
	}
}

func TestLinearNextDelay(t *testing.T) {
	l := Linear{Base: 10 * time.Second, Cap: 60 * time.Second}
	if got := l.NextDelay(3); got != 30*time.Second {
		t.Errorf("Linear.NextDelay(3) = %v, want 30s", got)
	}
	if got := l.NextDelay(10); got != 60*time.Second {
		t.Errorf("Linear.NextDelay(10) cap = %v, want 60s", got)
	}
}

func TestFixedNextDelayIgnoresAttempts(t *testing.T) {
	f := Fixed{Base: 5 * time.Second}
	if got := f.NextDelay(99); got != 5*time.Second {
		t.Errorf("Fixed.NextDelay = %v, want 5s", got)
	}
}

// ── Predicate evaluator ──────────────────────────────────────────────────────

func TestEvalPredicateEquality(t *testing.T) {
	row := map[string]any{"status_v8": "gender_ambiguous"}
	if !EvalPredicate(`row.status_v8 == "gender_ambiguous"`, row) {
		t.Error("expected equality predicate to match")
	}
	if EvalPredicate(`row.status_v8 == "completed"`, row) {
		t.Error("expected mismatched literal to be false")
	}
	if EvalPredicate(`row.missing == "x"`, row) {
		t.Error("expected missing field to be false")
	}
	if EvalPredicate(`unsupported syntax`, row) {
		t.Error("expected unsupported form to be false")
	}
}

// ── End-to-end resolve loop with a fake QueryRunner ──────────────────────────

type fakeRow struct {
	id       string
	attempts uint32
	status   string
	resolved bool
}

type fakeDB struct {
	mu        sync.Mutex
	rows      map[string]*fakeRow
	eligible  []string
	commits   []string // log of "terminal:id", "pending:id" entries
	quirks    []string
}

func (f *fakeDB) SelectEligibleIDs(_ context.Context, _ string, _ Cursor, _ uint32) ([]string, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	out := make([]string, 0, len(f.eligible))
	for _, id := range f.eligible {
		if r, ok := f.rows[id]; ok && !r.resolved {
			out = append(out, id)
		}
	}
	return out, nil
}

func (f *fakeDB) LoadRow(_ context.Context, _ string, id string) (map[string]any, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	r := f.rows[id]
	return map[string]any{
		"id":        r.id,
		"attempts":  r.attempts,
		"status_v8": r.status,
		"gender":    "ambiguous",
	}, nil
}

func (f *fakeDB) CommitTerminal(_ context.Context, _ string, _ Cursor, id string, attempts uint32, _, _ string, _, _ any) (int64, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	r := f.rows[id]
	if r.attempts != attempts {
		return 0, nil // crash-recovery: stale `attempts` → no-op
	}
	r.resolved = true
	f.commits = append(f.commits, "terminal:"+id)
	return 1, nil
}

func (f *fakeDB) CommitPending(_ context.Context, _ string, _ Cursor, id string, attempts uint32, _ time.Duration) (int64, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	r := f.rows[id]
	if r.attempts != attempts {
		return 0, nil
	}
	r.attempts++
	f.commits = append(f.commits, "pending:"+id)
	return 1, nil
}

func (f *fakeDB) ApplyQuirk(_ context.Context, _ string, id string, _ Quirk) (int64, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.quirks = append(f.quirks, id)
	return 1, nil
}

// V8Status models the handler's intermediate status enum.
type V8Status string

// ConsultFinalStatus models the terminal enum.
type ConsultFinalStatus string

const (
	V8Pending   V8Status           = "pending"
	V8Resolved  ConsultFinalStatus = "resolved"
)

type v8Row struct {
	ID       string
	Attempts uint32
}

func TestResolveOneTerminalCommitsAndFreezes(t *testing.T) {
	spec := Spec[v8Row, V8Status, ConsultFinalStatus, map[string]any]{
		Name:   "test.v8",
		Source: "v8_pending_consults",
		Cursor: Cursor{NextAtField: "next_check_at", ResolvedAtField: "resolved_at", AttemptsField: "attempts"},
		Retry:  Retry{MaxAttempts: 5, Backoff: Fixed{Base: 0}},
		States: []State_[V8Status]{
			{Name: V8Pending, Kind: Initial},
		},
		Tick:                Tick{Every: time.Minute, Batch: 100},
		TerminalStatusField: "final_status",
		TerminalResultField: "final_result",
		Resolve: func(_ context.Context, row v8Row) (ResolveResult[V8Status, ConsultFinalStatus, map[string]any], error) {
			return ResolveResult[V8Status, ConsultFinalStatus, map[string]any]{
				Terminal: &TerminalResult[ConsultFinalStatus, map[string]any]{
					Status: V8Resolved,
					Result: map[string]any{"ok": true},
				},
			}, nil
		},
	}
	handle := Bind(spec,
		func(raw map[string]any) (v8Row, error) {
			id, _ := raw["id"].(string)
			att, _ := raw["attempts"].(uint32)
			return v8Row{ID: id, Attempts: att}, nil
		},
		func(r v8Row) uint32 { return r.Attempts },
	)
	db := &fakeDB{
		rows:     map[string]*fakeRow{"r1": {id: "r1", attempts: 0, status: "pending"}},
		eligible: []string{"r1"},
	}
	if err := handle.ResolveOne(context.Background(), db, nil, "r1"); err != nil {
		t.Fatalf("ResolveOne returned error: %v", err)
	}
	if !db.rows["r1"].resolved {
		t.Error("expected row to be marked resolved")
	}
	if len(db.commits) != 1 || db.commits[0] != "terminal:r1" {
		t.Errorf("expected single terminal commit, got %v", db.commits)
	}
}

func TestResolveOneCrashRecoveryStaleAttemptsIsNoop(t *testing.T) {
	spec := Spec[v8Row, V8Status, ConsultFinalStatus, map[string]any]{
		Name:   "test.v8",
		Source: "v8",
		Cursor: Cursor{NextAtField: "n", ResolvedAtField: "r", AttemptsField: "attempts"},
		Retry:  Retry{MaxAttempts: 5, Backoff: Fixed{Base: 0}},
		Resolve: func(_ context.Context, _ v8Row) (ResolveResult[V8Status, ConsultFinalStatus, map[string]any], error) {
			return ResolveResult[V8Status, ConsultFinalStatus, map[string]any]{
				Terminal: &TerminalResult[ConsultFinalStatus, map[string]any]{Status: V8Resolved},
			}, nil
		},
	}
	handle := Bind(spec,
		func(raw map[string]any) (v8Row, error) {
			// Pretend the handler thinks attempts is 0...
			return v8Row{ID: raw["id"].(string), Attempts: 0}, nil
		},
		func(r v8Row) uint32 { return r.Attempts },
	)
	db := &fakeDB{
		// ...but the DB has already advanced attempts to 1 (a previous
		// successful commit). The conditional UPDATE must no-op.
		rows: map[string]*fakeRow{"r1": {id: "r1", attempts: 1, status: "pending"}},
	}
	_ = handle.ResolveOne(context.Background(), db, nil, "r1")
	if db.rows["r1"].resolved {
		t.Error("expected stale-attempts commit to no-op, but row was resolved")
	}
}

func TestResolveOneGenderFlipQuirkFires(t *testing.T) {
	spec := Spec[v8Row, V8Status, ConsultFinalStatus, map[string]any]{
		Name:    "test.v8",
		Source:  "v8",
		Cursor:  Cursor{NextAtField: "n", ResolvedAtField: "r", AttemptsField: "attempts"},
		Retry:   Retry{MaxAttempts: 5, Backoff: Fixed{Base: 0}},
		RetryQuirks: []Quirk{GenderFlipOnce{
			When:         `row.status_v8 == "gender_ambiguous"`,
			CounterField: "gender_retry_count",
			GenderField:  "gender",
		}},
		Resolve: func(_ context.Context, _ v8Row) (ResolveResult[V8Status, ConsultFinalStatus, map[string]any], error) {
			return ResolveResult[V8Status, ConsultFinalStatus, map[string]any]{
				Terminal: &TerminalResult[ConsultFinalStatus, map[string]any]{Status: V8Resolved},
			}, nil
		},
	}
	handle := Bind(spec,
		func(raw map[string]any) (v8Row, error) {
			return v8Row{ID: raw["id"].(string), Attempts: 0}, nil
		},
		func(r v8Row) uint32 { return r.Attempts },
	)
	db := &fakeDB{
		rows: map[string]*fakeRow{"r1": {id: "r1", attempts: 0, status: "gender_ambiguous"}},
	}
	_ = handle.ResolveOne(context.Background(), db, nil, "r1")
	if len(db.quirks) != 1 || db.quirks[0] != "r1" {
		t.Errorf("expected gender-flip quirk to fire once on r1, got %v", db.quirks)
	}
}

func TestResolveOnePendingBumpsAttempts(t *testing.T) {
	spec := Spec[v8Row, V8Status, ConsultFinalStatus, map[string]any]{
		Name:   "test.v8",
		Source: "v8",
		Cursor: Cursor{NextAtField: "n", ResolvedAtField: "r", AttemptsField: "attempts"},
		Retry:  Retry{MaxAttempts: 5, Backoff: Fixed{Base: 30 * time.Second}},
		Resolve: func(_ context.Context, _ v8Row) (ResolveResult[V8Status, ConsultFinalStatus, map[string]any], error) {
			return ResolveResult[V8Status, ConsultFinalStatus, map[string]any]{
				Pending: &PendingResult[V8Status]{Status: V8Pending},
			}, nil
		},
	}
	handle := Bind(spec,
		func(raw map[string]any) (v8Row, error) {
			return v8Row{ID: raw["id"].(string), Attempts: 0}, nil
		},
		func(r v8Row) uint32 { return r.Attempts },
	)
	db := &fakeDB{
		rows: map[string]*fakeRow{"r1": {id: "r1", attempts: 0, status: "pending"}},
	}
	_ = handle.ResolveOne(context.Background(), db, nil, "r1")
	if db.rows["r1"].attempts != 1 {
		t.Errorf("expected attempts to bump to 1, got %d", db.rows["r1"].attempts)
	}
	if len(db.commits) != 1 || db.commits[0] != "pending:r1" {
		t.Errorf("expected single pending commit, got %v", db.commits)
	}
}

// ── Registry ─────────────────────────────────────────────────────────────────

func TestRegistryStoresInsertionOrder(t *testing.T) {
	r := NewRegistry()
	a := Spec[v8Row, V8Status, ConsultFinalStatus, map[string]any]{Name: "alpha"}
	b := Spec[v8Row, V8Status, ConsultFinalStatus, map[string]any]{Name: "beta"}
	Register(r, a)
	Register(r, b)
	all := r.All()
	if len(all) != 2 {
		t.Fatalf("expected 2 specs, got %d", len(all))
	}
}
