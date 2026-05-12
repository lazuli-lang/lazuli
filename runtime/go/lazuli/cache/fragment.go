package cache

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"time"
)

const (
	fragmentKeyPrefix = "fragment"
	fragmentEmptyTags = "-"
)

var (
	// ErrFragmentKeyRequired reports that a fragment cache key/name is missing.
	ErrFragmentKeyRequired = errors.New("lazuli/cache: fragment key is required")
	// ErrFragmentVersionRequired reports that a fragment cache version is missing.
	ErrFragmentVersionRequired = errors.New("lazuli/cache: fragment version is required")
	// ErrFragmentTagInvalid reports that a fragment cache tag cannot be normalized.
	ErrFragmentTagInvalid = errors.New("lazuli/cache: fragment tag is invalid")
	// ErrFragmentStaleWhileRevalidateInvalid reports an invalid stale window.
	ErrFragmentStaleWhileRevalidateInvalid = errors.New("lazuli/cache: fragment stale-while-revalidate is invalid")
)

// FragmentSpec describes adapter-neutral cache behavior for a rendered fragment.
//
// Key is the authored fragment identity, Version is an explicit content/schema
// revision, Tags are invalidation labels, and Namespace scopes the fragment in
// multi-app or pack deployments. Callers should pass the effective TTL to
// NewFragmentMetadata when using stale-while-revalidate metadata.
type FragmentSpec struct {
	Key                  string
	Namespace            string
	Version              string
	TTL                  time.Duration
	StaleWhileRevalidate time.Duration
	Tags                 []string
}

// FragmentKeyParts describes the inputs for a fragment cache key.
//
// Key, Namespace, Version, and Tags override Spec when set. Vary carries the
// template inputs that distinguish one rendered fragment instance from another.
type FragmentKeyParts struct {
	Spec      FragmentSpec
	Key       string
	Namespace string
	Version   string
	Tags      []string
	Vary      any
}

// BuildFragmentKey returns the canonical cache key for a fragment instance.
//
// Keys are shaped as:
//
//	fragment|<namespace>|<key>|<version>|<sorted tags>|<vary hash>
//
// Empty namespaces use "-" to keep the segment explicit. Tags are normalized,
// deduplicated, and sorted so equivalent tag sets build the same key.
func BuildFragmentKey(parts FragmentKeyParts) (string, error) {
	key := strings.TrimSpace(parts.Key)
	if key == "" {
		key = parts.Spec.Key
	}
	key = NormalizeNamespace(key)
	if key == "" {
		return "", ErrFragmentKeyRequired
	}

	namespace := parts.Namespace
	if namespace == "" {
		namespace = parts.Spec.Namespace
	}
	namespace = NormalizeNamespace(namespace)
	if namespace == "" {
		namespace = unscopedTenantID
	}

	version := strings.TrimSpace(parts.Version)
	if version == "" {
		version = parts.Spec.Version
	}
	version = NormalizeNamespace(version)
	if version == "" {
		return "", ErrFragmentVersionRequired
	}

	tags := parts.Tags
	if tags == nil {
		tags = parts.Spec.Tags
	}
	normalizedTags, err := NormalizeFragmentTags(tags)
	if err != nil {
		return "", err
	}
	tagSegment := fragmentEmptyTags
	if len(normalizedTags) > 0 {
		tagSegment = strings.Join(normalizedTags, ",")
	}

	varyHash, err := HashArgs(parts.Vary)
	if err != nil {
		return "", err
	}

	return strings.Join([]string{
		fragmentKeyPrefix,
		namespace,
		key,
		version,
		tagSegment,
		varyHash,
	}, keySeparator), nil
}

// ValidateFragmentSpec validates the adapter-neutral fragment cache contract.
func ValidateFragmentSpec(spec FragmentSpec) error {
	var errs []error
	if NormalizeNamespace(spec.Key) == "" {
		errs = append(errs, ErrFragmentKeyRequired)
	}
	if NormalizeNamespace(spec.Version) == "" {
		errs = append(errs, ErrFragmentVersionRequired)
	}
	if _, err := NormalizeFragmentTags(spec.Tags); err != nil {
		errs = append(errs, err)
	}
	if spec.StaleWhileRevalidate < 0 {
		errs = append(errs, ErrFragmentStaleWhileRevalidateInvalid)
	}
	if spec.TTL < 0 && spec.StaleWhileRevalidate > 0 {
		errs = append(errs, fmt.Errorf("%w: stale window requires expiring ttl", ErrFragmentStaleWhileRevalidateInvalid))
	}
	return errors.Join(errs...)
}

