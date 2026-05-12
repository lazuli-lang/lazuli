package cache

import (
	"errors"
	"strings"
	"testing"
	"time"
)

func TestBuildQueryKeyIncludesParamsAndPage(t *testing.T) {
	first, err := BuildQueryKey(QueryKeyParts{
		Spec:   QuerySpec{Namespace: " Customer Reports "},
		Query:  " customer.query.search ",
		Tenant: " org-7 ",
		Params: map[string]any{
			"filters": map[string]any{"plan": "pro", "region": "sa"},
			"status":  "active",
		},
		Page: map[string]any{"limit": 20, "offset": 40},
	})
	if err != nil {
		t.Fatalf("BuildQueryKey(first) error = %v", err)
	}

	second, err := BuildQueryKey(QueryKeyParts{
		Spec:   QuerySpec{Namespace: "Customer Reports"},
		Query:  "customer.query.search",
		Tenant: "org-7",
		Params: map[string]any{
			"status":  "active",
			"filters": map[string]any{"region": "sa", "plan": "pro"},
		},
		Page: map[string]any{"offset": 40, "limit": 20},
	})
	if err != nil {
		t.Fatalf("BuildQueryKey(second) error = %v", err)
	}
	if second != first {
		t.Fatalf("BuildQueryKey(second) = %q, want %q", second, first)
	}

	const prefix = "customer.query.search|customer-reports|org-7|"
	if !strings.HasPrefix(first, prefix) {
		t.Fatalf("BuildQueryKey prefix = %q, want prefix %q", first, prefix)
	}

	changedPage, err := BuildQueryKey(QueryKeyParts{
		Spec:   QuerySpec{Namespace: "Customer Reports"},
		Query:  "customer.query.search",
		Tenant: "org-7",
		Params: map[string]any{
			"filters": map[string]any{"plan": "pro", "region": "sa"},
			"status":  "active",
		},
		Page: map[string]any{"limit": 20, "offset": 60},
	})
	if err != nil {
		t.Fatalf("BuildQueryKey(changed page) error = %v", err)
	}
	if changedPage == first {
		t.Fatalf("BuildQueryKey(changed page) = %q, want different key", changedPage)
	}
}

func TestBuildQueryKeyPropagatesErrors(t *testing.T) {
	if _, err := BuildQueryKey(QueryKeyParts{Params: map[string]any{}}); err == nil {
		t.Fatal("BuildQueryKey(empty query) error = nil, want error")
	}
	_, err := BuildQueryKey(QueryKeyParts{
		Query:  "customer.query.search",
		Params: make(chan int),
	})
	if err == nil {
		t.Fatal("BuildQueryKey(unencodable params) error = nil, want error")
	}
}

func TestQueryCacheConfigValidate(t *testing.T) {
	valid := QueryCacheConfig{
		TTL:           5 * time.Minute,
		SlidingTTL:    time.Minute,
		NegativeCache: true,
	}
	if err := valid.Validate(); err != nil {
		t.Fatalf("Validate(valid) error = %v", err)
	}
	if err := (QueryCacheConfig{}).Validate(); err != nil {
		t.Fatalf("Validate(zero) error = %v", err)
	}

	tests := []struct {
		name   string
		config QueryCacheConfig
	}{
		{name: "negative ttl", config: QueryCacheConfig{TTL: -time.Second}},
		{name: "negative sliding ttl", config: QueryCacheConfig{SlidingTTL: -time.Second}},
		{name: "sliding ttl requires ttl", config: QueryCacheConfig{SlidingTTL: time.Second}},
		{name: "sliding ttl exceeds ttl", config: QueryCacheConfig{TTL: time.Second, SlidingTTL: 2 * time.Second}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := tt.config.Validate(); !errors.Is(err, ErrInvalidQueryCacheConfig) {
				t.Fatalf("Validate() error = %v, want ErrInvalidQueryCacheConfig", err)
			}
		})
	}
}

func TestBuildInvalidationTokens(t *testing.T) {
	tokens, err := BuildInvalidationTokens(" customer ",
		Query("", "list", ""),
		Query("billing", "query.by_id", "id: input.id"),
		Query("", "orders.query.recent", ""),
		QueryWildcard(""),
		Tag(" shared "),
	)
	if err != nil {
		t.Fatalf("BuildInvalidationTokens() error = %v", err)
	}

	want := []InvalidationToken{
		{Kind: InvalidationQuery, Value: "customer.query.list"},
		{Kind: InvalidationQuery, Value: "billing.query.by_id", Args: "id: input.id"},
		{Kind: InvalidationQuery, Value: "orders.query.recent"},
		{Kind: InvalidationQueryWildcard, Value: "customer.query.*"},
		{Kind: InvalidationTag, Value: "shared"},
	}
	if len(tokens) != len(want) {
		t.Fatalf("BuildInvalidationTokens() len = %d, want %d: %#v", len(tokens), len(want), tokens)
	}
	for i := range want {
		if tokens[i] != want[i] {
			t.Fatalf("BuildInvalidationTokens()[%d] = %#v, want %#v", i, tokens[i], want[i])
		}
	}
	if got := tokens[1].String(); got != "query:billing.query.by_id" {
		t.Fatalf("String() = %q, want query token label", got)
	}
}

func TestBuildInvalidationTokenRejectsInvalidTargets(t *testing.T) {
	tests := []struct {
		name   string
		target InvalidationTarget
	}{
		{name: "nil", target: nil},
		{name: "query missing name", target: QueryTarget{Feature: "customer"}},
		{name: "short query missing name", target: QueryTarget{Feature: "customer", Name: "query."}},
		{name: "qualified query missing name", target: QueryTarget{Name: "customer.query."}},
		{name: "query missing feature", target: QueryTarget{Name: "list"}},
		{name: "wildcard missing feature", target: QueryWildcardTarget{}},
		{name: "tag missing label", target: TagTarget{}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := BuildInvalidationToken("", tt.target)
			if !errors.Is(err, ErrInvalidInvalidationTarget) {
				t.Fatalf("BuildInvalidationToken() error = %v, want ErrInvalidInvalidationTarget", err)
			}
		})
	}
}
