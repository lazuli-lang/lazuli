package lazuli

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestTimeoutMiddlewareReturns503OnTimeout(t *testing.T) {
	entered := make(chan struct{})
	release := make(chan struct{})
	writeErr := make(chan error, 1)
	var releaseOnce sync.Once
	releaseHandler := func() {
		releaseOnce.Do(func() {
			close(release)
		})
	}
	t.Cleanup(releaseHandler)

	handler := TimeoutMiddleware(10*time.Millisecond, "request timed out")(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		close(entered)
		<-release
		_, err := w.Write([]byte("late response"))
		writeErr <- err
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	done := make(chan struct{})
	go func() {
		handler.ServeHTTP(rec, req)
		close(done)
	}()

	select {
	case <-entered:
	case <-time.After(time.Second):
		t.Fatal("handler was not called")
	}

	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("handler did not time out")
	}

	releaseHandler()
	select {
	case err := <-writeErr:
		if !errors.Is(err, http.ErrHandlerTimeout) {
			t.Fatalf("late write error = %v, want %v", err, http.ErrHandlerTimeout)
		}
	case <-time.After(time.Second):
		t.Fatal("timed-out handler did not resume")
	}

	if rec.Code != http.StatusServiceUnavailable {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusServiceUnavailable)
	}
	if body := rec.Body.String(); !strings.Contains(body, "request timed out") {
		t.Fatalf("body = %q, want timeout message", body)
	}
}

func TestTimeoutMiddlewareAllowsFastHandler(t *testing.T) {
	hadDeadline := false
	handler := TimeoutMiddleware(time.Second, "request timed out")(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, hadDeadline = r.Context().Deadline()
		w.Header().Set("X-Lazuli-Test", "ok")
		w.WriteHeader(http.StatusAccepted)
		_, _ = w.Write([]byte("accepted"))
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusAccepted {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusAccepted)
	}
	if !hadDeadline {
		t.Fatal("request context did not have a deadline")
	}
	if got := rec.Header().Get("X-Lazuli-Test"); got != "ok" {
		t.Fatalf("X-Lazuli-Test = %q, want ok", got)
	}
	if got := rec.Body.String(); got != "accepted" {
		t.Fatalf("body = %q, want accepted", got)
	}
}

func TestTimeoutMiddlewareZeroOrNegativeTimeoutDisables(t *testing.T) {
	tests := []struct {
		name    string
		timeout time.Duration
	}{
		{name: "zero", timeout: 0},
		{name: "negative", timeout: -time.Second},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			hadDeadline := false
			handler := TimeoutMiddleware(tt.timeout, "request timed out")(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				_, hadDeadline = r.Context().Deadline()
				w.WriteHeader(http.StatusAccepted)
				_, _ = w.Write([]byte("accepted"))
			}))

			rec := httptest.NewRecorder()
			req := httptest.NewRequest(http.MethodGet, "/", nil)

			handler.ServeHTTP(rec, req)

			if rec.Code != http.StatusAccepted {
				t.Fatalf("status = %d, want %d", rec.Code, http.StatusAccepted)
			}
			if hadDeadline {
				t.Fatal("request context had a deadline")
			}
			if got := rec.Body.String(); got != "accepted" {
				t.Fatalf("body = %q, want accepted", got)
			}
		})
	}
}
