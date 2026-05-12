package lazuli

import (
	"errors"
	"net"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"

	"golang.org/x/time/rate"
)

// ErrRateLimitMalformed is returned when a DSL rate-limit string cannot be
// parsed.
var ErrRateLimitMalformed = errors.New("lazuli: rate limit string malformed")

// RateLimitSpec is the executable form of a RateLimit declaration.
type RateLimitSpec struct {
	PerSecond float64
	Burst     int
	Key       RateLimitKey
}

// RateLimitKey declares which caller dimension owns a limiter bucket.
type RateLimitKey int

const (
	RateLimitKeyGlobal RateLimitKey = iota
	RateLimitKeyIP
	RateLimitKeyUser
	RateLimitKeyOrg
)

const (
	rateLimitGCInterval     = time.Minute
	rateLimitMinIdleTTL     = time.Minute
	rateLimitRetryAfterMin  = time.Second
	rateLimitMalformedState = "rate limit middleware misconfigured"
)

var (
	defaultRateLimitStore = newRateLimitStore()
	rateLimitParseCache   sync.Map
)

// RateLimitMiddleware returns an http.Handler that gates next by the declared
// rate-limit string. When the bucket is empty, it returns 429 Too Many
// Requests with a Retry-After header.
//
//	handler = lazuli.RateLimitMiddleware("5 per 10 minutes per ip", next)
func RateLimitMiddleware(limit RateLimit, next http.Handler) http.Handler {
	if strings.TrimSpace(string(limit)) == "" {
		return next
	}

	var (
		once sync.Once
		spec RateLimitSpec
		err  error
	)

	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		once.Do(func() {
			spec, err = ParseRateLimit(limit)
		})
		if err != nil {
			writeError(w, &Error{
				Status:  http.StatusInternalServerError,
				Code:    CodeInternal,
				Message: rateLimitMalformedState,
			})
			return
		}

		bucketKey := rateLimitBucketKey(spec, rateLimitRequestKey(spec.Key, r))
		limiter := defaultRateLimitStore.limiter(bucketKey, spec, time.Now())
		reservation := limiter.Reserve()
		if !reservation.OK() {
			w.Header().Set("Retry-After", "1")
			writeRateLimited(w)
			return
		}
		if delay := reservation.Delay(); delay > 0 {
			reservation.Cancel()
			w.Header().Set("Retry-After", retryAfterSeconds(delay))
			writeRateLimited(w)
			return
		}

		next.ServeHTTP(w, r)
	})
}

// ParseRateLimit exposes the parser for callers that want to inspect a
// RateLimit declaration without wiring an HTTP handler. Supported forms are
// "<N> per <window> [per <key>]" and "<N> per <M> <window> [per <key>]".
// Window units are second/minute/hour/day, with plural forms accepted.
func ParseRateLimit(s RateLimit) (RateLimitSpec, error) {
	normalized := strings.Join(strings.Fields(strings.ToLower(string(s))), " ")
	if normalized == "" {
		return RateLimitSpec{}, ErrRateLimitMalformed
	}

	if cached, ok := rateLimitParseCache.Load(normalized); ok {
		result := cached.(rateLimitParseResult)
		return result.spec, result.err
	}

	spec, err := parseRateLimit(normalized)
	result := rateLimitParseResult{spec: spec, err: err}
	actual, _ := rateLimitParseCache.LoadOrStore(normalized, result)
	result = actual.(rateLimitParseResult)
	return result.spec, result.err
}

type rateLimitParseResult struct {
	spec RateLimitSpec
	err  error
}

// rateLimitStore owns limiter buckets keyed by parsed spec and request key.
// Buckets are removed once they have been idle long enough to refill fully.
type rateLimitStore struct {
	mu       sync.RWMutex
	buckets  map[string]*rate.Limiter
	lastSeen map[string]time.Time
	idleTTL  map[string]time.Duration
	lastGC   time.Time
}

func newRateLimitStore() *rateLimitStore {
	return &rateLimitStore{
		buckets:  make(map[string]*rate.Limiter),
		lastSeen: make(map[string]time.Time),
		idleTTL:  make(map[string]time.Duration),
	}
}

func (s *rateLimitStore) limiter(key string, spec RateLimitSpec, now time.Time) *rate.Limiter {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.buckets == nil {
		s.buckets = make(map[string]*rate.Limiter)
		s.lastSeen = make(map[string]time.Time)
		s.idleTTL = make(map[string]time.Duration)
	}

	limiter := s.buckets[key]
	if limiter == nil {
		limiter = rate.NewLimiter(rate.Limit(spec.PerSecond), spec.Burst)
		s.buckets[key] = limiter
		s.idleTTL[key] = rateLimitIdleTTL(spec)
	}
	s.lastSeen[key] = now

	if s.lastGC.IsZero() || now.Sub(s.lastGC) >= rateLimitGCInterval {
		s.gcLocked(now)
	}

	return limiter
}

func (s *rateLimitStore) gcLocked(now time.Time) {
	s.lastGC = now
	for key, seen := range s.lastSeen {
		ttl := s.idleTTL[key]
		if ttl <= 0 {
			ttl = rateLimitMinIdleTTL
		}
		if now.Sub(seen) > ttl {
			delete(s.buckets, key)
			delete(s.lastSeen, key)
			delete(s.idleTTL, key)
		}
	}
}

