package observability

import (
	"context"
	"runtime/pprof"

	"lazuli.dev/runtime/lazuli"
)

// StartOp opens a pprof label scope keyed by the SourceTag attached to ctx.
// Generated handlers call this immediately after lazuli.WithSource. The
// returned function must be deferred to restore the previous goroutine labels.
func StartOp(ctx context.Context) (context.Context, func()) {
	tag := lazuli.SourceTagFromContext(ctx)
	if tag.Op == "" {
		return ctx, func() {}
	}
	labels := pprof.Labels(
		"capsule", tag.Capsule,
		"feature", tag.Feature,
		"kind", tag.Kind,
		"op", tag.Op,
	)
	newCtx := pprof.WithLabels(ctx, labels)
	pprof.SetGoroutineLabels(newCtx)
	return newCtx, func() {
		pprof.SetGoroutineLabels(ctx)
	}
}
