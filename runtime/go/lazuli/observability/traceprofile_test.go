package observability

import (
	"bytes"
	"context"
	"errors"
	"testing"
	"time"
)

func TestStartTraceStopsAndWritesTrace(t *testing.T) {
	var buf bytes.Buffer
	profile := startTraceForTest(t, &buf)

	profile.Stop()
	profile.Stop()

	if buf.Len() == 0 {
		t.Fatal("trace buffer is empty")
	}
}

func TestStartTraceRejectsConcurrentTrace(t *testing.T) {
	var first bytes.Buffer
	profile := startTraceForTest(t, &first)
	defer profile.Stop()

	var second bytes.Buffer
	_, err := StartTrace(&second)
	if !errors.Is(err, ErrTraceAlreadyRunning) {
		t.Fatalf("StartTrace while active err = %v, want ErrTraceAlreadyRunning", err)
	}
}

func TestStartTraceRequiresWriter(t *testing.T) {
	_, err := StartTrace(nil)
	if !errors.Is(err, ErrTraceWriterRequired) {
		t.Fatalf("StartTrace nil writer err = %v, want ErrTraceWriterRequired", err)
	}
}

func TestCaptureTraceStopsAfterDuration(t *testing.T) {
	var buf bytes.Buffer
	if err := CaptureTrace(context.Background(), time.Millisecond, &buf); err != nil {
		skipIfExternalTrace(t, err)
		t.Fatalf("CaptureTrace returned error: %v", err)
	}
	if buf.Len() == 0 {
		t.Fatal("trace buffer is empty")
	}

	var next bytes.Buffer
	profile := startTraceForTest(t, &next)
	profile.Stop()
}

func TestCaptureTraceStopsOnContextCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	var buf bytes.Buffer
	done := make(chan error, 1)
	go func() {
		done <- CaptureTrace(ctx, time.Hour, &buf)
	}()

	waitForActiveTrace(t, done)
	cancel()

	select {
	case err := <-done:
		if !errors.Is(err, context.Canceled) {
			t.Fatalf("CaptureTrace err = %v, want context.Canceled", err)
		}
	case <-time.After(time.Second):
		t.Fatal("CaptureTrace did not return after context cancellation")
	}

	if buf.Len() == 0 {
		t.Fatal("trace buffer is empty")
	}

	var next bytes.Buffer
	profile := startTraceForTest(t, &next)
	profile.Stop()
}

func TestCaptureTraceReturnsCanceledContextWithoutStarting(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	var buf bytes.Buffer
	err := CaptureTrace(ctx, time.Hour, &buf)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("CaptureTrace err = %v, want context.Canceled", err)
	}
	if buf.Len() != 0 {
		t.Fatalf("trace buffer length = %d, want 0", buf.Len())
	}
}

func startTraceForTest(t *testing.T, buf *bytes.Buffer) *TraceProfile {
	t.Helper()

	profile, err := StartTrace(buf)
	if err == nil {
		return profile
	}

	skipIfExternalTrace(t, err)
	t.Fatalf("StartTrace returned error: %v", err)
	return nil
}

func waitForActiveTrace(t *testing.T, done <-chan error) {
	t.Helper()

	deadline := time.After(time.Second)
	ticker := time.NewTicker(time.Millisecond)
	defer ticker.Stop()

	for {
		if traceProfileActiveForTest() {
			return
		}

		select {
		case err := <-done:
			skipIfExternalTrace(t, err)
			t.Fatalf("CaptureTrace returned before starting: %v", err)
		case <-deadline:
			t.Fatal("timed out waiting for active trace")
		case <-ticker.C:
		}
	}
}

func traceProfileActiveForTest() bool {
	traceProfileState.Lock()
	defer traceProfileState.Unlock()
	return traceProfileState.active != nil
}

func skipIfExternalTrace(t *testing.T, err error) {
	t.Helper()

	if errors.Is(err, ErrTraceAlreadyRunning) && !traceProfileActiveForTest() {
		t.Skipf("runtime trace is already active outside observability helper: %v", err)
	}
}
