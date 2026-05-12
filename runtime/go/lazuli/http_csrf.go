package lazuli

import "net/http"

// CSRFMiddleware uses Go 1.26's net/http.CrossOriginProtection to reject
// cross-origin POST/PUT/PATCH/DELETE requests. GET/HEAD/OPTIONS pass through
// unchanged.
func CSRFMiddleware(guard *http.CrossOriginProtection) Middleware {
	return func(next http.Handler) http.Handler {
		return guard.Handler(next)
	}
}

// NewCSRFGuard returns a CrossOriginProtection configured for the declared
// app CORS allowlist. Production code should pass the AppCors.Allow slice; dev
// can use "*" to bypass CSRF checks.
func NewCSRFGuard(allowedOrigins []string) *http.CrossOriginProtection {
	g := http.NewCrossOriginProtection()
	for _, origin := range allowedOrigins {
		if origin == "*" {
			g.AddInsecureBypassPattern("/")
			continue
		}
		if err := g.AddTrustedOrigin(origin); err != nil {
			panic(err)
		}
	}
	return g
}
