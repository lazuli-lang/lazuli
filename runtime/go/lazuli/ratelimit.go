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
	rateLimitMalformedState = "rate limit middleware misconfigured"
)

var (
	activeStore         Store = newInMemoryStore()
	rateLimitParseCache sync.Map
	trustedProxiesMu    sync.RWMutex
	trustedProxies      []netip.Prefix
)

// Store is the backing store for per-bucket rate-limit counters. The default is
// in-process; @plugin/ratelimit-redis replaces it with Redis across replicas.
type Store interface {
	// Inc increments the counter for key by 1 and returns the new count.
	// If the key was just created, the store sets its TTL to window.
	Inc(ctx context.Context, key string, window time.Duration) (int, error)
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
			writeError(w, r, &Error{
				Status:  http.StatusInternalServerError,
				Code:    CodeInternal,
				Message: rateLimitMalformedState,
			})
			return
		}

		key := rateLimitBucketKey(spec, rateLimitRequestKey(spec.Key, r))
		count, err := activeStore.Inc(r.Context(), key, rateLimitWindow(spec))
		if err != nil {
			writeError(w, r, &Error{
				Status:  http.StatusInternalServerError,
				Code:    CodeInternal,
				Message: rateLimitMalformedState,
			})
			return
		}
		if count > spec.Burst {
			w.Header().Set("Retry-After", "1")
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

// "30 per 10 minutes per ip"
// "5 per minute per user"
type rateLimitSpec struct {
	Limit  int
	Window time.Duration
	Scope  string // "ip" | "user" | "actor"
}

// inMemoryStore owns counters keyed by parsed spec and request key.
// Buckets are removed once their fixed window expires.
type inMemoryStore struct {
	state *inMemoryRateLimitState
}

type inMemoryRateLimitState struct {
	mu       sync.Mutex
	counters map[string]rateLimitCounter
	lastGC   time.Time
}

type rateLimitCounter struct {
	count   int
	expires time.Time
}

func newInMemoryStore() inMemoryStore {
	return inMemoryStore{state: &inMemoryRateLimitState{counters: make(map[string]rateLimitCounter)}}
}

func (s inMemoryStore) Inc(ctx context.Context, key string, window time.Duration) (int, error) {
	if err := ctx.Err(); err != nil {
		return 0, err
	}

	state := s.state
	if state == nil {
		state = newInMemoryStore().state
	}

	now := time.Now()
	state.mu.Lock()
	defer state.mu.Unlock()

	counter := state.counters[key]
	if !counter.expires.After(now) {
		counter = rateLimitCounter{expires: now.Add(window)}
	}
	counter.count++
	state.counters[key] = counter

	if state.lastGC.IsZero() || now.Sub(state.lastGC) >= rateLimitGCInterval {
		state.gcLocked(now)
	}

	return counter.count, nil
}

func (s *inMemoryRateLimitState) gcLocked(now time.Time) {
	s.lastGC = now
	for key, counter := range s.counters {
		if !counter.expires.After(now) {
			delete(s.counters, key)
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

func parseRateLimitSpec(literal string) (rateLimitSpec, error) {
	fields := strings.Fields(strings.ToLower(literal))
	if len(fields) < 5 {
		return rateLimitSpec{}, errors.New("rate_limit: too few fields")
	}
	if fields[1] != "per" {
		return rateLimitSpec{}, ErrRateLimitMalformed
	}

	limit, err := strconv.Atoi(fields[0])
	if err != nil || limit <= 0 {
		return rateLimitSpec{}, ErrRateLimitMalformed
	}

	windowSize := 1
	unitIndex := 2
	if n, err := strconv.Atoi(fields[2]); err == nil {
		if n <= 0 || len(fields) < 6 {
			return rateLimitSpec{}, ErrRateLimitMalformed
		}
		windowSize = n
		unitIndex = 3
	}

	unit, ok := rateLimitWindowUnit(fields[unitIndex])
	if !ok {
		return rateLimitSpec{}, ErrRateLimitMalformed
	}
	next := unitIndex + 1
	if len(fields)-next != 2 || fields[next] != "per" {
		return rateLimitSpec{}, ErrRateLimitMalformed
	}

	scope := fields[next+1]
	switch scope {
	case "ip", "user", "actor":
	default:
		return rateLimitSpec{}, ErrRateLimitMalformed
	}

	window := time.Duration(windowSize) * unit
	if window <= 0 {
		return rateLimitSpec{}, ErrRateLimitMalformed
	}
	return rateLimitSpec{Limit: limit, Window: window, Scope: scope}, nil
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

func buildRateLimitKey(ctx *Ctx, cmd string, spec rateLimitSpec) string {
	scopeValue := "-"
	switch spec.Scope {
	case "ip":
		if ctx != nil {
			if r := requestFromContext(ctx.Context); r != nil {
				scopeValue = clientIPFromRequest(r)
			}
		}
	case "user":
		if ctx != nil && ctx.User != nil {
			scopeValue = strconv.FormatInt(int64(ctx.User.ID), 10)
		}
	case "actor":
		if ctx != nil && ctx.Actor != "" {
			scopeValue = string(ctx.Actor)
		}
	}
	if scopeValue == "" {
		scopeValue = "-"
	}
	return cmd + ":" + scopeValue
}

func rateLimitBucketKey(spec RateLimitSpec, key string) string {
	return strconv.FormatFloat(spec.PerSecond, 'g', -1, 64) +
		"|" + strconv.Itoa(spec.Burst) +
		"|" + strconv.Itoa(int(spec.Key)) +
		"|" + key
}

func rateLimitWindow(spec RateLimitSpec) time.Duration {
	if spec.PerSecond <= 0 || spec.Burst <= 0 {
		return rateLimitMinIdleTTL
	}
	window := time.Duration(float64(spec.Burst) / spec.PerSecond * float64(time.Second))
	if window <= 0 {
		return rateLimitMinIdleTTL
	}
	return window
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