func parseRateLimit(s string) (RateLimitSpec, error) {
	parts := strings.Fields(s)
	if len(parts) < 3 || parts[1] != "per" {
		return RateLimitSpec{}, ErrRateLimitMalformed
	}

	burst, err := strconv.Atoi(parts[0])
	if err != nil || burst <= 0 {
		return RateLimitSpec{}, ErrRateLimitMalformed
	}

	windowSize := 1
	unitIndex := 2
	if n, err := strconv.Atoi(parts[2]); err == nil {
		if n <= 0 || len(parts) < 4 {
			return RateLimitSpec{}, ErrRateLimitMalformed
		}
		windowSize = n
		unitIndex = 3
	}

	unit, ok := rateLimitWindowUnit(parts[unitIndex])
	if !ok {
		return RateLimitSpec{}, ErrRateLimitMalformed
	}
	window := time.Duration(windowSize) * unit
	if window <= 0 {
		return RateLimitSpec{}, ErrRateLimitMalformed
	}

	key := RateLimitKeyGlobal
	next := unitIndex + 1
	if next < len(parts) {
		if len(parts)-next != 2 || parts[next] != "per" {
			return RateLimitSpec{}, ErrRateLimitMalformed
		}
		key, ok = rateLimitKey(parts[next+1])
		if !ok {
			return RateLimitSpec{}, ErrRateLimitMalformed
		}
	}

	return RateLimitSpec{
		PerSecond: float64(burst) / window.Seconds(),
		Burst:     burst,
		Key:       key,
	}, nil
}

func rateLimitWindowUnit(s string) (time.Duration, bool) {
	switch strings.TrimSuffix(s, "s") {
	case "second":
		return time.Second, true
	case "minute":
		return time.Minute, true
	case "hour":
		return time.Hour, true
	case "day":
		return 24 * time.Hour, true
	default:
		return 0, false
	}
}

func rateLimitKey(s string) (RateLimitKey, bool) {
	switch s {
	case "ip":
		return RateLimitKeyIP, true
	case "user":
		return RateLimitKeyUser, true
	case "org":
		return RateLimitKeyOrg, true
	default:
		return RateLimitKeyGlobal, false
	}
}

func rateLimitRequestKey(key RateLimitKey, r *http.Request) string {
	switch key {
	case RateLimitKeyIP:
		return rateLimitIP(r)
	case RateLimitKeyUser:
		// TODO: read an already-attached lazuli.Ctx once transports wire it.
		ctx := newRequestCtx(r)
		if ctx.User == nil {
			return ""
		}
		return strconv.FormatInt(int64(ctx.User.ID), 10)
	case RateLimitKeyOrg:
		// TODO: read an already-attached lazuli.Ctx once transports wire it.
		ctx := newRequestCtx(r)
		if ctx.Tenant != nil {
			return strconv.FormatInt(int64(ctx.Tenant.OrgID), 10)
		}
		if ctx.User != nil && ctx.User.OrgID != 0 {
			return strconv.FormatInt(int64(ctx.User.OrgID), 10)
		}
		return ""
	default:
		return ""
	}
}

func rateLimitIP(r *http.Request) string {
	if forwarded := r.Header.Get("X-Forwarded-For"); forwarded != "" {
		parts := strings.Split(forwarded, ",")
		for i := len(parts) - 1; i >= 0; i-- {
			if ip := hostOnly(parts[i]); ip != "" {
				return ip
			}
		}
	}
	return hostOnly(r.RemoteAddr)
}

func hostOnly(addr string) string {
	addr = strings.TrimSpace(addr)
	if addr == "" {
		return ""
	}
	if host, _, err := net.SplitHostPort(addr); err == nil {
		return host
	}
	return strings.Trim(addr, "[]")
}

func rateLimitBucketKey(spec RateLimitSpec, key string) string {
	return strconv.FormatFloat(spec.PerSecond, 'g', -1, 64) +
		"|" + strconv.Itoa(spec.Burst) +
		"|" + strconv.Itoa(int(spec.Key)) +
		"|" + key
}

func rateLimitIdleTTL(spec RateLimitSpec) time.Duration {
	if spec.PerSecond <= 0 || spec.Burst <= 0 {
		return rateLimitMinIdleTTL
	}
	ttl := time.Duration(float64(spec.Burst)/spec.PerSecond*float64(time.Second)) + time.Second
	if ttl < rateLimitMinIdleTTL {
		return rateLimitMinIdleTTL
	}
	return ttl
}

func retryAfterSeconds(delay time.Duration) string {
	if delay < rateLimitRetryAfterMin {
		delay = rateLimitRetryAfterMin
	}
	seconds := int64((delay + time.Second - 1) / time.Second)
	return strconv.FormatInt(seconds, 10)
}

func writeRateLimited(w http.ResponseWriter) {
	WriteProblem(w, Problem{
		Status: http.StatusTooManyRequests,
		Detail: "rate limit exceeded",
		Extensions: map[string]any{
			"code": CodeRateLimited,
		},
	})
}
