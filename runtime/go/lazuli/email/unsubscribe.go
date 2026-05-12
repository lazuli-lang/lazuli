package email

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"net/url"
	"strings"
	"time"
	"unicode"
)

const (
	unsubscribeTokenVersion = "u1"

	// UnsubscribePurpose is the default purpose string for unsubscribe tokens.
	UnsubscribePurpose = "unsubscribe"

	// HeaderListUnsubscribe is the standard header carrying unsubscribe URIs.
	HeaderListUnsubscribe = "List-Unsubscribe"
)

var (
	// ErrUnsubscribeTokenInvalid is returned for malformed unsubscribe tokens
	// or invalid token inputs.
	ErrUnsubscribeTokenInvalid = errors.New("email: unsubscribe token invalid")
	// ErrUnsubscribeTokenExpired is returned when an unsubscribe token is expired.
	ErrUnsubscribeTokenExpired = errors.New("email: unsubscribe token expired")
	// ErrUnsubscribeTokenSignature is returned when an unsubscribe token HMAC does not match.
	ErrUnsubscribeTokenSignature = errors.New("email: unsubscribe token signature mismatch")
	// ErrUnsubscribeTokenScope is returned when a token does not match the expected purpose or list.
	ErrUnsubscribeTokenScope = errors.New("email: unsubscribe token scope mismatch")
	// ErrListUnsubscribeHeaderInvalid is returned for invalid List-Unsubscribe header inputs.
	ErrListUnsubscribeHeaderInvalid = errors.New("email: invalid list-unsubscribe header")
)

// UnsubscribeClaims is the signed payload carried by an unsubscribe token.
type UnsubscribeClaims struct {
	// Subject is the application-defined subscriber, user, or recipient identifier.
	Subject string `json:"sub"`
	// ListID identifies the mailing list or notification list being scoped.
	ListID string `json:"list"`
	// Purpose scopes this token to a specific action, usually UnsubscribePurpose.
	Purpose string `json:"purpose"`
	// ExpiresAt is the token expiry as Unix seconds.
	ExpiresAt int64 `json:"exp"`
	// IssuedAt is the optional token creation time as Unix seconds.
	IssuedAt int64 `json:"iat,omitempty"`
}

// UnsubscribeScope is the expected verification context for an unsubscribe token.
type UnsubscribeScope struct {
	// Purpose must match the token purpose exactly.
	Purpose string
	// ListID must match the token list exactly.
	ListID string
	// Now is used for expiry checks. When zero, time.Now is used.
	Now time.Time
}

// ListUnsubscribeOptions configures a List-Unsubscribe header value.
type ListUnsubscribeOptions struct {
	// URL is an optional http or https unsubscribe endpoint.
	URL string
	// Mailto is an optional mailto unsubscribe URI.
	Mailto string
}

// SignUnsubscribeToken returns a compact HMAC-SHA256 signed unsubscribe token.
func SignUnsubscribeToken(secret []byte, claims UnsubscribeClaims) (string, error) {
	if len(secret) == 0 {
		return "", unsubscribeTokenInvalidf("secret is required")
	}
	if err := validateUnsubscribeClaims(claims); err != nil {
		return "", err
	}

	payloadJSON, err := json.Marshal(claims)
	if err != nil {
		return "", err
	}

	encoding := base64.RawURLEncoding
	payload := encoding.EncodeToString(payloadJSON)
	signingInput := unsubscribeTokenVersion + "." + payload
	signature := signUnsubscribeToken(secret, signingInput)
	return signingInput + "." + encoding.EncodeToString(signature), nil
}

// VerifyUnsubscribeToken validates a token HMAC, expiry, purpose, and list scope.
func VerifyUnsubscribeToken(secret []byte, token string, scope UnsubscribeScope) (UnsubscribeClaims, error) {
	if len(secret) == 0 {
		return UnsubscribeClaims{}, unsubscribeTokenInvalidf("secret is required")
	}
	if err := validateUnsubscribeScope(scope); err != nil {
		return UnsubscribeClaims{}, err
	}

	parts := strings.Split(token, ".")
	if len(parts) != 3 || parts[0] != unsubscribeTokenVersion || parts[1] == "" || parts[2] == "" {
		return UnsubscribeClaims{}, ErrUnsubscribeTokenInvalid
	}

	encoding := base64.RawURLEncoding
	payloadJSON, err := encoding.DecodeString(parts[1])
	if err != nil {
		return UnsubscribeClaims{}, ErrUnsubscribeTokenInvalid
	}
	signature, err := encoding.DecodeString(parts[2])
	if err != nil {
		return UnsubscribeClaims{}, ErrUnsubscribeTokenInvalid
	}

	signingInput := parts[0] + "." + parts[1]
	if !hmac.Equal(signature, signUnsubscribeToken(secret, signingInput)) {
		return UnsubscribeClaims{}, ErrUnsubscribeTokenSignature
	}

	var claims UnsubscribeClaims
	if err := json.Unmarshal(payloadJSON, &claims); err != nil {
		return UnsubscribeClaims{}, ErrUnsubscribeTokenInvalid
	}
	if err := validateUnsubscribeClaims(claims); err != nil {
		return UnsubscribeClaims{}, err
	}

	if claims.Purpose != scope.Purpose {
		return UnsubscribeClaims{}, fmt.Errorf("%w: purpose %q", ErrUnsubscribeTokenScope, claims.Purpose)
	}
	if claims.ListID != scope.ListID {
		return UnsubscribeClaims{}, fmt.Errorf("%w: list %q", ErrUnsubscribeTokenScope, claims.ListID)
	}

	now := scope.Now
	if now.IsZero() {
		now = time.Now()
	}
	if claims.ExpiresAt <= now.Unix() {
		return UnsubscribeClaims{}, ErrUnsubscribeTokenExpired
	}
	return claims, nil
}

