package lazuli

import (
	"net/http"
	"strings"
)

const (
	headerStrictTransportSecurity = "Strict-Transport-Security"
	headerContentSecurityPolicy   = "Content-Security-Policy"
	headerXFrameOptions           = "X-Frame-Options"
	headerXContentTypeOptions     = "X-Content-Type-Options"
	headerReferrerPolicy          = "Referrer-Policy"
	headerPermissionsPolicy       = "Permissions-Policy"
)

// SecurityHeaders configures common HTTP response headers that harden browser
// behavior. Empty fields are not written.
type SecurityHeaders struct {
	// StrictTransportSecurity is the HSTS policy written as
	// Strict-Transport-Security when non-empty.
	StrictTransportSecurity string

	// ContentSecurityPolicy is written as Content-Security-Policy when
	// non-empty.
	ContentSecurityPolicy string

	// FrameOptions is written as X-Frame-Options when non-empty.
	FrameOptions string

	// ContentTypeOptions is written as X-Content-Type-Options when non-empty.
	ContentTypeOptions string

	// ReferrerPolicy is written as Referrer-Policy when non-empty.
	ReferrerPolicy string

	// PermissionsPolicy is written as Permissions-Policy when non-empty.
	PermissionsPolicy string

	// OverrideExisting replaces matching headers already set by downstream
	// handlers. When false, downstream headers are preserved.
	OverrideExisting bool
}

// DefaultSecurityHeaders returns conservative browser security headers suitable
// as a starting point for HTTPS applications. Adjust the CSP and permissions
// policy for applications that need third-party assets or browser capabilities.
func DefaultSecurityHeaders() SecurityHeaders {
	return SecurityHeaders{
		StrictTransportSecurity: "max-age=31536000; includeSubDomains",
		ContentSecurityPolicy:   "default-src 'self'; base-uri 'self'; frame-ancestors 'none'; object-src 'none'",
		FrameOptions:            "DENY",
		ContentTypeOptions:      "nosniff",
		ReferrerPolicy:          "no-referrer",
		PermissionsPolicy:       "accelerometer=(), camera=(), geolocation=(), gyroscope=(), magnetometer=(), microphone=(), payment=(), usb=()",
	}
}

// SecurityHeadersMiddleware applies configured browser security headers before
// the response is written. It preserves matching headers set by downstream
// handlers unless SecurityHeaders.OverrideExisting is true.
func SecurityHeadersMiddleware(config SecurityHeaders) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			sw := &securityHeadersResponseWriter{
				ResponseWriter: w,
				config:         config,
			}

			next.ServeHTTP(sw, r)
			if !sw.wroteHeader {
				config.apply(w.Header())
			}
		})
	}
}

func (h SecurityHeaders) apply(header http.Header) {
	h.set(header, headerStrictTransportSecurity, h.StrictTransportSecurity)
	h.set(header, headerContentSecurityPolicy, h.ContentSecurityPolicy)
	h.set(header, headerXFrameOptions, h.FrameOptions)
	h.set(header, headerXContentTypeOptions, h.ContentTypeOptions)
	h.set(header, headerReferrerPolicy, h.ReferrerPolicy)
	h.set(header, headerPermissionsPolicy, h.PermissionsPolicy)
}

func (h SecurityHeaders) set(header http.Header, name, value string) {
	value = strings.TrimSpace(value)
	if value == "" {
		return
	}
	if !h.OverrideExisting && len(header.Values(name)) > 0 {
		return
	}
	header.Set(name, value)
}

type securityHeadersResponseWriter struct {
	http.ResponseWriter
	config      SecurityHeaders
	wroteHeader bool
}

func (w *securityHeadersResponseWriter) WriteHeader(status int) {
	if status >= 100 && status < 200 && status != http.StatusSwitchingProtocols {
		w.ResponseWriter.WriteHeader(status)
		return
	}
	if w.wroteHeader {
		return
	}
	w.wroteHeader = true
	w.config.apply(w.Header())
	w.ResponseWriter.WriteHeader(status)
}

func (w *securityHeadersResponseWriter) Write(p []byte) (int, error) {
	if !w.wroteHeader {
		w.wroteHeader = true
		w.config.apply(w.Header())
	}
	return w.ResponseWriter.Write(p)
}

func (w *securityHeadersResponseWriter) Unwrap() http.ResponseWriter {
	return w.ResponseWriter
}
