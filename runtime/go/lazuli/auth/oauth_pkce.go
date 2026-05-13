package auth

import (
	"crypto/sha256"
	"encoding/base64"
	"errors"
	"fmt"
	"net"
	"net/url"
	"slices"
	"strings"
)

const (
	OAuthPKCECodeChallengeMethodS256 = "S256"
	OAuthPKCEVerifierMinLength       = 43
	OAuthPKCEVerifierMaxLength       = 128
	OAuthPKCEVerifierMinEntropyBytes = 32
	OAuthPKCEVerifierMaxEntropyBytes = 96
)

var ErrOAuthPKCEInvalid = errors.New("auth: oauth pkce invalid")

// OAuthPKCEAuthorizationDescriptor is the provider-neutral input for planning
// an OAuth authorization redirect with PKCE. It performs no provider calls.
type OAuthPKCEAuthorizationDescriptor struct {
	AuthorizationURL string
	ClientID         string
	RedirectURL      string
	Scopes           []string
	State            string
	CodeVerifier     string
	ExtraParams      url.Values
}

// OAuthPKCEAuthorizationPlan is the normalized authorization redirect plan.
// CodeVerifier is retained so callers can persist it for the token exchange.
type OAuthPKCEAuthorizationPlan struct {
	AuthorizationURL     string
	Params               url.Values
	CodeVerifier         string
	CodeChallenge        string
	CodeChallengeMethod  string
	AuthorizationRequest string
}

// OAuthPKCEDiagnostics is safe to log or expose in diagnostics. Secret-bearing
// values are represented by presence only, and URL query values are removed.
type OAuthPKCEDiagnostics struct {
	AuthorizationURL       string
	RedirectURL            string
	ClientID               string
	Scopes                 []string
	StateConfigured        bool
	CodeVerifierConfigured bool
	CodeChallengeMethod    string
	CodeChallengeLength    int
	ExtraParamKeys         []string
}

// NormalizeOAuthPKCEVerifier trims surrounding whitespace from a code verifier.
func NormalizeOAuthPKCEVerifier(verifier string) string {
	return strings.TrimSpace(verifier)
}

// ValidateOAuthPKCEVerifier validates the RFC 7636 verifier shape.
func ValidateOAuthPKCEVerifier(verifier string) error {
	if verifier == "" {
		return fmt.Errorf("%w: code_verifier is required", ErrOAuthPKCEInvalid)
	}
	if len(verifier) < OAuthPKCEVerifierMinLength || len(verifier) > OAuthPKCEVerifierMaxLength {
		return fmt.Errorf("%w: code_verifier length must be between %d and %d", ErrOAuthPKCEInvalid, OAuthPKCEVerifierMinLength, OAuthPKCEVerifierMaxLength)
	}
	for _, r := range verifier {
		if !isOAuthPKCEUnreserved(r) {
			return fmt.Errorf("%w: code_verifier contains invalid character %q", ErrOAuthPKCEInvalid, r)
		}
	}
	return nil
}

// OAuthPKCEVerifierLengthForEntropy returns the raw-base64url verifier length
// produced by entropyBytes random bytes without generating the verifier.
func OAuthPKCEVerifierLengthForEntropy(entropyBytes int) (int, error) {
	if entropyBytes < OAuthPKCEVerifierMinEntropyBytes {
		return 0, fmt.Errorf("%w: code_verifier entropy must be at least %d bytes", ErrOAuthPKCEInvalid, OAuthPKCEVerifierMinEntropyBytes)
	}
	if entropyBytes > OAuthPKCEVerifierMaxEntropyBytes {
		return 0, fmt.Errorf("%w: code_verifier entropy must be at most %d bytes", ErrOAuthPKCEInvalid, OAuthPKCEVerifierMaxEntropyBytes)
	}
	length := base64.RawURLEncoding.EncodedLen(entropyBytes)
	if length < OAuthPKCEVerifierMinLength || length > OAuthPKCEVerifierMaxLength {
		return 0, fmt.Errorf("%w: code_verifier length must be between %d and %d", ErrOAuthPKCEInvalid, OAuthPKCEVerifierMinLength, OAuthPKCEVerifierMaxLength)
	}
	return length, nil
}

