package lazuli

import (
	"crypto/tls"
	"net/http"
	"strings"
)

const (
	// DefaultServerTLSMinVersion is the minimum TLS version Lazuli applies to
	// production server configs.
	DefaultServerTLSMinVersion uint16 = tls.VersionTLS12

	// DefaultHSTSPolicy is Lazuli's default Strict-Transport-Security value.
	DefaultHSTSPolicy = "max-age=31536000; includeSubDomains"
)

var defaultServerTLSNextProtos = []string{"h2", "http/1.1"}

// DefaultServerTLSNextProtos returns Lazuli's default ALPN protocols for TLS
// HTTP servers. The returned slice is safe for callers to modify.
func DefaultServerTLSNextProtos() []string {
	return cloneStringSlice(defaultServerTLSNextProtos)
}

// CloneTLSConfig returns a clone of config, or a new empty tls.Config when
// config is nil. Lazuli also isolates the mutable TLS policy slices it reads or
// writes so callers can safely adjust the returned config.
func CloneTLSConfig(config *tls.Config) *tls.Config {
	if config == nil {
		return &tls.Config{}
	}
	clone := config.Clone()
	clone.NextProtos = cloneStringSlice(config.NextProtos)
	clone.CipherSuites = cloneUint16Slice(config.CipherSuites)
	clone.CurvePreferences = cloneCurveIDSlice(config.CurvePreferences)
	return clone
}

// DefaultServerTLSConfig returns a cloned TLS config with Lazuli's production
// server TLS defaults applied. It preserves caller-specified fields, raises an
// unset or weaker MinVersion to TLS 1.2, ensures HTTP/2 and HTTP/1.1 ALPN are
// advertised, and sets PreferServerCipherSuites for older Go/toolchain
// combinations where that field is still honored. Go's standard library keeps
// cipher suite selection on its safe defaults.
func DefaultServerTLSConfig(config *tls.Config) *tls.Config {
	clone := CloneTLSConfig(config)
	if clone.MinVersion == 0 || clone.MinVersion < DefaultServerTLSMinVersion {
		clone.MinVersion = DefaultServerTLSMinVersion
	}
	clone.NextProtos = mergeTLSNextProtos(defaultServerTLSNextProtos, clone.NextProtos)
	clone.PreferServerCipherSuites = true
	return clone
}

// ServerWithTLSDefaults returns a shallow clone of server whose TLSConfig has
// Lazuli's production TLS defaults applied. The input server and TLS config are
// not mutated. A nil server returns a new http.Server with only TLS defaults.
func ServerWithTLSDefaults(server *http.Server) *http.Server {
	clone := &http.Server{}
	if server != nil {
		*clone = *server
	}
	clone.TLSConfig = DefaultServerTLSConfig(clone.TLSConfig)
	return clone
}

// DefaultHSTSSecurityHeaders returns a SecurityHeaders config containing only
// Lazuli's default Strict-Transport-Security policy.
func DefaultHSTSSecurityHeaders() SecurityHeaders {
	return SecurityHeaders{StrictTransportSecurity: DefaultHSTSPolicy}
}

// WithHSTS returns headers with StrictTransportSecurity set. An empty policy
// uses DefaultHSTSPolicy. Other SecurityHeaders fields are preserved.
func WithHSTS(headers SecurityHeaders, policy string) SecurityHeaders {
	policy = strings.TrimSpace(policy)
	if policy == "" {
		policy = DefaultHSTSPolicy
	}
	headers.StrictTransportSecurity = policy
	return headers
}

func mergeTLSNextProtos(defaults, configured []string) []string {
	merged := make([]string, 0, len(defaults)+len(configured))
	seen := make(map[string]struct{}, len(defaults)+len(configured))
	for _, proto := range defaults {
		if proto = strings.TrimSpace(proto); proto != "" {
			merged = appendUniqueString(merged, seen, proto)
		}
	}
	for _, proto := range configured {
		if proto = strings.TrimSpace(proto); proto != "" {
			merged = appendUniqueString(merged, seen, proto)
		}
	}
	return merged
}

func appendUniqueString(values []string, seen map[string]struct{}, value string) []string {
	if _, ok := seen[value]; ok {
		return values
	}
	seen[value] = struct{}{}
	return append(values, value)
}

func cloneStringSlice(values []string) []string {
	if values == nil {
		return nil
	}
	cloned := make([]string, len(values))
	copy(cloned, values)
	return cloned
}

func cloneUint16Slice(values []uint16) []uint16 {
	if values == nil {
		return nil
	}
	cloned := make([]uint16, len(values))
	copy(cloned, values)
	return cloned
}

func cloneCurveIDSlice(values []tls.CurveID) []tls.CurveID {
	if values == nil {
		return nil
	}
	cloned := make([]tls.CurveID, len(values))
	copy(cloned, values)
	return cloned
}
