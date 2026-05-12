package lazuli

import (
	"net/http"
	"strconv"
	"strings"
)

const (
	defaultHSTSMaxAgeSeconds = 31536000
	headerXForwardedProto    = "X-Forwarded-Proto"
)

// HSTSConfig configures Strict-Transport-Security header generation.
type HSTSConfig struct {
	// MaxAgeSeconds controls the max-age directive in seconds. Use
	// DefaultHSTSConfig for Lazuli's production default.
	MaxAgeSeconds int64

	// IncludeSubDomains appends the includeSubDomains directive.
	IncludeSubDomains bool

	// Preload appends the preload directive for applications that are ready for
	// browser preload lists.
	Preload bool

	// TrustXForwardedProto treats X-Forwarded-Proto: https as an HTTPS request.
	// Enable this only behind a proxy that controls or strips that header.
	TrustXForwardedProto bool

	// AllowHTTP writes the header even when the request is not HTTPS. It is
	// false by default because browsers only honor HSTS received over HTTPS.
	AllowHTTP bool

	// OverrideExisting replaces a downstream Strict-Transport-Security header.
	// When false, downstream headers are preserved.
	OverrideExisting bool
}

// DefaultHSTSConfig returns Lazuli's production HSTS policy: one year max-age,
// includeSubDomains, HTTPS-only emission, and X-Forwarded-Proto awareness for
// common reverse proxy deployments.
func DefaultHSTSConfig() HSTSConfig {
	return HSTSConfig{
		MaxAgeSeconds:        defaultHSTSMaxAgeSeconds,
		IncludeSubDomains:    true,
		TrustXForwardedProto: true,
	}
}

// HSTSHeaderValue builds a Strict-Transport-Security value from config.
func HSTSHeaderValue(config HSTSConfig) string {
	maxAge := config.MaxAgeSeconds
	if maxAge < 0 {
		maxAge = 0
	}

	parts := []string{"max-age=" + strconv.FormatInt(maxAge, 10)}
	if config.IncludeSubDomains {
		parts = append(parts, "includeSubDomains")
	}
	if config.Preload {
		parts = append(parts, "preload")
	}
	return strings.Join(parts, "; ")
}

// DefaultHSTSMiddleware returns HSTSMiddleware configured with
// DefaultHSTSConfig.
func DefaultHSTSMiddleware() Middleware {
	return HSTSMiddleware(DefaultHSTSConfig())
}

// HSTSMiddleware writes Strict-Transport-Security for HTTPS requests. Direct
// TLS requests are treated as HTTPS; X-Forwarded-Proto is honored when
// HSTSConfig.TrustXForwardedProto is true.
func HSTSMiddleware(config HSTSConfig) Middleware {
	headers := SecurityHeaders{
		StrictTransportSecurity: HSTSHeaderValue(config),
		OverrideExisting:        config.OverrideExisting,
	}
	withHSTS := SecurityHeadersMiddleware(headers)

	return func(next http.Handler) http.Handler {
		nextWithHSTS := withHSTS(next)
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if !config.AllowHTTP && !hstsRequestIsHTTPS(r, config.TrustXForwardedProto) {
				next.ServeHTTP(w, r)
				return
			}

			nextWithHSTS.ServeHTTP(w, r)
		})
	}
}

func hstsRequestIsHTTPS(r *http.Request, trustXForwardedProto bool) bool {
	if r == nil {
		return false
	}
	if r.TLS != nil {
		return true
	}
	if !trustXForwardedProto {
		return false
	}
	return firstHSTSForwardedProto(r.Header.Values(headerXForwardedProto)) == "https"
}

func firstHSTSForwardedProto(values []string) string {
	for _, value := range values {
		for _, part := range strings.Split(value, ",") {
			proto := strings.ToLower(strings.TrimSpace(part))
			if proto != "" {
				return proto
			}
		}
	}
	return ""
}
