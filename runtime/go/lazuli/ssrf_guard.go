package lazuli

import (
	"context"
	"errors"
	"fmt"
	"net"
	"net/netip"
	"net/url"
	"strconv"
	"strings"
)

// ErrSSRFInvalidURL is returned when an outbound URL cannot be parsed as an
// absolute HTTP(S) URL.
var ErrSSRFInvalidURL = errors.New("lazuli: invalid outbound URL")

// ErrSSRFBlocked is returned when an outbound URL is denied by SSRFGuard.
var ErrSSRFBlocked = errors.New("lazuli: outbound URL blocked by SSRF guard")

// SSRFGuard configures ValidateOutboundURL.
//
// The zero value allows http and https URLs whose host resolves only to public
// addresses. Private, loopback, link-local, multicast, and unspecified
// addresses are denied by default.
type SSRFGuard struct {
	// AllowedHosts, when non-empty, restricts outbound URLs to exact host
	// matches or wildcard subdomains such as "*.example.com". Matching is
	// case-insensitive and ignores a trailing dot. Host entries do not bypass
	// IP address safety checks.
	AllowedHosts []string

	// AllowedCIDRs lists IP ranges that may be reached even if they are in a
	// normally denied range. Entries may be CIDR blocks or literal IPs.
	AllowedCIDRs []string
}

// SSRFResolver resolves hostnames for ValidateOutboundURL. *net.Resolver
// satisfies this interface.
type SSRFResolver interface {
	LookupIPAddr(ctx context.Context, host string) ([]net.IPAddr, error)
}

// SSRFResolverFunc adapts a function to SSRFResolver.
type SSRFResolverFunc func(context.Context, string) ([]net.IPAddr, error)

// LookupIPAddr implements SSRFResolver.
func (f SSRFResolverFunc) LookupIPAddr(ctx context.Context, host string) ([]net.IPAddr, error) {
	if f == nil {
		return nil, errors.New("lazuli: nil SSRF resolver function")
	}
	return f(ctx, host)
}

// ValidateOutboundURL verifies that rawURL is safe to use for an outbound
// HTTP(S) request under guard. DNS hostnames are resolved with resolver; when
// resolver is nil, net.DefaultResolver is used. Every resolved address must be
// allowed so mixed safe and unsafe DNS answers fail closed.
func ValidateOutboundURL(ctx context.Context, rawURL string, guard SSRFGuard, resolver SSRFResolver) error {
	rawURL = strings.TrimSpace(rawURL)
	u, err := url.Parse(rawURL)
	if err != nil {
		return fmt.Errorf("%w: %v", ErrSSRFInvalidURL, err)
	}
	if u.Scheme == "" {
		return fmt.Errorf("%w: absolute URL with scheme required", ErrSSRFInvalidURL)
	}

	switch strings.ToLower(u.Scheme) {
	case "http", "https":
	default:
		return fmt.Errorf("%w: scheme %q is not allowed", ErrSSRFBlocked, u.Scheme)
	}
	if u.Host == "" {
		return fmt.Errorf("%w: absolute URL with host required", ErrSSRFInvalidURL)
	}

	if err := validateSSRFHostPort(u.Host); err != nil {
		return err
	}

	host := u.Hostname()
	if host == "" {
		return fmt.Errorf("%w: host required", ErrSSRFInvalidURL)
	}
	if strings.Contains(host, "%") {
		return fmt.Errorf("%w: scoped address host %q is not allowed", ErrSSRFBlocked, host)
	}

	allowedCIDRs, err := parseSSRFAllowedCIDRs(guard.AllowedCIDRs)
	if err != nil {
		return err
	}

	if addr, ok := parseSSRFAddr(host); ok {
		if !ssrfHostAllowed(host, guard.AllowedHosts) && !ssrfAddrInPrefixes(addr, allowedCIDRs) {
			return fmt.Errorf("%w: host %q is not allowlisted", ErrSSRFBlocked, host)
		}
		return validateSSRFAddr(addr, allowedCIDRs)
	}

	if !ssrfHostAllowed(host, guard.AllowedHosts) {
		return fmt.Errorf("%w: host %q is not allowlisted", ErrSSRFBlocked, host)
	}

	if ctx == nil {
		ctx = context.Background()
	}
	if resolver == nil {
		resolver = net.DefaultResolver
	}

	addrs, err := resolver.LookupIPAddr(ctx, host)
	if err != nil {
		return fmt.Errorf("%w: resolve %q: %v", ErrSSRFBlocked, host, err)
	}
	if len(addrs) == 0 {
		return fmt.Errorf("%w: %q resolved no addresses", ErrSSRFBlocked, host)
	}

	for _, ipAddr := range addrs {
		addr, ok := netip.AddrFromSlice(ipAddr.IP)
		if !ok {
			return fmt.Errorf("%w: resolver returned invalid address for %q", ErrSSRFBlocked, host)
		}
		if err := validateSSRFAddr(addr.Unmap(), allowedCIDRs); err != nil {
			return err
		}
	}

	return nil
}

