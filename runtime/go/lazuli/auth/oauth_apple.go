package auth

import (
	"encoding/pem"
	"errors"
	"fmt"
	"net"
	"net/url"
	"sort"
	"strings"
)

const (
	// AppleOAuthAuthorizeEndpoint is Apple's Sign in with Apple authorization
	// endpoint. It is static provider metadata; callers still own state/nonce.
	AppleOAuthAuthorizeEndpoint = "https://appleid.apple.com/auth/authorize"
	// AppleOAuthTokenEndpoint is Apple's Sign in with Apple token endpoint.
	AppleOAuthTokenEndpoint = "https://appleid.apple.com/auth/token"
)

var (
	// ErrAppleOAuthDescriptorInvalid reports an unusable Sign in with Apple
	// provider descriptor.
	ErrAppleOAuthDescriptorInvalid = errors.New("auth: apple oauth descriptor invalid")
	// ErrAppleOAuthCallbackURLInvalid reports a callback URL Apple cannot use.
	ErrAppleOAuthCallbackURLInvalid = errors.New("auth: apple oauth callback url invalid")
	// ErrAppleOAuthScopeInvalid reports an unsupported Sign in with Apple scope.
	ErrAppleOAuthScopeInvalid = errors.New("auth: apple oauth scope invalid")
)

var appleOAuthAllowedScopes = map[string]struct{}{
	"email": {},
	"name":  {},
}

// AppleOAuthProviderDescriptor carries provider metadata needed to prepare a
// Sign in with Apple OAuth configuration. It is descriptor-only and does not
// sign client secrets, exchange tokens, or perform HTTP calls.
type AppleOAuthProviderDescriptor struct {
	TeamID        string
	ClientID      string
	KeyID         string
	PrivateKeyPEM string
	CallbackURL   string
	Scopes        []string
}

// AppleOAuthProviderPlan is the normalized descriptor data needed by a future
// adapter to build an Apple OAuth flow.
type AppleOAuthProviderPlan struct {
	AuthorizeURL string
	TokenURL     string
	TeamID       string
	ClientID     string
	KeyID        string
	CallbackURL  string
	Scopes       []string
}

// AppleOAuthProviderSummary is safe for logs and diagnostics.
type AppleOAuthProviderSummary struct {
	AuthorizeURL         string
	TokenURL             string
	TeamID               string
	ClientID             string
	KeyID                string
	CallbackURL          string
	Scopes               []string
	PrivateKeyConfigured bool
}

// Normalize returns a trimmed, deterministic descriptor without mutating the
// receiver.
func (d AppleOAuthProviderDescriptor) Normalize() AppleOAuthProviderDescriptor {
	d.TeamID = strings.TrimSpace(d.TeamID)
	d.ClientID = strings.TrimSpace(d.ClientID)
	d.KeyID = strings.TrimSpace(d.KeyID)
	d.PrivateKeyPEM = strings.TrimSpace(d.PrivateKeyPEM)
	d.CallbackURL = strings.TrimSpace(d.CallbackURL)
	d.Scopes = NormalizeAppleOAuthScopes(d.Scopes)
	return d
}

// Validate checks required Apple team/client/key metadata, callback URL shape,
// and supported Sign in with Apple scopes.
func (d AppleOAuthProviderDescriptor) Validate() error {
	d = d.Normalize()

	var errs []error
	if d.TeamID == "" {
		errs = append(errs, fmt.Errorf("%w: team id required", ErrAppleOAuthDescriptorInvalid))
	}
	if d.ClientID == "" {
		errs = append(errs, fmt.Errorf("%w: client id required", ErrAppleOAuthDescriptorInvalid))
	}
	if d.KeyID == "" {
		errs = append(errs, fmt.Errorf("%w: key id required", ErrAppleOAuthDescriptorInvalid))
	}
	if d.PrivateKeyPEM == "" {
		errs = append(errs, fmt.Errorf("%w: private key required", ErrAppleOAuthDescriptorInvalid))
	} else if !validAppleOAuthPrivateKeyPEM(d.PrivateKeyPEM) {
		errs = append(errs, fmt.Errorf("%w: private key pem invalid", ErrAppleOAuthDescriptorInvalid))
	}
	if err := ValidateAppleOAuthCallbackURL(d.CallbackURL); err != nil {
		errs = append(errs, err)
	}
	if err := ValidateAppleOAuthScopes(d.Scopes); err != nil {
		errs = append(errs, err)
	}
	return errors.Join(errs...)
}

// SafeSummary returns redacted descriptor metadata suitable for logs.
func (d AppleOAuthProviderDescriptor) SafeSummary() AppleOAuthProviderSummary {
	normalized := d.Normalize()
	return AppleOAuthProviderSummary{
		AuthorizeURL:         AppleOAuthAuthorizeEndpoint,
		TokenURL:             AppleOAuthTokenEndpoint,
		TeamID:               redactAppleOAuthValue(normalized.TeamID),
		ClientID:             redactAppleOAuthValue(normalized.ClientID),
		KeyID:                redactAppleOAuthValue(normalized.KeyID),
		CallbackURL:          redactAppleOAuthURL(normalized.CallbackURL),
		Scopes:               cloneAppleOAuthStrings(normalized.Scopes),
		PrivateKeyConfigured: normalized.PrivateKeyPEM != "",
	}
}

