package search

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"sort"
	"strings"
)

const (
	// AnalyticsQueryHashPrefix prefixes unsalted SHA-256 query fingerprints.
	AnalyticsQueryHashPrefix = "sha256:"
	// AnalyticsQueryHMACPrefix prefixes salted HMAC-SHA-256 query fingerprints.
	AnalyticsQueryHMACPrefix = "hmac-sha256:"
)

// AnalyticsEventKind names a storage-agnostic search analytics event.
type AnalyticsEventKind string

const (
	// AnalyticsEventSearch records one or more executed searches.
	AnalyticsEventSearch AnalyticsEventKind = "search"
	// AnalyticsEventClick records one or more result clicks for a query.
	AnalyticsEventClick AnalyticsEventKind = "click"
	// AnalyticsEventConversion records one or more conversions for a query.
	AnalyticsEventConversion AnalyticsEventKind = "conversion"
)

// AnalyticsEvent is an adapter-neutral analytics input.
//
// Count lets callers feed already-aggregated rows. A zero Count means one event
// so simple literals and constructor values behave as single observations.
// Negative Count values are ignored. ResultCount is only used for search
// events; zero result counts are tracked, while negative counts mean unknown.
type AnalyticsEvent struct {
	Kind        AnalyticsEventKind
	Query       string
	ResultCount int
	Count       int
}

// AnalyticsOptions configures AggregateAnalytics.
type AnalyticsOptions struct {
	// MaxQueries caps per-query summaries. Zero or negative values keep every
	// query bucket.
	MaxQueries int
	// HashSalt switches query fingerprints from SHA-256 to HMAC-SHA-256. Use a
	// deployment-private value when summaries may leave a trusted boundary.
	HashSalt string
}

// AnalyticsSummary reports aggregate search behavior without raw query text.
type AnalyticsSummary struct {
	Searches    int
	ZeroResults int
	Clicks      int
	Conversions int
	Queries     []AnalyticsQuerySummary
}

// AnalyticsQuerySummary reports one query bucket without exposing the query.
type AnalyticsQuerySummary struct {
	QueryHash   string
	Searches    int
	ZeroResults int
	Clicks      int
	Conversions int
}

// NewAnalyticsSearch returns a single search event for query.
func NewAnalyticsSearch(query string, resultCount int) AnalyticsEvent {
	return AnalyticsEvent{
		Kind:        AnalyticsEventSearch,
		Query:       query,
		ResultCount: resultCount,
		Count:       1,
	}
}

// NewAnalyticsClick returns a single click event for query.
func NewAnalyticsClick(query string) AnalyticsEvent {
	return AnalyticsEvent{
		Kind:  AnalyticsEventClick,
		Query: query,
		Count: 1,
	}
}

// NewAnalyticsConversion returns a single conversion event for query.
func NewAnalyticsConversion(query string) AnalyticsEvent {
	return AnalyticsEvent{
		Kind:  AnalyticsEventConversion,
		Query: query,
		Count: 1,
	}
}

// AggregateAnalytics rolls search, click, and conversion events into a
// deterministic redaction-safe summary.
//
// Queries are normalized before bucketing by trimming, lowercasing, and
// collapsing whitespace. Empty queries and unknown event kinds are skipped. An
// empty Kind is treated as AnalyticsEventSearch so search records can be built
// with compact literals.
func AggregateAnalytics(events []AnalyticsEvent, options AnalyticsOptions) AnalyticsSummary {
	type queryBucket struct {
		summary AnalyticsQuerySummary
	}

	var summary AnalyticsSummary
	buckets := make(map[string]*queryBucket)
	for _, event := range events {
		query := normalizeAnalyticsQuery(event.Query)
		if query == "" {
			continue
		}
		count := analyticsEventCount(event.Count)
		if count == 0 {
			continue
		}

		kind := analyticsEventKind(event.Kind)
		if kind == "" {
			continue
		}

		bucket := buckets[query]
		if bucket == nil {
			bucket = &queryBucket{
				summary: AnalyticsQuerySummary{
					QueryHash: analyticsQueryHash(query, options.HashSalt),
				},
			}
			buckets[query] = bucket
		}

		switch kind {
		case AnalyticsEventSearch:
			summary.Searches += count
			bucket.summary.Searches += count
			if event.ResultCount == 0 {
				summary.ZeroResults += count
				bucket.summary.ZeroResults += count
			}
		case AnalyticsEventClick:
			summary.Clicks += count
			bucket.summary.Clicks += count
		case AnalyticsEventConversion:
			summary.Conversions += count
			bucket.summary.Conversions += count
		}
	}

	if len(buckets) == 0 {
		return summary
	}

	summary.Queries = make([]AnalyticsQuerySummary, 0, len(buckets))
	for _, bucket := range buckets {
		summary.Queries = append(summary.Queries, bucket.summary)
	}
	SortAnalyticsQuerySummaries(summary.Queries)
	if options.MaxQueries > 0 && len(summary.Queries) > options.MaxQueries {
		summary.Queries = summary.Queries[:options.MaxQueries]
	}
	return summary
}

// SortAnalyticsQuerySummaries sorts query summaries by activity and then hash.
func SortAnalyticsQuerySummaries(queries []AnalyticsQuerySummary) {
	sort.SliceStable(queries, func(i, j int) bool {
		left := queries[i]
		right := queries[j]
		if left.Searches != right.Searches {
			return left.Searches > right.Searches
		}
		if left.ZeroResults != right.ZeroResults {
			return left.ZeroResults > right.ZeroResults
		}
		if left.Clicks != right.Clicks {
			return left.Clicks > right.Clicks
		}
		if left.Conversions != right.Conversions {
			return left.Conversions > right.Conversions
		}
		return left.QueryHash < right.QueryHash
	})
}

// AnalyticsQueryHash returns the redaction-safe fingerprint used for query
// buckets. Empty queries return an empty string.
func AnalyticsQueryHash(query, salt string) string {
	query = normalizeAnalyticsQuery(query)
	if query == "" {
		return ""
	}
	return analyticsQueryHash(query, salt)
}

func analyticsEventKind(kind AnalyticsEventKind) AnalyticsEventKind {
	switch kind {
	case "":
		return AnalyticsEventSearch
	case AnalyticsEventSearch, AnalyticsEventClick, AnalyticsEventConversion:
		return kind
	default:
		return ""
	}
}

func analyticsEventCount(count int) int {
	switch {
	case count < 0:
		return 0
	case count == 0:
		return 1
	default:
		return count
	}
}

func normalizeAnalyticsQuery(query string) string {
	fields := strings.Fields(query)
	if len(fields) == 0 {
		return ""
	}
	return strings.ToLower(strings.Join(fields, " "))
}

func analyticsQueryHash(query, salt string) string {
	if salt != "" {
		mac := hmac.New(sha256.New, []byte(salt))
		_, _ = mac.Write([]byte(query))
		return AnalyticsQueryHMACPrefix + hex.EncodeToString(mac.Sum(nil))
	}

	sum := sha256.Sum256([]byte(query))
	return AnalyticsQueryHashPrefix + hex.EncodeToString(sum[:])
}
