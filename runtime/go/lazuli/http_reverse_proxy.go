package lazuli

import (
	"errors"
	"fmt"
	"log/slog"
	"net"
	"net/http"
	"net/http/httputil"
	"net/url"
	"strings"
	"unicode"
)

const (
	reverseProxyForwardedHeader       = "Forwarded"
	reverseProxyForwardedForHeader    = "X-Forwarded-For"
	reverseProxyForwardedHostHeader   = "X-Forwarded-Host"
	reverseProxyForwardedProtoHeader  = "X-Forwarded-Proto"
	reverseProxyDefaultUpstreamDetail = "reverse proxy upstream error"
)

// ErrReverseProxyTargetRejected is returned when a reverse proxy target URL is
// not safe or usable as an upstream origin.
var ErrReverseProxyTargetRejected = errors.New("lazuli: reverse proxy target rejected")

// ReverseProxyOptions configures NewReverseProxy.
type ReverseProxyOptions struct {
	// StripPrefix removes this inbound path prefix before forwarding. The
	// prefix is matched on path segment boundaries, so "/app" matches "/app"
	// and "/app/users" but not "/apple".
	StripPrefix string

	// TargetPrefix is added after StripPrefix is applied and before the
	// request path is joined to any base path in the target URL.
	TargetPrefix string

	// RewriteHost rewrites the outbound Host header to the target host. When
	// false, the original inbound Host is preserved.
	RewriteHost bool
}

// NewReverseProxy validates target and returns a reverse proxy handler using
// the standard library httputil.ReverseProxy. target may include a base path,
// but not a query string or fragment.
func NewReverseProxy(target string, options ReverseProxyOptions) (*httputil.ReverseProxy, error) {
	targetURL, err := ParseReverseProxyTarget(target)
	if err != nil {
		return nil, err
	}

	stripPrefix := normalizeReverseProxyPrefix(options.StripPrefix)
	targetPrefix := normalizeReverseProxyPrefix(options.TargetPrefix)

	return &httputil.ReverseProxy{
		Rewrite: func(req *httputil.ProxyRequest) {
			req.SetURL(targetURL)

			path, rawPath := reverseProxyPath(targetURL, req.In.URL, stripPrefix, targetPrefix)
			req.Out.URL.Path = path
			req.Out.URL.RawPath = rawPath
			req.Out.URL.RawQuery = req.In.URL.RawQuery
			req.Out.URL.ForceQuery = req.In.URL.ForceQuery

			if !options.RewriteHost {
				req.Out.Host = req.In.Host
			}

			preserveReverseProxyForwardedHeaders(req)
		},
		ErrorHandler: ReverseProxyProblemErrorHandler,
	}, nil
}

// ParseReverseProxyTarget parses and validates a reverse proxy target URL.
// Only absolute http and https URLs with hosts are accepted.
func ParseReverseProxyTarget(raw string) (*url.URL, error) {
	if err := validateReverseProxyTargetRaw(raw); err != nil {
		return nil, err
	}

	u, err := url.Parse(raw)
	if err != nil {
		return nil, reverseProxyTargetReject("invalid URL", err)
	}
	if !u.IsAbs() {
		return nil, reverseProxyTargetReject("relative URL", nil)
	}

	scheme := strings.ToLower(u.Scheme)
	if scheme != "http" && scheme != "https" {
		return nil, reverseProxyTargetReject("scheme is not allowed", nil)
	}
	if u.Host == "" {
		return nil, reverseProxyTargetReject("missing host", nil)
	}
	if u.User != nil {
		return nil, reverseProxyTargetReject("userinfo is not allowed", nil)
	}
	if u.Opaque != "" {
		return nil, reverseProxyTargetReject("opaque URL is not allowed", nil)
	}
	if u.RawQuery != "" || u.ForceQuery {
		return nil, reverseProxyTargetReject("query is not allowed", nil)
	}
	if u.Fragment != "" {
		return nil, reverseProxyTargetReject("fragment is not allowed", nil)
	}

	target := *u
	target.Scheme = scheme
	return &target, nil
}

// ReverseProxyProblemErrorHandler writes upstream proxy errors as RFC 9457
// problem JSON with a 502 Bad Gateway status.
func ReverseProxyProblemErrorHandler(w http.ResponseWriter, r *http.Request, err error) {
	if err != nil {
		slog.Error("lazuli: reverse proxy upstream error",
			"error", err,
			"path", reverseProxyRequestPath(r),
		)
	}

	WriteProblem(w, Problem{
		Status: http.StatusBadGateway,
		Detail: reverseProxyDefaultUpstreamDetail,
		Extensions: map[string]any{
			"code": CodeIntegrationError,
		},
	})
}

func validateReverseProxyTargetRaw(raw string) error {
	if raw == "" {
		return reverseProxyTargetReject("empty URL", nil)
	}
	if strings.TrimSpace(raw) != raw {
		return reverseProxyTargetReject("leading or trailing whitespace", nil)
	}
	for _, r := range raw {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return reverseProxyTargetReject("whitespace or control character", nil)
		}
		if r == '\\' {
			return reverseProxyTargetReject("backslash is not allowed", nil)
		}
	}
	return nil
}

