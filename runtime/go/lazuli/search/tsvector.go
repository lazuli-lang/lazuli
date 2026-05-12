package search

import (
	"errors"
	"fmt"
	"strings"
)

var (
	errInvalidTSLanguage  = errors.New("lazuli/search: invalid text search language")
	errInvalidTSWeight    = errors.New("lazuli/search: invalid text search weight")
	errInvalidTSQueryMode = errors.New("lazuli/search: invalid text search query mode")
)

// TSQueryMode selects the PostgreSQL tsquery builder used for a search term.
type TSQueryMode int

const (
	// TSQueryPlain builds plainto_tsquery(language, query).
	TSQueryPlain TSQueryMode = iota
	// TSQueryWebsearch builds websearch_to_tsquery(language, query).
	TSQueryWebsearch
)

// TSVectorColumn describes a column included in a generated tsvector.
//
// Weight is optional. When set, it must be A, B, C, or D. Lowercase weights are
// normalized to PostgreSQL's uppercase weight labels.
type TSVectorColumn struct {
	// Name is the column identifier, such as "title" or "listing.title".
	Name string
	// Column is accepted as an alias for Name.
	Column string
	// Weight optionally sets the tsvector weight label A, B, C, or D.
	Weight string
}

// TSVectorOptions configures PostgreSQL full-text search fragments.
type TSVectorOptions struct {
	// Query is trimmed before being bound. Empty queries produce an empty
	// fragment, matching BuildLike's optional-filter behavior.
	Query string
	// Placeholder is the 1-based PostgreSQL placeholder for Query.
	Placeholder int
	// Language is a PostgreSQL text search configuration name, such as
	// "english" or "pg_catalog.simple".
	Language string
	// QueryMode selects plainto_tsquery or websearch_to_tsquery.
	QueryMode TSQueryMode
	// Columns are the text columns included in to_tsvector.
	Columns []TSVectorColumn
}

// BuildTSVector builds a to_tsvector @@ plainto_tsquery search predicate.
func BuildTSVector(query string, placeholder int, language string, columns ...string) (Fragment, error) {
	return BuildTSVectorSearch(TSVectorOptions{
		Query:       query,
		Placeholder: placeholder,
		Language:    language,
		QueryMode:   TSQueryPlain,
		Columns:     unweightedTSVectorColumns(columns),
	})
}

// BuildWebsearchTSVector builds a to_tsvector @@ websearch_to_tsquery search
// predicate.
func BuildWebsearchTSVector(query string, placeholder int, language string, columns ...string) (Fragment, error) {
	return BuildTSVectorSearch(TSVectorOptions{
		Query:       query,
		Placeholder: placeholder,
		Language:    language,
		QueryMode:   TSQueryWebsearch,
		Columns:     unweightedTSVectorColumns(columns),
	})
}

// BuildTSVectorSearch returns a PostgreSQL full-text search predicate.
//
// The returned SQL has the shape "(<to_tsvector> @@ <tsquery>)" and binds the
// trimmed query as its single argument. Column identifiers and language names
// are validated before being interpolated into SQL.
func BuildTSVectorSearch(opts TSVectorOptions) (Fragment, error) {
	query := strings.TrimSpace(opts.Query)
	if query == "" {
		return Fragment{}, nil
	}

	sql, err := TSVectorSearchExpression(opts.Language, opts.Placeholder, opts.QueryMode, opts.Columns)
	if err != nil {
		return Fragment{}, err
	}
	return Fragment{
		SQL:  sql,
		Args: []any{query},
	}, nil
}

// BuildTSVectorRank returns a ts_rank expression for the configured search.
func BuildTSVectorRank(opts TSVectorOptions) (Fragment, error) {
	query := strings.TrimSpace(opts.Query)
	if query == "" {
		return Fragment{}, nil
	}

	sql, err := TSVectorRankExpression(opts.Language, opts.Placeholder, opts.QueryMode, opts.Columns)
	if err != nil {
		return Fragment{}, err
	}
	return Fragment{
		SQL:  sql,
		Args: []any{query},
	}, nil
}

// BuildTSVectorOrderByRank returns a descending ts_rank ORDER BY expression.
func BuildTSVectorOrderByRank(opts TSVectorOptions) (Fragment, error) {
	fragment, err := BuildTSVectorRank(opts)
	if err != nil {
		return Fragment{}, err
	}
	if fragment.SQL != "" {
		fragment.SQL += " DESC"
	}
	return fragment, nil
}

// TSVectorSearchExpression builds a "(to_tsvector @@ tsquery)" predicate.
func TSVectorSearchExpression(language string, placeholder int, mode TSQueryMode, columns []TSVectorColumn) (string, error) {
	query, err := tsQueryExpression(language, placeholder, mode)
	if err != nil {
		return "", err
	}
	vector, err := WeightedToTSVector(language, columns...)
	if err != nil {
		return "", err
	}
	return "(" + vector + " @@ " + query + ")", nil
}

// TSVectorRankExpression builds a ts_rank(to_tsvector, tsquery) expression.
func TSVectorRankExpression(language string, placeholder int, mode TSQueryMode, columns []TSVectorColumn) (string, error) {
	query, err := tsQueryExpression(language, placeholder, mode)
	if err != nil {
		return "", err
	}
	vector, err := WeightedToTSVector(language, columns...)
	if err != nil {
		return "", err
	}
	return "ts_rank(" + vector + ", " + query + ")", nil
}

