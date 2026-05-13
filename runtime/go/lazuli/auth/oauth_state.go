package auth

import (
	"errors"
	"fmt"
	"net"
	"net/url"
	pathpkg "path"
	"strings"
	"time"
)

const (
	DefaultOAuthStateTTL = 10 * time.Minute

	minOAuthStateTokenLength = 16
	maxOAuthStateTokenLength = 512
	minOAuthNonceLength      = 12
	maxOAuthNonceLength      = 512
)

var (
	ErrOAuthStateInvalid     = errors.New("auth: oauth state invalid")
	ErrOAuthNonceInvalid     = errors.New("auth: oauth nonce invalid")
	ErrOAuthReturnURLInvalid = errors.New("auth: oauth return url invalid")
	ErrOAuthCallbackInvalid  = errors.New("auth: oauth callback invalid")
)

// OAuthStateMetadata is the provider-neutral state record generated transports
// can persist while the user is away at an OAuth provider. It contains only
// deterministic metadata; storage, cookies, and handlers stay outside auth.
type OAuthStateMetadata struct {
	State     string
	Nonce     string
	ReturnURL string
	IssuedAt  time.Time
	ExpiresAt time.Time
}

// OAuthStateSummary is safe for logs and diagnostics.
type OAuthStateSummary struct {
	State     string
	Nonce     string
	ReturnURL string
	IssuedAt  time.Time
	ExpiresAt time.Time
	Expired   bool
}

// OAuthReturnURLPolicy defines the post-callback destinations a generated
// transport may use. Origins are absolute scheme+host entries; path prefixes
// are same-origin, root-relative entries such as "/dashboard".
type OAuthReturnURLPolicy struct {
	DefaultURL          string
	AllowedOrigins      []string
	AllowedPathPrefixes []string
}

// OAuthReturnURLPlan is the normalized allowlist decision for one return URL.
type OAuthReturnURLPlan struct {
	ReturnURL       string
	RedactedURL     string
	MatchedRule     string
	Defaulted       bool
	AllowedExternal bool
}

// OAuthCallbackInput is the provider-neutral data needed to validate the local
// callback envelope before provider-specific token exchange.
type OAuthCallbackInput struct {
	State string
	Nonce string
	Code  string
	Now   time.Time
}

// ValidateOAuthStateToken reports malformed state tokens. Tokens are treated as
// opaque URL-safe values and must not contain whitespace or control characters.
func ValidateOAuthStateToken(token string) error {
	token = strings.TrimSpace(token)
	if len(token) < minOAuthStateTokenLength {
		return fmt.Errorf("%w: token must be at least %d bytes", ErrOAuthStateInvalid, minOAuthStateTokenLength)
	}
	if len(token) > maxOAuthStateTokenLength {
		return fmt.Errorf("%w: token exceeds %d bytes", ErrOAuthStateInvalid, maxOAuthStateTokenLength)
	}
	for _, r := range token {
		if !isOAuthOpaqueTokenChar(r) {
			return fmt.Errorf("%w: token contains an invalid character", ErrOAuthStateInvalid)
		}
	}
	return nil
}

// RedactOAuthStateToken returns a short stable hint without exposing the secret
// state value.
func RedactOAuthStateToken(token string) string {
	token = strings.TrimSpace(token)
	if token == "" {
		return ""
	}
	if len(token) <= 8 {
		return "[redacted]"
	}
	return token[:4] + "..." + token[len(token)-4:]
}

// ValidateOAuthNonce reports malformed OIDC nonce values. Empty nonces are
// allowed so pure OAuth providers can skip nonce handling.
func ValidateOAuthNonce(nonce string) error {
	nonce = strings.TrimSpace(nonce)
	if nonce == "" {
		return nil
	}
	if len(nonce) < minOAuthNonceLength {
		return fmt.Errorf("%w: nonce must be at least %d bytes", ErrOAuthNonceInvalid, minOAuthNonceLength)
	}
	if len(nonce) > maxOAuthNonceLength {
		return fmt.Errorf("%w: nonce exceeds %d bytes", ErrOAuthNonceInvalid, maxOAuthNonceLength)
	}
	for _, r := range nonce {
		if !isOAuthOpaqueTokenChar(r) {
			return fmt.Errorf("%w: nonce contains an invalid character", ErrOAuthNonceInvalid)
		}
	}
	return nil
}

// RedactOAuthNonce returns a non-secret nonce hint suitable for diagnostics.
func RedactOAuthNonce(nonce string) string {
	return RedactOAuthStateToken(nonce)
}

