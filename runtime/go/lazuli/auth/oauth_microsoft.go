package auth

import (
	"errors"
	"fmt"
	"net"
	"net/url"
	"strings"
	"unicode"
)

const (
	MicrosoftOAuthProvider = "microsoft"

	MicrosoftOAuthDefaultTenant = "common"

	microsoftOAuthAuthorizeBase = "https://login.microsoftonline.com"
	microsoftOAuthProfileURL    = "https://graph.microsoft.com/v1.0/me"

	maxMicrosoftOAuthTenantLength = 256
)

var (
	ErrMicrosoftOAuthDescriptorInvalid = errors.New("auth: microsoft oauth descriptor invalid")
	ErrMicrosoftOAuthTenantInvalid     = errors.New("auth: microsoft oauth tenant invalid")
	ErrMicrosoftOAuthCallbackInvalid   = errors.New("auth: microsoft oauth callback invalid")
	ErrMicrosoftOAuthScopeInvalid      = errors.New("auth: microsoft oauth scope invalid")
)

// MicrosoftOAuthDescriptor is the provider-neutral OAuth configuration shape
// adapters and generated code can validate before starting a Microsoft flow.
type MicrosoftOAuthDescriptor struct {
	Tenant       string
	ClientID     string
	ClientSecret string
	RedirectURL  string
	Scopes       []string
}

// MicrosoftOAuthEndpoints are the tenant-aware Microsoft OAuth URLs.
type MicrosoftOAuthEndpoints struct {
	AuthorizeURL string
	TokenURL     string
	ProfileURL   string
}

// MicrosoftOAuthPlan is a dry-run descriptor concrete adapters can apply.
type MicrosoftOAuthPlan struct {
	Provider    string
	Tenant      string
	ClientID    string
	RedirectURL string
	Scopes      []string
	Endpoints   MicrosoftOAuthEndpoints
}

// MicrosoftOAuthSummary is safe for logs, diagnostics, and audit payloads.
type MicrosoftOAuthSummary struct {
	Provider     string
	Tenant       string
	ClientID     string
	ClientSecret string
	RedirectURL  string
	Scopes       []string
	Endpoints    MicrosoftOAuthEndpoints
}

// MicrosoftOAuthDefaultScopes returns the common OpenID Connect + Graph scopes
// used for a Microsoft sign-in profile descriptor.
func MicrosoftOAuthDefaultScopes() []string {
	return []string{"openid", "profile", "email", "User.Read"}
}

// NormalizeMicrosoftOAuthTenant trims a tenant and defaults an empty value to
// "common". Tenant values are lower-cased because Microsoft tenant aliases,
// GUIDs, and verified domain names are case-insensitive in endpoint paths.
func NormalizeMicrosoftOAuthTenant(tenant string) string {
	tenant = strings.TrimSpace(tenant)
	if tenant == "" {
		return MicrosoftOAuthDefaultTenant
	}
	return strings.ToLower(tenant)
}

// ValidateMicrosoftOAuthTenant reports malformed tenant path segments.
func ValidateMicrosoftOAuthTenant(tenant string) error {
	tenant = NormalizeMicrosoftOAuthTenant(tenant)
	if len(tenant) > maxMicrosoftOAuthTenantLength {
		return fmt.Errorf("%w: tenant exceeds %d bytes", ErrMicrosoftOAuthTenantInvalid, maxMicrosoftOAuthTenantLength)
	}
	if strings.Contains(tenant, "..") {
		return fmt.Errorf("%w: tenant contains an empty label", ErrMicrosoftOAuthTenantInvalid)
	}
	for _, r := range tenant {
		if unicode.IsSpace(r) || r < 0x20 || r == 0x7f {
			return fmt.Errorf("%w: tenant contains an invalid character", ErrMicrosoftOAuthTenantInvalid)
		}
		if !(r >= 'a' && r <= 'z') &&
			!(r >= '0' && r <= '9') &&
			r != '.' &&
			r != '-' {
			return fmt.Errorf("%w: tenant contains an invalid character", ErrMicrosoftOAuthTenantInvalid)
		}
	}
	if strings.HasPrefix(tenant, ".") ||
		strings.HasPrefix(tenant, "-") ||
		strings.HasSuffix(tenant, ".") ||
		strings.HasSuffix(tenant, "-") {
		return fmt.Errorf("%w: tenant has an invalid boundary", ErrMicrosoftOAuthTenantInvalid)
	}
	return nil
}

