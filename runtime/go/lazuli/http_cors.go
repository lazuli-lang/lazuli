package lazuli

import (
	"net/http"
	"net/url"
	"strconv"
	"strings"
)

const (
	headerOrigin                        = "Origin"
	headerAccessControlRequestMethod    = "Access-Control-Request-Method"
	headerAccessControlRequestHeaders   = "Access-Control-Request-Headers"
	headerAccessControlAllowOrigin      = "Access-Control-Allow-Origin"
	headerAccessControlAllowCredentials = "Access-Control-Allow-Credentials"
	headerAccessControlAllowMethods     = "Access-Control-Allow-Methods"
	headerAccessControlAllowHeaders     = "Access-Control-Allow-Headers"
	headerAccessControlMaxAge           = "Access-Control-Max-Age"
	corsAllowMethods                    = "GET, POST, PUT, PATCH, DELETE"
)

// CORSMiddleware applies the app-level CORS policy emitted as AppCors.
//
// Allow accepts exact origins, wildcard subdomains such as
// "https://*.example.com", or "*" for public non-credentialed APIs. A wildcard
// origin with Credentials is rejected at construction time because browsers
// reject that combination and silently shipping it would be insecure.
func CORSMiddleware(config AppCors) func(http.Handler) http.Handler {
	policy := newCORSPolicy(config)

	return func(next http.Handler) http.Handler {
		if !policy.enabled {
			return next
		}

		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			header := w.Header()
			addVaryHeader(header, headerOrigin)

			preflight := isCORSPreflight(r)
			if preflight {
				addVaryHeader(header, headerAccessControlRequestMethod)
				addVaryHeader(header, headerAccessControlRequestHeaders)
			}

			origin := strings.TrimSpace(r.Header.Get(headerOrigin))
			if origin == "" {
				next.ServeHTTP(w, r)
				return
			}

			allowOrigin, ok := policy.allowOrigin(origin)
			if !ok {
				if preflight {
					w.WriteHeader(http.StatusForbidden)
					return
				}
				next.ServeHTTP(w, r)
				return
			}

			policy.applySimpleHeaders(header, allowOrigin)
			if !preflight {
				next.ServeHTTP(w, r)
				return
			}

			if !corsMethodAllowed(r.Header.Get(headerAccessControlRequestMethod)) {
				w.WriteHeader(http.StatusMethodNotAllowed)
				return
			}

			policy.applyPreflightHeaders(header, r)
			w.WriteHeader(http.StatusNoContent)
		})
	}
}

type corsPolicy struct {
	enabled         bool
	allowAny        bool
	credentials     bool
	maxAge          int64
	exactOrigins    map[string]struct{}
	wildcardOrigins []corsWildcardOrigin
}

type corsWildcardOrigin struct {
	scheme string
	suffix string
	port   string
}

func newCORSPolicy(config AppCors) corsPolicy {
	policy := corsPolicy{
		credentials:  config.Credentials,
		maxAge:       config.MaxAge,
		exactOrigins: make(map[string]struct{}),
	}

	for _, origin := range config.Allow {
		origin = strings.TrimSpace(origin)
		if origin == "" {
			continue
		}
		if origin == "*" {
			if config.Credentials {
				panic("lazuli: AppCors cannot combine wildcard origin with credentials")
			}
			policy.enabled = true
			policy.allowAny = true
			continue
		}

		if wildcard, ok := parseCORSWildcardOrigin(origin); ok {
			policy.enabled = true
			policy.wildcardOrigins = append(policy.wildcardOrigins, wildcard)
			continue
		}

		if canonical, ok := canonicalCORSOrigin(origin); ok {
			policy.enabled = true
			policy.exactOrigins[canonical] = struct{}{}
		}
	}

	return policy
}

func (p corsPolicy) allowOrigin(origin string) (string, bool) {
	if p.allowAny {
		return "*", true
	}

	canonical, ok := canonicalCORSOrigin(origin)
	if !ok {
		return "", false
	}
	if _, ok := p.exactOrigins[canonical]; ok {
		return origin, true
	}

	u, err := url.Parse(canonical)
	if err != nil {
		return "", false
	}
	host := strings.ToLower(u.Hostname())
	scheme := strings.ToLower(u.Scheme)
	port := u.Port()
	for _, wildcard := range p.wildcardOrigins {
		if scheme == wildcard.scheme &&
			port == wildcard.port &&
			len(host) > len(wildcard.suffix) &&
			strings.HasSuffix(host, wildcard.suffix) {
			return origin, true
		}
	}

	return "", false
}

func (p corsPolicy) applySimpleHeaders(header http.Header, allowOrigin string) {
	header.Set(headerAccessControlAllowOrigin, allowOrigin)
	if p.credentials {
		header.Set(headerAccessControlAllowCredentials, "true")
	}
}

func (p corsPolicy) applyPreflightHeaders(header http.Header, r *http.Request) {
	header.Set(headerAccessControlAllowMethods, corsAllowMethods)
	if requestHeaders := canonicalCORSRequestHeaders(r.Header.Get(headerAccessControlRequestHeaders)); requestHeaders != "" {
		header.Set(headerAccessControlAllowHeaders, requestHeaders)
	}
	if p.maxAge > 0 {
		header.Set(headerAccessControlMaxAge, strconv.FormatInt(p.maxAge, 10))
	}
}

func isCORSPreflight(r *http.Request) bool {
	return r.Method == http.MethodOptions &&
		strings.TrimSpace(r.Header.Get(headerAccessControlRequestMethod)) != ""
}

func corsMethodAllowed(method string) bool {
	switch strings.ToUpper(strings.TrimSpace(method)) {
	case http.MethodGet, http.MethodPost, http.MethodPut, http.MethodPatch, http.MethodDelete:
		return true
	default:
		return false
	}
}

func canonicalCORSRequestHeaders(value string) string {
	var headers []string
	for _, part := range strings.Split(value, ",") {
		if part = strings.TrimSpace(part); part != "" {
			headers = append(headers, http.CanonicalHeaderKey(part))
		}
	}
	return strings.Join(headers, ", ")
}

func canonicalCORSOrigin(origin string) (string, bool) {
	origin = strings.TrimSpace(origin)
	if origin == "null" {
		return origin, true
	}

	u, err := url.Parse(origin)
	if err != nil ||
		u.Scheme == "" ||
		u.Host == "" ||
		u.User != nil ||
		u.Path != "" ||
		u.RawQuery != "" ||
		u.Fragment != "" {
		return "", false
	}

	return strings.ToLower(u.Scheme) + "://" + strings.ToLower(u.Host), true
}

func parseCORSWildcardOrigin(origin string) (corsWildcardOrigin, bool) {
	u, err := url.Parse(strings.TrimSpace(origin))
	if err != nil ||
		u.Scheme == "" ||
		u.Host == "" ||
		u.User != nil ||
		u.Path != "" ||
		u.RawQuery != "" ||
		u.Fragment != "" {
		return corsWildcardOrigin{}, false
	}

	host := strings.ToLower(u.Hostname())
	if !strings.HasPrefix(host, "*.") || strings.Count(host, "*") != 1 {
		return corsWildcardOrigin{}, false
	}

	suffix := strings.TrimPrefix(host, "*")
	if suffix == "." {
		return corsWildcardOrigin{}, false
	}

	return corsWildcardOrigin{
		scheme: strings.ToLower(u.Scheme),
		suffix: suffix,
		port:   u.Port(),
	}, true
}
