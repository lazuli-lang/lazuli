package config

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestFilePollerCheckOnceSuppressesIdenticalContent(t *testing.T) {
	ctx := context.Background()
	path := filepath.Join(t.TempDir(), "app.conf")
	if err := os.WriteFile(path, []byte("first"), 0o600); err != nil {
		t.Fatalf("WriteFile(first) error = %v", err)
	}

	var values []string
	poller := NewFilePoller(path, time.Second, parseString, func(_ context.Context, value string) error {
		values = append(values, value)
		return nil
	})

	changed, err := poller.CheckOnce(ctx)
	if err != nil {
		t.Fatalf("CheckOnce(first) error = %v", err)
	}
	if !changed {
		t.Fatal("CheckOnce(first) changed = false, want true")
	}

	changed, err = poller.CheckOnce(ctx)
	if err != nil {
		t.Fatalf("CheckOnce(duplicate) error = %v", err)
	}
	if changed {
		t.Fatal("CheckOnce(duplicate) changed = true, want false")
	}

	if err := os.WriteFile(path, []byte("second"), 0o600); err != nil {
		t.Fatalf("WriteFile(second) error = %v", err)
	}
	changed, err = poller.CheckOnce(ctx)
	if err != nil {
		t.Fatalf("CheckOnce(second) error = %v", err)
	}
	if !changed {
		t.Fatal("CheckOnce(second) changed = false, want true")
	}

	want := []string{"first", "second"}
	if !equalStrings(values, want) {
		t.Fatalf("values = %#v, want %#v", values, want)
	}
}

func TestPollerUsesLoaderVersion(t *testing.T) {
	ctx := context.Background()
	value := 1
	version := "v1"
	loader := func(context.Context) (Snapshot[int], error) {
		return Snapshot[int]{Value: value, Version: version}, nil
	}

	var values []int
	poller := NewPoller(time.Second, loader, func(_ context.Context, value int) error {
		values = append(values, value)
		return nil
	})

	if changed, err := poller.CheckOnce(ctx); err != nil || !changed {
		t.Fatalf("CheckOnce(v1) changed = %v, error = %v; want true, nil", changed, err)
	}

	value = 2
	if changed, err := poller.CheckOnce(ctx); err != nil || changed {
		t.Fatalf("CheckOnce(duplicate version) changed = %v, error = %v; want false, nil", changed, err)
	}

	version = "v2"
	if changed, err := poller.CheckOnce(ctx); err != nil || !changed {
		t.Fatalf("CheckOnce(v2) changed = %v, error = %v; want true, nil", changed, err)
	}

	want := []int{1, 2}
	if !equalInts(values, want) {
		t.Fatalf("values = %#v, want %#v", values, want)
	}
}

func TestPollerHashesLoaderContentWhenVersionIsEmpty(t *testing.T) {
	ctx := context.Background()
	value := 1
	content := "same"
	loader := func(context.Context) (Snapshot[int], error) {
		return Snapshot[int]{Value: value, Content: []byte(content)}, nil
	}

	var values []int
	poller := NewPoller(time.Second, loader, func(_ context.Context, value int) error {
		values = append(values, value)
		return nil
	})

	if changed, err := poller.CheckOnce(ctx); err != nil || !changed {
		t.Fatalf("CheckOnce(first) changed = %v, error = %v; want true, nil", changed, err)
	}

	value = 2
	if changed, err := poller.CheckOnce(ctx); err != nil || changed {
		t.Fatalf("CheckOnce(duplicate content) changed = %v, error = %v; want false, nil", changed, err)
	}

	content = "changed"
	if changed, err := poller.CheckOnce(ctx); err != nil || !changed {
		t.Fatalf("CheckOnce(changed content) changed = %v, error = %v; want true, nil", changed, err)
	}

	want := []int{1, 2}
	if !equalInts(values, want) {
		t.Fatalf("values = %#v, want %#v", values, want)
	}
}

func TestCheckOnceRetriesWhenOnChangeFails(t *testing.T) {
	ctx := context.Background()
	errApply := errors.New("apply config")
	calls := 0
	poller := NewPoller(time.Second, func(context.Context) (Snapshot[string], error) {
		return Snapshot[string]{Value: "current", Content: []byte("current")}, nil
	}, func(_ context.Context, _ string) error {
		calls++
		if calls == 1 {
			return errApply
		}
		return nil
	})

	changed, err := poller.CheckOnce(ctx)
	if !errors.Is(err, errApply) {
		t.Fatalf("CheckOnce(first) error = %v, want %v", err, errApply)
	}
	if changed {
		t.Fatal("CheckOnce(first) changed = true, want false")
	}

	changed, err = poller.CheckOnce(ctx)
	if err != nil {
		t.Fatalf("CheckOnce(retry) error = %v", err)
	}
	if !changed {
		t.Fatal("CheckOnce(retry) changed = false, want true")
	}
	if calls != 2 {
		t.Fatalf("calls = %d, want 2", calls)
	}
}

func TestRunUsesInjectedTickerAndStopsOnContextCancel(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	version := "v1"
	value := 1
	loader := func(context.Context) (Snapshot[int], error) {
		return Snapshot[int]{Value: value, Version: version}, nil
	}

	ticker := &manualTicker{
		c:       make(chan time.Time),
		stopped: make(chan struct{}),
	}
	intervals := make(chan time.Duration, 1)
	values := make(chan int)
	poller := NewPoller(time.Hour, loader, func(_ context.Context, value int) error {
		values <- value
		return nil
	})
	poller.TickerFactory = func(interval time.Duration) Ticker {
		intervals <- interval
		return ticker
	}

	errc := make(chan error, 1)
	go func() {
		errc <- poller.Run(ctx)
	}()

	if got := receive(t, values); got != 1 {
		t.Fatalf("initial value = %d, want 1", got)
	}
	if got := receive(t, intervals); got != time.Hour {
		t.Fatalf("interval = %s, want %s", got, time.Hour)
	}

	version = "v2"
	value = 2
	ticker.c <- time.Now()
	if got := receive(t, values); got != 2 {
		t.Fatalf("reloaded value = %d, want 2", got)
	}

	cancel()
	if err := receive(t, errc); !errors.Is(err, context.Canceled) {
		t.Fatalf("Run error = %v, want context.Canceled", err)
	}
	select {
	case <-ticker.stopped:
	case <-time.After(time.Second):
		t.Fatal("ticker was not stopped")
	}
}

func parseString(ctx context.Context, content []byte) (string, error) {
	if err := ctx.Err(); err != nil {
		return "", err
	}
	return string(content), nil
}

func equalStrings(left, right []string) bool {
	if len(left) != len(right) {
		return false
	}
	for i := range left {
		if left[i] != right[i] {
			return false
		}
	}
	return true
}

func equalInts(left, right []int) bool {
	if len(left) != len(right) {
		return false
	}
	for i := range left {
		if left[i] != right[i] {
			return false
		}
	}
	return true
}

func receive[T any](t *testing.T, ch <-chan T) T {
	t.Helper()

	select {
	case value := <-ch:
		return value
	case <-time.After(time.Second):
		t.Fatal("timed out waiting for channel")
	}

	var zero T
	return zero
}

type manualTicker struct {
	c       chan time.Time
	stopped chan struct{}
}

func (t *manualTicker) C() <-chan time.Time {
	return t.c
}

func (t *manualTicker) Stop() {
	close(t.stopped)
}
