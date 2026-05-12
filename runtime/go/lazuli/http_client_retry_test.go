package lazuli

import (
	"context"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestHTTPRetryTransportRetriesStatusAndRewindsBody(t *testing.T) {
	var (
		mu       sync.Mutex
		attempts int
		bodies   []string
	)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, err := io.ReadAll(r.Body)
		if err != nil {
			t.Errorf("ReadAll request body error = %v", err)
		}

		mu.Lock()
		attempts++
		attempt := attempts
		bodies = append(bodies, string(body))
		mu.Unlock()

		if attempt < 3 {
			http.Error(w, "try again", http.StatusServiceUnavailable)
			return
		}
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte("ok"))
	}))
	defer server.Close()

	var delays []time.Duration
	client := HTTPClientWithRetry(server.Client(), HTTPRetryPolicy{
		MaxAttempts: 3,
		BaseDelay:   time.Millisecond,
		MaxDelay:    time.Millisecond,
		Sleep: func(ctx context.Context, delay time.Duration) error {
			delays = append(delays, delay)
			return ctx.Err()
		},
	})

	req, err := http.NewRequest(http.MethodPut, server.URL, strings.NewReader("payload"))
	if err != nil {
		t.Fatalf("NewRequest error = %v", err)
	}
	resp, err := client.Do(req)
	if err != nil {
		t.Fatalf("Do error = %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status = %d, want %d", resp.StatusCode, http.StatusOK)
	}
	if attempts != 3 {
		t.Fatalf("attempts = %d, want 3", attempts)
	}
	for i, body := range bodies {
		if body != "payload" {
			t.Fatalf("body attempt %d = %q, want payload", i+1, body)
		}
	}
	if len(delays) != 2 {
		t.Fatalf("sleep calls = %d, want 2", len(delays))
	}
}

func TestHTTPRetryTransportDoesNotRetryNonIdempotentMethod(t *testing.T) {
	attempts := 0
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		attempts++
		http.Error(w, "try again", http.StatusServiceUnavailable)
	}))
	defer server.Close()

	client := HTTPClientWithRetry(server.Client(), HTTPRetryPolicy{
		MaxAttempts: 3,
		Sleep: func(context.Context, time.Duration) error {
			t.Fatal("POST should not sleep for retry")
			return nil
		},
	})

	resp, err := client.Post(server.URL, "text/plain", strings.NewReader("payload"))
	if err != nil {
		t.Fatalf("Post error = %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusServiceUnavailable {
		t.Fatalf("status = %d, want %d", resp.StatusCode, http.StatusServiceUnavailable)
	}
	if attempts != 1 {
		t.Fatalf("attempts = %d, want 1", attempts)
	}
}

func TestHTTPRetryTransportRetries429AndHonorsRetryAfter(t *testing.T) {
	attempts := 0
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		attempts++
		if attempts == 1 {
			w.Header().Set("Retry-After", "2")
			http.Error(w, "rate limited", http.StatusTooManyRequests)
			return
		}
		w.WriteHeader(http.StatusNoContent)
	}))
	defer server.Close()

	var delays []time.Duration
	client := HTTPClientWithRetry(server.Client(), HTTPRetryPolicy{
		MaxAttempts: 2,
		BaseDelay:   time.Millisecond,
		MaxDelay:    5 * time.Second,
		Sleep: func(ctx context.Context, delay time.Duration) error {
			delays = append(delays, delay)
			return ctx.Err()
		},
	})

	resp, err := client.Get(server.URL)
	if err != nil {
		t.Fatalf("Get error = %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusNoContent {
		t.Fatalf("status = %d, want %d", resp.StatusCode, http.StatusNoContent)
	}
	if attempts != 2 {
		t.Fatalf("attempts = %d, want 2", attempts)
	}
	if len(delays) != 1 || delays[0] != 2*time.Second {
		t.Fatalf("delays = %v, want [2s]", delays)
	}
}

