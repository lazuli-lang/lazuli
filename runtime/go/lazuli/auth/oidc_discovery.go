package auth

import (
	"errors"
	"fmt"
	"net/url"
	"path"
	"strings"
	"unicode"
)

const oidcWellKnownPath = ".well-known/openid-configuration"

// ErrOIDCDiscoveryInvalid reports malformed OpenID Connect discovery metadata.
var ErrOIDCDiscoveryInvalid = errors.New("auth: oidc discovery invalid")

// OIDCDiscoveryDescriptor is the provider-neutral subset of OpenID Connect
// discovery metadata the runtime needs before wiring a concrete provider.
type OIDCDiscoveryDescriptor struct {
	Issuer                           string
	AuthorizationEndpoint            string
	TokenEndpoint                    string
	UserInfoEndpoint                 string
	JWKSURI                          string
	ScopesSupported                  []string
	ResponseTypesSupported           []string
	SubjectTypesSupported            []string
	IDTokenSigningAlgValuesSupported []string
}

// OIDCScopePlan is a deterministic authorization scope request plan.
type OIDCScopePlan struct {
	Requested   []string
	Unsupported []string
}

// OIDCDiscoverySummary is safe for logs and diagnostics.
type OIDCDiscoverySummary struct {
	Issuer                string
	WellKnownURL          string
	AuthorizationEndpoint string
	TokenEndpoint         string
	UserInfoEndpoint      string
	JWKSURI               string
	Scopes                []string
}

// NormalizeOIDCIssuerURL returns a canonical issuer URL without query,
// fragment, userinfo, or a trailing slash. Issuers must be absolute HTTPS URLs.
func NormalizeOIDCIssuerURL(raw string) (string, error) {
	u, err := parseOIDCURL(raw)
	if err != nil {
		return "", err
	}
	u.RawQuery = ""
	u.Fragment = ""
	u.User = nil
	u.Path = cleanOIDCPath(u.Path)
	return u.String(), nil
}

// OIDCWellKnownURL derives the OpenID Connect discovery URL from an issuer.
func OIDCWellKnownURL(issuer string) (string, error) {
	normalized, err := NormalizeOIDCIssuerURL(issuer)
	if err != nil {
		return "", err
	}
	u, err := url.Parse(normalized)
	if err != nil {
		return "", fmt.Errorf("%w: issuer url", ErrOIDCDiscoveryInvalid)
	}
	if u.Path == "" {
		u.Path = "/" + oidcWellKnownPath
	} else {
		u.Path = strings.TrimRight(u.Path, "/") + "/" + oidcWellKnownPath
	}
	return u.String(), nil
}

// NormalizeOIDCDiscoveryDescriptor trims and canonicalizes descriptor fields
// without fetching the provider's discovery document.
func NormalizeOIDCDiscoveryDescriptor(desc OIDCDiscoveryDescriptor) (OIDCDiscoveryDescriptor, error) {
	var err error
	if desc.Issuer, err = NormalizeOIDCIssuerURL(desc.Issuer); err != nil {
		return OIDCDiscoveryDescriptor{}, err
	}
	if desc.AuthorizationEndpoint, err = normalizeOIDCEndpointURL(desc.AuthorizationEndpoint); err != nil {
		return OIDCDiscoveryDescriptor{}, fmt.Errorf("%w: authorization endpoint", err)
	}
	if desc.TokenEndpoint, err = normalizeOIDCEndpointURL(desc.TokenEndpoint); err != nil {
		return OIDCDiscoveryDescriptor{}, fmt.Errorf("%w: token endpoint", err)
	}
	if desc.UserInfoEndpoint, err = normalizeOIDCEndpointURL(desc.UserInfoEndpoint); err != nil {
		return OIDCDiscoveryDescriptor{}, fmt.Errorf("%w: userinfo endpoint", err)
	}
	if desc.JWKSURI, err = normalizeOIDCEndpointURL(desc.JWKSURI); err != nil {
		return OIDCDiscoveryDescriptor{}, fmt.Errorf("%w: jwks uri", err)
	}
	desc.ScopesSupported = normalizeOIDCStringSet(desc.ScopesSupported)
	desc.ResponseTypesSupported = normalizeOIDCStringSet(desc.ResponseTypesSupported)
	desc.SubjectTypesSupported = normalizeOIDCStringSet(desc.SubjectTypesSupported)
	desc.IDTokenSigningAlgValuesSupported = normalizeOIDCStringSet(desc.IDTokenSigningAlgValuesSupported)
	return desc, nil
}

// ValidateOIDCDiscoveryDescriptor checks required OIDC discovery metadata.
func ValidateOIDCDiscoveryDescriptor(desc OIDCDiscoveryDescriptor) error {
	normalized, err := NormalizeOIDCDiscoveryDescriptor(desc)
	if err != nil {
		return err
	}
	if !containsOIDCString(normalized.ResponseTypesSupported, "code") {
		return fmt.Errorf("%w: response_types_supported must include code", ErrOIDCDiscoveryInvalid)
	}
	if len(normalized.SubjectTypesSupported) == 0 {
		return fmt.Errorf("%w: subject_types_supported required", ErrOIDCDiscoveryInvalid)
	}
	if len(normalized.IDTokenSigningAlgValuesSupported) == 0 {
		return fmt.Errorf("%w: id_token_signing_alg_values_supported required", ErrOIDCDiscoveryInvalid)
	}
	return nil
}

