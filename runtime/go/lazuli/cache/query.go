package cache

import (
	"errors"
	"fmt"
	"strings"
	"time"
)

var (
	// ErrInvalidQueryCacheConfig reports an invalid generated query-cache
	// configuration before it reaches a concrete adapter.
	ErrInvalidQueryCacheConfig = errors.New("lazuli/cache: invalid query cache config")
	// ErrInvalidInvalidationTarget reports an invalid generated invalidation
	// target before it reaches a concrete adapter.
	ErrInvalidInvalidationTarget = errors.New("lazuli/cache: invalid invalidation target")
)

// QueryKeyParts describes the canonical generated-repository query cache key.
//
// Params is the generated query parameter value. Page is the normalized
// pagination value for list queries. Both values are encoded together and
// hashed, so different pages of the same query never share a cache slot.
type QueryKeyParts struct {
	Spec      QuerySpec
	Namespace string
	Query     string
	Tenant    string
	Params    any
	Page      any
}

// BuildQueryKey returns the canonical cache key for a generated query call.
func BuildQueryKey(parts QueryKeyParts) (string, error) {
	return BuildKey(KeyParts{
		Spec:      parts.Spec,
		Namespace: parts.Namespace,
		Query:     parts.Query,
		Tenant:    parts.Tenant,
		Args: queryKeyArgs{
			Params: parts.Params,
			Page:   parts.Page,
		},
	})
}

type queryKeyArgs struct {
	Params any `json:"params,omitempty"`
	Page   any `json:"page,omitempty"`
}

// QueryCacheConfig is the adapter-neutral cache behavior generated next to a
// query repository.
type QueryCacheConfig struct {
	// TTL is the fixed lifetime for cached entries. Zero leaves the concrete
	// backend default in charge.
	TTL time.Duration
	// SlidingTTL enables access-renewal when positive. It must be bounded by a
	// positive fixed TTL so adapters can keep a deterministic upper window.
	SlidingTTL time.Duration
	// NegativeCache allows generated repositories to cache not-found/empty
	// results separately from successful positive values.
	NegativeCache bool
}

// Validate reports invalid TTL and sliding-TTL combinations.
func (c QueryCacheConfig) Validate() error {
	return ValidateQueryCacheConfig(c)
}

// ValidateQueryCacheConfig reports invalid TTL and sliding-TTL combinations.
func ValidateQueryCacheConfig(config QueryCacheConfig) error {
	var errs []error
	if config.TTL < 0 {
		errs = append(errs, fmt.Errorf("%w: TTL must not be negative", ErrInvalidQueryCacheConfig))
	}
	if config.SlidingTTL < 0 {
		errs = append(errs, fmt.Errorf("%w: SlidingTTL must not be negative", ErrInvalidQueryCacheConfig))
	}
	if config.SlidingTTL > 0 && config.TTL <= 0 {
		errs = append(errs, fmt.Errorf("%w: SlidingTTL requires a positive TTL", ErrInvalidQueryCacheConfig))
	}
	if config.TTL > 0 && config.SlidingTTL > config.TTL {
		errs = append(errs, fmt.Errorf("%w: SlidingTTL must not exceed TTL", ErrInvalidQueryCacheConfig))
	}
	return errors.Join(errs...)
}

// InvalidationTokenKind classifies adapter-neutral invalidation tokens.
type InvalidationTokenKind string

const (
	// InvalidationQuery targets one fully qualified query name.
	InvalidationQuery InvalidationTokenKind = "query"
	// InvalidationQueryWildcard targets every query in one feature.
	InvalidationQueryWildcard InvalidationTokenKind = "query_wildcard"
	// InvalidationTag targets every cached query carrying a tag.
	InvalidationTag InvalidationTokenKind = "tag"
)

// InvalidationToken is the generated, adapter-neutral invalidation shape.
type InvalidationToken struct {
	Kind  InvalidationTokenKind
	Value string
	Args  string
}

// String returns a compact token label for logs and tests.
func (t InvalidationToken) String() string {
	if t.Value == "" {
		return string(t.Kind) + ":"
	}
	return string(t.Kind) + ":" + t.Value
}

// BuildInvalidationTokens lowers generated invalidation targets to typed
// adapter-neutral tokens. defaultFeature resolves same-feature query targets.
func BuildInvalidationTokens(defaultFeature string, targets ...InvalidationTarget) ([]InvalidationToken, error) {
	tokens := make([]InvalidationToken, 0, len(targets))
	for _, target := range targets {
		token, err := BuildInvalidationToken(defaultFeature, target)
		if err != nil {
			return nil, err
		}
		tokens = append(tokens, token)
	}
	return tokens, nil
}

// BuildInvalidationToken lowers one generated invalidation target to a typed
// adapter-neutral token.
func BuildInvalidationToken(defaultFeature string, target InvalidationTarget) (InvalidationToken, error) {
	if target == nil {
		return InvalidationToken{}, fmt.Errorf("%w: target is nil", ErrInvalidInvalidationTarget)
	}

	switch t := target.(type) {
	case QueryTarget:
		value, err := invalidationQueryValue(defaultFeature, t.Feature, t.Name)
		if err != nil {
			return InvalidationToken{}, err
		}
		return InvalidationToken{
			Kind:  InvalidationQuery,
			Value: value,
			Args:  strings.TrimSpace(t.Args),
		}, nil
	case QueryWildcardTarget:
		feature := strings.TrimSpace(t.Feature)
		if feature == "" {
			feature = strings.TrimSpace(defaultFeature)
		}
		if feature == "" {
			return InvalidationToken{}, fmt.Errorf("%w: query wildcard feature is required", ErrInvalidInvalidationTarget)
		}
		return InvalidationToken{
			Kind:  InvalidationQueryWildcard,
			Value: feature + ".query.*",
		}, nil
	case TagTarget:
		label := strings.TrimSpace(t.Label)
		if label == "" {
			return InvalidationToken{}, fmt.Errorf("%w: tag label is required", ErrInvalidInvalidationTarget)
		}
		return InvalidationToken{
			Kind:  InvalidationTag,
			Value: label,
		}, nil
	default:
		return InvalidationToken{}, fmt.Errorf("%w: %T", ErrInvalidInvalidationTarget, target)
	}
}

func invalidationQueryValue(defaultFeature, feature, name string) (string, error) {
	query := strings.TrimSpace(name)
	if query == "" {
		return "", fmt.Errorf("%w: query name is required", ErrInvalidInvalidationTarget)
	}
	if strings.Contains(query, ".query.") {
		featurePart, namePart, _ := strings.Cut(query, ".query.")
		if featurePart == "" || namePart == "" {
			return "", fmt.Errorf("%w: query name is required", ErrInvalidInvalidationTarget)
		}
		return query, nil
	}
	query = strings.TrimPrefix(query, "query.")
	if query == "" {
		return "", fmt.Errorf("%w: query name is required", ErrInvalidInvalidationTarget)
	}

	feature = strings.TrimSpace(feature)
	if feature == "" {
		feature = strings.TrimSpace(defaultFeature)
	}
	if feature == "" {
		return "", fmt.Errorf("%w: query feature is required", ErrInvalidInvalidationTarget)
	}
	return feature + ".query." + query, nil
}
