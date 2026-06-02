package lazuli

import (
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"os"
)

// CSRFMiddleware uses Go 1.26's net/http.CrossOriginProtection to reject
// cross-origin POST/PUT/PATCH/DELETE requests. GET/HEAD/OPTIONS pass through
// unchanged.
//
// Bearer-token / non-browser clients are CSRF-exempt *by construction*:
// CrossOriginProtection only rejects requests whose `Sec-Fetch-Site` marks
// them cross-site (or whose Origin disagrees with Host). API clients that do
// not send a browser `Sec-Fetch-Site`/`Origin` (the Bearer case) flow through
// untouched. The guard therefore protects cookie/session-authenticated,
// state-changing browser requests without taxing token auth.
func CSRFMiddleware(guard *http.CrossOriginProtection) Middleware {
	return func(next http.Handler) http.Handler {
		return guard.Handler(next)
	}
}

// ErrCSRFWildcardProd is returned by NewCSRFGuard when a wildcard ("*") CORS
// origin is configured in a production environment. A wildcard origin combined
// with credentialed (cookie) requests is invalid/insecure, so boot must fail
// loudly rather than serve.
//
// Guard code: CORS-WILDCARD-PROD-001.
var ErrCSRFWildcardProd = errors.New(
	"CORS-WILDCARD-PROD-001: wildcard CORS origin \"*\" is not permitted in a production environment " +
		"(a wildcard origin with credentialed requests is invalid and disables origin isolation); " +
		"configure an explicit AllowOrigins allowlist for the production env",
)

// NewCSRFGuard returns a CrossOriginProtection configured for the declared
// app CORS allowlist.
//
// CSRF enforcement is INDEPENDENT of the CORS wildcard. A wildcard ("*") origin
// never disables CSRF: cookie/session-authenticated state-changing requests are
// always protected. The previous behaviour — `AddInsecureBypassPattern("/")`
// when CORS was "*", silently turning CSRF off app-wide — was a footgun that
// shipped to prod (SEC-CSRF-WILDCARD-OFF) and is removed.
//
// Wildcard handling:
//   - production env (LAZULI_ENV not in the dev allow-list): a "*" origin is an
//     ERROR (ErrCSRFWildcardProd, code CORS-WILDCARD-PROD-001) — refuse to boot.
//   - dev env (dev/local): a "*" origin is WARNED via slog but allowed; it is
//     simply not registered as a trusted cross-origin (so cross-site browser
//     POSTs are still rejected by CSRF — the safe default). Same-origin and
//     Bearer/non-browser clients remain unaffected.
//
// Explicit origins are registered as trusted cross-origins so legitimate SPA
// front-ends on a different origin can issue state-changing requests.
func NewCSRFGuard(allowedOrigins []string) (*http.CrossOriginProtection, error) {
	g := http.NewCrossOriginProtection()
	prod := !devSessionEnvAllowed(normalizeEnv(os.Getenv("LAZULI_ENV")))
	for _, origin := range allowedOrigins {
		if origin == "*" {
			if prod {
				return nil, ErrCSRFWildcardProd
			}
			slog.Warn(
				"wildcard CORS origin \"*\" in a dev environment; CSRF stays enforced "+
					"and \"*\" is NOT registered as a trusted cross-origin "+
					"(cross-site browser writes will be rejected). "+
					"This configuration is rejected outright in production (CORS-WILDCARD-PROD-001).",
				"guard_code", "CORS-WILDCARD-PROD-001",
				"lazuli_env", normalizeEnv(os.Getenv("LAZULI_ENV")),
			)
			continue
		}
		if err := g.AddTrustedOrigin(origin); err != nil {
			return nil, fmt.Errorf("CSRF: invalid trusted origin %q: %w", origin, err)
		}
	}
	return g, nil
}
