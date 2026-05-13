package perf

import (
	"context"
	"errors"
	"io"
	"net/http"
	"reflect"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func TestLoadTestConfigValidate(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		cfg  LoadTestConfig
	}{
		{
			name: "requests",
			cfg:  LoadTestConfig{Concurrency: 1},
		},
		{
			name: "concurrency",
			cfg:  LoadTestConfig{Requests: 1},
		},
		{
			name: "method",
			cfg:  LoadTestConfig{Requests: 1, Concurrency: 1, Method: "BAD METHOD"},
		},
		{
			name: "path",
			cfg:  LoadTestConfig{Requests: 1, Concurrency: 1, Path: "relative"},
		},
		{
			name: "ramp",
			cfg:  LoadTestConfig{Requests: 1, Concurrency: 1, RampUp: -time.Nanosecond},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if err := tt.cfg.Validate(); err == nil {
				t.Fatal("Validate() returned nil, want error")
			}
		})
	}

	valid := LoadTestConfig{
		Requests:    2,
		Concurrency: 4,
		Method:      http.MethodPost,
		Path:        "/submit",
		RampUp:      time.Second,
	}
	if err := valid.Validate(); err != nil {
		t.Fatalf("Validate() returned error: %v", err)
	}
}

func TestPlanLoadTestScheduleDeterministic(t *testing.T) {
	t.Parallel()

	got, err := PlanLoadTestSchedule(LoadTestConfig{
		Requests:    5,
		Concurrency: 2,
		RampUp:      time.Second,
	})
	if err != nil {
		t.Fatalf("PlanLoadTestSchedule() returned error: %v", err)
	}

	want := []ScheduledRequest{
		{Index: 0, Worker: 0, StartAfter: 0},
		{Index: 1, Worker: 1, StartAfter: 250 * time.Millisecond},
		{Index: 2, Worker: 0, StartAfter: 500 * time.Millisecond},
		{Index: 3, Worker: 1, StartAfter: 750 * time.Millisecond},
		{Index: 4, Worker: 0, StartAfter: time.Second},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("schedule = %#v, want %#v", got, want)
	}
}

func TestRunnerExecutesHandlerAndSummarizes(t *testing.T) {
	t.Parallel()

	var seen int64
	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			t.Errorf("r.Method = %q, want POST", r.Method)
		}
		if r.URL.Path != "/load" {
			t.Errorf("r.URL.Path = %q, want /load", r.URL.Path)
		}
		if r.Header.Get("X-Test") != "yes" {
			t.Errorf("X-Test = %q, want yes", r.Header.Get("X-Test"))
		}
		body, err := io.ReadAll(r.Body)
		if err != nil {
			t.Errorf("ReadAll() returned error: %v", err)
		}
		if string(body) != "payload" {
			t.Errorf("body = %q, want payload", string(body))
		}

		n := atomic.AddInt64(&seen, 1)
		if n%2 == 0 {
			http.Error(w, "failed", http.StatusInternalServerError)
			return
		}
		w.WriteHeader(http.StatusCreated)
	})

	result, err := Runner{
		Handler: handler,
		Config: LoadTestConfig{
			Requests:    4,
			Concurrency: 2,
			Method:      http.MethodPost,
			Path:        "/load",
			Header:      http.Header{"X-Test": []string{"yes"}},
			Body:        []byte("payload"),
		},
	}.Run(context.Background())
	if err != nil {
		t.Fatalf("Run() returned error: %v", err)
	}

	if len(result.Samples) != 4 {
		t.Fatalf("len(Samples) = %d, want 4", len(result.Samples))
	}
	if result.Summary.Requests != 4 {
		t.Fatalf("Summary.Requests = %d, want 4", result.Summary.Requests)
	}
	if result.Summary.Successes != 2 {
		t.Fatalf("Summary.Successes = %d, want 2", result.Summary.Successes)
	}
	if result.Summary.Failures != 2 {
		t.Fatalf("Summary.Failures = %d, want 2", result.Summary.Failures)
	}
	if result.Summary.StatusCodes[http.StatusCreated] != 2 {
		t.Fatalf("created responses = %d, want 2", result.Summary.StatusCodes[http.StatusCreated])
	}
	if result.Summary.StatusCodes[http.StatusInternalServerError] != 2 {
		t.Fatalf("server error responses = %d, want 2", result.Summary.StatusCodes[http.StatusInternalServerError])
	}
	if result.Summary.Errors["Internal Server Error"] != 2 {
		t.Fatalf("server error count = %d, want 2", result.Summary.Errors["Internal Server Error"])
	}
}

