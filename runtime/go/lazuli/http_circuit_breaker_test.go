package lazuli

import (
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestCircuitBreakerOpensAfterFailureThreshold(t *testing.T) {
	clock := newCircuitBreakerTestClock()
	breaker := NewCircuitBreaker(CircuitBreakerOptions{
		FailureThreshold: 2,
		ResetTimeout:     time.Minute,
		Clock:            clock.Now,
	})
	calls := 0
	handler := breaker.Middleware()(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		calls++
		w.WriteHeader(http.StatusInternalServerError)
	}))

	first := serveCircuitBreakerRequest(handler)
	if first.Code != http.StatusInternalServerError {
		t.Fatalf("first status = %d, want %d", first.Code, http.StatusInternalServerError)
	}
	if breaker.State() != CircuitBreakerStateClosed {
		t.Fatalf("state after first failure = %s, want %s", breaker.State(), CircuitBreakerStateClosed)
	}

	second := serveCircuitBreakerRequest(handler)
	if second.Code != http.StatusInternalServerError {
		t.Fatalf("second status = %d, want %d", second.Code, http.StatusInternalServerError)
	}
	if breaker.State() != CircuitBreakerStateOpen {
		t.Fatalf("state after threshold = %s, want %s", breaker.State(), CircuitBreakerStateOpen)
	}

	blocked := serveCircuitBreakerRequest(handler)
	if blocked.Code != http.StatusServiceUnavailable {
		t.Fatalf("blocked status = %d, want %d", blocked.Code, http.StatusServiceUnavailable)
	}
	if calls != 2 {
		t.Fatalf("downstream calls = %d, want 2", calls)
	}
	if got := blocked.Header().Get("Content-Type"); got != "application/problem+json" {
		t.Fatalf("Content-Type = %q, want application/problem+json", got)
	}
	if got := blocked.Header().Get("Retry-After"); got != "60" {
		t.Fatalf("Retry-After = %q, want 60", got)
	}
	body := decodeProblemResponse(t, blocked)
	if body["status"] != float64(http.StatusServiceUnavailable) {
		t.Fatalf("problem status = %v, want %d", body["status"], http.StatusServiceUnavailable)
	}
	if body["detail"] != "circuit breaker is open" {
		t.Fatalf("problem detail = %v, want circuit breaker is open", body["detail"])
	}
	if body["code"] != CodeCircuitBreakerOpen {
		t.Fatalf("problem code = %v, want %s", body["code"], CodeCircuitBreakerOpen)
	}
}

func TestCircuitBreakerSuccessResetsConsecutiveFailures(t *testing.T) {
	clock := newCircuitBreakerTestClock()
	statuses := []int{
		http.StatusInternalServerError,
		http.StatusNoContent,
		http.StatusInternalServerError,
	}
	breaker := NewCircuitBreaker(CircuitBreakerOptions{
		FailureThreshold: 2,
		ResetTimeout:     time.Minute,
		Clock:            clock.Now,
	})
	handler := breaker.Middleware()(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		status := statuses[0]
		statuses = statuses[1:]
		w.WriteHeader(status)
	}))

	for _, want := range []int{http.StatusInternalServerError, http.StatusNoContent, http.StatusInternalServerError} {
		rec := serveCircuitBreakerRequest(handler)
		if rec.Code != want {
			t.Fatalf("status = %d, want %d", rec.Code, want)
		}
	}
	if breaker.State() != CircuitBreakerStateClosed {
		t.Fatalf("state = %s, want %s", breaker.State(), CircuitBreakerStateClosed)
	}
}

