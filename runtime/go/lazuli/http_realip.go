package lazuli

import (
	"context"
	"fmt"
	"net"
	"net/http"
	"strings"
)

const (
	realIPForwardedForHeader = "X-Forwarded-For"
	realIPHeader             = "X-Real-IP"
)

type clientIPKey struct{}

// RealIPConfig configures RealIPMiddleware.
type RealIPConfig struct {
	// TrustedProxies lists proxy source IPs or CIDR blocks whose
	// X-Forwarded-For and X-Real-IP headers should be trusted. Literal IPs
	// match exactly.
	TrustedProxies []string
}

// RealIPMiddleware resolves the originating client IP for requests received
// through trusted reverse proxies and stores it in the request context for
// ClientIP. Forwarded headers are honored only when the immediate RemoteAddr
// is listed in TrustedProxies.
//
// The middleware panics during construction if any trusted proxy entry is not
// a valid IP address or CIDR block.
func RealIPMiddleware(config RealIPConfig) Middleware {
	trusted := parseRealIPTrustedProxies(config.TrustedProxies)

	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			ip := resolveRealIP(r, trusted)
			if ip != nil {
				ctx := context.WithValue(r.Context(), clientIPKey{}, cloneRealIP(ip))
				r = r.WithContext(ctx)
			}

			next.ServeHTTP(w, r)
		})
	}
}

// ClientIP returns the resolved client IP for r. When RealIPMiddleware has not
// populated the request context, ClientIP safely falls back to RemoteAddr. The
// returned IP is a copy and may be mutated by the caller.
func ClientIP(r *http.Request) net.IP {
	if r == nil {
		return nil
	}
	if ip, ok := r.Context().Value(clientIPKey{}).(net.IP); ok {
		return cloneRealIP(ip)
	}
	return parseRealIPAddr(r.RemoteAddr)
}

// ClientIPString returns ClientIP(r) formatted as a string, or "" when the
// request does not contain a valid client IP.
func ClientIPString(r *http.Request) string {
	ip := ClientIP(r)
	if ip == nil {
		return ""
	}
	return ip.String()
}

type realIPTrustedProxy struct {
	ip      net.IP
	network *net.IPNet
}

func parseRealIPTrustedProxies(entries []string) []realIPTrustedProxy {
	trusted := make([]realIPTrustedProxy, 0, len(entries))
	for _, entry := range entries {
		entry = strings.TrimSpace(entry)
		if entry == "" {
			continue
		}

		if strings.Contains(entry, "/") {
			_, network, err := net.ParseCIDR(entry)
			if err != nil {
				panic(fmt.Sprintf("lazuli: invalid trusted proxy CIDR/IP %q", entry))
			}
			trusted = append(trusted, realIPTrustedProxy{network: network})
			continue
		}

		ip := parseRealIPLiteral(entry)
		if ip == nil {
			panic(fmt.Sprintf("lazuli: invalid trusted proxy CIDR/IP %q", entry))
		}
		trusted = append(trusted, realIPTrustedProxy{ip: ip})
	}
	return trusted
}

func resolveRealIP(r *http.Request, trusted []realIPTrustedProxy) net.IP {
	if r == nil {
		return nil
	}

	remoteIP := parseRealIPAddr(r.RemoteAddr)
	if remoteIP == nil {
		return nil
	}
	if !realIPTrusted(remoteIP, trusted) {
		return remoteIP
	}

	if ip := firstForwardedForIP(r.Header.Values(realIPForwardedForHeader)); ip != nil {
		return ip
	}
	if ip := parseRealIPAddr(r.Header.Get(realIPHeader)); ip != nil {
		return ip
	}
	return remoteIP
}

func realIPTrusted(ip net.IP, trusted []realIPTrustedProxy) bool {
	if ip == nil {
		return false
	}
	for _, proxy := range trusted {
		if proxy.network != nil && proxy.network.Contains(ip) {
			return true
		}
		if proxy.ip != nil && proxy.ip.Equal(ip) {
			return true
		}
	}
	return false
}

func firstForwardedForIP(values []string) net.IP {
	for _, value := range values {
		for _, part := range strings.Split(value, ",") {
			if ip := parseRealIPAddr(part); ip != nil {
				return ip
			}
		}
	}
	return nil
}

func parseRealIPLiteral(addr string) net.IP {
	addr = strings.Trim(strings.TrimSpace(addr), `"[]`)
	return normalizeRealIP(net.ParseIP(addr))
}

func parseRealIPAddr(addr string) net.IP {
	addr = strings.Trim(strings.TrimSpace(addr), `"`)
	if addr == "" {
		return nil
	}
	if ip := normalizeRealIP(net.ParseIP(strings.Trim(addr, "[]"))); ip != nil {
		return ip
	}
	host, _, err := net.SplitHostPort(addr)
	if err != nil {
		return nil
	}
	return normalizeRealIP(net.ParseIP(strings.Trim(host, "[]")))
}

func normalizeRealIP(ip net.IP) net.IP {
	if ip == nil {
		return nil
	}
	if v4 := ip.To4(); v4 != nil {
		return cloneRealIP(v4)
	}
	return cloneRealIP(ip.To16())
}

func cloneRealIP(ip net.IP) net.IP {
	if ip == nil {
		return nil
	}
	out := make(net.IP, len(ip))
	copy(out, ip)
	return out
}
