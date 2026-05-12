package auth

import (
	"errors"
	"strings"
	"time"
	"unicode"

	"lazuli.dev/runtime/lazuli"
)

// APITokenStatus is the derived lifecycle state for an API token.
type APITokenStatus string

const (
	APITokenStatusActive  APITokenStatus = "active"
	APITokenStatusExpired APITokenStatus = "expired"
	APITokenStatusRotated APITokenStatus = "rotated"
	APITokenStatusRevoked APITokenStatus = "revoked"
)

var (
	ErrAPITokenExpired     = errors.New("auth: api token expired")
	ErrAPITokenRevoked     = errors.New("auth: api token revoked")
	ErrAPITokenRotated     = errors.New("auth: api token rotated")
	ErrAPITokenScopeDenied = errors.New("auth: api token scope denied")
)

// APITokenScopes is the grant set attached to an API token.
//
// Scopes are opaque strings to the runtime. Exact grants match exactly, and
// grants containing '*' match with shell-style wildcard semantics:
//
//	"customers:*" grants "customers:read" and "customers:write"
//	"*" grants every non-empty requested scope
type APITokenScopes []string

// APITokenMetadata carries the non-secret API token state generated code and
// adapters need to validate a bearer token after lookup by hash.
type APITokenMetadata struct {
	ID                 string
	Name               string
	UserID             lazuli.ID
	Subject            string
	Scopes             APITokenScopes
	CreatedAt          time.Time
	ExpiresAt          time.Time
	LastUsedAt         time.Time
	RotatedAt          time.Time
	RevokedAt          time.Time
	ReplacementTokenID string
	Attrs              map[string]any
}

// Clone returns metadata with cloned scope and attr containers.
func (m APITokenMetadata) Clone() APITokenMetadata {
	m.Scopes = append(APITokenScopes(nil), m.Scopes...)
	m.Attrs = cloneAPITokenAttrs(m.Attrs)
	return m
}

// Expired reports whether the token is past ExpiresAt. A zero ExpiresAt means
// no expiry is enforced by this helper.
func (m APITokenMetadata) Expired(now time.Time) bool {
	return !m.ExpiresAt.IsZero() && !m.ExpiresAt.After(now)
}

// IsExpired is an alias for Expired.
func (m APITokenMetadata) IsExpired(now time.Time) bool {
	return m.Expired(now)
}

// Revoked reports whether the token has been explicitly revoked.
func (m APITokenMetadata) Revoked() bool {
	return !m.RevokedAt.IsZero()
}

// IsRevoked is an alias for Revoked.
func (m APITokenMetadata) IsRevoked() bool {
	return m.Revoked()
}

// Rotated reports whether the token has been superseded by rotation.
func (m APITokenMetadata) Rotated() bool {
	return !m.RotatedAt.IsZero() || m.ReplacementTokenID != ""
}

// IsRotated is an alias for Rotated.
func (m APITokenMetadata) IsRotated() bool {
	return m.Rotated()
}

// Status returns the token lifecycle status at now. Revocation wins over
// rotation, and rotation wins over expiry.
func (m APITokenMetadata) Status(now time.Time) APITokenStatus {
	switch {
	case m.Revoked():
		return APITokenStatusRevoked
	case m.Rotated():
		return APITokenStatusRotated
	case m.Expired(now):
		return APITokenStatusExpired
	default:
		return APITokenStatusActive
	}
}

// HasScope reports whether the token grants required.
func (m APITokenMetadata) HasScope(required string) bool {
	return m.Scopes.Has(required)
}

// AllowsScope is an alias for HasScope.
func (m APITokenMetadata) AllowsScope(required string) bool {
	return m.HasScope(required)
}

// Validate returns nil when the token is active and grants every required
// scope. Empty or whitespace-only required scopes are invalid.
func (m APITokenMetadata) Validate(now time.Time, requiredScopes ...string) error {
	return ValidateAPIToken(m, now, requiredScopes...)
}

// Has reports whether scopes grant required.
func (scopes APITokenScopes) Has(required string) bool {
	return APITokenHasScope(scopes, required)
}

// Allows is an alias for Has.
func (scopes APITokenScopes) Allows(required string) bool {
	return scopes.Has(required)
}

// HasAll reports whether scopes grant every required scope.
func (scopes APITokenScopes) HasAll(required ...string) bool {
	for _, scope := range required {
		if !scopes.Has(scope) {
			return false
		}
	}
	return true
}