// RedactOIDCJWKSURI removes credentials, query values, and fragments from a
// JWKS URI while preserving enough location detail for diagnostics.
func RedactOIDCJWKSURI(raw string) string {
	u, err := url.Parse(strings.TrimSpace(raw))
	if err != nil || u.Scheme == "" || u.Host == "" {
		return "<redacted>"
	}
	u.User = nil
	u.Fragment = ""
	if u.RawQuery != "" {
		u.RawQuery = "redacted"
	}
	return u.String()
}

// PlanOIDCScopes trims requested scopes, ensures openid is present, dedupes in
// request order, and records unsupported scopes when provider metadata lists
// scopes_supported.
func PlanOIDCScopes(requested, supported []string) (OIDCScopePlan, error) {
	scopes := normalizeOIDCStringSet(append([]string{"openid"}, requested...))
	for _, scope := range scopes {
		if err := validateOIDCScope(scope); err != nil {
			return OIDCScopePlan{}, err
		}
	}

	supportedSet := make(map[string]struct{}, len(supported))
	for _, scope := range normalizeOIDCStringSet(supported) {
		if err := validateOIDCScope(scope); err != nil {
			return OIDCScopePlan{}, err
		}
		supportedSet[scope] = struct{}{}
	}

	var unsupported []string
	if len(supportedSet) > 0 {
		for _, scope := range scopes {
			if _, ok := supportedSet[scope]; !ok {
				unsupported = append(unsupported, scope)
			}
		}
	}
	return OIDCScopePlan{Requested: scopes, Unsupported: unsupported}, nil
}

// SafeOIDCDiscoverySummary returns redacted descriptor metadata for logs.
func SafeOIDCDiscoverySummary(desc OIDCDiscoveryDescriptor) (OIDCDiscoverySummary, error) {
	normalized, err := NormalizeOIDCDiscoveryDescriptor(desc)
	if err != nil {
		return OIDCDiscoverySummary{}, err
	}
	wellKnownURL, err := OIDCWellKnownURL(normalized.Issuer)
	if err != nil {
		return OIDCDiscoverySummary{}, err
	}
	return OIDCDiscoverySummary{
		Issuer:                normalized.Issuer,
		WellKnownURL:          wellKnownURL,
		AuthorizationEndpoint: normalized.AuthorizationEndpoint,
		TokenEndpoint:         normalized.TokenEndpoint,
		UserInfoEndpoint:      normalized.UserInfoEndpoint,
		JWKSURI:               RedactOIDCJWKSURI(normalized.JWKSURI),
		Scopes:                append([]string(nil), normalized.ScopesSupported...),
	}, nil
}

func parseOIDCURL(raw string) (*url.URL, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil, fmt.Errorf("%w: url required", ErrOIDCDiscoveryInvalid)
	}
	u, err := url.Parse(raw)
	if err != nil || u.Scheme == "" || u.Host == "" {
		return nil, fmt.Errorf("%w: url must be absolute", ErrOIDCDiscoveryInvalid)
	}
	if !strings.EqualFold(u.Scheme, "https") {
		return nil, fmt.Errorf("%w: url must use https", ErrOIDCDiscoveryInvalid)
	}
	if u.User != nil {
		return nil, fmt.Errorf("%w: url must not include credentials", ErrOIDCDiscoveryInvalid)
	}
	u.Scheme = "https"
	u.Host = strings.ToLower(u.Host)
	return u, nil
}

func normalizeOIDCEndpointURL(raw string) (string, error) {
	u, err := parseOIDCURL(raw)
	if err != nil {
		return "", err
	}
	u.Fragment = ""
	u.Path = cleanOIDCPath(u.Path)
	return u.String(), nil
}

func cleanOIDCPath(raw string) string {
	if raw == "" || raw == "/" {
		return ""
	}
	clean := path.Clean("/" + strings.TrimSpace(raw))
	if clean == "/" {
		return ""
	}
	return strings.TrimRight(clean, "/")
}

func normalizeOIDCStringSet(values []string) []string {
	normalized := make([]string, 0, len(values))
	seen := make(map[string]struct{}, len(values))
	for _, value := range values {
		value = strings.TrimSpace(value)
		if value == "" {
			continue
		}
		if _, ok := seen[value]; ok {
			continue
		}
		seen[value] = struct{}{}
		normalized = append(normalized, value)
	}
	return normalized
}

func containsOIDCString(values []string, want string) bool {
	for _, value := range values {
		if value == want {
			return true
		}
	}
	return false
}

func validateOIDCScope(scope string) error {
	if strings.TrimSpace(scope) == "" {
		return fmt.Errorf("%w: scope required", ErrOIDCDiscoveryInvalid)
	}
	for _, r := range scope {
		if unicode.IsSpace(r) {
			return fmt.Errorf("%w: scope must not contain whitespace", ErrOIDCDiscoveryInvalid)
		}
	}
	return nil
}
