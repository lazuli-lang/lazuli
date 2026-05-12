package lazuli

import (
	"net/http"
	"strconv"
	"sync"
	"time"
)

const (
	defaultCircuitBreakerFailureThreshold = 5
	defaultCircuitBreakerResetTimeout     = 30 * time.Second
)

// CodeCircuitBreakerOpen is the problem extension code returned while a
// circuit breaker is open.
const CodeCircuitBreakerOpen = "circuit_breaker_open"

// CircuitBreakerState is the current request-gating state of a circuit
// breaker.
type CircuitBreakerState int

const (
	CircuitBreakerStateClosed CircuitBreakerState = iota
	CircuitBreakerStateOpen
	CircuitBreakerStateHalfOpen
)

func (s CircuitBreakerState) String() string {
	switch s {
	case CircuitBreakerStateClosed:
		return "closed"
	case CircuitBreakerStateOpen:
		return "open"
	case CircuitBreakerStateHalfOpen:
		return "half-open"
	default:
		return "unknown"
	}
}

// CircuitBreakerFailureClassifier reports whether an HTTP response status
// should count as a failed request.
type CircuitBreakerFailureClassifier func(status int) bool

// DefaultCircuitBreakerFailureClassifier treats 5xx responses as failures.
func DefaultCircuitBreakerFailureClassifier(status int) bool {
	return status >= http.StatusInternalServerError && status <= 599
}

// CircuitBreakerOptions configures an HTTP circuit breaker. Zero values use
// conservative defaults: 5 consecutive failures and a 30 second reset timeout.
type CircuitBreakerOptions struct {
	// FailureThreshold is the number of consecutive classified failures needed
	// to open the circuit. Values less than 1 use the default.
	FailureThreshold int

	// ResetTimeout controls how long the circuit stays open before one
	// half-open probe request is allowed. Values less than or equal to zero use
	// the default.
	ResetTimeout time.Duration

	// FailureClassifier classifies downstream response status codes. Nil uses
	// DefaultCircuitBreakerFailureClassifier.
	FailureClassifier CircuitBreakerFailureClassifier

	// Clock returns the current time. Nil uses time.Now.
	Clock func() time.Time

	// OpenProblem is written when the circuit is open. Missing fields default
	// to a 503 problem with code "circuit_breaker_open".
	OpenProblem Problem
}

// CircuitBreaker tracks failures and gates HTTP requests through closed, open,
// and half-open states.
type CircuitBreaker struct {
	mu                sync.Mutex
	state             CircuitBreakerState
	failures          int
	openedAt          time.Time
	halfOpenInFlight  bool
	failureThreshold  int
	resetTimeout      time.Duration
	failureClassifier CircuitBreakerFailureClassifier
	clock             func() time.Time
	openProblem       Problem
}

// NewCircuitBreaker returns a reusable circuit breaker helper.
func NewCircuitBreaker(options CircuitBreakerOptions) *CircuitBreaker {
	threshold := options.FailureThreshold
	if threshold < 1 {
		threshold = defaultCircuitBreakerFailureThreshold
	}

	resetTimeout := options.ResetTimeout
	if resetTimeout <= 0 {
		resetTimeout = defaultCircuitBreakerResetTimeout
	}

	classifier := options.FailureClassifier
	if classifier == nil {
		classifier = DefaultCircuitBreakerFailureClassifier
	}

	clock := options.Clock
	if clock == nil {
		clock = time.Now
	}

	return &CircuitBreaker{
		state:             CircuitBreakerStateClosed,
		failureThreshold:  threshold,
		resetTimeout:      resetTimeout,
		failureClassifier: classifier,
		clock:             clock,
		openProblem:       normalizeCircuitBreakerOpenProblem(options.OpenProblem),
	}
}

// CircuitBreakerMiddleware returns middleware backed by a new circuit breaker.
func CircuitBreakerMiddleware(options CircuitBreakerOptions) Middleware {
	return NewCircuitBreaker(options).Middleware()
}

// Middleware returns middleware backed by b.
func (b *CircuitBreaker) Middleware() Middleware {
	return func(next http.Handler) http.Handler {
		if b == nil {
			return next
		}
		b.ensureDefaults()

		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			allowed, retryAfter := b.beforeRequest()
			if !allowed {
				writeCircuitBreakerOpen(w, b.openProblem, retryAfter)
				return
			}

			rec := &circuitBreakerResponseWriter{
				ResponseWriter: w,
				status:         http.StatusOK,
			}
			defer func() {
				if recovered := recover(); recovered != nil {
					b.afterRequest(http.StatusInternalServerError)
					panic(recovered)
				}
			}()

			next.ServeHTTP(rec, r)
			b.afterRequest(rec.status)
		})
	}
}

