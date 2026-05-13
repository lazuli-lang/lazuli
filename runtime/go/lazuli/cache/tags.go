package cache

import (
	"errors"
	"fmt"
	"sort"
	"strings"
)

var (
	// ErrCacheTagInvalid reports that a cache tag cannot be normalized.
	ErrCacheTagInvalid = errors.New("lazuli/cache: cache tag is invalid")
	// ErrCacheTagKeyRequired reports that a tagged cache entry is missing its
	// cache key.
	ErrCacheTagKeyRequired = errors.New("lazuli/cache: tagged cache key is required")
)

// TagIndexEntry is the adapter-neutral index shape used by cache tag planners.
type TagIndexEntry struct {
	Key  string
	Tags []string
}

// CacheTagSet tracks the current tag membership for cache keys.
//
// The zero value is ready to use. It is not synchronized; concrete backends
// should guard it with their own locks when using it as mutable state.
type CacheTagSet struct {
	byKey map[string][]string
	byTag map[string]map[string]struct{}
}

// NewCacheTagSet returns a tag set populated with entries.
func NewCacheTagSet(entries ...TagIndexEntry) (CacheTagSet, error) {
	var set CacheTagSet
	for _, entry := range entries {
		if err := set.Add(entry.Key, entry.Tags); err != nil {
			return CacheTagSet{}, err
		}
	}
	return set, nil
}

// Add records key as a member of tags, replacing any previous membership for
// the same key.
func (s *CacheTagSet) Add(key string, tags []string) error {
	key = strings.TrimSpace(key)
	if key == "" {
		return ErrCacheTagKeyRequired
	}

	normalized, err := NormalizeCacheTags(tags)
	if err != nil {
		return err
	}

	s.ensure()
	s.Remove(key)
	s.byKey[key] = normalized
	for _, tag := range normalized {
		keys := s.byTag[tag]
		if keys == nil {
			keys = make(map[string]struct{})
			s.byTag[tag] = keys
		}
		keys[key] = struct{}{}
	}
	return nil
}

// Remove deletes key and its tag memberships.
func (s *CacheTagSet) Remove(key string) {
	key = strings.TrimSpace(key)
	if key == "" || s == nil || s.byKey == nil {
		return
	}

	tags, ok := s.byKey[key]
	if !ok {
		return
	}
	delete(s.byKey, key)
	for _, tag := range tags {
		keys := s.byTag[tag]
		if keys == nil {
			continue
		}
		delete(keys, key)
		if len(keys) == 0 {
			delete(s.byTag, tag)
		}
	}
}

// Contains reports whether key has any membership in the set.
func (s *CacheTagSet) Contains(key string) bool {
	key = strings.TrimSpace(key)
	if key == "" || s == nil || s.byKey == nil {
		return false
	}
	_, ok := s.byKey[key]
	return ok
}

// Has reports whether key is a member of tag after tag normalization.
func (s *CacheTagSet) Has(key, tag string) bool {
	key = strings.TrimSpace(key)
	if key == "" || s == nil || s.byTag == nil {
		return false
	}

	normalized, err := NormalizeCacheTag(tag)
	if err != nil {
		return false
	}
	_, ok := s.byTag[normalized][key]
	return ok
}

// Tags returns the normalized tags for key in deterministic order.
func (s *CacheTagSet) Tags(key string) []string {
	key = strings.TrimSpace(key)
	if key == "" || s == nil || s.byKey == nil {
		return nil
	}
	return cloneStrings(s.byKey[key])
}

// Keys returns the sorted keys whose tags intersect labels.
func (s *CacheTagSet) Keys(labels []string) ([]string, error) {
	normalized, err := NormalizeCacheTags(labels)
	if err != nil {
		return nil, err
	}
	if len(normalized) == 0 || s == nil || s.byTag == nil {
		return nil, nil
	}

	keys := make(map[string]struct{})
	for _, label := range normalized {
		for key := range s.byTag[label] {
			keys[key] = struct{}{}
		}
	}
	return sortedKeys(keys), nil
}

// Entries returns the set contents in deterministic key order.
func (s *CacheTagSet) Entries() []TagIndexEntry {
	if s == nil || len(s.byKey) == 0 {
		return nil
	}

	keys := make([]string, 0, len(s.byKey))
	for key := range s.byKey {
		keys = append(keys, key)
	}
	sort.Strings(keys)

	entries := make([]TagIndexEntry, 0, len(keys))
	for _, key := range keys {
		entries = append(entries, TagIndexEntry{
			Key:  key,
			Tags: cloneStrings(s.byKey[key]),
		})
	}
	return entries
}

func (s *CacheTagSet) ensure() {
	if s.byKey == nil {
		s.byKey = make(map[string][]string)
	}
	if s.byTag == nil {
		s.byTag = make(map[string]map[string]struct{})
	}
}

// CachePurgePlan lists exact cache keys and tag labels selected for deletion.
type CachePurgePlan struct {
	Keys []string
	Tags []string
}