func TestCircuitBreakerHalfOpenProbeSuccessClosesCircuit(t *testing.T) {
	clock := newCircuitBreakerTestClock()
	breaker := NewCircuitBreaker(CircuitBreakerOptions{
		FailureThreshold: 1,
		ResetTimeout:     time.Minute,
		Clock:            clock.Now,
	})
	status := http.StatusInternalServerError
	handler := breaker.Middleware()(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		if status == http.StatusNoContent && breaker.State() != CircuitBreakerStateHalfOpen {
			t.Fatalf("state during probe = %s, want %s", breaker.State(), CircuitBreakerStateHalfOpen)
		}
		w.WriteHeader(status)
	}))

	if rec := serveCircuitBreakerRequest(handler); rec.Code != http.StatusInternalServerError {
		t.Fatalf("opening status = %d, want %d", rec.Code, http.StatusInternalServerError)
	}
	if breaker.State() != CircuitBreakerStateOpen {
		t.Fatalf("state = %s, want %s", breaker.State(), CircuitBreakerStateOpen)
	}

	status = http.StatusNoContent
	clock.Advance(time.Minute)
	if rec := serveCircuitBreakerRequest(handler); rec.Code != http.StatusNoContent {
		t.Fatalf("probe status = %d, want %d", rec.Code, http.StatusNoContent)
	}
	if breaker.State() != CircuitBreakerStateClosed {
		t.Fatalf("state after probe = %s, want %s", breaker.State(), CircuitBreakerStateClosed)
	}
}

func TestCircuitBreakerHalfOpenProbeFailureReopensCircuit(t *testing.T) {
	clock := newCircuitBreakerTestClock()
	breaker := NewCircuitBreaker(CircuitBreakerOptions{
		FailureThreshold: 1,
		ResetTimeout:     time.Minute,
		Clock:            clock.Now,
	})
	handler := breaker.Middleware()(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusBadGateway)
	}))

	if rec := serveCircuitBreakerRequest(handler); rec.Code != http.StatusBadGateway {
		t.Fatalf("opening status = %d, want %d", rec.Code, http.StatusBadGateway)
	}
	clock.Advance(time.Minute)
	if rec := serveCircuitBreakerRequest(handler); rec.Code != http.StatusBadGateway {
		t.Fatalf("probe status = %d, want %d", rec.Code, http.StatusBadGateway)
	}
	if breaker.State() != CircuitBreakerStateOpen {
		t.Fatalf("state after failed probe = %s, want %s", breaker.State(), CircuitBreakerStateOpen)
	}

	blocked := serveCircuitBreakerRequest(handler)
	if blocked.Code != http.StatusServiceUnavailable {
		t.Fatalf("blocked status = %d, want %d", blocked.Code, http.StatusServiceUnavailable)
	}
}

func TestCircuitBreakerUsesCustomFailureClassifier(t *testing.T) {
	clock := newCircuitBreakerTestClock()
	breaker := NewCircuitBreaker(CircuitBreakerOptions{
		FailureThreshold: 1,
		ResetTimeout:     time.Minute,
		Clock:            clock.Now,
		FailureClassifier: func(status int) bool {
			return status == http.StatusTooManyRequests
		},
	})
	handler := breaker.Middleware()(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusTooManyRequests)
	}))

	if rec := serveCircuitBreakerRequest(handler); rec.Code != http.StatusTooManyRequests {
		t.Fatalf("first status = %d, want %d", rec.Code, http.StatusTooManyRequests)
	}
	if breaker.State() != CircuitBreakerStateOpen {
		t.Fatalf("state = %s, want %s", breaker.State(), CircuitBreakerStateOpen)
	}
}

type circuitBreakerTestClock struct {
	now time.Time
}

func newCircuitBreakerTestClock() *circuitBreakerTestClock {
	return &circuitBreakerTestClock{now: time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)}
}

func (c *circuitBreakerTestClock) Now() time.Time {
	return c.now
}

func (c *circuitBreakerTestClock) Advance(d time.Duration) {
	c.now = c.now.Add(d)
}

func serveCircuitBreakerRequest(handler http.Handler) *httptest.ResponseRecorder {
	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/", nil))
	return rec
}
