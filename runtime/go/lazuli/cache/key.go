package cache

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"strings"
)

const (
	keySeparator     = "|"
	unscopedTenantID = "-"
)

var errMissingQuery = errors.New("lazuli/cache: cache key query is required")

// KeyParts describes the canonical inputs for one query cache lookup.
//
// Spec can supply Namespace from the lowered QuerySpec. Query should be the
// generated fully-qualified query name, for example "customer.query.list".
// Namespace overrides Spec.Namespace when set. Tenant is required for
// tenant-scoped resources; when empty, BuildKey uses "-" to keep unscoped
// queries isolated from an empty key segment.
type KeyParts struct {
	Spec      QuerySpec
	Namespace string
	Query     string
	Tenant    string
	Args      any
}

// BuildKey returns the canonical cache key for a query invocation.
//
// Keys are shaped as "<query>|<tenant>|<args hash>" and include a normalized
// namespace segment after the query when Namespace is set:
// "<query>|<namespace>|<tenant>|<args hash>". Keeping the query first
// preserves prefix invalidation by query name while still separating
// namespace aliases, tenants, and argument sets.
func BuildKey(parts KeyParts) (string, error) {
	query := strings.TrimSpace(parts.Query)
	if query == "" {
		return "", errMissingQuery
	}

	argsHash, err := HashArgs(parts.Args)
	if err != nil {
		return "", err
	}

	tenant := strings.TrimSpace(parts.Tenant)
	if tenant == "" {
		tenant = unscopedTenantID
	}

	namespace := parts.Namespace
	if namespace == "" {
		namespace = parts.Spec.Namespace
	}
	namespace = NormalizeNamespace(namespace)
	if namespace == "" {
		return strings.Join([]string{query, tenant, argsHash}, keySeparator), nil
	}
	return strings.Join([]string{query, namespace, tenant, argsHash}, keySeparator), nil
}

// HashArgs returns the SHA-256 digest of args encoded with encoding/json.
//
// The standard JSON encoder gives deterministic output for the plain map,
// slice, and struct values generated for Lazuli query arguments, including
// sorted map keys.
func HashArgs(args any) (string, error) {
	buf, err := json.Marshal(args)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(buf)
	return hex.EncodeToString(sum[:]), nil
}

// NormalizeNamespace returns a safe, lowercase namespace segment.
//
// Lazuli-authored namespace labels are already lowercase identifiers. This
// helper trims defensive input, lowercases ASCII letters, preserves common
// identifier separators, and rewrites all other runes to "-". Empty input
// returns an empty namespace so callers can keep the legacy key shape.
func NormalizeNamespace(namespace string) string {
	namespace = strings.TrimSpace(strings.ToLower(namespace))
	if namespace == "" {
		return ""
	}

	var b strings.Builder
	b.Grow(len(namespace))
	lastDash := false
	for _, r := range namespace {
		switch {
		case r >= 'a' && r <= 'z',
			r >= '0' && r <= '9',
			r == '_',
			r == '.':
			b.WriteRune(r)
			lastDash = false
		case r == '-':
			if !lastDash {
				b.WriteRune(r)
				lastDash = true
			}
		default:
			if !lastDash {
				b.WriteRune('-')
				lastDash = true
			}
		}
	}

	return strings.Trim(b.String(), "-")
}
