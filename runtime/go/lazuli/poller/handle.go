// Typed handle wrapper. Codegen instantiates `Bind[Row, S, T, R](spec,
// decode)` once per authored poller; the resulting `SpecHandle` is
// stored on the dispatcher's `handles` slice and dispatched at runtime.
//
// The split lets the dispatcher stay generic-erased (one runOne loop
// for all pollers) while keeping the per-row handler call typed
// end-to-end. ~80 LOC.
package poller

import (
	"context"
	"fmt"
	"time"
)

// RowDecoder reconstructs the typed Row from a JSON-style map produced
// by `QueryRunner.LoadRow`. Codegen emits one decoder per poller's row
// type. The dispatcher uses it once per resolution attempt.
type RowDecoder[Row any] func(raw map[string]any) (Row, error)

// AttemptsAccessor reports the row's current `attempts` counter so the
// dispatcher can scope the conditional UPDATE. Codegen emits one per
// row type (one-line getter).
type AttemptsAccessor[Row any] func(row Row) uint32

// typedHandle implements SpecHandle by closing over a typed Spec plus
// the per-row decoders. Codegen emits `Bind(spec, decode, attempts)` to
// build one.
type typedHandle[Row any, S ~string, T ~string, R any] struct {
	spec      Spec[Row, S, T, R]
	decode    RowDecoder[Row]
	attempts  AttemptsAccessor[Row]
}

// Bind wires a typed Spec into a SpecHandle. Codegen emits one Bind
// call per registered poller. The returned handle is dispatcher-ready.
func Bind[Row any, S ~string, T ~string, R any](
	spec Spec[Row, S, T, R],
	decode RowDecoder[Row],
	attempts AttemptsAccessor[Row],
) SpecHandle {
	return &typedHandle[Row, S, T, R]{spec: spec, decode: decode, attempts: attempts}
}

func (h *typedHandle[Row, S, T, R]) Name() string  { return h.spec.Name }
func (h *typedHandle[Row, S, T, R]) Tick() Tick    { return h.spec.Tick }
func (h *typedHandle[Row, S, T, R]) Source() string { return h.spec.Source }
func (h *typedHandle[Row, S, T, R]) Cursor() Cursor { return h.spec.Cursor }
func (h *typedHandle[Row, S, T, R]) Retry() Retry   { return h.spec.Retry }
func (h *typedHandle[Row, S, T, R]) Quirks() []Quirk { return h.spec.RetryQuirks }

// ResolveOne loads the row, evaluates quirks, calls the handler, and
// commits the result via the QueryRunner. Errors are swallowed at this
// layer (the dispatcher logs them via QueryRunner's side-channel); the
// loop must keep ticking.
func (h *typedHandle[Row, S, T, R]) ResolveOne(
	ctx context.Context, db QueryRunner, bus EventPublisher, id string,
) error {
	raw, err := db.LoadRow(ctx, h.spec.Source, id)
	if err != nil {
		return err
	}

	// Quirk application — pre-handler hooks. v0.1: gender_flip_once.
	for _, q := range h.spec.RetryQuirks {
		switch qv := q.(type) {
		case GenderFlipOnce:
			if EvalPredicate(qv.When, raw) {
				// Counter < 1 is enforced by the UPDATE's WHERE clause.
				_, _ = db.ApplyQuirk(ctx, h.spec.Source, id, qv)
				// Reload the row after mutation so the handler sees
				// fresh values.
				if fresh, err := db.LoadRow(ctx, h.spec.Source, id); err == nil {
					raw = fresh
				}
			}
		}
	}

	row, err := h.decode(raw)
	if err != nil {
		return err
	}

	if h.spec.WithSource != nil {
		ctx = h.spec.WithSource(ctx)
	}

	result, err := h.spec.Resolve(ctx, row)
	if err != nil {
		// Bump attempts via a pending commit to advance the counter
		// even on handler error; the next tick retries.
		_, _ = db.CommitPending(ctx, h.spec.Source, h.spec.Cursor, id, h.attempts(row),
			h.spec.Retry.Backoff.NextDelay(h.attempts(row)+1))
		return err
	}

	switch {
	case result.Terminal != nil:
		statusValue := any(string(result.Terminal.Status))
		_, err := db.CommitTerminal(ctx, h.spec.Source, h.spec.Cursor, id,
			h.attempts(row),
			h.spec.TerminalStatusField, h.spec.TerminalResultField,
			statusValue, result.Terminal.Result)
		if err != nil {
			return err
		}
		if bus != nil {
			for _, evt := range h.spec.Emits {
				_ = bus.Publish(ctx, evt, map[string]any{
					"id":     id,
					"status": statusValue,
				})
			}
		}
		return nil
	case result.Pending != nil:
		next := h.spec.Retry.Backoff.NextDelay(h.attempts(row) + 1)
		if result.Pending.NextCheckAt != nil {
			next = result.Pending.NextCheckAt.Sub(time.Now())
		}
		_, err := db.CommitPending(ctx, h.spec.Source, h.spec.Cursor, id,
			h.attempts(row), next)
		return err
	default:
		return fmt.Errorf("%w: poller %q", ErrResolveResultEmpty, h.spec.Name)
	}
}
