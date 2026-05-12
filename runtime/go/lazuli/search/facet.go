package search

import (
	"errors"
	"fmt"
	"sort"
	"strings"
)

const (
	// DefaultFacetLimit is used when a facet aggregation does not specify a
	// positive bucket limit.
	DefaultFacetLimit = 20
)

var (
	errEmptyFacetFields      = errors.New("lazuli/search: at least one facet field is required")
	errDuplicateFacetField   = errors.New("lazuli/search: duplicate facet field")
	errUnknownFacetField     = errors.New("lazuli/search: unknown facet field")
	errInvalidFacetLimit     = errors.New("lazuli/search: facet limit must be non-negative")
	errEmptyRankBoosts       = errors.New("lazuli/search: at least one rank boost is required")
	errDuplicateRankBoost    = errors.New("lazuli/search: duplicate rank boost field")
	errInvalidRankBoost      = errors.New("lazuli/search: rank boost weight must be positive")
	errInvalidRankTieBreaker = errors.New("lazuli/search: invalid rank tie-breaker")
)

// FacetField describes a field that can be aggregated into facet buckets.
//
// Name is the preferred field identifier. Column is accepted as an alias for
// Name to match the other SQL helper structs in this package. Limit is optional;
// zero uses DefaultFacetLimit.
type FacetField struct {
	Name   string
	Column string
	Limit  int
}

// FacetAggregation contains SQL fragments for a single facet bucket query.
//
// The fragments are intentionally small so generated code can embed them into
// either a standalone SELECT or an adapter-specific search response shape.
type FacetAggregation struct {
	Field      FacetField
	ValueSQL   string
	CountSQL   string
	GroupBySQL string
	OrderBySQL string
	Limit      int
}

// FacetAggregationOptions configures BuildFacetAggregationQuery.
type FacetAggregationOptions struct {
	Table string
	Field FacetField
	Where Fragment
	Limit int
}

// RankBoost assigns an integer relevance boost to matches on a field.
type RankBoost struct {
	Field  string
	Column string
	Weight int
}

// RankOrderField is an optional deterministic tie-breaker for ranked results.
type RankOrderField struct {
	Field  string
	Column string
	Desc   bool
}

// RankBoostConfig configures rank boost SQL helpers.
//
// Query is trimmed before being bound. Empty queries produce an empty fragment,
// matching BuildLike and BuildTSVectorSearch optional-filter behavior.
type RankBoostConfig struct {
	Query       string
	Placeholder int
	Mode        Mode
	Boosts      []RankBoost
	TieBreakers []RankOrderField
}

// ValidateFacetFields validates facet fields against safe SQL identifier rules
// and, when allowedFields is non-empty, against that generated field catalog.
func ValidateFacetFields(fields []FacetField, allowedFields ...string) error {
	_, err := NormalizeFacetFields(fields, allowedFields...)
	return err
}

// NormalizeFacetFields returns a validated copy of fields. The authored field
// order is preserved; bucket row order is handled by FacetAggregation.OrderBySQL.
func NormalizeFacetFields(fields []FacetField, allowedFields ...string) ([]FacetField, error) {
	if len(fields) == 0 {
		return nil, errEmptyFacetFields
	}

	allowed, err := allowedFieldSet(allowedFields)
	if err != nil {
		return nil, err
	}

	seen := make(map[string]struct{}, len(fields))
	normalized := make([]FacetField, 0, len(fields))
	for _, field := range fields {
		clean, err := normalizeFacetField(field)
		if err != nil {
			return nil, err
		}
		if len(allowed) > 0 {
			if _, ok := allowed[clean.Name]; !ok {
				return nil, errUnknownFacetField
			}
		}
		if _, ok := seen[clean.Name]; ok {
			return nil, errDuplicateFacetField
		}
		seen[clean.Name] = struct{}{}
		normalized = append(normalized, clean)
	}
	return normalized, nil
}

