// Package search contains SQL-backed search helpers for generated Lazuli
// query code.
package search

import (
	"errors"
	"fmt"
	"strings"
)

const defaultEscape = `\`

var (
	errEmptyColumns       = errors.New("lazuli/search: at least one search column is required")
	errInvalidColumn      = errors.New("lazuli/search: invalid search column")
	errInvalidPlaceholder = errors.New("lazuli/search: placeholder index must be positive")
)

// Mode controls how a LIKE pattern is shaped around the escaped query.
type Mode int

const (
	// Contains matches rows where a column contains the query text.
	Contains Mode = iota
	// StartsWith matches rows where a column starts with the query text.
	StartsWith
	// Exact matches rows where a column equals the escaped query text under LIKE.
	Exact
)

// Fragment is a SQL WHERE fragment and its bind arguments.
type Fragment struct {
	SQL  string
	Args []any
}

// EscapeLike escapes SQL LIKE metacharacters for patterns that use backslash
// as the ESCAPE character.
func EscapeLike(query string) string {
	return strings.NewReplacer(`\`, `\\`, `%`, `\%`, `_`, `\_`).Replace(query)
}

// Pattern escapes query and wraps it according to mode.
func Pattern(query string, mode Mode) string {
	escaped := EscapeLike(query)
	switch mode {
	case StartsWith:
		return escaped + "%"
	case Exact:
		return escaped
	default:
		return "%" + escaped + "%"
	}
}

// BuildLike returns a SQL LIKE fragment for searching query across columns.
//
// Empty or whitespace-only query returns an empty fragment and no arguments.
// The fragment reuses one placeholder for every column, so callers append the
// returned single argument exactly once. Placeholder is 1-based.
func BuildLike(query string, placeholder int, columns ...string) (Fragment, error) {
	return BuildLikeMode(query, placeholder, Contains, columns...)
}

// BuildLikeMode returns a SQL LIKE fragment for searching query across
// columns using the requested pattern mode.
func BuildLikeMode(query string, placeholder int, mode Mode, columns ...string) (Fragment, error) {
	query = strings.TrimSpace(query)
	if query == "" {
		return Fragment{}, nil
	}
	if placeholder <= 0 {
		return Fragment{}, errInvalidPlaceholder
	}
	if len(columns) == 0 {
		return Fragment{}, errEmptyColumns
	}

	placeholderToken := fmt.Sprintf("$%d", placeholder)
	ors := make([]string, 0, len(columns))
	for _, column := range columns {
		quoted, err := quoteIdent(column)
		if err != nil {
			return Fragment{}, err
		}
		ors = append(ors, fmt.Sprintf("%s LIKE %s ESCAPE '%s'", quoted, placeholderToken, defaultEscape))
	}

	return Fragment{
		SQL:  "(" + strings.Join(ors, " OR ") + ")",
		Args: []any{Pattern(query, mode)},
	}, nil
}

func quoteIdent(name string) (string, error) {
	if name == "" {
		return "", errInvalidColumn
	}
	for i, c := range name {
		ok := c == '_' || (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
			(i > 0 && c >= '0' && c <= '9')
		if !ok {
			return "", errInvalidColumn
		}
	}
	return `"` + name + `"`, nil
}