// HasAny reports whether scopes grant at least one required scope.
func (scopes APITokenScopes) HasAny(required ...string) bool {
	for _, scope := range required {
		if scopes.Has(scope) {
			return true
		}
	}
	return false
}

// NormalizeAPITokenScopes trims scopes, drops empty entries, and removes
// duplicates while preserving the first occurrence order.
func NormalizeAPITokenScopes(scopes []string) APITokenScopes {
	normalized := make(APITokenScopes, 0, len(scopes))
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
	return normalized
}

// ValidateAPITokenScope validates the runtime scope atom shape. The runtime
// intentionally keeps scope names opaque, but rejects empty scopes and
// whitespace because those are ambiguous in headers, DSL lists, and logs.
func ValidateAPITokenScope(scope string) error {
	scope = strings.TrimSpace(scope)
	if scope == "" {
		return ErrTokenInvalid
	}
	for _, r := range scope {
		if unicode.IsSpace(r) {
			return ErrTokenInvalid
		}
	}
	return nil
}

// ValidateAPITokenScopes validates a scope grant list.
func ValidateAPITokenScopes(scopes []string) error {
	for _, scope := range scopes {
		if err := ValidateAPITokenScope(scope); err != nil {
			return err
		}
	}
	return nil
}

// MatchAPITokenScope reports whether a granted scope pattern covers a required
// concrete scope. Wildcards are honored only on the grant side unless both
// strings are exactly equal.
func MatchAPITokenScope(granted, required string) bool {
	granted = strings.TrimSpace(granted)
	required = strings.TrimSpace(required)
	if granted == required && granted != "" {
		return true
	}
	if err := ValidateAPITokenScope(granted); err != nil {
		return false
	}
	if err := ValidateAPITokenScope(required); err != nil {
		return false
	}
	if strings.Contains(required, "*") {
		return false
	}
	return matchAPITokenScopePattern(granted, required)
}

// APITokenHasScope reports whether grants contain a scope that covers required.
func APITokenHasScope(grants []string, required string) bool {
	for _, grant := range grants {
		if MatchAPITokenScope(grant, required) {
			return true
		}
	}
	return false
}

// ValidateAPIToken returns nil when metadata describes an active token that
// grants every required scope.
func ValidateAPIToken(meta APITokenMetadata, now time.Time, requiredScopes ...string) error {
	switch meta.Status(now) {
	case APITokenStatusRevoked:
		return ErrAPITokenRevoked
	case APITokenStatusRotated:
		return ErrAPITokenRotated
	case APITokenStatusExpired:
		return ErrAPITokenExpired
	}

	for _, required := range requiredScopes {
		if err := ValidateAPITokenScope(required); err != nil {
			return err
		}
		if !meta.HasScope(required) {
			return ErrAPITokenScopeDenied
		}
	}
	return nil
}

func matchAPITokenScopePattern(pattern, value string) bool {
	if pattern == "*" {
		return true
	}
	if !strings.Contains(pattern, "*") {
		return pattern == value
	}

	parts := strings.Split(pattern, "*")
	pos := 0
	if parts[0] != "" {
		if !strings.HasPrefix(value, parts[0]) {
			return false
		}
		pos = len(parts[0])
	}

	for i := 1; i < len(parts); i++ {
		part := parts[i]
		if part == "" {
			continue
		}
		found := strings.Index(value[pos:], part)
		if found < 0 {
			return false
		}
		pos += found + len(part)
	}

	last := parts[len(parts)-1]
	return last == "" || strings.HasSuffix(value, last)
}

func cloneAPITokenAttrs(attrs map[string]any) map[string]any {
	if len(attrs) == 0 {
		return map[string]any{}
	}

	cloned := make(map[string]any, len(attrs))
	for key, value := range attrs {
		cloned[key] = cloneAPITokenAttrValue(value)
	}
	return cloned
}

func cloneAPITokenAttrValue(value any) any {
	switch v := value.(type) {
	case []byte:
		return append([]byte(nil), v...)
	case []string:
		return append([]string(nil), v...)
	case []any:
		cloned := make([]any, len(v))
		for i, item := range v {
			cloned[i] = cloneAPITokenAttrValue(item)
		}
		return cloned
	case map[string]any:
		return cloneAPITokenAttrs(v)
	default:
		return v
	}
}
