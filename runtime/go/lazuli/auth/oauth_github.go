package auth

import (
	"errors"
	"fmt"
	"net"
	"net/url"
	"slices"
	"strings"
)

const (
	DefaultGitHubOAuthAuthorizeURL  = "https://github.com/login/oauth/authorize"
	DefaultGitHubOAuthTokenURL      = "https://github.com/login/oauth/access_token"
	DefaultGitHubOAuthUserEmailsURL = "https://api.github.com/user/emails"
)

var (
	ErrGitHubOAuthDescriptorInvalid = errors.New("auth: github oauth descriptor invalid")
	defaultGitHubOAuthScopes        = []string{"read:user", "user:email"}
)

// GitHubOAuthDescriptor is provider configuration before it is bound to a
// concrete OAuth flow. It carries only deterministic descriptor metadata and
// never performs provider calls.
type GitHubOAuthDescriptor struct {
	ClientID     string
	ClientSecret string
	CallbackURL  string
	Scopes       []string

	AuthorizeURL  string
	TokenURL      string
	UserEmailsURL string
}

// GitHubOAuthPlan is the normalized, validated provider descriptor used by
// generated transports or adapters to assemble a future OAuth flow.
type GitHubOAuthPlan struct {
	Provider string

	ClientID     string
	ClientSecret string
	CallbackURL  string
	Scopes       []string

	AuthorizeURL  string
	TokenURL      string
	UserEmailsURL string
}

// GitHubOAuthSummary is safe to log or expose in diagnostics. Secrets are
// represented only by presence, and callback URL query values are removed.
type GitHubOAuthSummary struct {
	Provider               string
	ClientID               string
	ClientSecretConfigured bool
	CallbackURL            string
	Scopes                 []string
	AuthorizeURL           string
	TokenURL               string
	UserEmailsURL          string
}

// DefaultGitHubOAuthDescriptor returns GitHub's canonical endpoints and the
// minimal scopes needed to read the authenticated user's profile and emails.
func DefaultGitHubOAuthDescriptor() GitHubOAuthDescriptor {
	return GitHubOAuthDescriptor{
		Scopes:        slices.Clone(defaultGitHubOAuthScopes),
		AuthorizeURL:  DefaultGitHubOAuthAuthorizeURL,
		TokenURL:      DefaultGitHubOAuthTokenURL,
		UserEmailsURL: DefaultGitHubOAuthUserEmailsURL,
	}
}

// Normalize returns a trimmed descriptor with defaults applied.
func (d GitHubOAuthDescriptor) Normalize() (GitHubOAuthDescriptor, error) {
	return NormalizeGitHubOAuthDescriptor(d)
}

// Validate reports whether the descriptor can be planned safely.
func (d GitHubOAuthDescriptor) Validate() error {
	return ValidateGitHubOAuthDescriptor(d)
}

// NormalizeGitHubOAuthDescriptor applies default endpoints and scopes, trims
// text inputs, and canonicalizes callback and endpoint URLs.
func NormalizeGitHubOAuthDescriptor(d GitHubOAuthDescriptor) (GitHubOAuthDescriptor, error) {
	var errs []error

	out := GitHubOAuthDescriptor{
		ClientID:      strings.TrimSpace(d.ClientID),
		ClientSecret:  strings.TrimSpace(d.ClientSecret),
		Scopes:        normalizeGitHubOAuthScopes(d.Scopes),
		AuthorizeURL:  strings.TrimSpace(d.AuthorizeURL),
		TokenURL:      strings.TrimSpace(d.TokenURL),
		UserEmailsURL: strings.TrimSpace(d.UserEmailsURL),
	}

	if strings.TrimSpace(d.CallbackURL) != "" {
		callbackURL, err := NormalizeGitHubOAuthCallbackURL(d.CallbackURL)
		if err != nil {
			errs = append(errs, err)
		} else {
			out.CallbackURL = callbackURL
		}
	}
	if len(out.Scopes) == 0 {
		out.Scopes = slices.Clone(defaultGitHubOAuthScopes)
	}
	if out.AuthorizeURL == "" {
		out.AuthorizeURL = DefaultGitHubOAuthAuthorizeURL
	}
	if out.TokenURL == "" {
		out.TokenURL = DefaultGitHubOAuthTokenURL
	}
	if out.UserEmailsURL == "" {
		out.UserEmailsURL = DefaultGitHubOAuthUserEmailsURL
	}

	out.AuthorizeURL = normalizeGitHubOAuthEndpointURL(out.AuthorizeURL, "authorize_url", &errs)
	out.TokenURL = normalizeGitHubOAuthEndpointURL(out.TokenURL, "token_url", &errs)
	out.UserEmailsURL = normalizeGitHubOAuthEndpointURL(out.UserEmailsURL, "user_emails_url", &errs)

	if err := errors.Join(errs...); err != nil {
		return GitHubOAuthDescriptor{}, err
	}
	return out, nil
}