// BuildFacetAggregation returns standard SQL fragments for counting buckets by
// one field. Results are ordered deterministically by count descending and then
// bucket value ascending.
func BuildFacetAggregation(field FacetField) (FacetAggregation, error) {
	clean, err := normalizeFacetField(field)
	if err != nil {
		return FacetAggregation{}, err
	}

	quoted, err := quoteDottedIdent(clean.Name)
	if err != nil {
		return FacetAggregation{}, err
	}
	limit, err := facetLimit(clean.Limit, 0)
	if err != nil {
		return FacetAggregation{}, err
	}

	return FacetAggregation{
		Field:      clean,
		ValueSQL:   quoted + " AS facet_value",
		CountSQL:   "COUNT(*) AS facet_count",
		GroupBySQL: quoted,
		OrderBySQL: "facet_count DESC, facet_value ASC",
		Limit:      limit,
	}, nil
}

// BuildFacetAggregationQuery returns a standalone facet bucket SELECT.
//
// Where is optional and is appended as-is, with its Args copied into the
// returned fragment. Table and field identifiers are quoted segment-by-segment.
func BuildFacetAggregationQuery(opts FacetAggregationOptions) (Fragment, error) {
	table, err := quoteDottedIdent(opts.Table)
	if err != nil {
		return Fragment{}, err
	}
	aggregation, err := BuildFacetAggregation(opts.Field)
	if err != nil {
		return Fragment{}, err
	}
	limit, err := facetLimit(opts.Field.Limit, opts.Limit)
	if err != nil {
		return Fragment{}, err
	}

	var b strings.Builder
	b.WriteString("SELECT ")
	b.WriteString(aggregation.ValueSQL)
	b.WriteString(", ")
	b.WriteString(aggregation.CountSQL)
	b.WriteString(" FROM ")
	b.WriteString(table)
	if opts.Where.SQL != "" {
		b.WriteString(" WHERE ")
		b.WriteString(opts.Where.SQL)
	}
	b.WriteString(" GROUP BY ")
	b.WriteString(aggregation.GroupBySQL)
	b.WriteString(" ORDER BY ")
	b.WriteString(aggregation.OrderBySQL)
	b.WriteString(fmt.Sprintf(" LIMIT %d", limit))

	return Fragment{
		SQL:  b.String(),
		Args: append([]any(nil), opts.Where.Args...),
	}, nil
}

// ValidateRankBoosts validates rank boosts against safe SQL identifier rules
// and, when allowedFields is non-empty, against that generated field catalog.
func ValidateRankBoosts(boosts []RankBoost, allowedFields ...string) error {
	_, err := NormalizeRankBoosts(boosts, allowedFields...)
	return err
}

// NormalizeRankBoosts returns a validated copy of boosts in deterministic SQL
// generation order: higher weight first, then field name.
func NormalizeRankBoosts(boosts []RankBoost, allowedFields ...string) ([]RankBoost, error) {
	if len(boosts) == 0 {
		return nil, errEmptyRankBoosts
	}

	allowed, err := allowedFieldSet(allowedFields)
	if err != nil {
		return nil, err
	}

	seen := make(map[string]struct{}, len(boosts))
	normalized := make([]RankBoost, 0, len(boosts))
	for _, boost := range boosts {
		clean, err := normalizeRankBoost(boost)
		if err != nil {
			return nil, err
		}
		if len(allowed) > 0 {
			if _, ok := allowed[clean.Field]; !ok {
				return nil, errUnknownFacetField
			}
		}
		if _, ok := seen[clean.Field]; ok {
			return nil, errDuplicateRankBoost
		}
		seen[clean.Field] = struct{}{}
		normalized = append(normalized, clean)
	}
	sort.SliceStable(normalized, func(i, j int) bool {
		if normalized[i].Weight != normalized[j].Weight {
			return normalized[i].Weight > normalized[j].Weight
		}
		return normalized[i].Field < normalized[j].Field
	})
	return normalized, nil
}

