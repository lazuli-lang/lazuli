package lazuli

import (
	"context"
	"errors"
	"net"
	"net/http"
	"net/netip"
	"strconv"
	"strings"
	"sync"
	"time"

	"golang.org/x/time/rate"
)

// ErrRateLimitMalformed is returned when a DSL rate-limit string cannot be
// parsed.
var ErrRateLimitMalformed = errors.New("lazuli: rate limit string malformed")

// RateLimitKey declares which caller dimension owns a limiter bucket.
type RateLimitKey int

const (
	RateLimitKeyGlobal RateLimitKey = iota
	RateLimitKeyIP
	RateLimitKeyUser
	RateLimitKeyOrg
)

// String renders the key dimension as the lowercase DSL scope token.
// "global" is used for the keyless form; "actor" aliases the
// command-pipeline anonymous-caller scope used by handle.go.
func (k RateLimitKey) String() string {
	switch k {
	case RateLimitKeyIP:
		return "ip"
	case RateLimitKeyUser:
		return "user"
	case RateLimitKeyOrg:
		return "org"
	default:
		return "global"
	}
}

const (
	rateLimitMalformedState = "rate limit middleware misconfigured"
)

var (
	activeStore         Store = newInMemoryStore()
	rateLimitParseCache sync.Map
	trustedProxiesMu    sync.RWMutex
	trustedProxies      []netip.Prefix
)

// RateLimitSpec is the single executable form of a RateLimit declaration.
// It is produced by the one-and-only parser (parseRateLimitSpec /
// parseRateLimitString) and consumed by both the HTTP middleware and the
// Command pipeline (handle.go). `Limit` requests are admitted per `Window`
// at a smooth token-bucket rate (no fixed-window boundary burst).
type RateLimitSpec struct {
	// Limit is the number of requests admitted per Window. It also doubles
	// as the token-bucket burst ceiling (a freshly-idle bucket admits up to
	// Limit immediately, then refills smoothly over Window).
	Limit int
	// Window is the period over which Limit requests refill.
	Window time.Duration
	// Key is the caller dimension that owns a bucket.
	Key RateLimitKey
}

// PerSecond is the steady-state refill rate (tokens per second) implied by
// the spec. Retained for callers/tests that reason about the rate directly.
func (s RateLimitSpec) PerSecond() float64 {
	if s.Window <= 0 {
		return 0
	}
	return float64(s.Limit) / s.Window.Seconds()
}

// Store is the backing store for per-bucket rate limiting. The default is an
// in-process token bucket (golang.org/x/time/rate); @lazuli/plugin-ratelimit-redis
// replaces it with a distributed token-bucket / GCRA implementation across replicas.
//
// Allow consumes one token from the bucket addressed by key and reports
// whether the request may proceed. The (limit, window) pair defines the
// bucket: limit requests refill smoothly over window, with a burst ceiling of
// limit. Implementations MUST NOT admit more than ~limit requests per window
// across any window boundary (the property that distinguishes a token bucket
// from the old fixed-window counter). Implementations MUST be safe for
// concurrent use. retryAfter, when allowed is false, is a hint for how long
// the caller should wait before the bucket admits the next request.
type Store interface {
	Allow(ctx context.Context, key string, limit int, window time.Duration) (allowed bool, retryAfter time.Duration, err error)
}

// SetStore replaces the active store. Plugin backends call this from init.
func SetStore(s Store) {
	activeStore = s
}

// SetTrustedProxies configures the CIDRs allowed to set X-Forwarded-For.
// Pass nil/empty to disable XFF (most secure default).
func SetTrustedProxies(cidrs []netip.Prefix) {
	trustedProxiesMu.Lock()
	defer trustedProxiesMu.Unlock()
	trustedProxies = append([]netip.Prefix(nil), cidrs...)
}