// PlanCacheTagInvalidation selects cache keys whose tags intersect labels.
func PlanCacheTagInvalidation(entries []TagIndexEntry, labels []string) (CachePurgePlan, error) {
	normalizedLabels, err := NormalizeCacheTags(labels)
	if err != nil {
		return CachePurgePlan{}, err
	}

	labelSet := make(map[string]struct{}, len(normalizedLabels))
	for _, label := range normalizedLabels {
		labelSet[label] = struct{}{}
	}

	keySet := make(map[string]struct{})
	for _, entry := range entries {
		key := strings.TrimSpace(entry.Key)
		if key == "" {
			continue
		}

		tags, err := NormalizeCacheTags(entry.Tags)
		if err != nil {
			return CachePurgePlan{}, err
		}
		for _, tag := range tags {
			if _, ok := labelSet[tag]; ok {
				keySet[key] = struct{}{}
				break
			}
		}
	}
	return CachePurgePlan{Keys: sortedKeys(keySet), Tags: normalizedLabels}, nil
}

// PlanCacheInvalidation selects exact cache keys affected by adapter-neutral
// invalidation tokens. Query wildcards are matched as a generated
// "<feature>.query.*" suffix only; all other wildcard-looking characters are
// treated literally.
func PlanCacheInvalidation(entries []TagIndexEntry, tokens []InvalidationToken) (CachePurgePlan, error) {
	if len(tokens) == 0 {
		return CachePurgePlan{}, nil
	}

	var labels []string
	var queryTokens []InvalidationToken
	for _, token := range tokens {
		switch token.Kind {
		case InvalidationTag:
			labels = append(labels, token.Value)
		case InvalidationQuery, InvalidationQueryWildcard:
			queryTokens = append(queryTokens, token)
		}
	}

	keySet := make(map[string]struct{})
	for _, entry := range entries {
		key := strings.TrimSpace(entry.Key)
		if key == "" {
			continue
		}
		for _, token := range queryTokens {
			if InvalidationTokenMatchesKey(token, key) {
				keySet[key] = struct{}{}
				break
			}
		}
	}

	var normalizedLabels []string
	if len(labels) > 0 {
		tagPlan, err := PlanCacheTagInvalidation(entries, labels)
		if err != nil {
			return CachePurgePlan{}, err
		}
		normalizedLabels = tagPlan.Tags
		for _, key := range tagPlan.Keys {
			keySet[key] = struct{}{}
		}
	}

	return CachePurgePlan{
		Keys: sortedKeys(keySet),
		Tags: normalizedLabels,
	}, nil
}

// InvalidationTokenMatchesKey reports whether token targets key's query name.
func InvalidationTokenMatchesKey(token InvalidationToken, key string) bool {
	query, ok := queryFromCacheKey(key)
	if !ok {
		return false
	}

	value := strings.TrimSpace(token.Value)
	if value == "" {
		return false
	}

	switch token.Kind {
	case InvalidationQuery:
		return query == value
	case InvalidationQueryWildcard:
		const suffix = ".query.*"
		if !strings.HasSuffix(value, suffix) {
			return false
		}
		prefix := strings.TrimSuffix(value, "*")
		return len(query) > len(prefix) && strings.HasPrefix(query, prefix)
	default:
		return false
	}
}

func queryFromCacheKey(key string) (string, bool) {
	key = strings.TrimSpace(key)
	if key == "" {
		return "", false
	}
	query, _, _ := strings.Cut(key, keySeparator)
	query = strings.TrimSpace(query)
	return query, query != ""
}

// NormalizeCacheTag trims and normalizes a tag label.
func NormalizeCacheTag(tag string) (string, error) {
	normalized := NormalizeNamespace(tag)
	if normalized == "" {
		return "", fmt.Errorf("%w: %q", ErrCacheTagInvalid, tag)
	}
	return normalized, nil
}

// NormalizeCacheTags trims, normalizes, deduplicates, and sorts tag labels.
func NormalizeCacheTags(tags []string) ([]string, error) {
	if len(tags) == 0 {
		return nil, nil
	}

	seen := make(map[string]struct{}, len(tags))
	normalized := make([]string, 0, len(tags))
	for _, tag := range tags {
		label, err := NormalizeCacheTag(tag)
		if err != nil {
			return nil, err
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

// IntersectTags returns true when entryTags shares at least one normalized
// label with wanted.
func IntersectTags(entryTags []string, wanted []string) bool {
	entrySet, err := NormalizeCacheTags(entryTags)
	if err != nil {
		return false
	}
	wantedSet, err := NormalizeCacheTags(wanted)
	if err != nil {
		return false
	}
	if len(entrySet) == 0 || len(wantedSet) == 0 {
		return false
	}

	wantedLabels := make(map[string]struct{}, len(wantedSet))
	for _, label := range wantedSet {
		wantedLabels[label] = struct{}{}
	}
	for _, tag := range entrySet {
		if _, ok := wantedLabels[tag]; ok {
			return true
		}
	}
	return false
}

func sortedKeys(values map[string]struct{}) []string {
	if len(values) == 0 {
		return nil
	}
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

func cloneStrings(values []string) []string {
	if len(values) == 0 {
		return nil
	}
	return append([]string(nil), values...)
}