// PlanAppleOAuthProvider validates and normalizes Apple provider descriptor
// metadata into a deterministic dry-run plan.
func PlanAppleOAuthProvider(descriptor AppleOAuthProviderDescriptor) (AppleOAuthProviderPlan, error) {
	normalized := descriptor.Normalize()
	if err := normalized.Validate(); err != nil {
		return AppleOAuthProviderPlan{}, err
	}
	return AppleOAuthProviderPlan{
		AuthorizeURL: AppleOAuthAuthorizeEndpoint,
		TokenURL:     AppleOAuthTokenEndpoint,
		TeamID:       normalized.TeamID,
		ClientID:     normalized.ClientID,
		KeyID:        normalized.KeyID,
		CallbackURL:  normalized.CallbackURL,
		Scopes:       cloneAppleOAuthStrings(normalized.Scopes),
	}, nil
}

// SafeSummary returns redacted plan metadata suitable for logs.
func (p AppleOAuthProviderPlan) SafeSummary() AppleOAuthProviderSummary {
	return AppleOAuthProviderSummary{
		AuthorizeURL:         p.AuthorizeURL,
		TokenURL:             p.TokenURL,
		TeamID:               redactAppleOAuthValue(p.TeamID),
		ClientID:             redactAppleOAuthValue(p.ClientID),
		KeyID:                redactAppleOAuthValue(p.KeyID),
		CallbackURL:          redactAppleOAuthURL(p.CallbackURL),
		Scopes:               cloneAppleOAuthStrings(p.Scopes),
		PrivateKeyConfigured: true,
	}
}

// NormalizeAppleOAuthScopes trims, lowercases, deduplicates, and sorts Sign in
// with Apple scopes.
func NormalizeAppleOAuthScopes(scopes []string) []string {
	if len(scopes) == 0 {
		return nil
	}

	seen := make(map[string]struct{}, len(scopes))
	normalized := make([]string, 0, len(scopes))
	for _, scope := range scopes {
		scope = strings.ToLower(strings.TrimSpace(scope))
		if scope == "" {
			continue
		}
		if _, ok := seen[scope]; ok {
			continue
		}
		seen[scope] = struct{}{}
		normalized = append(normalized, scope)
	}
	sort.Strings(normalized)
	return normalized
}

// ValidateAppleOAuthScopes checks that every scope is supported by Sign in with
// Apple. Empty scope sets are valid.
func ValidateAppleOAuthScopes(scopes []string) error {
	normalized := NormalizeAppleOAuthScopes(scopes)
	for _, scope := range normalized {
		if _, ok := appleOAuthAllowedScopes[scope]; !ok {
			return fmt.Errorf("%w: %q", ErrAppleOAuthScopeInvalid, scope)
		}
	}
	return nil
}

// ValidateAppleOAuthCallbackURL checks the redirect URL shape required for an
// Apple web callback. The function only validates syntax and safety; it does
// not check Apple developer console registration.
func ValidateAppleOAuthCallbackURL(callbackURL string) error {
	callbackURL = strings.TrimSpace(callbackURL)
	if callbackURL == "" {
		return fmt.Errorf("%w: required", ErrAppleOAuthCallbackURLInvalid)
	}

	u, err := url.Parse(callbackURL)
	if err != nil {
		return fmt.Errorf("%w: %v", ErrAppleOAuthCallbackURLInvalid, err)
	}
	if u.Scheme != "https" {
		return fmt.Errorf("%w: scheme must be https", ErrAppleOAuthCallbackURLInvalid)
	}
	if u.Host == "" || u.Hostname() == "" {
		return fmt.Errorf("%w: host required", ErrAppleOAuthCallbackURLInvalid)
	}
	if u.User != nil {
		return fmt.Errorf("%w: userinfo not allowed", ErrAppleOAuthCallbackURLInvalid)
	}
	if u.Fragment != "" {
		return fmt.Errorf("%w: fragment not allowed", ErrAppleOAuthCallbackURLInvalid)
	}
	if ip := net.ParseIP(u.Hostname()); ip != nil && !ip.IsGlobalUnicast() {
		return fmt.Errorf("%w: host must be public", ErrAppleOAuthCallbackURLInvalid)
	}
	return nil
}

func redactAppleOAuthValue(value string) string {
	value = strings.TrimSpace(value)
	if value == "" {
		return ""
	}
	if len(value) <= 4 {
		return "[REDACTED]"
	}
	return value[:2] + "[REDACTED]" + value[len(value)-2:]
}

func validAppleOAuthPrivateKeyPEM(value string) bool {
	block, rest := pem.Decode([]byte(value))
	if block == nil || len(strings.TrimSpace(string(rest))) != 0 {
		return false
	}
	return block.Type == "PRIVATE KEY" || strings.HasSuffix(block.Type, " PRIVATE KEY")
}

func redactAppleOAuthURL(raw string) string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return ""
	}
	u, err := url.Parse(raw)
	if err != nil || u.Scheme == "" || u.Host == "" {
		return "[REDACTED]"
	}
	u.User = nil
	u.RawQuery = ""
	u.Fragment = ""
	return u.String()
}

func cloneAppleOAuthStrings(values []string) []string {
	if len(values) == 0 {
		return nil
	}
	cloned := make([]string, len(values))
	copy(cloned, values)
	return cloned
}