// RateLimitMiddleware returns an http.Handler that gates next by the declared
// rate-limit policy. When the resolved bucket is empty (no throttle for
// the active env), the middleware short-circuits to `next`. When the
// bucket is exhausted, it returns 429 Too Many Requests with a
// Retry-After header.
//
// `ir-rate-limit-env-aware` Cell 2: `limit.Resolve()` runs ONCE per
// middleware instance via `sync.Once`. If the host process rotates
// `LAZULI_ENV` (uncommon), the middleware needs to be re-installed.
// Per-request resolution lives on the Command pipeline
// (`handle.go:Handle`) where it matches the runtime's existing
// `LAZULI_ENV` read patterns (CORS, session injection).
//
//	handler = lazuli.RateLimitMiddleware(lazuli.RateLimitFromDefault("5 per 10 minutes per ip"), next)
func RateLimitMiddleware(limit RateLimit, next http.Handler) http.Handler {
	resolved := limit.Resolve()
	if strings.TrimSpace(resolved) == "" {
		return next
	}

	var (
		once sync.Once
		spec RateLimitSpec
		err  error
	)

	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		once.Do(func() {
			spec, err = parseRateLimitString(resolved)
		})
		if err != nil {
			writeError(w, r, &Error{
				Status:  http.StatusInternalServerError,
				Code:    CodeInternal,
				Message: rateLimitMalformedState,
			})
			return
		}

		key := rateLimitBucketKey(spec, rateLimitRequestKey(spec.Key, r))
		allowed, retryAfter, err := activeStore.Allow(r.Context(), key, spec.Limit, spec.Window)
		if err != nil {
			writeError(w, r, &Error{
				Status:  http.StatusInternalServerError,
				Code:    CodeInternal,
				Message: rateLimitMalformedState,
			})
			return
		}
		if !allowed {
			w.Header().Set("Retry-After", retryAfterSeconds(retryAfter))
			writeRateLimited(w)
			return
		}

		next.ServeHTTP(w, r)
	})
}

// ParseRateLimit exposes the parser for callers that want to inspect a
// RateLimit declaration without wiring an HTTP handler. Resolves the
// active env first (Cell 2) and parses the resulting string. Supported
// forms are "<N> per <window> [per <key>]" and
// "<N> per <M> <window> [per <key>]". Window units are
// second/minute/hour/day, with plural forms accepted.
func ParseRateLimit(s RateLimit) (RateLimitSpec, error) {
	return parseRateLimitString(s.Resolve())
}

// parseRateLimitString runs the underlying spec parser on a pre-resolved
// rate-limit literal. Exposed internally so the middleware can resolve
// once and pass the resulting string verbatim. Results are memoized.
func parseRateLimitString(s string) (RateLimitSpec, error) {
	normalized := strings.Join(strings.Fields(strings.ToLower(s)), " ")
	if normalized == "" {
		return RateLimitSpec{}, ErrRateLimitMalformed
	}

	if cached, ok := rateLimitParseCache.Load(normalized); ok {
		result := cached.(rateLimitParseResult)
		return result.spec, result.err
	}

	spec, err := parseRateLimitSpec(normalized)
	result := rateLimitParseResult{spec: spec, err: err}
	actual, _ := rateLimitParseCache.LoadOrStore(normalized, result)
	result = actual.(rateLimitParseResult)
	return result.spec, result.err
}

type rateLimitParseResult struct {
	spec RateLimitSpec
	err  error
}

// inMemoryStore is the default Store: a per-key token bucket backed by
// golang.org/x/time/rate (the same primitive notifications throttling uses).
// A token bucket refills continuously, so it never admits the ~2×limit burst
// a fixed-window counter allows across a window boundary.
type inMemoryStore struct {
	state *inMemoryRateLimitState
}

type inMemoryRateLimitState struct {
	mu       sync.Mutex
	limiters map[string]*rateLimitEntry
	lastGC   time.Time
}

type rateLimitEntry struct {
	limiter  *rate.Limiter
	lastSeen time.Time
}

const (
	rateLimitGCInterval = time.Minute
	// rateLimitIdleTTL evicts buckets untouched for at least this long. It
	// is scaled up to the bucket's own window so a long-window limiter
	// ("1 per day") is not GC'd between requests, which would reset its
	// remaining tokens and reopen the burst.
	rateLimitIdleTTL = time.Minute
)

func newInMemoryStore() inMemoryStore {
	return inMemoryStore{state: &inMemoryRateLimitState{limiters: make(map[string]*rateLimitEntry)}}
}