// PlanOAuthStateMetadata validates state, nonce, and return URL policy inputs
// and returns a normalized metadata record with expiry populated.
func PlanOAuthStateMetadata(
	state, nonce, returnURL string,
	policy OAuthReturnURLPolicy,
	issuedAt time.Time,
	ttl time.Duration,
) (OAuthStateMetadata, OAuthReturnURLPlan, error) {
	state = strings.TrimSpace(state)
	nonce = strings.TrimSpace(nonce)
	if err := ValidateOAuthStateToken(state); err != nil {
		return OAuthStateMetadata{}, OAuthReturnURLPlan{}, err
	}
	if err := ValidateOAuthNonce(nonce); err != nil {
		return OAuthStateMetadata{}, OAuthReturnURLPlan{}, err
	}
	if ttl < 0 {
		return OAuthStateMetadata{}, OAuthReturnURLPlan{}, fmt.Errorf("%w: ttl must not be negative", ErrOAuthStateInvalid)
	}
	if ttl == 0 {
		ttl = DefaultOAuthStateTTL
	}
	returnPlan, err := PlanOAuthReturnURL(returnURL, policy)
	if err != nil {
		return OAuthStateMetadata{}, OAuthReturnURLPlan{}, err
	}
	meta := OAuthStateMetadata{
		State:     state,
		Nonce:     nonce,
		ReturnURL: returnPlan.ReturnURL,
		IssuedAt:  issuedAt,
		ExpiresAt: issuedAt.Add(ttl),
	}
	return meta, returnPlan, nil
}

// Expired reports whether the state metadata is expired at now.
func (m OAuthStateMetadata) Expired(now time.Time) bool {
	return !m.ExpiresAt.IsZero() && !now.Before(m.ExpiresAt)
}

// Validate reports malformed or expired state metadata.
func (m OAuthStateMetadata) Validate(now time.Time) error {
	if err := ValidateOAuthStateToken(m.State); err != nil {
		return err
	}
	if err := ValidateOAuthNonce(m.Nonce); err != nil {
		return err
	}
	if m.ExpiresAt.IsZero() {
		return fmt.Errorf("%w: ExpiresAt is required", ErrOAuthStateInvalid)
	}
	if m.Expired(now) {
		return fmt.Errorf("%w: state expired", ErrOAuthStateInvalid)
	}
	return nil
}

// RedactedSummary returns metadata safe for logs and audit payloads.
func (m OAuthStateMetadata) RedactedSummary(now time.Time) OAuthStateSummary {
	return OAuthStateSummary{
		State:     RedactOAuthStateToken(m.State),
		Nonce:     RedactOAuthNonce(m.Nonce),
		ReturnURL: RedactOAuthURL(m.ReturnURL),
		IssuedAt:  m.IssuedAt,
		ExpiresAt: m.ExpiresAt,
		Expired:   m.Expired(now),
	}
}

// NormalizeOAuthReturnURLPolicy trims and canonicalizes allowlist entries.
func NormalizeOAuthReturnURLPolicy(policy OAuthReturnURLPolicy) (OAuthReturnURLPolicy, error) {
	var errs []error
	out := OAuthReturnURLPolicy{
		AllowedOrigins:      make([]string, 0, len(policy.AllowedOrigins)),
		AllowedPathPrefixes: make([]string, 0, len(policy.AllowedPathPrefixes)),
	}
	if strings.TrimSpace(policy.DefaultURL) != "" {
		defaultURL, err := normalizeOAuthReturnURL(policy.DefaultURL)
		if err != nil {
			errs = append(errs, fmt.Errorf("%w: default_url: %v", ErrOAuthReturnURLInvalid, err))
		} else {
			out.DefaultURL = defaultURL
		}
	}
	seenOrigins := make(map[string]struct{}, len(policy.AllowedOrigins))
	for _, raw := range policy.AllowedOrigins {
		origin, err := normalizeOAuthReturnOrigin(raw)
		if err != nil {
			errs = append(errs, err)
			continue
		}
		if origin == "" {
			continue
		}
		if _, ok := seenOrigins[origin]; ok {
			continue
		}
		seenOrigins[origin] = struct{}{}
		out.AllowedOrigins = append(out.AllowedOrigins, origin)
	}
	seenPaths := make(map[string]struct{}, len(policy.AllowedPathPrefixes))
	for _, raw := range policy.AllowedPathPrefixes {
		prefix, err := normalizeOAuthReturnPathPrefix(raw)
		if err != nil {
			errs = append(errs, err)
			continue
		}
		if prefix == "" {
			continue
		}
		if _, ok := seenPaths[prefix]; ok {
			continue
		}
		seenPaths[prefix] = struct{}{}
		out.AllowedPathPrefixes = append(out.AllowedPathPrefixes, prefix)
	}
	if err := errors.Join(errs...); err != nil {
		return OAuthReturnURLPolicy{}, err
	}
	return out, nil
}

