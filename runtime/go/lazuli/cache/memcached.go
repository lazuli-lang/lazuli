package cache

import (
	"errors"
	"fmt"
	"net"
	"net/url"
	"slices"
	"strconv"
	"strings"
	"time"
	"unicode"
)

const (
	memcachedDefaultPort = "11211"
	memcachedMaxKeyBytes = 250
	memcachedMaxTTL      = 30 * 24 * time.Hour
	memcachedRedacted    = "redacted"
)

var ErrMemcachedDescriptorInvalid = errors.New("lazuli/cache: memcached descriptor invalid")

// MemcachedDescriptor describes provider-neutral Memcached cache settings.
//
// It is intentionally metadata-only: helpers in this file do not create
// clients, open sockets, or depend on any Memcached client package.
type MemcachedDescriptor struct {
	Servers   []string
	Namespace string
	Key       string
	TTL       time.Duration
}

// MemcachedPlan is the normalized form of a MemcachedDescriptor.
type MemcachedPlan struct {
	Servers         []string
	Namespace       string
	KeyPrefix       string
	Key             string
	TTL             MemcachedTTLPlan
	RedactedServers []string
}

// MemcachedTTLPlan describes the expiration value a Memcached adapter should use.
//
// Memcached treats expiration values greater than 30 days as absolute Unix
// timestamps. Duration is therefore clamped to 30 days so future adapters can
// use relative expirations without accidentally crossing that protocol boundary.
type MemcachedTTLPlan struct {
	Duration                   time.Duration
	Seconds                    int32
	NeverExpires               bool
	Clamped                    bool
	AbsoluteExpirationBoundary bool
}

// MemcachedSummary is a deterministic, redacted descriptor summary for logs.
type MemcachedSummary struct {
	Servers          []string
	ServerCount      int
	Namespace        string
	KeyPrefix        string
	TTLSeconds       int32
	NeverExpires     bool
	TTLClamped       bool
	AbsoluteBoundary bool
}

// NormalizeMemcachedServerAddress trims and canonicalizes a server address.
//
// Bare host names receive the default Memcached port. URL-like inputs preserve
// only scheme, host, and path; user info, query strings, and fragments are
// dropped so descriptor summaries cannot leak credentials.
func NormalizeMemcachedServerAddress(address string) string {
	address = strings.TrimSpace(address)
	if address == "" {
		return ""
	}

	if parsed, ok := parseMemcachedURL(address); ok {
		parsed.User = nil
		parsed.RawQuery = ""
		parsed.Fragment = ""
		parsed.Host = normalizeMemcachedHostPort(parsed.Host)
		return parsed.String()
	}

	return normalizeMemcachedHostPort(address)
}

// ValidateMemcachedKey checks Memcached protocol key limits.
func ValidateMemcachedKey(key string) error {
	var errs []error
	if key == "" {
		errs = append(errs, fmt.Errorf("%w: key is required", ErrMemcachedDescriptorInvalid))
	}
	if len(key) > memcachedMaxKeyBytes {
		errs = append(errs, fmt.Errorf("%w: key exceeds %d bytes", ErrMemcachedDescriptorInvalid, memcachedMaxKeyBytes))
	}
	for _, r := range key {
		if unicode.IsControl(r) || unicode.IsSpace(r) {
			errs = append(errs, fmt.Errorf("%w: key contains whitespace or control characters", ErrMemcachedDescriptorInvalid))
			break
		}
	}
	return errors.Join(errs...)
}

// ValidateMemcachedServerAddress checks that a server address can be used safely.
func ValidateMemcachedServerAddress(address string) error {
	normalized := NormalizeMemcachedServerAddress(address)
	if normalized == "" {
		return fmt.Errorf("%w: server address is required", ErrMemcachedDescriptorInvalid)
	}
	for _, r := range normalized {
		if unicode.IsControl(r) || unicode.IsSpace(r) {
			return fmt.Errorf("%w: server address contains whitespace or control characters", ErrMemcachedDescriptorInvalid)
		}
	}
	return nil
}

// ValidateMemcachedDescriptor checks descriptor metadata without mutating it.
func ValidateMemcachedDescriptor(descriptor MemcachedDescriptor) error {
	_, err := PlanMemcachedDescriptor(descriptor)
	return err
}

// ClampMemcachedTTL returns a safe relative Memcached TTL plan.
func ClampMemcachedTTL(ttl time.Duration) MemcachedTTLPlan {
	if ttl <= 0 {
		return MemcachedTTLPlan{NeverExpires: true}
	}

	plan := MemcachedTTLPlan{Duration: ttl}
	if plan.Duration > memcachedMaxTTL {
		plan.Duration = memcachedMaxTTL
		plan.Clamped = true
		plan.AbsoluteExpirationBoundary = true
	} else if plan.Duration == memcachedMaxTTL {
		plan.AbsoluteExpirationBoundary = true
	}
	plan.Seconds = int32((plan.Duration + time.Second - 1) / time.Second)
	return plan
}