// DeriveOAuthPKCES256Challenge returns the base64url SHA-256 challenge for a
// validated code verifier.
func DeriveOAuthPKCES256Challenge(verifier string) (string, error) {
	verifier = NormalizeOAuthPKCEVerifier(verifier)
	if err := ValidateOAuthPKCEVerifier(verifier); err != nil {
		return "", err
	}
	sum := sha256.Sum256([]byte(verifier))
	return base64.RawURLEncoding.EncodeToString(sum[:]), nil
}

// ValidateOAuthPKCEChallenge validates a provider callback or stored challenge
// for the supported S256 challenge method.
func ValidateOAuthPKCEChallenge(challenge, method string) error {
	method = strings.TrimSpace(method)
	if method == "" {
		method = OAuthPKCECodeChallengeMethodS256
	}
	if method != OAuthPKCECodeChallengeMethodS256 {
		return fmt.Errorf("%w: code_challenge_method must be S256", ErrOAuthPKCEInvalid)
	}
	if challenge == "" {
		return fmt.Errorf("%w: code_challenge is required", ErrOAuthPKCEInvalid)
	}
	if len(challenge) != OAuthPKCEVerifierMinLength {
		return fmt.Errorf("%w: S256 code_challenge length must be %d", ErrOAuthPKCEInvalid, OAuthPKCEVerifierMinLength)
	}
	for _, r := range challenge {
		if !isOAuthPKCEUnreserved(r) {
			return fmt.Errorf("%w: code_challenge contains invalid character %q", ErrOAuthPKCEInvalid, r)
		}
	}
	return nil
}

// Normalize returns a trimmed descriptor with copied scopes and params.
func (d OAuthPKCEAuthorizationDescriptor) Normalize() (OAuthPKCEAuthorizationDescriptor, error) {
	return NormalizeOAuthPKCEAuthorizationDescriptor(d)
}

// Validate reports whether the descriptor can be planned safely.
func (d OAuthPKCEAuthorizationDescriptor) Validate() error {
	return ValidateOAuthPKCEAuthorizationDescriptor(d)
}

// NormalizeOAuthPKCEAuthorizationDescriptor trims scalar fields, canonicalizes
// URLs, deduplicates scopes, and copies ExtraParams.
func NormalizeOAuthPKCEAuthorizationDescriptor(d OAuthPKCEAuthorizationDescriptor) (OAuthPKCEAuthorizationDescriptor, error) {
	var errs []error
	out := OAuthPKCEAuthorizationDescriptor{
		ClientID:     strings.TrimSpace(d.ClientID),
		Scopes:       normalizeOAuthPKCEScopes(d.Scopes),
		State:        strings.TrimSpace(d.State),
		CodeVerifier: NormalizeOAuthPKCEVerifier(d.CodeVerifier),
		ExtraParams:  copyOAuthPKCEValues(d.ExtraParams),
	}

	if strings.TrimSpace(d.AuthorizationURL) != "" {
		authorizationURL, err := normalizeOAuthPKCEURL(d.AuthorizationURL, "authorization_url", false)
		if err != nil {
			errs = append(errs, err)
		} else {
			out.AuthorizationURL = authorizationURL
		}
	}
	if strings.TrimSpace(d.RedirectURL) != "" {
		redirectURL, err := normalizeOAuthPKCEURL(d.RedirectURL, "redirect_url", true)
		if err != nil {
			errs = append(errs, err)
		} else {
			out.RedirectURL = redirectURL
		}
	}

	if err := errors.Join(errs...); err != nil {
		return OAuthPKCEAuthorizationDescriptor{}, err
	}
	return out, nil
}