// ValidateOAuthReturnURLPolicy reports malformed allowlist policy entries.
func ValidateOAuthReturnURLPolicy(policy OAuthReturnURLPolicy) error {
	normalized, err := NormalizeOAuthReturnURLPolicy(policy)
	if err != nil {
		return err
	}
	if normalized.DefaultURL == "" {
		return fmt.Errorf("%w: default_url is required", ErrOAuthReturnURLInvalid)
	}
	if len(normalized.AllowedOrigins) == 0 && len(normalized.AllowedPathPrefixes) == 0 {
		return fmt.Errorf("%w: at least one allowlist rule is required", ErrOAuthReturnURLInvalid)
	}
	if _, err := PlanOAuthReturnURL(normalized.DefaultURL, normalized); err != nil {
		return fmt.Errorf("%w: default_url is not allowed", err)
	}
	return nil
}

// PlanOAuthReturnURL selects a normalized return URL from request and policy.
func PlanOAuthReturnURL(raw string, policy OAuthReturnURLPolicy) (OAuthReturnURLPlan, error) {
	normalized, err := NormalizeOAuthReturnURLPolicy(policy)
	if err != nil {
		return OAuthReturnURLPlan{}, err
	}
	requested := strings.TrimSpace(raw)
	defaulted := requested == ""
	if defaulted {
		requested = normalized.DefaultURL
	}
	if requested == "" {
		return OAuthReturnURLPlan{}, fmt.Errorf("%w: return_url is required", ErrOAuthReturnURLInvalid)
	}
	returnURL, err := normalizeOAuthReturnURL(requested)
	if err != nil {
		return OAuthReturnURLPlan{}, fmt.Errorf("%w: return_url: %v", ErrOAuthReturnURLInvalid, err)
	}
	matched, external := matchOAuthReturnURL(returnURL, normalized)
	if matched == "" {
		return OAuthReturnURLPlan{}, fmt.Errorf("%w: return_url is not allowlisted", ErrOAuthReturnURLInvalid)
	}
	return OAuthReturnURLPlan{
		ReturnURL:       returnURL,
		RedactedURL:     RedactOAuthURL(returnURL),
		MatchedRule:     matched,
		Defaulted:       defaulted,
		AllowedExternal: external,
	}, nil
}

