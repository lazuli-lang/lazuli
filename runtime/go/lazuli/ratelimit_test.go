package lazuli

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestParseRateLimit(t *testing.T) {
	tests := []struct {
		name      string
		input     RateLimit
		wantRate  float64
		wantBurst int
		wantKey   RateLimitKey
	}{
		{
			name:      "hour per ip",
			input:     "30 per hour per ip",
			wantRate:  30.0 / 3600.0,
			wantBurst: 30,
			wantKey:   RateLimitKeyIP,
		},
		{
			name:      "multi minute global",
			input:     "5 per 10 minutes",
			wantRate:  5.0 / 600.0,
			wantBurst: 5,
			wantKey:   RateLimitKeyGlobal,
		},
		{
			name:      "day per org",
			input:     "1 per day per org",
			wantRate:  1.0 / 86400.0,
			wantBurst: 1,
			wantKey:   RateLimitKeyOrg,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := ParseRateLimit(tt.input)
			if err != nil {
				t.Fatalf("ParseRateLimit(%q) error = %v", tt.input, err)
			}
			if !closeFloat(got.PerSecond, tt.wantRate) {
				t.Fatalf("PerSecond = %.12f, want %.12f", got.PerSecond, tt.wantRate)
			}
			if got.Burst != tt.wantBurst {
				t.Fatalf("Burst = %d, want %d", got.Burst, tt.wantBurst)
			}
			if got.Key != tt.wantKey {
				t.Fatalf("Key = %d, want %d", got.Key, tt.wantKey)
			}
		})
	}
}

func TestParseRateLimitMalformed(t *testing.T) {
	for _, input := range []RateLimit{
		"",
		"per hour",
		"0 per minute",
		"5 every minute",
		"5 per fortnight",
		"5 per minute per team",
	} {
		if _, err := ParseRateLimit(input); !errors.Is(err, ErrRateLimitMalformed) {
			t.Fatalf("ParseRateLimit(%q) error = %v, want ErrRateLimitMalformed", input, err)
		}
	}
}

func TestRateLimitMiddlewareAllowsBurst(t *testing.T) {
	defaultRateLimitStore = newRateLimitStore()

	calls := 0
	handler := RateLimitMiddleware("2 per hour per ip", http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
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
	defaultRateLimitStore = newRateLimitStore()

	handler := RateLimitMiddleware("1 per hour per ip", http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
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

func closeFloat(a, b float64) bool {
	const tolerance = 0.0000000001
	diff := a - b
	if diff < 0 {
		diff = -diff
	}
	return diff <= tolerance
}