// NormalizeFragmentTags trims, normalizes, deduplicates, and sorts tag labels.
func NormalizeFragmentTags(tags []string) ([]string, error) {
	if len(tags) == 0 {
		return nil, nil
	}

	seen := make(map[string]struct{}, len(tags))
	normalized := make([]string, 0, len(tags))
	for _, tag := range tags {
		label := NormalizeNamespace(tag)
		if label == "" {
			return nil, fmt.Errorf("%w: %q", ErrFragmentTagInvalid, tag)
		}
		if _, ok := seen[label]; ok {
			continue
		}
		seen[label] = struct{}{}
		normalized = append(normalized, label)
	}
	sort.Strings(normalized)
	return normalized, nil
}

// FragmentMetadata records freshness windows for stale-while-revalidate.
type FragmentMetadata struct {
	StoredAt   time.Time
	FreshUntil time.Time
	StaleUntil time.Time
}

// FragmentState describes whether a fragment may be served at a point in time.
type FragmentState int

const (
	// FragmentFresh means the fragment is within its TTL.
	FragmentFresh FragmentState = iota
	// FragmentStale means the fragment is past TTL but inside its stale window.
	FragmentStale
	// FragmentExpired means the fragment should not be served.
	FragmentExpired
)

// NewFragmentMetadata returns freshness metadata for an entry stored at now.
//
// Negative TTL means the entry does not expire. Negative stale windows are
// treated as zero; ValidateFragmentSpec rejects them for authored specs.
func NewFragmentMetadata(now time.Time, ttl, staleWhileRevalidate time.Duration) FragmentMetadata {
	if ttl < 0 {
		return FragmentMetadata{StoredAt: now}
	}
	if staleWhileRevalidate < 0 {
		staleWhileRevalidate = 0
	}

	freshUntil := now.Add(ttl)
	return FragmentMetadata{
		StoredAt:   now,
		FreshUntil: freshUntil,
		StaleUntil: freshUntil.Add(staleWhileRevalidate),
	}
}

// State reports the fragment freshness state at now.
func (m FragmentMetadata) State(now time.Time) FragmentState {
	if m.FreshUntil.IsZero() || now.Before(m.FreshUntil) {
		return FragmentFresh
	}
	if m.StaleUntil.After(m.FreshUntil) && now.Before(m.StaleUntil) {
		return FragmentStale
	}
	return FragmentExpired
}

// CanServe reports whether a fragment can be returned without blocking.
func (m FragmentMetadata) CanServe(now time.Time) bool {
	return m.State(now) != FragmentExpired
}

// ShouldRevalidate reports whether callers should refresh the fragment.
func (m FragmentMetadata) ShouldRevalidate(now time.Time) bool {
	return m.State(now) == FragmentStale
}

// FragmentIndexEntry is the adapter-neutral fragment index shape used by the
// tag invalidation planner.
type FragmentIndexEntry struct {
	Key  string
	Tags []string
}

// FragmentInvalidationPlan lists fragment cache keys and tag labels to delete.
type FragmentInvalidationPlan struct {
	Keys []string
	Tags []string
}

// PlanFragmentTagInvalidation selects fragment keys whose tags intersect labels.
func PlanFragmentTagInvalidation(entries []FragmentIndexEntry, labels []string) (FragmentInvalidationPlan, error) {
	normalizedLabels, err := NormalizeFragmentTags(labels)
	if err != nil {
		return FragmentInvalidationPlan{}, err
	}
	if len(normalizedLabels) == 0 || len(entries) == 0 {
		return FragmentInvalidationPlan{Tags: normalizedLabels}, nil
	}

	var keys []string
	seenKeys := make(map[string]struct{}, len(entries))
	for _, entry := range entries {
		key := strings.TrimSpace(entry.Key)
		if key == "" {
			continue
		}
		entryTags, err := NormalizeFragmentTags(entry.Tags)
		if err != nil {
			return FragmentInvalidationPlan{}, err
		}
		if !IntersectTags(entryTags, normalizedLabels) {
			continue
		}
		if _, ok := seenKeys[key]; ok {
			continue
		}
		seenKeys[key] = struct{}{}
		keys = append(keys, key)
	}

	return FragmentInvalidationPlan{Keys: keys, Tags: normalizedLabels}, nil
}
