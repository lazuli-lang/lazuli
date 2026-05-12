package i18n

import (
	"context"
	"net/http"
	"strings"
	"time"
)

const (
	// TimezoneHeader names the HTTP header used to propagate a user's
	// preferred IANA timezone through Lazuli transports.
	TimezoneHeader = "X-Lazuli-Timezone"

	defaultTimezoneFallback = "UTC"
)

type timezoneContextKey struct{}

// WithTimezone returns a new context carrying loc as the resolved timezone.
func WithTimezone(ctx context.Context, loc *time.Location) context.Context {
	return context.WithValue(ctx, timezoneContextKey{}, loc)
}

// TimezoneFromContext reads the resolved timezone from ctx. It returns nil
// when no timezone has been propagated.
func TimezoneFromContext(ctx context.Context) *time.Location {
	if ctx == nil {
		return nil
	}
	loc, _ := ctx.Value(timezoneContextKey{}).(*time.Location)
	return loc
}

// ParseTimezone resolves name with time.LoadLocation. When allowed is not
// empty, name must exactly match one of its entries. Empty, invalid, or
// disallowed names resolve to fallback; an empty fallback resolves to UTC.
//
// Fallback is trusted application configuration and is loaded even when it is
// not present in allowed. A malformed fallback is returned as an error so
// callers can surface bad configuration at boot.
func ParseTimezone(name string, allowed []string, fallback string) (*time.Location, error) {
	if loc, ok := loadAllowedTimezone(name, allowed); ok {
		return loc, nil
	}
	return loadFallbackTimezone(fallback)
}

// ResolveTimezone resolves timezone precedence for request handling: user
// preference first, tenant default second, and fallback last. Invalid or
// disallowed user values do not mask a valid tenant value.
func ResolveTimezone(userTimezone, tenantTimezone string, allowed []string, fallback string) (*time.Location, error) {
	if loc, ok := loadAllowedTimezone(userTimezone, allowed); ok {
		return loc, nil
	}
	if loc, ok := loadAllowedTimezone(tenantTimezone, allowed); ok {
		return loc, nil
	}
	return loadFallbackTimezone(fallback)
}

// TimezoneFromHeader reads TimezoneHeader from header and resolves it with
// ParseTimezone.
func TimezoneFromHeader(header http.Header, allowed []string, fallback string) (*time.Location, error) {
	if header == nil {
		return ParseTimezone("", allowed, fallback)
	}
	return ParseTimezone(header.Get(TimezoneHeader), allowed, fallback)
}

func loadAllowedTimezone(name string, allowed []string) (*time.Location, bool) {
	name = strings.TrimSpace(name)
	if name == "" || !timezoneAllowed(name, allowed) {
		return nil, false
	}
	loc, err := time.LoadLocation(name)
	if err != nil {
		return nil, false
	}
	return loc, true
}

func loadFallbackTimezone(fallback string) (*time.Location, error) {
	fallback = strings.TrimSpace(fallback)
	if fallback == "" {
		fallback = defaultTimezoneFallback
	}
	return time.LoadLocation(fallback)
}

func timezoneAllowed(name string, allowed []string) bool {
	if len(allowed) == 0 {
		return true
	}
	for _, candidate := range allowed {
		if name == strings.TrimSpace(candidate) {
			return true
		}
	}
	return false
}
