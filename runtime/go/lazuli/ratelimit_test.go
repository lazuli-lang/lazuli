package lazuli

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestParseRateLimit(t *testing.T) {
	tests := []struct {
		name       string
		input      RateLimit
		wantRate   float64
		wantLimit  int
		wantWindow time.Duration
		wantKey    RateLimitKey
	}{
		{
			name:       "hour per ip",
			input:      RateLimit{Default: "30 per hour per ip"},
			wantRate:   30.0 / 3600.0,
			wantLimit:  30,
			wantWindow: time.Hour,
			wantKey:    RateLimitKeyIP,
		},
		{
			name:       "multi minute global",
			input:      RateLimit{Default: "5 per 10 minutes"},
			wantRate:   5.0 / 600.0,
			wantLimit:  5,
			wantWindow: 10 * time.Minute,
			wantKey:    RateLimitKeyGlobal,
		},
		{
			name:       "day per org",
			input:      RateLimit{Default: "1 per day per org"},
			wantRate:   1.0 / 86400.0,
			wantLimit:  1,
			wantWindow: 24 * time.Hour,
			wantKey:    RateLimitKeyOrg,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := ParseRateLimit(tt.input)
			if err != nil {
				t.Fatalf("ParseRateLimit(%q) error = %v", tt.input, err)
			}
			if !closeFloat(got.PerSecond(), tt.wantRate) {
				t.Fatalf("PerSecond = %.12f, want %.12f", got.PerSecond(), tt.wantRate)
			}
			if got.Limit != tt.wantLimit {
				t.Fatalf("Limit = %d, want %d", got.Limit, tt.wantLimit)
			}
			if got.Window != tt.wantWindow {
				t.Fatalf("Window = %s, want %s", got.Window, tt.wantWindow)
			}
			if got.Key != tt.wantKey {
				t.Fatalf("Key = %d, want %d", got.Key, tt.wantKey)
			}
		})
	}
}

func TestParseRateLimitMalformed(t *testing.T) {
	for _, input := range []RateLimit{
		{Default: ""},
		{Default: "per hour"},
		{Default: "0 per minute"},
		{Default: "5 every minute"},
		{Default: "5 per fortnight"},
		{Default: "5 per minute per team"},
	} {
		if _, err := ParseRateLimit(input); !errors.Is(err, ErrRateLimitMalformed) {
			t.Fatalf("ParseRateLimit(%v) error = %v, want ErrRateLimitMalformed", input, err)
		}
	}
}

func TestRateLimitDefaultStoreInMemory(t *testing.T) {
	s := activeStore
	if _, ok := s.(inMemoryStore); !ok {
		t.Fatalf("default store should be inMemoryStore; got %T", s)
	}
}

func TestRateLimitMiddlewareAllowsBurst(t *testing.T) {
	previous := activeStore
	activeStore = newInMemoryStore()
	t.Cleanup(func() { activeStore = previous })

	calls := 0
	handler := RateLimitMiddleware(RateLimitFromDefault("2 per hour per ip"), http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		calls++
		w.WriteHeader(http.StatusNoContent)
	}))

	for i := 0; i < 2; i++ {
		rec := httptest.NewRecorder()
		req := httptest.NewRequest(http.MethodGet, "/", nil)
		req.RemoteAddr = "203.0.113.10:1234"
		handler.ServeHTTP(rec, req)
		if rec.Code != http.StatusNoContent {
			t.Fatalf("request %d status = %d, want %d", i+1, rec.Code, http.StatusNoContent)
		}
	}

	if calls != 2 {
		t.Fatalf("next calls = %d, want 2", calls)
	}
}

func TestRateLimitMiddlewareOverflowReturns429(t *testing.T) {
	previous := activeStore
	activeStore = newInMemoryStore()
	t.Cleanup(func() { activeStore = previous })

	handler := RateLimitMiddleware(RateLimitFromDefault("1 per hour per ip"), http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	}))

	for i := 0; i < 2; i++ {
		rec := httptest.NewRecorder()
		req := httptest.NewRequest(http.MethodGet, "/", nil)
		req.RemoteAddr = "203.0.113.20:1234"
		handler.ServeHTTP(rec, req)

		if i == 0 && rec.Code != http.StatusNoContent {
			t.Fatalf("first status = %d, want %d", rec.Code, http.StatusNoContent)
		}
		if i == 1 {
			if rec.Code != http.StatusTooManyRequests {
				t.Fatalf("second status = %d, want %d", rec.Code, http.StatusTooManyRequests)
			}
			if got := rec.Header().Get("Retry-After"); got == "" {
				t.Fatal("Retry-After header is empty")
			}
		}
	}
}