// ValidateOAuthPKCEAuthorizationDescriptor checks descriptor shape without
// performing any OAuth exchange or HTTP request.
func ValidateOAuthPKCEAuthorizationDescriptor(d OAuthPKCEAuthorizationDescriptor) error {
	normalized, err := NormalizeOAuthPKCEAuthorizationDescriptor(d)
	if err != nil {
		return err
	}

	var errs []error
	if normalized.AuthorizationURL == "" {
		errs = append(errs, fmt.Errorf("%w: authorization_url is required", ErrOAuthPKCEInvalid))
	}
	if normalized.ClientID == "" {
		errs = append(errs, fmt.Errorf("%w: client_id is required", ErrOAuthPKCEInvalid))
	}
	if normalized.RedirectURL == "" {
		errs = append(errs, fmt.Errorf("%w: redirect_url is required", ErrOAuthPKCEInvalid))
	}
	if normalized.State == "" {
		errs = append(errs, fmt.Errorf("%w: state is required", ErrOAuthPKCEInvalid))
	}
	if err := ValidateOAuthPKCEVerifier(normalized.CodeVerifier); err != nil {
		errs = append(errs, err)
	}
	for key := range normalized.ExtraParams {
		if isOAuthPKCEReservedParam(key) {
			errs = append(errs, fmt.Errorf("%w: extra param %q is reserved", ErrOAuthPKCEInvalid, key))
		}
	}
	return errors.Join(errs...)
}

// PlanOAuthPKCEAuthorization returns a deterministic authorization redirect
// plan with RFC 7636 S256 parameters.
func PlanOAuthPKCEAuthorization(d OAuthPKCEAuthorizationDescriptor) (OAuthPKCEAuthorizationPlan, error) {
	normalized, err := NormalizeOAuthPKCEAuthorizationDescriptor(d)
	if err != nil {
		return OAuthPKCEAuthorizationPlan{}, err
	}
	if err := ValidateOAuthPKCEAuthorizationDescriptor(normalized); err != nil {
		return OAuthPKCEAuthorizationPlan{}, err
	}
	challenge, err := DeriveOAuthPKCES256Challenge(normalized.CodeVerifier)
	if err != nil {
		return OAuthPKCEAuthorizationPlan{}, err
	}

	params := copyOAuthPKCEValues(normalized.ExtraParams)
	params.Set("response_type", "code")
	params.Set("client_id", normalized.ClientID)
	params.Set("redirect_uri", normalized.RedirectURL)
	if len(normalized.Scopes) > 0 {
		params.Set("scope", strings.Join(normalized.Scopes, " "))
	}
	params.Set("state", normalized.State)
	params.Set("code_challenge", challenge)
	params.Set("code_challenge_method", OAuthPKCECodeChallengeMethodS256)

	authorizationRequest, err := appendOAuthPKCEQuery(normalized.AuthorizationURL, params)
	if err != nil {
		return OAuthPKCEAuthorizationPlan{}, err
	}

	return OAuthPKCEAuthorizationPlan{
		AuthorizationURL:     normalized.AuthorizationURL,
		Params:               params,
		CodeVerifier:         normalized.CodeVerifier,
		CodeChallenge:        challenge,
		CodeChallengeMethod:  OAuthPKCECodeChallengeMethodS256,
		AuthorizationRequest: authorizationRequest,
	}, nil
}

// SafeDiagnostics returns redaction-safe plan metadata.
func (p OAuthPKCEAuthorizationPlan) SafeDiagnostics() OAuthPKCEDiagnostics {
	return SafeOAuthPKCEDiagnostics(p)
}

// SafeOAuthPKCEDiagnostics returns a summary that does not leak verifier,
// state, code challenge, or URL query values.
func SafeOAuthPKCEDiagnostics(p OAuthPKCEAuthorizationPlan) OAuthPKCEDiagnostics {
	return OAuthPKCEDiagnostics{
		AuthorizationURL:       redactOAuthPKCEURL(p.AuthorizationURL),
		RedirectURL:            redactOAuthPKCEURL(p.Params.Get("redirect_uri")),
		ClientID:               p.Params.Get("client_id"),
		Scopes:                 normalizeOAuthPKCEScopes(strings.Fields(p.Params.Get("scope"))),
		StateConfigured:        strings.TrimSpace(p.Params.Get("state")) != "",
		CodeVerifierConfigured: strings.TrimSpace(p.CodeVerifier) != "",
		CodeChallengeMethod:    p.CodeChallengeMethod,
		CodeChallengeLength:    len(p.CodeChallenge),
		ExtraParamKeys:         oauthPKCEExtraParamKeys(p.Params),
	}
}