func (s inMemoryStore) Allow(ctx context.Context, key string, limit int, window time.Duration) (bool, time.Duration, error) {
	if err := ctx.Err(); err != nil {
		return false, 0, err
	}
	if limit <= 0 || window <= 0 {
		// Defensive: a non-positive spec means "no admittance".
		return false, window, nil
	}

	state := s.state
	if state == nil {
		state = newInMemoryStore().state
	}

	now := time.Now()
	state.mu.Lock()
	defer state.mu.Unlock()

	entry := state.limiters[key]
	if entry == nil {
		// One token every window/limit, with a burst ceiling of limit.
		entry = &rateLimitEntry{
			limiter: rate.NewLimiter(rate.Every(window/time.Duration(limit)), limit),
		}
		state.limiters[key] = entry
	}
	entry.lastSeen = now

	reservation := entry.limiter.ReserveN(now, 1)
	if !reservation.OK() {
		return false, window, nil
	}
	delay := reservation.DelayFrom(now)
	if delay > 0 {
		// Bucket is empty right now — do not consume the (future) token.
		reservation.CancelAt(now)
		return false, delay, nil
	}

	if state.lastGC.IsZero() || now.Sub(state.lastGC) >= rateLimitGCInterval {
		state.gcLocked(now, window)
	}

	return true, 0, nil
}

func (s *inMemoryRateLimitState) gcLocked(now time.Time, window time.Duration) {
	s.lastGC = now
	ttl := rateLimitIdleTTL
	if window > ttl {
		ttl = window
	}
	for key, entry := range s.limiters {
		if now.Sub(entry.lastSeen) >= ttl {
			delete(s.limiters, key)
		}
	}
}