// TSVectorOrderByRankExpression builds a descending ts_rank ORDER BY expression.
func TSVectorOrderByRankExpression(language string, placeholder int, mode TSQueryMode, columns []TSVectorColumn) (string, error) {
	rank, err := TSVectorRankExpression(language, placeholder, mode, columns)
	if err != nil {
		return "", err
	}
	return rank + " DESC", nil
}

// ToTSVector builds a PostgreSQL to_tsvector expression over unweighted columns.
func ToTSVector(language string, columns ...string) (string, error) {
	return WeightedToTSVector(language, unweightedTSVectorColumns(columns)...)
}

// WeightedToTSVector builds a PostgreSQL to_tsvector expression over weighted
// columns.
func WeightedToTSVector(language string, columns ...TSVectorColumn) (string, error) {
	languageSQL, err := tsLanguageSQL(language)
	if err != nil {
		return "", err
	}
	if len(columns) == 0 {
		return "", errEmptyColumns
	}

	weighted := false
	for _, column := range columns {
		if strings.TrimSpace(column.Weight) != "" {
			weighted = true
			break
		}
	}
	if weighted {
		return weightedToTSVector(languageSQL, columns)
	}

	expressions := make([]string, 0, len(columns))
	for _, column := range columns {
		expression, err := columnTextExpression(column.columnName())
		if err != nil {
			return "", err
		}
		expressions = append(expressions, expression)
	}
	return "to_tsvector(" + languageSQL + ", " + strings.Join(expressions, " || ' ' || ") + ")", nil
}

// PlainToTSQuery builds a PostgreSQL plainto_tsquery expression.
func PlainToTSQuery(language string, placeholder int) (string, error) {
	if placeholder <= 0 {
		return "", errInvalidPlaceholder
	}
	languageSQL, err := tsLanguageSQL(language)
	if err != nil {
		return "", err
	}
	return fmt.Sprintf("plainto_tsquery(%s, $%d)", languageSQL, placeholder), nil
}

// WebsearchToTSQuery builds a PostgreSQL websearch_to_tsquery expression.
func WebsearchToTSQuery(language string, placeholder int) (string, error) {
	if placeholder <= 0 {
		return "", errInvalidPlaceholder
	}
	languageSQL, err := tsLanguageSQL(language)
	if err != nil {
		return "", err
	}
	return fmt.Sprintf("websearch_to_tsquery(%s, $%d)", languageSQL, placeholder), nil
}

func unweightedTSVectorColumns(columns []string) []TSVectorColumn {
	out := make([]TSVectorColumn, 0, len(columns))
	for _, column := range columns {
		out = append(out, TSVectorColumn{Name: column})
	}
	return out
}

func weightedToTSVector(languageSQL string, columns []TSVectorColumn) (string, error) {
	expressions := make([]string, 0, len(columns))
	for _, column := range columns {
		textExpression, err := columnTextExpression(column.columnName())
		if err != nil {
			return "", err
		}
		vector := "to_tsvector(" + languageSQL + ", " + textExpression + ")"
		weight, err := normalizeTSWeight(column.Weight)
		if err != nil {
			return "", err
		}
		if weight != "" {
			vector = "setweight(" + vector + ", '" + weight + "')"
		}
		expressions = append(expressions, vector)
	}
	return strings.Join(expressions, " || "), nil
}

func (column TSVectorColumn) columnName() string {
	if column.Name != "" {
		return column.Name
	}
	return column.Column
}

func tsQueryExpression(language string, placeholder int, mode TSQueryMode) (string, error) {
	switch mode {
	case TSQueryPlain:
		return PlainToTSQuery(language, placeholder)
	case TSQueryWebsearch:
		return WebsearchToTSQuery(language, placeholder)
	default:
		return "", errInvalidTSQueryMode
	}
}

func columnTextExpression(column string) (string, error) {
	quoted, err := quoteDottedIdent(column)
	if err != nil {
		return "", err
	}
	return "coalesce(" + quoted + "::text, '')", nil
}

func quoteDottedIdent(name string) (string, error) {
	name = strings.TrimSpace(name)
	if name == "" {
		return "", errInvalidColumn
	}

	parts := strings.Split(name, ".")
	quoted := make([]string, 0, len(parts))
	for _, part := range parts {
		quotedPart, err := quoteIdent(part)
		if err != nil {
			return "", err
		}
		quoted = append(quoted, quotedPart)
	}
	return strings.Join(quoted, "."), nil
}

func tsLanguageSQL(language string) (string, error) {
	language = strings.TrimSpace(language)
	if language == "" {
		return "", errInvalidTSLanguage
	}
	parts := strings.Split(language, ".")
	if len(parts) > 2 {
		return "", errInvalidTSLanguage
	}
	for _, part := range parts {
		if !validTSIdentifier(part) {
			return "", errInvalidTSLanguage
		}
	}
	return "'" + language + "'::regconfig", nil
}

func validTSIdentifier(identifier string) bool {
	if identifier == "" {
		return false
	}
	for i := 0; i < len(identifier); i++ {
		c := identifier[i]
		if i == 0 {
			if !isTSIdentifierLetter(c) && c != '_' {
				return false
			}
			continue
		}
		if !isTSIdentifierLetter(c) && !isTSIdentifierDigit(c) && c != '_' {
			return false
		}
	}
	return true
}

func isTSIdentifierLetter(c byte) bool {
	return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')
}

func isTSIdentifierDigit(c byte) bool {
	return c >= '0' && c <= '9'
}

func normalizeTSWeight(weight string) (string, error) {
	weight = strings.ToUpper(strings.TrimSpace(weight))
	switch weight {
	case "":
		return "", nil
	case "A", "B", "C", "D":
		return weight, nil
	default:
		return "", errInvalidTSWeight
	}
}