func TestHTTPRetryTransportRetriesTransportError(t *testing.T) {
	temporaryErr := errors.New("temporary")
	attempts := 0
	transport := &HTTPRetryTransport{
		Base: roundTripFunc(func(req *http.Request) (*http.Response, error) {
			attempts++
			if attempts == 1 {
				return nil, temporaryErr
			}
			return &http.Response{
				StatusCode: http.StatusOK,
				Status:     "200 OK",
				Header:     make(http.Header),
				Body:       io.NopCloser(strings.NewReader("ok")),
				Request:    req,
			}, nil
		}),
		Policy: HTTPRetryPolicy{
			MaxAttempts: 2,
			BaseDelay:   time.Millisecond,
			Sleep: func(ctx context.Context, delay time.Duration) error {
				return ctx.Err()
			},
		},
	}

	req, err := http.NewRequest(http.MethodGet, "http://example.test", nil)
	if err != nil {
		t.Fatalf("NewRequest error = %v", err)
	}
	resp, err := transport.RoundTrip(req)
	if err != nil {
		t.Fatalf("RoundTrip error = %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status = %d, want %d", resp.StatusCode, http.StatusOK)
	}
	if attempts != 2 {
		t.Fatalf("attempts = %d, want 2", attempts)
	}
}

func TestHTTPRetryTransportDoesNotRetryBodyWithoutGetBody(t *testing.T) {
	attempts := 0
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		attempts++
		http.Error(w, "try again", http.StatusServiceUnavailable)
	}))
	defer server.Close()

	client := HTTPClientWithRetry(server.Client(), HTTPRetryPolicy{
		MaxAttempts: 3,
		Sleep: func(context.Context, time.Duration) error {
			t.Fatal("request without GetBody should not sleep for retry")
			return nil
		},
	})

	req, err := http.NewRequest(http.MethodPut, server.URL, io.NopCloser(strings.NewReader("payload")))
	if err != nil {
		t.Fatalf("NewRequest error = %v", err)
	}
	if req.GetBody != nil {
		t.Fatal("test request unexpectedly has GetBody")
	}

	resp, err := client.Do(req)
	if err != nil {
		t.Fatalf("Do error = %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusServiceUnavailable {
		t.Fatalf("status = %d, want %d", resp.StatusCode, http.StatusServiceUnavailable)
	}
	if attempts != 1 {
		t.Fatalf("attempts = %d, want 1", attempts)
	}
}

func TestHTTPRetryTransportStopsWhenContextCancelledDuringBackoff(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	attempts := 0
	transport := &HTTPRetryTransport{
		Base: roundTripFunc(func(req *http.Request) (*http.Response, error) {
			attempts++
			cancel()
			return &http.Response{
				StatusCode: http.StatusServiceUnavailable,
				Status:     "503 Service Unavailable",
				Header:     make(http.Header),
				Body:       io.NopCloser(strings.NewReader("try again")),
				Request:    req,
			}, nil
		}),
		Policy: HTTPRetryPolicy{
			MaxAttempts: 2,
			BaseDelay:   time.Millisecond,
			Sleep: func(ctx context.Context, delay time.Duration) error {
				if !errors.Is(ctx.Err(), context.Canceled) {
					t.Fatalf("ctx.Err() = %v, want context.Canceled", ctx.Err())
				}
				return ctx.Err()
			},
		},
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, "http://example.test", nil)
	if err != nil {
		t.Fatalf("NewRequestWithContext error = %v", err)
	}
	resp, err := transport.RoundTrip(req)
	if resp != nil {
		t.Fatalf("resp = %#v, want nil", resp)
	}
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("RoundTrip error = %v, want context.Canceled", err)
	}
	if attempts != 1 {
		t.Fatalf("attempts = %d, want 1", attempts)
	}
}

type roundTripFunc func(*http.Request) (*http.Response, error)

func (fn roundTripFunc) RoundTrip(req *http.Request) (*http.Response, error) {
	return fn(req)
}
