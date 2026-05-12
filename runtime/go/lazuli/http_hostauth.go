package lazuli

import (
	"net"
	"net/http"
	"strconv"
	"strings"
)

const headerXForwardedHost = "X-Forwarded-Host"

// HostAuthorization configures HostAuthorizationMiddleware.
type HostAuthorization struct {
	// AllowedHosts lists exact hosts and wildcard suffixes that are allowed to
	// reach the application. Wildcards must begin with "*." and match
	// subdomains only, so "*.example.com" matches "api.example.com" but not
	// "example.com".
	AllowedHosts []string

	// NormalizePort strips a valid port from Host and X-Forwarded-Host values
	// before matching. When false, ports are part of exact and wildcard
	// matches.
	NormalizePort bool

	// AllowLocalhost permits common local development hosts, including
	// localhost, *.localhost, loopback IP literals, and unspecified IP
	// literals, with or without ports.
	AllowLocalhost bool

	// StatusCode controls the rejection status. Zero and unsupported values
	// default to 421 Misdirected Request; 403 Forbidden is also supported.
	StatusCode int
}

// HostAuthorizationMiddleware rejects requests whose Host or
// X-Forwarded-Host values are not explicitly allowed by config.
func HostAuthorizationMiddleware(config HostAuthorization) func(http.Handler) http.Handler {
	matcher := newHostAuthorizationMatcher(config)

	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if !matcher.authorized(r) {
				writeHostAuthorizationRejection(w, r, matcher.statusCode)
				return
			}

			next.ServeHTTP(w, r)
		})
	}
}

type hostAuthorizationMatcher struct {
	exact            map[string]struct{}
	wildcardSuffixes []string
	normalizePort    bool
	allowLocalhost   bool
	statusCode       int
}

func newHostAuthorizationMatcher(config HostAuthorization) hostAuthorizationMatcher {
	matcher := hostAuthorizationMatcher{
		exact:          make(map[string]struct{}),
		normalizePort:  config.NormalizePort,
		allowLocalhost: config.AllowLocalhost,
		statusCode:     hostAuthorizationStatus(config.StatusCode),
	}

	for _, entry := range config.AllowedHosts {
		host, ok := normalizeHostAuthorizationHost(entry, config.NormalizePort)
		if !ok {
			continue
		}

		if strings.HasPrefix(host, "*.") && len(host) > len("*.") {
			matcher.wildcardSuffixes = append(matcher.wildcardSuffixes, host[1:])
			continue
		}

		matcher.exact[host] = struct{}{}
	}

	return matcher
}

func (m hostAuthorizationMatcher) authorized(r *http.Request) bool {
	if r == nil || !m.allows(r.Host) {
		return false
	}

	for _, value := range r.Header.Values(headerXForwardedHost) {
		for _, host := range strings.Split(value, ",") {
			if !m.allows(host) {
				return false
			}
		}
	}

	return true
}

func (m hostAuthorizationMatcher) allows(value string) bool {
	host, ok := normalizeHostAuthorizationHost(value, m.normalizePort)
	if !ok {
		return false
	}
	if strings.Contains(host, "*") {
		return false
	}

	if _, ok := m.exact[host]; ok {
		return true
	}

	for _, suffix := range m.wildcardSuffixes {
		if len(host) > len(suffix) && strings.HasSuffix(host, suffix) {
			return true
		}
	}

	return m.allowLocalhost && hostAuthorizationLocalhost(host)
}

func normalizeHostAuthorizationHost(value string, stripPort bool) (string, bool) {
	host := strings.TrimSpace(value)
	host = strings.Trim(host, `"`)
	if host == "" || strings.ContainsAny(host, " \t\r\n/\\,") {
		return "", false
	}

	if stripPort {
		var ok bool
		host, ok = stripHostAuthorizationPort(host)
		if !ok {
			return "", false
		}
	}

	host = strings.TrimRight(strings.ToLower(strings.TrimSpace(host)), ".")
	if host == "" {
		return "", false
	}
	return host, true
}

func stripHostAuthorizationPort(host string) (string, bool) {
	if strings.HasPrefix(host, "[") {
		end := strings.IndexByte(host, ']')
		if end < 0 {
			return "", false
		}

		addr := host[1:end]
		rest := host[end+1:]
		if rest == "" {
			return addr, true
		}
		if !strings.HasPrefix(rest, ":") || !validHostAuthorizationPort(rest[1:]) {
			return "", false
		}
		return addr, true
	}

	if hostPart, port, err := net.SplitHostPort(host); err == nil {
		if !validHostAuthorizationPort(port) {
			return "", false
		}
		return hostPart, true
	}

	if strings.Count(host, ":") == 1 {
		hostPart, port, _ := strings.Cut(host, ":")
		if hostPart == "" || !validHostAuthorizationPort(port) {
			return "", false
		}
		return hostPart, true
	}

	return host, true
}

func validHostAuthorizationPort(port string) bool {
	if port == "" {
		return false
	}
	for _, r := range port {
		if r < '0' || r > '9' {
			return false
		}
	}

	value, err := strconv.Atoi(port)
	return err == nil && value >= 0 && value <= 65535
}

func hostAuthorizationLocalhost(host string) bool {
	host, ok := normalizeHostAuthorizationHost(host, true)
	if !ok {
		return false
	}
	if host == "localhost" || strings.HasSuffix(host, ".localhost") {
		return true
	}

	ip := net.ParseIP(host)
	return ip != nil && (ip.IsLoopback() || ip.IsUnspecified())
}

func hostAuthorizationStatus(status int) int {
	if status == http.StatusForbidden {
		return http.StatusForbidden
	}
	return http.StatusMisdirectedRequest
}

func writeHostAuthorizationRejection(w http.ResponseWriter, r *http.Request, status int) {
	title := http.StatusText(status)
	if title == "" {
		title = "host not allowed"
	}

	if hostAuthorizationWantsJSON(r.Header.Get("Accept")) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(status)
		_, _ = w.Write([]byte(`{"error":` + strconv.Quote(strings.ToLower(title)) + "}\n"))
		return
	}

	http.Error(w, title, status)
}

func hostAuthorizationWantsJSON(accept string) bool {
	for _, part := range strings.Split(accept, ",") {
		mediaType := strings.TrimSpace(part)
		if i := strings.IndexByte(mediaType, ';'); i >= 0 {
			mediaType = mediaType[:i]
		}
		mediaType = strings.ToLower(strings.TrimSpace(mediaType))
		if mediaType == "application/json" ||
			mediaType == "application/problem+json" ||
			strings.HasSuffix(mediaType, "+json") {
			return true
		}
	}
	return false
}