// BuildRankBoost returns a CASE-based relevance expression for boosted LIKE
// matches. The expression is adapter-neutral SQL and reuses one placeholder for
// every boosted field.
func BuildRankBoost(config RankBoostConfig) (Fragment, error) {
	query := strings.TrimSpace(config.Query)
	if query == "" {
		return Fragment{}, nil
	}
	if config.Placeholder <= 0 {
		return Fragment{}, errInvalidPlaceholder
	}

	boosts, err := NormalizeRankBoosts(config.Boosts)
	if err != nil {
		return Fragment{}, err
	}

	placeholderToken := fmt.Sprintf("$%d", config.Placeholder)
	expressions := make([]string, 0, len(boosts))
	for _, boost := range boosts {
		quoted, err := quoteDottedIdent(boost.Field)
		if err != nil {
			return Fragment{}, err
		}
		expressions = append(expressions, fmt.Sprintf(
			"CASE WHEN %s LIKE %s ESCAPE '%s' THEN %d ELSE 0 END",
			quoted,
			placeholderToken,
			defaultEscape,
			boost.Weight,
		))
	}

	return Fragment{
		SQL:  "(" + strings.Join(expressions, " + ") + ")",
		Args: []any{Pattern(query, config.Mode)},
	}, nil
}

// BuildRankBoostOrder returns a descending rank ORDER BY fragment with optional
// deterministic tie-breakers appended in the order provided.
func BuildRankBoostOrder(config RankBoostConfig) (Fragment, error) {
	rank, err := BuildRankBoost(config)
	if err != nil {
		return Fragment{}, err
	}
	if rank.SQL == "" {
		return Fragment{}, nil
	}

	parts := []string{rank.SQL + " DESC"}
	for _, tieBreaker := range config.TieBreakers {
		clean, err := normalizeRankOrderField(tieBreaker)
		if err != nil {
			return Fragment{}, err
		}
		quoted, err := quoteDottedIdent(clean.Field)
		if err != nil {
			return Fragment{}, err
		}
		direction := "ASC"
		if clean.Desc {
			direction = "DESC"
		}
		parts = append(parts, quoted+" "+direction)
	}
	rank.SQL = strings.Join(parts, ", ")
	return rank, nil
}

func normalizeFacetField(field FacetField) (FacetField, error) {
	name := strings.TrimSpace(field.Name)
	if name == "" {
		name = strings.TrimSpace(field.Column)
	}
	if _, err := quoteDottedIdent(name); err != nil {
		return FacetField{}, err
	}
	if field.Limit < 0 {
		return FacetField{}, errInvalidFacetLimit
	}
	return FacetField{
		Name:  name,
		Limit: field.Limit,
	}, nil
}

func normalizeRankBoost(boost RankBoost) (RankBoost, error) {
	field := strings.TrimSpace(boost.Field)
	if field == "" {
		field = strings.TrimSpace(boost.Column)
	}
	if _, err := quoteDottedIdent(field); err != nil {
		return RankBoost{}, err
	}
	if boost.Weight <= 0 {
		return RankBoost{}, errInvalidRankBoost
	}
	return RankBoost{
		Field:  field,
		Weight: boost.Weight,
	}, nil
}

func normalizeRankOrderField(field RankOrderField) (RankOrderField, error) {
	name := strings.TrimSpace(field.Field)
	if name == "" {
		name = strings.TrimSpace(field.Column)
	}
	if name == "" {
		return RankOrderField{}, errInvalidRankTieBreaker
	}
	if _, err := quoteDottedIdent(name); err != nil {
		return RankOrderField{}, err
	}
	return RankOrderField{
		Field: name,
		Desc:  field.Desc,
	}, nil
}

func allowedFieldSet(fields []string) (map[string]struct{}, error) {
	if len(fields) == 0 {
		return nil, nil
	}
	allowed := make(map[string]struct{}, len(fields))
	for _, field := range fields {
		name := strings.TrimSpace(field)
		if _, err := quoteDottedIdent(name); err != nil {
			return nil, err
		}
		allowed[name] = struct{}{}
	}
	return allowed, nil
}

func facetLimit(fieldLimit, overrideLimit int) (int, error) {
	switch {
	case fieldLimit < 0, overrideLimit < 0:
		return 0, errInvalidFacetLimit
	case overrideLimit > 0:
		return overrideLimit, nil
	case fieldLimit > 0:
		return fieldLimit, nil
	default:
		return DefaultFacetLimit, nil
	}
}