// RedactOAuthURL removes userinfo, query values, and fragments from a URL while
// retaining enough route information for diagnostics.
func RedactOAuthURL(raw string) string {
	u, err := url.Parse(strings.TrimSpace(raw))
	if err != nil {
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

// ValidateOAuthCallback checks local callback inputs against trusted metadata.
func ValidateOAuthCallback(input OAuthCallbackInput, expected OAuthStateMetadata) error {
	if err := expected.Validate(input.Now); err != nil {
		return err
	}
	if strings.TrimSpace(input.Code) == "" {
		return fmt.Errorf("%w: code is required", ErrOAuthCallbackInvalid)
	}
	state := strings.TrimSpace(input.State)
	if err := ValidateOAuthStateToken(state); err != nil {
		return fmt.Errorf("%w: %v", ErrOAuthCallbackInvalid, err)
	}
	if state != expected.State {
		return ErrOAuthStateMismatch
	}
	nonce := strings.TrimSpace(input.Nonce)
	if err := ValidateOAuthNonce(nonce); err != nil {
		return fmt.Errorf("%w: %v", ErrOAuthCallbackInvalid, err)
	}
	if expected.Nonce != "" && nonce != expected.Nonce {
		return fmt.Errorf("%w: nonce mismatch", ErrOAuthCallbackInvalid)
	}
	return nil
}

func normalizeOAuthReturnURL(raw string) (string, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return "", errors.New("is required")
	}
	u, err := url.Parse(raw)
	if err != nil {
		return "", err
	}
	if u.User != nil {
		return "", errors.New("must not include userinfo")
	}
	if u.Fragment != "" {
		return "", errors.New("must not include a fragment")
	}
	if u.IsAbs() {
		if u.Host == "" {
			return "", errors.New("must include a host")
		}
		u.Scheme = strings.ToLower(u.Scheme)
		u.Host = strings.ToLower(u.Host)
		if u.Scheme != "https" && !(u.Scheme == "http" && oauthReturnLoopback(u.Hostname())) {
			return "", errors.New("scheme must be https unless host is loopback")
		}
		u.Host = stripOAuthReturnDefaultPort(u)
		if u.Path == "" {
			u.Path = "/"
		} else {
			u.Path = cleanOAuthReturnPath(u.Path)
		}
		return u.String(), nil
	}
	if strings.HasPrefix(raw, "//") || !strings.HasPrefix(raw, "/") {
		return "", errors.New("must be absolute or root-relative")
	}
	u.Path = cleanOAuthReturnPath(u.Path)
	return u.String(), nil
}

func normalizeOAuthReturnOrigin(raw string) (string, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return "", nil
	}
	u, err := url.Parse(raw)
	if err != nil {
		return "", fmt.Errorf("%w: allowed_origin: %v", ErrOAuthReturnURLInvalid, err)
	}
	u.Scheme = strings.ToLower(u.Scheme)
	u.Host = strings.ToLower(u.Host)
	if !u.IsAbs() || u.Host == "" {
		return "", fmt.Errorf("%w: allowed_origin must be absolute", ErrOAuthReturnURLInvalid)
	}
	if u.User != nil || u.RawQuery != "" || u.Fragment != "" || (u.Path != "" && u.Path != "/") {
		return "", fmt.Errorf("%w: allowed_origin must only include scheme and host", ErrOAuthReturnURLInvalid)
	}
	if u.Scheme != "https" && !(u.Scheme == "http" && oauthReturnLoopback(u.Hostname())) {
		return "", fmt.Errorf("%w: allowed_origin scheme must be https unless host is loopback", ErrOAuthReturnURLInvalid)
	}
	u.Host = stripOAuthReturnDefaultPort(u)
	return u.Scheme + "://" + u.Host, nil
}

func normalizeOAuthReturnPathPrefix(raw string) (string, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return "", nil
	}
	u, err := url.Parse(raw)
	if err != nil {
		return "", fmt.Errorf("%w: allowed_path_prefix: %v", ErrOAuthReturnURLInvalid, err)
	}
	if u.IsAbs() || strings.HasPrefix(raw, "//") || !strings.HasPrefix(raw, "/") {
		return "", fmt.Errorf("%w: allowed_path_prefix must be root-relative", ErrOAuthReturnURLInvalid)
	}
	if u.RawQuery != "" || u.Fragment != "" {
		return "", fmt.Errorf("%w: allowed_path_prefix must not include query or fragment", ErrOAuthReturnURLInvalid)
	}
	if u.Path == "" {
		return "/", nil
	}
	return cleanOAuthReturnPath(u.Path), nil
}

func matchOAuthReturnURL(returnURL string, policy OAuthReturnURLPolicy) (string, bool) {
	u, _ := url.Parse(returnURL)
	if u.IsAbs() {
		origin := u.Scheme + "://" + u.Host
		for _, allowed := range policy.AllowedOrigins {
			if origin == allowed {
				return allowed, true
			}
		}
		return "", false
	}
	for _, prefix := range policy.AllowedPathPrefixes {
		if prefix == "/" || u.Path == prefix || strings.HasPrefix(u.Path, strings.TrimRight(prefix, "/")+"/") {
			return prefix, false
		}
	}
	return "", false
}

func stripOAuthReturnDefaultPort(u *url.URL) string {
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

func oauthReturnLoopback(host string) bool {
	host = strings.ToLower(strings.TrimSpace(host))
	if host == "localhost" {
		return true
	}
	ip := net.ParseIP(host)
	return ip != nil && ip.IsLoopback()
}

func cleanOAuthReturnPath(path string) string {
	if path == "" {
		return "/"
	}
	cleaned := pathpkg.Clean(path)
	if !strings.HasPrefix(cleaned, "/") {
		return "/" + cleaned
	}
	return cleaned
}

func isOAuthOpaqueTokenChar(r rune) bool {
	return r >= 'a' && r <= 'z' ||
		r >= 'A' && r <= 'Z' ||
		r >= '0' && r <= '9' ||
		r == '-' ||
		r == '_' ||
		r == '.' ||
		r == '~'
}
