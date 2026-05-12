package observability

import (
	"context"
	"runtime/pprof"
	"sync"
)

const (
	opLabelFeatureKey        = "feature"
	opLabelKindKey           = "kind"
	opLabelNameKey           = "name"
	opLabelSourceKey         = "source"
	opLabelPatternIDKey      = "pattern_id"
	opLabelPatternVersionKey = "pattern_version"
)

// OpTag identifies the Lazuli operation currently executing.
type OpTag struct {
	// Feature is the owning Lazuli feature.
	Feature string
	// Kind is the IR operation kind, such as command, query, job, or webhook.
	Kind string
	// Name is the operation name within the feature.
	Name string
	// Source is the Lazuli source location for the operation.
	Source string
	// PatternID is the codegen pattern identifier for the emitted operation.
	PatternID string
	// PatternVersion is the codegen pattern version for the emitted operation.
	PatternVersion string
}

// StartOp adds Lazuli operation labels to ctx and the current goroutine.
//
// Call the returned end function when the operation finishes. End is safe to
// call more than once.
func StartOp(ctx context.Context, tag OpTag) (context.Context, func()) {
	if ctx == nil {
		ctx = context.Background()
	}

	parent := ctx
	ctx = pprof.WithLabels(ctx, opLabelSet(tag))
	pprof.SetGoroutineLabels(ctx)

	var once sync.Once
	return ctx, func() {
		once.Do(func() {
			pprof.SetGoroutineLabels(parent)
		})
	}
}

func opLabelSet(tag OpTag) pprof.LabelSet {
	return pprof.Labels(
		opLabelFeatureKey, tag.Feature,
		opLabelKindKey, tag.Kind,
		opLabelNameKey, tag.Name,
		opLabelSourceKey, tag.Source,
		opLabelPatternIDKey, tag.PatternID,
		opLabelPatternVersionKey, tag.PatternVersion,
	)
}