// State returns the current circuit breaker state.
func (b *CircuitBreaker) State() CircuitBreakerState {
	if b == nil {
		return CircuitBreakerStateClosed
	}
	b.ensureDefaults()

	b.mu.Lock()
	defer b.mu.Unlock()
	return b.state
}

func (b *CircuitBreaker) ensureDefaults() {
	b.mu.Lock()
	defer b.mu.Unlock()

	if b.failureThreshold < 1 {
		b.failureThreshold = defaultCircuitBreakerFailureThreshold
	}
	if b.resetTimeout <= 0 {
		b.resetTimeout = defaultCircuitBreakerResetTimeout
	}
	if b.failureClassifier == nil {
		b.failureClassifier = DefaultCircuitBreakerFailureClassifier
	}
	if b.clock == nil {
		b.clock = time.Now
	}
	if b.openProblem.Status == 0 && b.openProblem.Detail == "" && len(b.openProblem.Extensions) == 0 {
		b.openProblem = normalizeCircuitBreakerOpenProblem(b.openProblem)
	}
}

func (b *CircuitBreaker) beforeRequest() (bool, time.Duration) {
	now := b.clock()

	b.mu.Lock()
	defer b.mu.Unlock()

	switch b.state {
	case CircuitBreakerStateClosed:
		return true, 0
	case CircuitBreakerStateOpen:
		elapsed := now.Sub(b.openedAt)
		if elapsed < b.resetTimeout {
			return false, b.resetTimeout - elapsed
		}
		b.state = CircuitBreakerStateHalfOpen
		b.halfOpenInFlight = true
		return true, 0
	case CircuitBreakerStateHalfOpen:
		if b.halfOpenInFlight {
			return false, 0
		}
		b.halfOpenInFlight = true
		return true, 0
	default:
		b.state = CircuitBreakerStateClosed
		return true, 0
	}
}

func (b *CircuitBreaker) afterRequest(status int) {
	failed := b.failureClassifier(status)
	now := b.clock()

	b.mu.Lock()
	defer b.mu.Unlock()

	switch b.state {
	case CircuitBreakerStateClosed:
		if !failed {
			b.failures = 0
			return
		}
		b.failures++
		if b.failures >= b.failureThreshold {
			b.openLocked(now)
		}
	case CircuitBreakerStateHalfOpen:
		b.halfOpenInFlight = false
		if failed {
			b.openLocked(now)
			return
		}
		b.state = CircuitBreakerStateClosed
		b.failures = 0
		b.openedAt = time.Time{}
	}
}

func (b *CircuitBreaker) openLocked(now time.Time) {
	b.state = CircuitBreakerStateOpen
	b.failures = 0
	b.openedAt = now
	b.halfOpenInFlight = false
}

type circuitBreakerResponseWriter struct {
	http.ResponseWriter
	status      int
	wroteHeader bool
}

func (w *circuitBreakerResponseWriter) WriteHeader(code int) {
	if code >= 100 && code < 200 && code != http.StatusSwitchingProtocols {
		w.ResponseWriter.WriteHeader(code)
		return
	}
	if w.wroteHeader {
		return
	}
	w.wroteHeader = true
	w.status = code
	w.ResponseWriter.WriteHeader(code)
}

func (w *circuitBreakerResponseWriter) Write(p []byte) (int, error) {
	if !w.wroteHeader {
		w.WriteHeader(http.StatusOK)
	}
	return w.ResponseWriter.Write(p)
}

func (w *circuitBreakerResponseWriter) Unwrap() http.ResponseWriter {
	return w.ResponseWriter
}

func normalizeCircuitBreakerOpenProblem(problem Problem) Problem {
	if problem.Status == 0 {
		problem.Status = http.StatusServiceUnavailable
	}
	if problem.Detail == "" {
		problem.Detail = "circuit breaker is open"
	}

	extensions := make(map[string]any, len(problem.Extensions)+1)
	for name, value := range problem.Extensions {
		extensions[name] = value
	}
	if _, ok := extensions["code"]; !ok {
		extensions["code"] = CodeCircuitBreakerOpen
	}
	problem.Extensions = extensions
	return problem
}

func writeCircuitBreakerOpen(w http.ResponseWriter, problem Problem, retryAfter time.Duration) {
	if retryAfter > 0 {
		w.Header().Set("Retry-After", strconv.FormatInt(int64((retryAfter+time.Second-1)/time.Second), 10))
	}
	WriteProblem(w, problem)
}