// TestRateLimitNoBoundaryBurst is the regression test for W1-5
// (RATELIMIT-BYPASS). The classic fixed-window bypass: an attacker bunches a
// full `limit` burst at the very END of window N, then — the instant the clock
// ticks into window N+1 — fires another full `limit` burst. A fixed-window
// counter resets its count to zero on the boundary, so it admits 2×limit
// requests in a tiny span straddling the edge. A token bucket only refills
// window/limit per token, so a few milliseconds past the boundary it has
// refilled essentially nothing and the second burst is denied.
//
// We simulate "the very end of window N" by draining the bucket, then wait
// only a SMALL slice past where a fixed window would reset (a fraction of the
// window — not a full window), and confirm the second burst is rejected.
func TestRateLimitNoBoundaryBurst(t *testing.T) {
	store := newInMemoryStore()
	ctx := context.Background()

	const limit = 5
	const window = 2 * time.Second
	const key = "boundary-test"

	allow := func() bool {
		ok, _, err := store.Allow(ctx, key, limit, window)
		if err != nil {
			t.Fatalf("Allow error: %v", err)
		}
		return ok
	}

	// Burst 1 — drain the bucket at the "end of window N": exactly `limit`.
	admittedBurst1 := 0
	for i := 0; i < limit; i++ {
		if allow() {
			admittedBurst1++
		}
	}
	if admittedBurst1 != limit {
		t.Fatalf("burst 1: admitted %d, want %d (full bucket)", admittedBurst1, limit)
	}

	// Bucket is now empty: immediate next request is denied.
	if allow() {
		t.Fatal("request immediately after draining bucket was admitted; token bucket should be empty")
	}

	// Cross the boundary by a SMALL margin — much less than one window. A
	// fixed-window counter would have reset to zero here, re-admitting a full
	// burst. The token bucket, refilling one token per window/limit (= 400ms
	// here), has refilled at most ~0 tokens after a ~50ms hop.
	time.Sleep(50 * time.Millisecond)

	// Burst 2 — attacker fires another `limit` requests right after the edge.
	admittedBurst2 := 0
	for i := 0; i < limit; i++ {
		if allow() {
			admittedBurst2++
		}
	}

	totalAcrossBoundary := admittedBurst1 + admittedBurst2
	if totalAcrossBoundary >= 2*limit {
		t.Fatalf("boundary burst not prevented: admitted %d across the window boundary (old fixed-window would admit %d=2×limit); token bucket must stay near %d",
			totalAcrossBoundary, 2*limit, limit)
	}
	// The second burst must be (almost) entirely rejected: at most a single
	// token could have trickled in during the 50ms hop.
	if admittedBurst2 > 1 {
		t.Fatalf("burst 2 admitted %d requests %dms after a drained bucket; token bucket refill is one per %s, so 0 (or at most 1) expected",
			admittedBurst2, 50, window/limit)
	}
}

// TestRateLimitTokenBucketRefillsSmoothly proves the steady-state rate holds:
// after draining, one token becomes available roughly every window/limit, not
// all at once at a window edge.
func TestRateLimitTokenBucketRefillsSmoothly(t *testing.T) {
	store := newInMemoryStore()
	ctx := context.Background()

	const limit = 4
	const window = 200 * time.Millisecond
	const key = "smooth-test"

	for i := 0; i < limit; i++ {
		if ok, _, _ := store.Allow(ctx, key, limit, window); !ok {
			t.Fatalf("initial drain request %d denied; full bucket should admit %d", i, limit)
		}
	}
	if ok, retryAfter, _ := store.Allow(ctx, key, limit, window); ok {
		t.Fatal("request after drain admitted; bucket should be empty")
	} else if retryAfter <= 0 || retryAfter > window {
		t.Fatalf("retryAfter = %s, want (0, %s] (one refill interval)", retryAfter, window)
	}

	// One refill interval is window/limit. Wait slightly more than that and
	// exactly one token should be available.
	time.Sleep(window/limit + 20*time.Millisecond)
	if ok, _, _ := store.Allow(ctx, key, limit, window); !ok {
		t.Fatal("after one refill interval, one token should be available")
	}
	if ok, _, _ := store.Allow(ctx, key, limit, window); ok {
		t.Fatal("second request after one refill interval admitted; only one token should have refilled")
	}
}

func closeFloat(a, b float64) bool {
	const tolerance = 0.0000000001
	diff := a - b
	if diff < 0 {
		diff = -diff
	}
	return diff <= tolerance
}
