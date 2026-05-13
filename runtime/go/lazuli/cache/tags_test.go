package cache

import (
	"errors"
	"reflect"
	"testing"
)

func TestNormalizeCacheTags(t *testing.T) {
	tags, err := NormalizeCacheTags([]string{
		" Product ",
		"customer.reports",
		"PRODUCT",
		"billing_v2",
		"Tier Gold",
	})
	if err != nil {
		t.Fatalf("NormalizeCacheTags() error = %v", err)
	}

	want := []string{"billing_v2", "customer.reports", "product", "tier-gold"}
	if !reflect.DeepEqual(tags, want) {
		t.Fatalf("NormalizeCacheTags() = %#v, want %#v", tags, want)
	}
}

func TestNormalizeCacheTagsRejectsInvalidTags(t *testing.T) {
	if _, err := NormalizeCacheTags([]string{"!!!"}); !errors.Is(err, ErrCacheTagInvalid) {
		t.Fatalf("NormalizeCacheTags() error = %v, want %v", err, ErrCacheTagInvalid)
	}
}

func TestCacheTagSetTracksKeyMembership(t *testing.T) {
	var set CacheTagSet
	if err := set.Add(" customer.query.list|1|a ", []string{" Shared ", "list", "shared"}); err != nil {
		t.Fatalf("Add(first) error = %v", err)
	}
	if err := set.Add("invoice.query.list|1|b", []string{"shared", "invoice"}); err != nil {
		t.Fatalf("Add(second) error = %v", err)
	}
	if err := set.Add("customer.query.detail|1|c", nil); err != nil {
		t.Fatalf("Add(untagged) error = %v", err)
	}

	if !set.Contains("customer.query.detail|1|c") {
		t.Fatal("Contains(untagged) = false, want true")
	}
	if !set.Has("customer.query.list|1|a", "SHARED") {
		t.Fatal("Has(normalized tag) = false, want true")
	}

	tags := set.Tags("customer.query.list|1|a")
	wantTags := []string{"list", "shared"}
	if !reflect.DeepEqual(tags, wantTags) {
		t.Fatalf("Tags() = %#v, want %#v", tags, wantTags)
	}
	tags[0] = "mutated"
	if got := set.Tags("customer.query.list|1|a"); !reflect.DeepEqual(got, wantTags) {
		t.Fatalf("Tags() after caller mutation = %#v, want %#v", got, wantTags)
	}

	keys, err := set.Keys([]string{"shared"})
	if err != nil {
		t.Fatalf("Keys(shared) error = %v", err)
	}
	wantKeys := []string{"customer.query.list|1|a", "invoice.query.list|1|b"}
	if !reflect.DeepEqual(keys, wantKeys) {
		t.Fatalf("Keys(shared) = %#v, want %#v", keys, wantKeys)
	}

	if err := set.Add("customer.query.list|1|a", []string{"detail"}); err != nil {
		t.Fatalf("Add(replace) error = %v", err)
	}
	if set.Has("customer.query.list|1|a", "shared") {
		t.Fatal("Has(old tag after replace) = true, want false")
	}
	if !set.Has("customer.query.list|1|a", "detail") {
		t.Fatal("Has(new tag after replace) = false, want true")
	}

	set.Remove("invoice.query.list|1|b")
	if set.Contains("invoice.query.list|1|b") {
		t.Fatal("Contains(removed) = true, want false")
	}
}

func TestCacheTagSetAddRequiresKey(t *testing.T) {
	var set CacheTagSet
	if err := set.Add(" ", []string{"shared"}); !errors.Is(err, ErrCacheTagKeyRequired) {
		t.Fatalf("Add(empty key) error = %v, want %v", err, ErrCacheTagKeyRequired)
	}
}

func TestPlanCacheTagInvalidationIsDeterministic(t *testing.T) {
	plan, err := PlanCacheTagInvalidation([]TagIndexEntry{
		{Key: "invoice.query.list|1|c", Tags: []string{"invoice"}},
		{Key: "customer.query.detail|1|b", Tags: []string{" Detail "}},
		{Key: "customer.query.list|1|a", Tags: []string{"shared", "list"}},
		{Key: "customer.query.list|1|a", Tags: []string{"list"}},
		{Key: " ", Tags: []string{"shared"}},
	}, []string{"SHARED", "detail", "shared"})
	if err != nil {
		t.Fatalf("PlanCacheTagInvalidation() error = %v", err)
	}

	want := CachePurgePlan{
		Keys: []string{
			"customer.query.detail|1|b",
			"customer.query.list|1|a",
		},
		Tags: []string{"detail", "shared"},
	}
	if !reflect.DeepEqual(plan, want) {
		t.Fatalf("PlanCacheTagInvalidation() = %#v, want %#v", plan, want)
	}
}

