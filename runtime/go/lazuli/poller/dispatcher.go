// Dispatcher — wire-thin scheduler for poller specs.
//
// Per docs/proposals/poller-vocab.md §6.2: one goroutine per registered
// spec; each tick runs one batched SELECT, then per-row handler call,
// then conditional UPDATE keyed on `(row.id, row.attempts)` for crash-
// recovery idempotency. The dispatcher is generic-erased: it works
// against a narrow `SpecHandle` interface that the typed wrapper below
// implements (one wrapper instantiation per spec via Register/Bind).
//
// Total file LOC budget: ~140. Stdlib + pgx/v5 only.
package poller

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"sync"
	"time"
)

// QueryRunner is the narrow DB surface the dispatcher consumes. pgx's
// `*pgxpool.Pool` satisfies it via `Query`/`Exec`. Keeping the type
// indirected avoids dragging the pgx import into the contract surface.
type QueryRunner interface {
	// SelectEligibleIDs returns up to `batch` row IDs eligible for
	// resolution given the cursor field bindings. Implementations write
	// the same parametric SQL: `SELECT id FROM <table> WHERE <next_at>
	// <= NOW() AND <resolved_at> IS NULL LIMIT <batch>`.
	SelectEligibleIDs(ctx context.Context, table string, cursor Cursor, batch uint32) ([]string, error)

	// LoadRow loads one row by id. The wrapper unmarshals it into the
	// Spec's `Row` type.
	LoadRow(ctx context.Context, table string, id string) (map[string]any, error)

	// CommitTerminal writes `<resolved_at> = NOW()` + optional terminal
	// status/result columns, gated on `id = $1 AND <attempts_field> =
	// $2` for idempotency. Returns rowsAffected; zero means a racing
	// handler already advanced the row.
	CommitTerminal(ctx context.Context, table string, cursor Cursor, id string, attempts uint32,
		statusField, resultField string, statusValue any, resultValue any) (int64, error)

	// CommitPending bumps `<attempts_field>` and sets `<next_at_field>`
	// to NOW() + delay, gated on `id = $1 AND <attempts_field> = $2`.
	CommitPending(ctx context.Context, table string, cursor Cursor, id string, attempts uint32,
		delay time.Duration) (int64, error)

	// ApplyQuirk applies a closed-catalog quirk mutation atomically with
	// the counter bump, gated on `id = $1 AND <counter_field> = 0`.
	// Returns rowsAffected. Only `GenderFlipOnce` is implemented in v0.1.
	ApplyQuirk(ctx context.Context, table string, id string, quirk Quirk) (int64, error)
}

// EventPublisher is the optional eventbus hook the dispatcher uses to
// surface `Spec.Emits`. nil-safe; codegen wires this at boot.
type EventPublisher interface {
	Publish(ctx context.Context, event string, payload map[string]any) error
}

// SpecHandle is the narrow interface the dispatcher consumes. The typed
// wrapper (one per `Spec[Row, S, T, R]` instantiation) implements it.
type SpecHandle interface {
	Name() string
	Tick() Tick
	Source() string
	Cursor() Cursor
	Retry() Retry
	Quirks() []Quirk
	// Resolve loads a row, calls the typed handler, evaluates quirks,
	// and commits via the QueryRunner. The dispatcher needs no further
	// awareness of the typed Spec.
	ResolveOne(ctx context.Context, db QueryRunner, bus EventPublisher, id string) error
}

// Scheduler orchestrates one goroutine per registered handle.
type Scheduler struct {
	db   QueryRunner
	bus  EventPublisher
	now  func() time.Time
	mu   sync.Mutex
	done chan struct{}
}

// NewScheduler returns a Scheduler bound to a QueryRunner and optional
// EventPublisher. The `now` clock parameter is dependency-injected for
// tests (use `time.Now` in production).
func NewScheduler(db QueryRunner, bus EventPublisher) *Scheduler {
	return &Scheduler{db: db, bus: bus, now: time.Now, done: make(chan struct{})}
}

// Run blocks until `ctx` is done. One goroutine per `handles[i]` runs
// `time.Ticker.C` triggered SELECT/dispatch loops.
func (s *Scheduler) Run(ctx context.Context, handles []SpecHandle) error {
	if s.db == nil {
		return errors.New("poller: scheduler has no QueryRunner")
	}
	var wg sync.WaitGroup
	for _, h := range handles {
		wg.Add(1)
		go func(h SpecHandle) {
			defer wg.Done()
			s.runOne(ctx, h)
		}(h)
	}
	wg.Wait()
	return ctx.Err()
}

// runOne is the per-handle ticker loop. Each tick: SELECT eligible IDs,
// then resolve each row in series (the batch is small; per-row
// parallelism is a follow-up). Errors are logged via the QueryRunner's
// side-channel; the loop never panics.
func (s *Scheduler) runOne(ctx context.Context, h SpecHandle) {
	tick := h.Tick()
	if tick.Every <= 0 {
		tick.Every = 30 * time.Second
	}
	if tick.Batch == 0 {
		tick.Batch = 100
	}
	t := time.NewTicker(tick.Every)
	defer t.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-t.C:
			ids, err := s.db.SelectEligibleIDs(ctx, h.Source(), h.Cursor(), tick.Batch)
			if err != nil {
				continue
			}
			for _, id := range ids {
				_ = h.ResolveOne(ctx, s.db, s.bus, id)
				if ctx.Err() != nil {
					return
				}
			}
		}
	}
}

// EvalPredicate is the closed-catalog predicate evaluator the
// dispatcher uses for `retry_quirk when <predicate>` matching.
//
// v0.1 supports exactly the equality form needed by the anchoring V8
// example: `row.<field> == "<literal>"`. Anything else returns false.
// The wider predicate language is reserved for L0 expansions (poller
// proposal §3.13).
func EvalPredicate(pred string, row map[string]any) bool {
	pred = strings.TrimSpace(pred)
	// Form: `row.<field> == "<literal>"` (and reverse).
	const sep = "=="
	idx := strings.Index(pred, sep)
	if idx < 0 {
		return false
	}
	lhs := strings.TrimSpace(pred[:idx])
	rhs := strings.TrimSpace(pred[idx+len(sep):])
	field := strings.TrimPrefix(lhs, "row.")
	if field == lhs {
		return false
	}
	want, ok := unquote(rhs)
	if !ok {
		return false
	}
	got, ok := row[field]
	if !ok {
		return false
	}
	return fmt.Sprint(got) == want
}

func unquote(s string) (string, bool) {
	if len(s) < 2 {
		return "", false
	}
	if s[0] == '"' && s[len(s)-1] == '"' {
		return s[1 : len(s)-1], true
	}
	return "", false
}