// BuildListUnsubscribeHeader returns a List-Unsubscribe header value containing
// the configured unsubscribe URIs in angle brackets.
func BuildListUnsubscribeHeader(opts ListUnsubscribeOptions) (string, error) {
	var parts []string
	if opts.URL != "" {
		if err := validateListUnsubscribeURI("url", opts.URL, "http", "https"); err != nil {
			return "", err
		}
		parts = append(parts, "<"+opts.URL+">")
	}
	if opts.Mailto != "" {
		if err := validateListUnsubscribeURI("mailto", opts.Mailto, "mailto"); err != nil {
			return "", err
		}
		parts = append(parts, "<"+opts.Mailto+">")
	}
	if len(parts) == 0 {
		return "", listUnsubscribeInvalidf("url or mailto is required")
	}
	return strings.Join(parts, ", "), nil
}

func signUnsubscribeToken(secret []byte, signingInput string) []byte {
	mac := hmac.New(sha256.New, secret)
	_, _ = mac.Write([]byte(signingInput))
	return mac.Sum(nil)
}

func validateUnsubscribeClaims(claims UnsubscribeClaims) error {
	if err := validateUnsubscribeTokenField("subject", claims.Subject); err != nil {
		return err
	}
	if err := validateUnsubscribeTokenField("list", claims.ListID); err != nil {
		return err
	}
	if err := validateUnsubscribeTokenField("purpose", claims.Purpose); err != nil {
		return err
	}
	if claims.ExpiresAt <= 0 {
		return unsubscribeTokenInvalidf("exp is required")
	}
	return nil
}

func validateUnsubscribeScope(scope UnsubscribeScope) error {
	if err := validateUnsubscribeTokenField("scope purpose", scope.Purpose); err != nil {
		return err
	}
	if err := validateUnsubscribeTokenField("scope list", scope.ListID); err != nil {
		return err
	}
	return nil
}

func validateUnsubscribeTokenField(field, value string) error {
	if strings.TrimSpace(value) == "" {
		return unsubscribeTokenInvalidf("%s is required", field)
	}
	if strings.TrimSpace(value) != value {
		return unsubscribeTokenInvalidf("%s has surrounding whitespace", field)
	}
	if containsControl(value) {
		return unsubscribeTokenInvalidf("%s contains control characters", field)
	}
	return nil
}

func validateListUnsubscribeURI(field, value string, allowedSchemes ...string) error {
	if strings.TrimSpace(value) == "" {
		return listUnsubscribeInvalidf("%s is required", field)
	}
	if strings.TrimSpace(value) != value {
		return listUnsubscribeInvalidf("%s has surrounding whitespace", field)
	}
	if containsControl(value) || containsWhitespace(value) || strings.ContainsAny(value, "<>,") {
		return listUnsubscribeInvalidf("%s contains invalid header characters", field)
	}

	parsed, err := url.Parse(value)
	if err != nil || parsed.Scheme == "" {
		return listUnsubscribeInvalidf("%s must be an absolute URI", field)
	}
	scheme := strings.ToLower(parsed.Scheme)
	if !allowedScheme(scheme, allowedSchemes) {
		return listUnsubscribeInvalidf("%s has unsupported scheme %q", field, parsed.Scheme)
	}
	switch scheme {
	case "http", "https":
		if parsed.Host == "" {
			return listUnsubscribeInvalidf("%s requires a host", field)
		}
	case "mailto":
		if parsed.Opaque == "" && parsed.Path == "" {
			return listUnsubscribeInvalidf("%s requires a recipient", field)
		}
	}
	return nil
}

func allowedScheme(scheme string, allowed []string) bool {
	for _, candidate := range allowed {
		if scheme == candidate {
			return true
		}
	}
	return false
}

func containsWhitespace(s string) bool {
	for _, r := range s {
		if unicode.IsSpace(r) {
			return true
		}
	}
	return false
}

func unsubscribeTokenInvalidf(format string, args ...any) error {
	return fmt.Errorf("%w: "+format, append([]any{ErrUnsubscribeTokenInvalid}, args...)...)
}

func listUnsubscribeInvalidf(format string, args ...any) error {
	return fmt.Errorf("%w: "+format, append([]any{ErrListUnsubscribeHeaderInvalid}, args...)...)
}