// PlanMemcachedNamespacePrefix returns the normalized prefix for cache keys.
func PlanMemcachedNamespacePrefix(namespace string) string {
	namespace = NormalizeNamespace(namespace)
	if namespace == "" {
		return ""
	}
	return namespace + ":"
}

// PlanMemcachedDescriptor validates and normalizes Memcached descriptor metadata.
func PlanMemcachedDescriptor(descriptor MemcachedDescriptor) (MemcachedPlan, error) {
	plan := MemcachedPlan{
		Namespace: NormalizeNamespace(descriptor.Namespace),
		Key:       strings.TrimSpace(descriptor.Key),
		TTL:       ClampMemcachedTTL(descriptor.TTL),
	}
	plan.KeyPrefix = PlanMemcachedNamespacePrefix(plan.Namespace)

	var errs []error
	seen := make(map[string]struct{}, len(descriptor.Servers))
	for _, server := range descriptor.Servers {
		normalized := NormalizeMemcachedServerAddress(server)
		if normalized == "" {
			continue
		}
		if err := ValidateMemcachedServerAddress(server); err != nil {
			errs = append(errs, err)
			continue
		}
		if _, ok := seen[normalized]; ok {
			continue
		}
		seen[normalized] = struct{}{}
		plan.Servers = append(plan.Servers, normalized)
		plan.RedactedServers = append(plan.RedactedServers, RedactMemcachedServerAddress(normalized))
	}
	slices.Sort(plan.Servers)
	slices.Sort(plan.RedactedServers)

	if len(plan.Servers) == 0 {
		errs = append(errs, fmt.Errorf("%w: at least one server is required", ErrMemcachedDescriptorInvalid))
	}
	if plan.Key != "" {
		key := plan.KeyPrefix + plan.Key
		if err := ValidateMemcachedKey(key); err != nil {
			errs = append(errs, err)
		}
	}
	if err := errors.Join(errs...); err != nil {
		return MemcachedPlan{}, err
	}
	return plan, nil
}

// RedactMemcachedServerAddress removes credentials and query values from an address.
func RedactMemcachedServerAddress(address string) string {
	address = NormalizeMemcachedServerAddress(address)
	if parsed, ok := parseMemcachedURL(address); ok {
		if parsed.User != nil {
			parsed.User = url.User(memcachedRedacted)
		}
		parsed.RawQuery = ""
		parsed.Fragment = ""
		return parsed.String()
	}
	return redactedMemcachedHostPort(address)
}

// RedactedSummary returns a log-safe summary of the normalized descriptor plan.
func (p MemcachedPlan) RedactedSummary() MemcachedSummary {
	servers := append([]string(nil), p.RedactedServers...)
	if len(servers) == 0 {
		servers = make([]string, 0, len(p.Servers))
		for _, server := range p.Servers {
			servers = append(servers, RedactMemcachedServerAddress(server))
		}
		slices.Sort(servers)
	}
	return MemcachedSummary{
		Servers:          servers,
		ServerCount:      len(servers),
		Namespace:        p.Namespace,
		KeyPrefix:        p.KeyPrefix,
		TTLSeconds:       p.TTL.Seconds,
		NeverExpires:     p.TTL.NeverExpires,
		TTLClamped:       p.TTL.Clamped,
		AbsoluteBoundary: p.TTL.AbsoluteExpirationBoundary,
	}
}

func parseMemcachedURL(address string) (*url.URL, bool) {
	parsed, err := url.Parse(address)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return nil, false
	}
	return parsed, true
}

func normalizeMemcachedHostPort(address string) string {
	host, port, err := net.SplitHostPort(address)
	if err == nil {
		return net.JoinHostPort(strings.ToLower(strings.Trim(host, "[]")), normalizeMemcachedPort(port))
	}

	host = strings.Trim(address, "[]")
	if ip := net.ParseIP(host); ip != nil {
		return net.JoinHostPort(strings.ToLower(ip.String()), memcachedDefaultPort)
	}

	return net.JoinHostPort(strings.ToLower(host), memcachedDefaultPort)
}

func normalizeMemcachedPort(port string) string {
	port = strings.TrimSpace(port)
	if port == "" {
		return memcachedDefaultPort
	}
	if value, err := strconv.Atoi(port); err == nil && value > 0 {
		return strconv.Itoa(value)
	}
	return port
}

func redactedMemcachedHostPort(address string) string {
	host, port, err := net.SplitHostPort(address)
	if err != nil {
		return address
	}
	if strings.Contains(host, "@") {
		host = memcachedRedacted + "@" + host[strings.LastIndex(host, "@")+1:]
	}
	return net.JoinHostPort(host, port)
}