func reverseProxyTargetReject(reason string, err error) error {
	if err != nil {
		return fmt.Errorf("%w: %s: %v", ErrReverseProxyTargetRejected, reason, err)
	}
	return fmt.Errorf("%w: %s", ErrReverseProxyTargetRejected, reason)
}

func reverseProxyRequestPath(r *http.Request) string {
	if r == nil || r.URL == nil {
		return ""
	}
	return r.URL.Path
}

func reverseProxyPath(targetURL, inboundURL *url.URL, stripPrefix, targetPrefix string) (string, string) {
	path := "/"
	rawPath := ""
	if inboundURL != nil {
		path = inboundURL.Path
		rawPath = inboundURL.RawPath
	}
	if path == "" {
		path = "/"
	}

	path = rewriteReverseProxyPrefix(path, stripPrefix, targetPrefix)
	path = joinReverseProxyPath(targetURL.Path, path)

	if rawPath == "" {
		return path, ""
	}

	rawPath = rewriteReverseProxyPrefix(rawPath, escapedReverseProxyPath(stripPrefix), escapedReverseProxyPath(targetPrefix))
	rawPath = joinReverseProxyPath(targetURL.EscapedPath(), rawPath)
	if rawPath == escapedReverseProxyPath(path) {
		rawPath = ""
	}
	return path, rawPath
}

func rewriteReverseProxyPrefix(path, stripPrefix, targetPrefix string) string {
	path = ensureReverseProxyLeadingSlash(path)
	if stripPrefix != "" {
		path = stripReverseProxyPrefix(path, stripPrefix)
	}
	if targetPrefix != "" {
		path = joinReverseProxyPath(targetPrefix, path)
	}
	return path
}

func stripReverseProxyPrefix(path, prefix string) string {
	if prefix == "" || prefix == "/" {
		return path
	}
	if path == prefix {
		return "/"
	}
	if strings.HasPrefix(path, prefix+"/") {
		return strings.TrimPrefix(path, prefix)
	}
	return path
}

func normalizeReverseProxyPrefix(prefix string) string {
	prefix = strings.TrimSpace(prefix)
	if prefix == "" || prefix == "/" {
		return ""
	}
	prefix = ensureReverseProxyLeadingSlash(prefix)
	return strings.TrimRight(prefix, "/")
}

func ensureReverseProxyLeadingSlash(path string) string {
	if path == "" {
		return "/"
	}
	if strings.HasPrefix(path, "/") {
		return path
	}
	return "/" + path
}

func joinReverseProxyPath(prefix, path string) string {
	if prefix == "" {
		return ensureReverseProxyLeadingSlash(path)
	}
	if path == "" {
		return ensureReverseProxyLeadingSlash(prefix)
	}

	prefix = ensureReverseProxyLeadingSlash(prefix)
	prefixSlash := strings.HasSuffix(prefix, "/")
	pathSlash := strings.HasPrefix(path, "/")
	switch {
	case prefixSlash && pathSlash:
		return prefix + path[1:]
	case !prefixSlash && !pathSlash:
		return prefix + "/" + path
	default:
		return prefix + path
	}
}

func escapedReverseProxyPath(path string) string {
	if path == "" {
		return ""
	}
	return (&url.URL{Path: path}).EscapedPath()
}

func preserveReverseProxyForwardedHeaders(req *httputil.ProxyRequest) {
	copyReverseProxyHeader(req.Out.Header, req.In.Header, reverseProxyForwardedHeader)
	setReverseProxyForwardedChain(req.Out.Header, reverseProxyForwardedForHeader, req.In.Header.Values(reverseProxyForwardedForHeader), reverseProxyClientIP(req.In))
	setReverseProxyForwardedChain(req.Out.Header, reverseProxyForwardedHostHeader, req.In.Header.Values(reverseProxyForwardedHostHeader), req.In.Host)
	setReverseProxyForwardedChain(req.Out.Header, reverseProxyForwardedProtoHeader, req.In.Header.Values(reverseProxyForwardedProtoHeader), reverseProxyProto(req.In))
}

func copyReverseProxyHeader(dst, src http.Header, key string) {
	dst.Del(key)
	for _, value := range src.Values(key) {
		if strings.TrimSpace(value) != "" {
			dst.Add(key, value)
		}
	}
}

func setReverseProxyForwardedChain(header http.Header, key string, prior []string, current string) {
	values := make([]string, 0, len(prior)+1)
	for _, value := range prior {
		value = strings.TrimSpace(value)
		if value != "" {
			values = append(values, value)
		}
	}
	current = strings.TrimSpace(current)
	if current != "" {
		values = append(values, current)
	}
	if len(values) == 0 {
		header.Del(key)
		return
	}
	header.Set(key, strings.Join(values, ", "))
}

func reverseProxyClientIP(r *http.Request) string {
	if r == nil {
		return ""
	}
	host, _, err := net.SplitHostPort(r.RemoteAddr)
	if err == nil {
		return host
	}
	if net.ParseIP(r.RemoteAddr) != nil {
		return r.RemoteAddr
	}
	return ""
}

func reverseProxyProto(r *http.Request) string {
	if r != nil && r.TLS != nil {
		return "https"
	}
	return "http"
}
