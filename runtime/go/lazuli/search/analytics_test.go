package search

import (
	"encoding/json"
	"reflect"
	"strings"
	"testing"
)

func TestAggregateAnalyticsCountsQueriesZeroResultsAndInteractions(t *testing.T) {
	got := AggregateAnalytics([]AnalyticsEvent{
		NewAnalyticsSearch(" Beach   House ", 12),
		{Kind: AnalyticsEventSearch, Query: "beach house", ResultCount: 0, Count: 2},
		{Query: "Garden", ResultCount: 0},
		NewAnalyticsClick("BEACH house"),
		{Kind: AnalyticsEventClick, Query: "beach house", Count: 2},
		NewAnalyticsConversion("beach house"),
		{Kind: AnalyticsEventConversion, Query: "garden", Count: 3},
		{Kind: AnalyticsEventClick, Query: "garden", Count: -4},
		{Kind: AnalyticsEventKind("view"), Query: "beach house", Count: 9},
		NewAnalyticsSearch("   ", 0),
	}, AnalyticsOptions{})

	want := AnalyticsSummary{
		Searches:    4,
		ZeroResults: 3,
		Clicks:      3,
		Conversions: 4,
		Queries: []AnalyticsQuerySummary{
			{
				QueryHash:   AnalyticsQueryHash("beach house", ""),
				Searches:    3,
				ZeroResults: 2,
				Clicks:      3,
				Conversions: 1,
			},
			{
				QueryHash:   AnalyticsQueryHash("garden", ""),
				Searches:    1,
				ZeroResults: 1,
				Conversions: 3,
			},
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("AggregateAnalytics() = %#v, want %#v", got, want)
	}

	data, err := json.Marshal(got)
	if err != nil {
		t.Fatalf("json.Marshal(summary) error = %v", err)
	}
	encoded := strings.ToLower(string(data))
	for _, raw := range []string{"beach", "garden"} {
		if strings.Contains(encoded, raw) {
			t.Fatalf("summary JSON leaks raw query %q: %s", raw, data)
		}
	}
}

func TestAggregateAnalyticsAppliesSaltAndLimit(t *testing.T) {
	got := AggregateAnalytics([]AnalyticsEvent{
		{Kind: AnalyticsEventSearch, Query: "alpha", ResultCount: 4},
		{Kind: AnalyticsEventSearch, Query: "beta", ResultCount: 4, Count: 2},
		{Kind: AnalyticsEventSearch, Query: "gamma", ResultCount: 0, Count: 2},
		{Kind: AnalyticsEventClick, Query: "beta", Count: 5},
	}, AnalyticsOptions{
		MaxQueries: 2,
		HashSalt:   "private-report-key",
	})

	if got.Searches != 5 || got.ZeroResults != 2 || got.Clicks != 5 {
		t.Fatalf("AggregateAnalytics() totals = searches %d zero %d clicks %d, want 5 2 5",
			got.Searches, got.ZeroResults, got.Clicks)
	}
	if len(got.Queries) != 2 {
		t.Fatalf("AggregateAnalytics().Queries len = %d, want 2", len(got.Queries))
	}
	for _, query := range got.Queries {
		if !strings.HasPrefix(query.QueryHash, AnalyticsQueryHMACPrefix) {
			t.Fatalf("QueryHash = %q, want %q prefix", query.QueryHash, AnalyticsQueryHMACPrefix)
		}
	}
	if got.Queries[0].QueryHash != AnalyticsQueryHash("gamma", "private-report-key") {
		t.Fatalf("first QueryHash = %q, want gamma bucket first", got.Queries[0].QueryHash)
	}
}

func TestAnalyticsQueryHashNormalizesAndSalts(t *testing.T) {
	plain := AnalyticsQueryHash("  Beach\tHouse  ", "")
	same := AnalyticsQueryHash("beach house", "")
	if plain != same {
		t.Fatalf("AnalyticsQueryHash() = %q and %q, want normalized queries to match", plain, same)
	}
	if !strings.HasPrefix(plain, AnalyticsQueryHashPrefix) {
		t.Fatalf("AnalyticsQueryHash() = %q, want %q prefix", plain, AnalyticsQueryHashPrefix)
	}

	salted := AnalyticsQueryHash("beach house", "tenant-secret")
	if salted == plain {
		t.Fatalf("salted AnalyticsQueryHash() = unsalted hash %q", salted)
	}
	if !strings.HasPrefix(salted, AnalyticsQueryHMACPrefix) {
		t.Fatalf("salted AnalyticsQueryHash() = %q, want %q prefix", salted, AnalyticsQueryHMACPrefix)
	}
	if got := AnalyticsQueryHash("   ", "tenant-secret"); got != "" {
		t.Fatalf("AnalyticsQueryHash(empty) = %q, want empty string", got)
	}
}

func TestAggregateAnalyticsTreatsNegativeResultCountAsUnknown(t *testing.T) {
	got := AggregateAnalytics([]AnalyticsEvent{
		{Query: "unknown results", ResultCount: -1},
	}, AnalyticsOptions{})

	want := AnalyticsSummary{
		Searches: 1,
		Queries: []AnalyticsQuerySummary{{
			QueryHash: AnalyticsQueryHash("unknown results", ""),
			Searches:  1,
		}},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("AggregateAnalytics() = %#v, want %#v", got, want)
	}
}