func isOAuthPKCEUnreserved(r rune) bool {
	return (r >= 'A' && r <= 'Z') ||
		(r >= 'a' && r <= 'z') ||
		(r >= '0' && r <= '9') ||
		r == '-' || r == '.' || r == '_' || r == '~'
}

func normalizeOAuthPKCEScopes(scopes []string) []string {
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

func copyOAuthPKCEValues(values url.Values) url.Values {
	if len(values) == 0 {
		return nil
	}
	out := make(url.Values, len(values))
	for key, in := range values {
		key = strings.TrimSpace(key)
		if key == "" {
			continue
		}
		for _, value := range in {
			value = strings.TrimSpace(value)
			if value != "" {
				out.Add(key, value)
			}
		}
	}
	if len(out) == 0 {
		return nil
	}
	return out
}

func normalizeOAuthPKCEURL(raw, field string, allowLoopbackHTTP bool) (string, error) {
	u, err := parseOAuthPKCEURL(raw, field)
	if err != nil {
		return "", err
	}
	if u.Scheme == "http" && !(allowLoopbackHTTP && isOAuthPKCELoopbackHost(u.Hostname())) {
		return "", fmt.Errorf("%w: %s http is only allowed for loopback hosts", ErrOAuthPKCEInvalid, field)
	}
	if u.Scheme != "https" && u.Scheme != "http" {
		return "", fmt.Errorf("%w: %s scheme must be https", ErrOAuthPKCEInvalid, field)
	}
	if !allowLoopbackHTTP && u.Scheme != "https" {
		return "", fmt.Errorf("%w: %s scheme must be https", ErrOAuthPKCEInvalid, field)
	}
	if u.Fragment != "" {
		return "", fmt.Errorf("%w: %s must not include a fragment", ErrOAuthPKCEInvalid, field)
	}
	return u.String(), nil
}

func parseOAuthPKCEURL(raw, field string) (*url.URL, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil, fmt.Errorf("%w: %s is required", ErrOAuthPKCEInvalid, field)
	}
	u, err := url.Parse(raw)
	if err != nil {
		return nil, fmt.Errorf("%w: %s: %v", ErrOAuthPKCEInvalid, field, err)
	}
	u.Scheme = strings.ToLower(u.Scheme)
	u.Host = strings.ToLower(u.Host)
	if u.Scheme == "" || u.Host == "" {
		return nil, fmt.Errorf("%w: %s must be an absolute URL", ErrOAuthPKCEInvalid, field)
	}
	if u.User != nil {
		return nil, fmt.Errorf("%w: %s must not include userinfo", ErrOAuthPKCEInvalid, field)
	}
	u.Host = stripOAuthPKCEDefaultPort(u)
	if u.Path == "" {
		u.Path = "/"
	}
	return u, nil
}

func stripOAuthPKCEDefaultPort(u *url.URL) string {
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

func isOAuthPKCELoopbackHost(host string) bool {
	switch strings.ToLower(host) {
	case "localhost":
		return true
	}
	ip := net.ParseIP(host)
	return ip != nil && ip.IsLoopback()
}

func isOAuthPKCEReservedParam(key string) bool {
	switch strings.ToLower(strings.TrimSpace(key)) {
	case "response_type", "client_id", "redirect_uri", "scope", "state", "code_challenge", "code_challenge_method":
		return true
	default:
		return false
	}
}

func appendOAuthPKCEQuery(raw string, params url.Values) (string, error) {
	u, err := url.Parse(raw)
	if err != nil {
		return "", err
	}
	query := u.Query()
	for key, values := range params {
		query.Del(key)
		for _, value := range values {
			query.Add(key, value)
		}
	}
	u.RawQuery = query.Encode()
	return u.String(), nil
}

func redactOAuthPKCEURL(raw string) string {
	u, err := url.Parse(strings.TrimSpace(raw))
	if err != nil {
		return ""
	}
	u.RawQuery = ""
	u.Fragment = ""
	u.User = nil
	return u.String()
}

func oauthPKCEExtraParamKeys(values url.Values) []string {
	if len(values) == 0 {
		return nil
	}
	keys := make([]string, 0, len(values))
	for key := range values {
		if !isOAuthPKCEReservedParam(key) {
			keys = append(keys, key)
		}
	}
	slices.Sort(keys)
	return keys
}