func TestRunnerHonorsConcurrencyLimit(t *testing.T) {
	t.Parallel()

	reachedLimit := make(chan struct{})
	release := make(chan struct{})
	var once sync.Once
	var current atomic.Int64
	var maxSeen atomic.Int64

	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		inFlight := current.Add(1)
		for {
			max := maxSeen.Load()
			if inFlight <= max || maxSeen.CompareAndSwap(max, inFlight) {
				break
			}
		}
		if inFlight == 2 {
			once.Do(func() {
				close(reachedLimit)
			})
		}
		select {
		case <-release:
		case <-r.Context().Done():
		}
		current.Add(-1)
		w.WriteHeader(http.StatusNoContent)
	})

	done := make(chan error, 1)
	go func() {
		_, err := Runner{
			Handler: handler,
			Config:  LoadTestConfig{Requests: 4, Concurrency: 2},
		}.Run(context.Background())
		done <- err
	}()

	select {
	case <-reachedLimit:
	case <-time.After(time.Second):
		t.Fatal("runner did not start requests up to the concurrency limit")
	}
	close(release)

	if err := <-done; err != nil {
		t.Fatalf("Run() returned error: %v", err)
	}
	if got := maxSeen.Load(); got != 2 {
		t.Fatalf("max in-flight requests = %d, want 2", got)
	}
}

func TestRunnerCancellationStopsBeforeStartingRequests(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	var called atomic.Bool
	result, err := Runner{
		Handler: http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
			called.Store(true)
		}),
		Config: LoadTestConfig{Requests: 3, Concurrency: 1},
	}.Run(ctx)

	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Run() error = %v, want context.Canceled", err)
	}
	if called.Load() {
		t.Fatal("handler was called after context cancellation")
	}
	if len(result.Samples) != 0 {
		t.Fatalf("len(Samples) = %d, want 0", len(result.Samples))
	}
	if result.Summary.Requests != 0 {
		t.Fatalf("Summary.Requests = %d, want 0", result.Summary.Requests)
	}
}

func TestSummarizeLoadTestSamplesAndSortedErrors(t *testing.T) {
	t.Parallel()

	summary := SummarizeLoadTestSamples([]LoadTestSample{
		{Index: 0, StatusCode: http.StatusOK, Latency: 10 * time.Millisecond},
		{Index: 1, StatusCode: http.StatusBadGateway, Latency: 30 * time.Millisecond, Error: "upstream"},
		{Index: 2, StatusCode: http.StatusGatewayTimeout, Latency: 20 * time.Millisecond, Error: "timeout"},
		{Index: 3, StatusCode: http.StatusGatewayTimeout, Latency: 40 * time.Millisecond, Error: "timeout"},
	})

	if summary.Requests != 4 || summary.Successes != 1 || summary.Failures != 3 {
		t.Fatalf("summary counts = requests:%d successes:%d failures:%d, want 4/1/3", summary.Requests, summary.Successes, summary.Failures)
	}
	if summary.MinLatency != 10*time.Millisecond {
		t.Fatalf("MinLatency = %s, want 10ms", summary.MinLatency)
	}
	if summary.MaxLatency != 40*time.Millisecond {
		t.Fatalf("MaxLatency = %s, want 40ms", summary.MaxLatency)
	}
	if summary.AvgLatency != 25*time.Millisecond {
		t.Fatalf("AvgLatency = %s, want 25ms", summary.AvgLatency)
	}
	if summary.StatusCodes[http.StatusGatewayTimeout] != 2 {
		t.Fatalf("gateway timeout count = %d, want 2", summary.StatusCodes[http.StatusGatewayTimeout])
	}
	if summary.Errors["timeout"] != 2 {
		t.Fatalf("timeout count = %d, want 2", summary.Errors["timeout"])
	}

	keys := SortedLoadTestErrors(summary)
	if got := strings.Join(keys, ","); got != "timeout,upstream" {
		t.Fatalf("sorted errors = %q, want timeout,upstream", got)
	}
}