// parseRateLimitSpec is the single rate-limit spec parser. It accepts the
// authored DSL grammar:
//
//	"<N> per <window-unit> [per <key>]"
//	"<N> per <M> <window-unit> [per <key>]"
//
// where <window-unit> is second/minute/hour/day (plural accepted) and
// <key> is one of ip/user/org/actor. The "per <key>" suffix is optional
// (absent == global). Earlier the codebase carried TWO parsers for this same
// grammar (an exported `parseRateLimit` returning a PerSecond/Burst struct for
// the HTTP middleware, and this `parseRateLimitSpec` returning a
// Limit/Window/Scope struct for the Command pipeline); they have been
// collapsed into this one function + one RateLimitSpec struct.
func parseRateLimitSpec(literal string) (RateLimitSpec, error) {
	fields := strings.Fields(strings.ToLower(literal))
	if len(fields) < 3 || fields[1] != "per" {
		return RateLimitSpec{}, ErrRateLimitMalformed
	}

	limit, err := strconv.Atoi(fields[0])
	if err != nil || limit <= 0 {
		return RateLimitSpec{}, ErrRateLimitMalformed
	}

	windowSize := 1
	unitIndex := 2
	if n, err := strconv.Atoi(fields[2]); err == nil {
		if n <= 0 || len(fields) < 4 {
			return RateLimitSpec{}, ErrRateLimitMalformed
		}
		windowSize = n
		unitIndex = 3
	}

	unit, ok := rateLimitWindowUnit(fields[unitIndex])
	if !ok {
		return RateLimitSpec{}, ErrRateLimitMalformed
	}
	window := time.Duration(windowSize) * unit
	if window <= 0 {
		return RateLimitSpec{}, ErrRateLimitMalformed
	}

	key := RateLimitKeyGlobal
	next := unitIndex + 1
	if next < len(fields) {
		if len(fields)-next != 2 || fields[next] != "per" {
			return RateLimitSpec{}, ErrRateLimitMalformed
		}
		key, ok = rateLimitScopeKey(fields[next+1])
		if !ok {
			return RateLimitSpec{}, ErrRateLimitMalformed
		}
	}

	return RateLimitSpec{Limit: limit, Window: window, Key: key}, nil
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

// rateLimitScopeKey maps a DSL scope token to a RateLimitKey. Both the HTTP
// middleware vocabulary (ip/user/org) and the Command-pipeline vocabulary
// (ip/user/actor) are accepted by the single parser; "actor" maps to the
// user dimension for bucketing purposes and is resolved against the anonymous
// actor in buildRateLimitKey.
func rateLimitScopeKey(s string) (RateLimitKey, bool) {
	switch s {
	case "ip":
		return RateLimitKeyIP, true
	case "user":
		return RateLimitKeyUser, true
	case "org":
		return RateLimitKeyOrg, true
	case "actor":
		return RateLimitKeyUser, true
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
	return clientIPFromRequest(r)
}

// clientIPFromRequest extracts the client IP, honoring X-Forwarded-For ONLY
// when the immediate remote IP is in the trusted-proxy list.
//
// SECURITY (SEC-S9): without this gate, attackers can forge XFF to bypass
// per-IP rate limits or attribute their requests to victim IPs.
func clientIPFromRequest(r *http.Request) string {
	if r == nil {
		return ""
	}

	remoteIP, err := remoteAddrToIP(r.RemoteAddr)
	if err != nil {
		return r.RemoteAddr
	}

	trustedProxiesMu.RLock()
	trusted := append([]netip.Prefix(nil), trustedProxies...)
	trustedProxiesMu.RUnlock()

	if !isInPrefixes(remoteIP, trusted) {
		return remoteIP.String()
	}

	xff := r.Header.Get("X-Forwarded-For")
	if xff == "" {
		return remoteIP.String()
	}
	parts := strings.Split(xff, ",")
	for i := len(parts) - 1; i >= 0; i-- {
		candidate := strings.TrimSpace(parts[i])
		ip, err := netip.ParseAddr(candidate)
		if err != nil {
			continue
		}
		if isInPrefixes(ip, trusted) {
			continue
		}
		return candidate
	}
	return remoteIP.String()
}

func remoteAddrToIP(addr string) (netip.Addr, error) {
	host, _, err := net.SplitHostPort(strings.TrimSpace(addr))
	if err != nil {
		host = strings.Trim(strings.TrimSpace(addr), "[]")
	}
	return netip.ParseAddr(host)
}

func isInPrefixes(ip netip.Addr, prefixes []netip.Prefix) bool {
	for _, p := range prefixes {
		if p.Contains(ip) {
			return true
		}
	}
	return false
}

// buildRateLimitKey computes the bucket address for the Command pipeline
// (handle.go). It keys on the spec's caller dimension resolved against the
// active lazuli.Ctx. The "actor" scope (anonymous-friendly) maps to
// RateLimitKeyUser at parse time but still resolves to the actor string here.
func buildRateLimitKey(ctx *Ctx, cmd string, spec RateLimitSpec) string {
	scopeValue := "-"
	switch spec.Key {
	case RateLimitKeyIP:
		if ctx != nil {
			if r := requestFromContext(ctx.Context); r != nil {
				scopeValue = clientIPFromRequest(r)
			}
		}
	case RateLimitKeyUser:
		switch {
		case ctx != nil && ctx.User != nil:
			scopeValue = strconv.FormatInt(int64(ctx.User.ID), 10)
		case ctx != nil && ctx.Actor != "":
			scopeValue = string(ctx.Actor)
		}
	case RateLimitKeyOrg:
		if ctx != nil {
			if ctx.Tenant != nil {
				scopeValue = strconv.FormatInt(int64(ctx.Tenant.OrgID), 10)
			} else if ctx.User != nil && ctx.User.OrgID != 0 {
				scopeValue = strconv.FormatInt(int64(ctx.User.OrgID), 10)
			}
		}
	}
	if scopeValue == "" {
		scopeValue = "-"
	}
	return cmd + ":" + scopeValue
}

// rateLimitBucketKey computes the bucket address for the HTTP middleware.
// It folds the resolved spec into the key so two different policies on the
// same caller dimension do not share a bucket.
func rateLimitBucketKey(spec RateLimitSpec, key string) string {
	return strconv.Itoa(spec.Limit) +
		"|" + strconv.FormatInt(int64(spec.Window), 10) +
		"|" + strconv.Itoa(int(spec.Key)) +
		"|" + key
}

func retryAfterSeconds(d time.Duration) string {
	secs := int(d.Round(time.Second) / time.Second)
	if secs < 1 {
		secs = 1
	}
	return strconv.Itoa(secs)
}

func writeRateLimited(w http.ResponseWriter) {
	writeJSON(w, http.StatusTooManyRequests, map[string]string{
		"code":    CodeRateLimited,
		"message": "rate limit exceeded",
	})
}

type rateLimitRequestContextKey struct{}

func contextWithRequest(ctx context.Context, r *http.Request) context.Context {
	if ctx == nil {
		ctx = context.Background()
	}
	return context.WithValue(ctx, rateLimitRequestContextKey{}, r)
}

func requestFromContext(ctx context.Context) *http.Request {
	if ctx == nil {
		return nil
	}
	r, _ := ctx.Value(rateLimitRequestContextKey{}).(*http.Request)
	return r
}
