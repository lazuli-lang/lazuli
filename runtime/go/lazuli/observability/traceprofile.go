package observability

import (
	"context"
	"errors"
	"fmt"
	"io"
	"runtime/trace"
	"sync"
	"time"
)

// Typed errors.
var (
	// ErrTraceAlreadyRunning is returned when a runtime trace is already active.
	ErrTraceAlreadyRunning = errors.New("lazuli/observability: trace_already_running")
	// ErrTraceWriterRequired is returned when StartTrace or CaptureTrace receives a nil writer.
	ErrTraceWriterRequired = errors.New("lazuli/observability: trace_writer_required")
)

var traceProfileState struct {
	sync.Mutex
	active *TraceProfile
}

// TraceProfile is an active runtime/trace capture started by StartTrace.
type TraceProfile struct {
	stopOnce sync.Once
}

// StartTrace starts a Go runtime trace writing trace data to w.
//
// Only one runtime trace may be active in the process at a time. Call Stop on
// the returned profile when the capture is complete.
func StartTrace(w io.Writer) (*TraceProfile, error) {
	if w == nil {
		return nil, ErrTraceWriterRequired
	}

	traceProfileState.Lock()
	defer traceProfileState.Unlock()

	if traceProfileState.active != nil {
		return nil, ErrTraceAlreadyRunning
	}

	profile := &TraceProfile{}
	if err := trace.Start(w); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrTraceAlreadyRunning, err)
	}
	traceProfileState.active = profile
	return profile, nil
}

// Stop ends the runtime trace. Stop is safe to call more than once.
func (p *TraceProfile) Stop() {
	if p == nil {
		return
	}

	p.stopOnce.Do(func() {
		traceProfileState.Lock()
		if traceProfileState.active != p {
			traceProfileState.Unlock()
			return
		}
		traceProfileState.Unlock()

		trace.Stop()

		traceProfileState.Lock()
		traceProfileState.active = nil
		traceProfileState.Unlock()
	})
}

// CaptureTrace records a runtime trace until duration elapses or ctx is canceled.
//
// A non-positive duration captures the smallest possible trace and returns after
// starting and stopping the trace. The trace is always stopped before
// CaptureTrace returns.
func CaptureTrace(ctx context.Context, duration time.Duration, w io.Writer) error {
	if ctx == nil {
		ctx = context.Background()
	}
	if err := ctx.Err(); err != nil {
		return err
	}

	profile, err := StartTrace(w)
	if err != nil {
		return err
	}
	defer profile.Stop()

	if duration <= 0 {
		return nil
	}

	timer := time.NewTimer(duration)
	defer timer.Stop()

	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-timer.C:
		return nil
	}
}