func validateSSRFHostPort(host string) error {
	if strings.HasPrefix(host, "[") {
		closeBracket := strings.LastIndex(host, "]")
		if closeBracket < 0 {
			return fmt.Errorf("%w: invalid host", ErrSSRFInvalidURL)
		}
		if closeBracket == len(host)-1 {
			return nil
		}
		if host[closeBracket+1] != ':' {
			return fmt.Errorf("%w: invalid host port", ErrSSRFInvalidURL)
		}
		_, port, err := net.SplitHostPort(host)
		if err != nil {
			return fmt.Errorf("%w: invalid host port", ErrSSRFInvalidURL)
		}
		return validateSSRFPort(port)
	}

	if !strings.Contains(host, ":") {
		return nil
	}
	_, port, err := net.SplitHostPort(host)
	if err != nil {
		return fmt.Errorf("%w: invalid host port", ErrSSRFInvalidURL)
	}
	return validateSSRFPort(port)
}

func validateSSRFPort(port string) error {
	n, err := strconv.Atoi(port)
	if err != nil || n < 0 || n > 65535 {
		return fmt.Errorf("%w: invalid port %q", ErrSSRFInvalidURL, port)
	}
	return nil
}

func parseSSRFAllowedCIDRs(entries []string) ([]netip.Prefix, error) {
	prefixes := make([]netip.Prefix, 0, len(entries))
	for _, entry := range entries {
		entry = strings.TrimSpace(entry)
		if entry == "" {
			continue
		}

		if strings.Contains(entry, "/") {
			prefix, err := netip.ParsePrefix(entry)
			if err != nil {
				return nil, fmt.Errorf("lazuli: invalid SSRF guard CIDR %q: %w", entry, err)
			}
			prefixes = append(prefixes, prefix.Masked())
			continue
		}

		addr, ok := parseSSRFAddr(entry)
		if !ok {
			return nil, fmt.Errorf("lazuli: invalid SSRF guard CIDR %q", entry)
		}
		prefixes = append(prefixes, netip.PrefixFrom(addr, addr.BitLen()))
	}
	return prefixes, nil
}

func validateSSRFAddr(addr netip.Addr, allowedCIDRs []netip.Prefix) error {
	addr = addr.Unmap()
	if !addr.IsValid() {
		return fmt.Errorf("%w: invalid IP address", ErrSSRFBlocked)
	}
	if ssrfAddrInPrefixes(addr, allowedCIDRs) {
		return nil
	}

	var reason string
	switch {
	case addr.IsUnspecified():
		reason = "unspecified"
	case addr.IsLoopback():
		reason = "loopback"
	case addr.IsPrivate():
		reason = "private"
	case addr.IsLinkLocalUnicast():
		reason = "link-local"
	case addr.IsMulticast():
		reason = "multicast"
	default:
		return nil
	}

	return fmt.Errorf("%w: %s address %s is not allowed", ErrSSRFBlocked, reason, addr)
}

func ssrfAddrInPrefixes(addr netip.Addr, prefixes []netip.Prefix) bool {
	addr = addr.Unmap()
	for _, prefix := range prefixes {
		if prefix.Contains(addr) {
			return true
		}
	}
	return false
}

func ssrfHostAllowed(host string, allowedHosts []string) bool {
	if len(allowedHosts) == 0 {
		return true
	}

	host = normalizeSSRFHost(host)
	hasPattern := false
	for _, pattern := range allowedHosts {
		normalized, wildcard, ok := normalizeSSRFHostPattern(pattern)
		if !ok {
			continue
		}
		hasPattern = true
		if wildcard {
			if host != normalized && strings.HasSuffix(host, "."+normalized) {
				return true
			}
			continue
		}
		if host == normalized {
			return true
		}
	}

	return !hasPattern
}

func normalizeSSRFHostPattern(pattern string) (string, bool, bool) {
	pattern = strings.TrimSpace(pattern)
	if pattern == "" {
		return "", false, false
	}

	wildcard := strings.HasPrefix(pattern, "*.")
	if wildcard {
		pattern = strings.TrimPrefix(pattern, "*.")
	}

	if host, _, err := net.SplitHostPort(pattern); err == nil {
		pattern = host
	}
	pattern = strings.Trim(pattern, "[]")
	return normalizeSSRFHost(pattern), wildcard, true
}

func normalizeSSRFHost(host string) string {
	host = strings.TrimSpace(host)
	if addr, ok := parseSSRFAddr(host); ok {
		return addr.String()
	}
	host = strings.ToLower(host)
	return strings.TrimSuffix(host, ".")
}

func parseSSRFAddr(raw string) (netip.Addr, bool) {
	raw = strings.Trim(strings.TrimSpace(raw), "[]")
	addr, err := netip.ParseAddr(raw)
	if err != nil {
		return netip.Addr{}, false
	}
	return addr.Unmap(), true
}
