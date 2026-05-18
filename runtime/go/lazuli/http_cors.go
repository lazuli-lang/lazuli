package lazuli

import (
	"net/http"
	"os"
	"sync/atomic"

	"github.com/rs/cors"
)

// SetCorsContract installs the CORS contract emitted by codegen
// (`var CorsContract = lazuli.AppCors{...}` in the generated
// `lazuli_app.gen.go`). The runtime HTTP middleware applies the
// configured origins for the active environment (LAZULI_ENV).
//
// Called from generated `init()` blocks; production code does not
// invoke this directly.
func SetCorsContract(c *AppCors) { currentCors.Store(c) }

var currentCors atomic.Pointer[AppCors]

// CorsMiddleware returns an http.Handler middleware applying the
// current CORS contract. Selects the origin allowlist for the
// active environment (LAZULI_ENV) and delegates to `github.com/rs/cors`
// for header emission, wildcard subdomain matching, and OPTIONS
// preflight handling.
//
// No contract set → passthrough.
func CorsMiddleware(next http.Handler) http.Handler {
	contract := currentCors.Load()
	if contract == nil || len(contract.AllowOrigins) == 0 {
		return next
	}
	env := os.Getenv("LAZULI_ENV")
	if env == "" {
		env = "local"
	}
	origins := contract.AllowOrigins[env]
	if len(origins) == 0 {
		// Active env has no entries — no CORS headers; the browser
		// will block any cross-origin call, surfacing the misconfig.
		return next
	}
	corsHandler := cors.New(cors.Options{
		AllowedOrigins:   origins,
		AllowCredentials: contract.AllowCredentials,
		MaxAge:           int(contract.MaxAge),
		AllowedMethods:   []string{http.MethodGet, http.MethodPost, http.MethodPut, http.MethodPatch, http.MethodDelete, http.MethodOptions},
		AllowedHeaders:   []string{"Content-Type", "Authorization", "X-Request-ID"},
	})
	return corsHandler.Handler(next)
}