// NormalizeMicrosoftOAuthScopes trims scopes, applies defaults when empty, and
// removes duplicates while preserving first occurrence order.
func NormalizeMicrosoftOAuthScopes(scopes []string) []string {
	if len(scopes) == 0 {
		return MicrosoftOAuthDefaultScopes()
	}
	normalized := make([]string, 0, len(scopes))
	seen := make(map[string]struct{}, len(scopes))
	for _, scope := range scopes {
		scope = strings.TrimSpace(scope)
		if scope == "" {
			continue
		}
		if _, ok := seen[scope]; ok {
			continue
		}
		seen[scope] = struct{}{}
		normalized = append(normalized, scope)
	}
	if len(normalized) == 0 {
		return MicrosoftOAuthDefaultScopes()
	}
	return normalized
}

// ValidateMicrosoftOAuthScopes validates a Microsoft OAuth scope list.
func ValidateMicrosoftOAuthScopes(scopes []string) error {
	var errs []error
	for _, scope := range scopes {
		if strings.TrimSpace(scope) == "" {
			errs = append(errs, fmt.Errorf("%w: scope is required", ErrMicrosoftOAuthScopeInvalid))
			continue
		}
		for _, r := range strings.TrimSpace(scope) {
			if unicode.IsSpace(r) || r < 0x20 || r == 0x7f {
				errs = append(errs, fmt.Errorf("%w: scope contains an invalid character", ErrMicrosoftOAuthScopeInvalid))
				break
			}
		}
	}
	return errors.Join(errs...)
}

// ValidateMicrosoftOAuthCallbackURL reports callback URLs that are unsafe or
// ambiguous for an OAuth redirect target. HTTP is allowed only for localhost.
func ValidateMicrosoftOAuthCallbackURL(rawURL string) error {
	rawURL = strings.TrimSpace(rawURL)
	if rawURL == "" {
		return fmt.Errorf("%w: RedirectURL is required", ErrMicrosoftOAuthCallbackInvalid)
	}
	u, err := url.Parse(rawURL)
	if err != nil {
		return fmt.Errorf("%w: RedirectURL is malformed", ErrMicrosoftOAuthCallbackInvalid)
	}
	if !u.IsAbs() || u.Host == "" {
		return fmt.Errorf("%w: RedirectURL must be absolute", ErrMicrosoftOAuthCallbackInvalid)
	}
	if u.User != nil {
		return fmt.Errorf("%w: RedirectURL must not include userinfo", ErrMicrosoftOAuthCallbackInvalid)
	}
	if u.Fragment != "" {
		return fmt.Errorf("%w: RedirectURL must not include a fragment", ErrMicrosoftOAuthCallbackInvalid)
	}
	switch u.Scheme {
	case "https":
		return nil
	case "http":
		if microsoftOAuthLocalhost(u.Hostname()) {
			return nil
		}
		return fmt.Errorf("%w: http RedirectURL is allowed only for localhost", ErrMicrosoftOAuthCallbackInvalid)
	default:
		return fmt.Errorf("%w: RedirectURL scheme must be http or https", ErrMicrosoftOAuthCallbackInvalid)
	}
}

// NormalizeMicrosoftOAuthDescriptor returns a trimmed descriptor with normalized
// tenant and scopes. It does not hide validation errors.
func NormalizeMicrosoftOAuthDescriptor(descriptor MicrosoftOAuthDescriptor) MicrosoftOAuthDescriptor {
	return MicrosoftOAuthDescriptor{
		Tenant:       NormalizeMicrosoftOAuthTenant(descriptor.Tenant),
		ClientID:     strings.TrimSpace(descriptor.ClientID),
		ClientSecret: strings.TrimSpace(descriptor.ClientSecret),
		RedirectURL:  strings.TrimSpace(descriptor.RedirectURL),
		Scopes:       NormalizeMicrosoftOAuthScopes(descriptor.Scopes),
	}
}