func TestPlanCacheTagInvalidationValidatesTags(t *testing.T) {
	if _, err := PlanCacheTagInvalidation(nil, []string{"!!!"}); !errors.Is(err, ErrCacheTagInvalid) {
		t.Fatalf("PlanCacheTagInvalidation(invalid label) error = %v, want %v", err, ErrCacheTagInvalid)
	}
	_, err := PlanCacheTagInvalidation([]TagIndexEntry{
		{Key: "customer.query.list|1|a", Tags: []string{"!!!"}},
	}, []string{"shared"})
	if !errors.Is(err, ErrCacheTagInvalid) {
		t.Fatalf("PlanCacheTagInvalidation(invalid entry tag) error = %v, want %v", err, ErrCacheTagInvalid)
	}
}

func TestInvalidationTokenMatchesKeyIsWildcardSafe(t *testing.T) {
	tests := []struct {
		name  string
		token InvalidationToken
		key   string
		want  bool
	}{
		{
			name:  "exact query matches literally",
			token: InvalidationToken{Kind: InvalidationQuery, Value: "customer.query.*"},
			key:   "customer.query.*|tenant|hash",
			want:  true,
		},
		{
			name:  "exact query does not treat star as wildcard",
			token: InvalidationToken{Kind: InvalidationQuery, Value: "customer.query.*"},
			key:   "customer.query.list|tenant|hash",
			want:  false,
		},
		{
			name:  "generated wildcard matches feature query",
			token: InvalidationToken{Kind: InvalidationQueryWildcard, Value: "customer.query.*"},
			key:   "customer.query.list|tenant|hash",
			want:  true,
		},
		{
			name:  "generated wildcard requires query separator",
			token: InvalidationToken{Kind: InvalidationQueryWildcard, Value: "customer.query.*"},
			key:   "customer.queryx.list|tenant|hash",
			want:  false,
		},
		{
			name:  "glob metacharacters before suffix are literal",
			token: InvalidationToken{Kind: InvalidationQueryWildcard, Value: "billing[eu].query.*"},
			key:   "billing[eu].query.list|tenant|hash",
			want:  true,
		},
		{
			name:  "malformed wildcard token does not match",
			token: InvalidationToken{Kind: InvalidationQueryWildcard, Value: "customer.query.list*"},
			key:   "customer.query.list|tenant|hash",
			want:  false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := InvalidationTokenMatchesKey(tt.token, tt.key); got != tt.want {
				t.Fatalf("InvalidationTokenMatchesKey() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestPlanCacheInvalidationCombinesTokensDeterministically(t *testing.T) {
	entries := []TagIndexEntry{
		{Key: "invoice.query.list|1|c", Tags: []string{"invoice"}},
		{Key: "customer.query.detail|1|b", Tags: []string{"detail"}},
		{Key: "customer.query.list|1|a", Tags: []string{"shared"}},
		{Key: "customer.query.*|1|literal", Tags: []string{"literal"}},
	}

	plan, err := PlanCacheInvalidation(entries, []InvalidationToken{
		{Kind: InvalidationTag, Value: " SHARED "},
		{Kind: InvalidationQueryWildcard, Value: "customer.query.*"},
		{Kind: InvalidationQuery, Value: "invoice.query.list"},
		{Kind: InvalidationQuery, Value: "customer.query.*"},
	})
	if err != nil {
		t.Fatalf("PlanCacheInvalidation() error = %v", err)
	}

	want := CachePurgePlan{
		Keys: []string{
			"customer.query.*|1|literal",
			"customer.query.detail|1|b",
			"customer.query.list|1|a",
			"invoice.query.list|1|c",
		},
		Tags: []string{"shared"},
	}
	if !reflect.DeepEqual(plan, want) {
		t.Fatalf("PlanCacheInvalidation() = %#v, want %#v", plan, want)
	}
}

func TestIntersectTagsNormalizesInputs(t *testing.T) {
	if !IntersectTags([]string{" Customer Reports "}, []string{"customer-reports"}) {
		t.Fatal("IntersectTags(normalized match) = false, want true")
	}
	if IntersectTags([]string{"!!!"}, []string{"customer"}) {
		t.Fatal("IntersectTags(invalid tag) = true, want false")
	}
}
