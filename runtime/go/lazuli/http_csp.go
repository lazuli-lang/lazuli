package lazuli

import (
	"net/http"
	"strings"
)

// CSPBuilder produces a Content-Security-Policy header value for a
// specific response. Returning an empty string skips the header.
type CSPBuilder interface {
	Build(w http.ResponseWriter, r *http.Request, ctx Context) string
}

// Context carries the response-shaping signals the CSP builder needs.
// Extend cautiously: the broader this surface gets, the harder the audit.
type Context struct {
	RouteKind RouteKind
	Audience  string
	NeedsEval bool
}

type RouteKind int

const (
	RouteHTML RouteKind = iota
	RouteAPI
	RouteStatic
)

var globalCSPBuilder CSPBuilder = DefaultCSPBuilder{}

// SetCSPBuilder overrides the default CSP builder. Called by
// @plugin/csp-builder's init block.
func SetCSPBuilder(b CSPBuilder) {
	if b == nil {
		globalCSPBuilder = DefaultCSPBuilder{}
		return
	}
	globalCSPBuilder = b
}

// DefaultCSPBuilder ships a strict baseline for HTML responses.
type DefaultCSPBuilder struct{}

func (DefaultCSPBuilder) Build(w http.ResponseWriter, r *http.Request, ctx Context) string {
	if ctx.RouteKind != RouteHTML {
		return ""
	}
	parts := []string{
		"default-src 'self'",
		"script-src 'self'",
		"style-src 'self' 'unsafe-inline'",
		"img-src 'self' data:",
		"connect-src 'self'",
		"frame-ancestors 'none'",
		"form-action 'self'",
	}
	if ctx.NeedsEval {
		parts[1] = "script-src 'self' 'unsafe-eval'"
	}
	return strings.Join(parts, "; ")
}

func cspMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ctx := Context{RouteKind: RouteHTML, Audience: "public"}
		if v := globalCSPBuilder.Build(w, r, ctx); v != "" {
			w.Header().Set("Content-Security-Policy", v)
		}
		next.ServeHTTP(w, r)
	})
}