// ValidateMicrosoftOAuthDescriptor reports malformed Microsoft OAuth provider
// descriptor inputs before an adapter builds a live flow.
func ValidateMicrosoftOAuthDescriptor(descriptor MicrosoftOAuthDescriptor) error {
	descriptor = NormalizeMicrosoftOAuthDescriptor(descriptor)
	var errs []error
	if err := ValidateMicrosoftOAuthTenant(descriptor.Tenant); err != nil {
		errs = append(errs, err)
	}
	if descriptor.ClientID == "" {
		errs = append(errs, fmt.Errorf("%w: ClientID is required", ErrMicrosoftOAuthDescriptorInvalid))
	}
	if descriptor.ClientSecret == "" {
		errs = append(errs, fmt.Errorf("%w: ClientSecret is required", ErrMicrosoftOAuthDescriptorInvalid))
	}
	if err := ValidateMicrosoftOAuthCallbackURL(descriptor.RedirectURL); err != nil {
		errs = append(errs, err)
	}
	if err := ValidateMicrosoftOAuthScopes(descriptor.Scopes); err != nil {
		errs = append(errs, err)
	}
	return errors.Join(errs...)
}

// MicrosoftOAuthTenantEndpoints returns the tenant-aware authorize/token URLs
// and Microsoft Graph profile endpoint.
func MicrosoftOAuthTenantEndpoints(tenant string) (MicrosoftOAuthEndpoints, error) {
	tenant = NormalizeMicrosoftOAuthTenant(tenant)
	if err := ValidateMicrosoftOAuthTenant(tenant); err != nil {
		return MicrosoftOAuthEndpoints{}, err
	}
	base := microsoftOAuthAuthorizeBase + "/" + url.PathEscape(tenant) + "/oauth2/v2.0"
	return MicrosoftOAuthEndpoints{
		AuthorizeURL: base + "/authorize",
		TokenURL:     base + "/token",
		ProfileURL:   microsoftOAuthProfileURL,
	}, nil
}

// PlanMicrosoftOAuthDescriptor returns a deterministic, storage-agnostic
// Microsoft OAuth descriptor plan. It does not start an OAuth flow.
func PlanMicrosoftOAuthDescriptor(descriptor MicrosoftOAuthDescriptor) (MicrosoftOAuthPlan, error) {
	descriptor = NormalizeMicrosoftOAuthDescriptor(descriptor)
	if err := ValidateMicrosoftOAuthDescriptor(descriptor); err != nil {
		return MicrosoftOAuthPlan{}, err
	}
	endpoints, err := MicrosoftOAuthTenantEndpoints(descriptor.Tenant)
	if err != nil {
		return MicrosoftOAuthPlan{}, err
	}
	return MicrosoftOAuthPlan{
		Provider:    MicrosoftOAuthProvider,
		Tenant:      descriptor.Tenant,
		ClientID:    descriptor.ClientID,
		RedirectURL: descriptor.RedirectURL,
		Scopes:      append([]string(nil), descriptor.Scopes...),
		Endpoints:   endpoints,
	}, nil
}

// RedactedSummary returns a non-secret plan summary.
func (p MicrosoftOAuthPlan) RedactedSummary() MicrosoftOAuthSummary {
	return MicrosoftOAuthSummary{
		Provider:    p.Provider,
		Tenant:      p.Tenant,
		ClientID:    p.ClientID,
		RedirectURL: redactMicrosoftOAuthURL(p.RedirectURL),
		Scopes:      append([]string(nil), p.Scopes...),
		Endpoints:   p.Endpoints,
	}
}

// MicrosoftOAuthRedactedSummary validates and plans a descriptor, then returns
// an audit-safe summary with the secret and callback query values redacted.
func MicrosoftOAuthRedactedSummary(descriptor MicrosoftOAuthDescriptor) (MicrosoftOAuthSummary, error) {
	normalized := NormalizeMicrosoftOAuthDescriptor(descriptor)
	plan, err := PlanMicrosoftOAuthDescriptor(normalized)
	if err != nil {
		return MicrosoftOAuthSummary{}, err
	}
	summary := plan.RedactedSummary()
	if normalized.ClientSecret != "" {
		summary.ClientSecret = "[redacted]"
	}
	return summary, nil
}

func microsoftOAuthLocalhost(host string) bool {
	host = strings.ToLower(strings.TrimSpace(host))
	if host == "localhost" {
		return true
	}
	ip := net.ParseIP(host)
	return ip != nil && ip.IsLoopback()
}

func redactMicrosoftOAuthURL(rawURL string) string {
	u, err := url.Parse(strings.TrimSpace(rawURL))
	if err != nil || u.Host == "" {
		return "[redacted]"
	}
	u.User = nil
	u.Fragment = ""
	if u.RawQuery != "" {
		q := u.Query()
		for key := range q {
			q.Set(key, "[redacted]")
		}
		u.RawQuery = q.Encode()
	}
	return u.String()
}