// ValidateGitHubOAuthDescriptor checks descriptor shape without performing any
// OAuth exchange or HTTP request.
func ValidateGitHubOAuthDescriptor(d GitHubOAuthDescriptor) error {
	normalized, err := NormalizeGitHubOAuthDescriptor(d)
	if err != nil {
		return err
	}

	var errs []error
	if normalized.ClientID == "" {
		errs = append(errs, fmt.Errorf("%w: client_id is required", ErrGitHubOAuthDescriptorInvalid))
	}
	if normalized.ClientSecret == "" {
		errs = append(errs, fmt.Errorf("%w: client_secret is required", ErrGitHubOAuthDescriptorInvalid))
	}
	if normalized.CallbackURL == "" {
		errs = append(errs, fmt.Errorf("%w: callback_url is required", ErrGitHubOAuthDescriptorInvalid))
	}
	for _, scope := range normalized.Scopes {
		if strings.ContainsAny(scope, " \t\r\n") {
			errs = append(errs, fmt.Errorf("%w: scope %q must not contain whitespace", ErrGitHubOAuthDescriptorInvalid, scope))
		}
	}
	return errors.Join(errs...)
}

// PlanGitHubOAuth returns a normalized provider descriptor. The result is a
// dry-run plan only; callers own any future OAuth flow and HTTP behavior.
func PlanGitHubOAuth(d GitHubOAuthDescriptor) (GitHubOAuthPlan, error) {
	normalized, err := NormalizeGitHubOAuthDescriptor(d)
	if err != nil {
		return GitHubOAuthPlan{}, err
	}
	if err := ValidateGitHubOAuthDescriptor(normalized); err != nil {
		return GitHubOAuthPlan{}, err
	}
	return GitHubOAuthPlan{
		Provider:      "github",
		ClientID:      normalized.ClientID,
		ClientSecret:  normalized.ClientSecret,
		CallbackURL:   normalized.CallbackURL,
		Scopes:        slices.Clone(normalized.Scopes),
		AuthorizeURL:  normalized.AuthorizeURL,
		TokenURL:      normalized.TokenURL,
		UserEmailsURL: normalized.UserEmailsURL,
	}, nil
}

// SafeSummary returns redaction-safe plan metadata.
func (p GitHubOAuthPlan) SafeSummary() GitHubOAuthSummary {
	return SafeGitHubOAuthSummary(p)
}

// SafeGitHubOAuthSummary returns a descriptor summary that does not leak
// secrets or callback query parameters.
func SafeGitHubOAuthSummary(p GitHubOAuthPlan) GitHubOAuthSummary {
	return GitHubOAuthSummary{
		Provider:               p.Provider,
		ClientID:               p.ClientID,
		ClientSecretConfigured: strings.TrimSpace(p.ClientSecret) != "",
		CallbackURL:            redactGitHubOAuthURL(p.CallbackURL),
		Scopes:                 slices.Clone(p.Scopes),
		AuthorizeURL:           redactGitHubOAuthURL(p.AuthorizeURL),
		TokenURL:               redactGitHubOAuthURL(p.TokenURL),
		UserEmailsURL:          redactGitHubOAuthURL(p.UserEmailsURL),
	}
}

// NormalizeGitHubOAuthCallbackURL validates and canonicalizes a callback URL.
// HTTPS is required except for loopback hosts, which may use HTTP for local
// development and tests.
func NormalizeGitHubOAuthCallbackURL(raw string) (string, error) {
	u, err := parseGitHubOAuthURL(raw, "callback_url")
	if err != nil {
		return "", err
	}
	if u.Scheme == "http" && !isGitHubOAuthLoopbackHost(u.Hostname()) {
		return "", fmt.Errorf("%w: callback_url http is only allowed for loopback hosts", ErrGitHubOAuthDescriptorInvalid)
	}
	if u.Scheme != "https" && u.Scheme != "http" {
		return "", fmt.Errorf("%w: callback_url scheme must be https", ErrGitHubOAuthDescriptorInvalid)
	}
	return u.String(), nil
}

func normalizeGitHubOAuthScopes(scopes []string) []string {
	out := make([]string, 0, len(scopes))
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
		out = append(out, scope)
	}
	return out
}

func normalizeGitHubOAuthEndpointURL(raw, field string, errs *[]error) string {
	u, err := parseGitHubOAuthURL(raw, field)
	if err != nil {
		*errs = append(*errs, err)
		return ""
	}
	if u.Scheme != "https" {
		*errs = append(*errs, fmt.Errorf("%w: %s scheme must be https", ErrGitHubOAuthDescriptorInvalid, field))
		return ""
	}
	if u.RawQuery != "" {
		*errs = append(*errs, fmt.Errorf("%w: %s must not include query parameters", ErrGitHubOAuthDescriptorInvalid, field))
		return ""
	}
	return u.String()
}

func parseGitHubOAuthURL(raw, field string) (*url.URL, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil, fmt.Errorf("%w: %s is required", ErrGitHubOAuthDescriptorInvalid, field)
	}
	u, err := url.Parse(raw)
	if err != nil {
		return nil, fmt.Errorf("%w: %s: %v", ErrGitHubOAuthDescriptorInvalid, field, err)
	}
	u.Scheme = strings.ToLower(u.Scheme)
	u.Host = strings.ToLower(u.Host)
	if u.Scheme == "" || u.Host == "" {
		return nil, fmt.Errorf("%w: %s must be an absolute URL", ErrGitHubOAuthDescriptorInvalid, field)
	}
	if u.User != nil {
		return nil, fmt.Errorf("%w: %s must not include userinfo", ErrGitHubOAuthDescriptorInvalid, field)
	}
	if u.Fragment != "" {
		return nil, fmt.Errorf("%w: %s must not include a fragment", ErrGitHubOAuthDescriptorInvalid, field)
	}
	u.Host = stripGitHubOAuthDefaultPort(u)
	if u.Path == "" {
		u.Path = "/"
	}
	return u, nil
}

func stripGitHubOAuthDefaultPort(u *url.URL) string {
	host := u.Hostname()
	port := u.Port()
	if port == "" {
		return u.Host
	}
	if (u.Scheme == "https" && port == "443") || (u.Scheme == "http" && port == "80") {
		if strings.Contains(host, ":") {
			return "[" + host + "]"
		}
		return host
	}
	return u.Host
}

func isGitHubOAuthLoopbackHost(host string) bool {
	switch strings.ToLower(host) {
	case "localhost":
		return true
	}
	ip := net.ParseIP(host)
	return ip != nil && ip.IsLoopback()
}

func redactGitHubOAuthURL(raw string) string {
	u, err := url.Parse(strings.TrimSpace(raw))
	if err != nil {
		return ""
	}
	u.RawQuery = ""
	u.Fragment = ""
	u.User = nil
	return u.String()
}
