package search

import (
	"html"
	"strings"
	"unicode/utf8"
)

const (
	defaultHighlightPreTag  = "<mark>"
	defaultHighlightPostTag = "</mark>"
)

// HighlightOptions configures HighlightTerms.
//
// PreTag and PostTag are emitted as markup around escaped matching text. Empty
// tags use <mark> defaults. FragmentLimit caps the number of source runes shown
// around the first match; zero or negative values return the whole text.
type HighlightOptions struct {
	PreTag        string
	PostTag       string
	FragmentLimit int
}

// SynonymExpansion records the synonyms planned for one normalized term.
type SynonymExpansion struct {
	Term     string
	Synonyms []string
}

// SynonymExpansionPlan is a deterministic, in-memory synonym expansion result.
type SynonymExpansionPlan struct {
	Terms      []string
	Expansions []SynonymExpansion
}

type highlightMatch struct {
	start int
	end   int
}

// HighlightTerms returns HTML with matching terms wrapped in mark tags.
//
// Source text and matching text are HTML-escaped. Matching is case-insensitive
// after term normalization and works on UTF-8 rune boundaries.
func HighlightTerms(text string, terms []string, opts HighlightOptions) string {
	normalized := NormalizeTerms(terms)
	if opts.PreTag == "" {
		opts.PreTag = defaultHighlightPreTag
	}
	if opts.PostTag == "" {
		opts.PostTag = defaultHighlightPostTag
	}

	fragment, prefix, suffix := highlightFragment(text, normalized, opts.FragmentLimit)
	return prefix + renderHighlighted(fragment, normalized, opts.PreTag, opts.PostTag) + suffix
}

// NormalizeTerms trims, collapses internal whitespace, lowercases, and
// deduplicates search terms while preserving first-seen order.
func NormalizeTerms(terms []string) []string {
	normalized := make([]string, 0, len(terms))
	seen := make(map[string]struct{}, len(terms))
	for _, term := range terms {
		fields := strings.Fields(term)
		if len(fields) == 0 {
			continue
		}
		clean := strings.ToLower(strings.Join(fields, " "))
		if _, ok := seen[clean]; ok {
			continue
		}
		seen[clean] = struct{}{}
		normalized = append(normalized, clean)
	}
	return normalized
}

// PlanSynonymExpansion expands normalized terms from the provided in-memory
// synonym map. It does not call or assume any search backend.
func PlanSynonymExpansion(terms []string, synonyms map[string][]string) SynonymExpansionPlan {
	baseTerms := NormalizeTerms(terms)
	plan := SynonymExpansionPlan{
		Terms: make([]string, 0, len(baseTerms)),
	}
	seen := make(map[string]struct{}, len(baseTerms))
	for _, term := range baseTerms {
		seen[term] = struct{}{}
		plan.Terms = append(plan.Terms, term)
	}

	lookup := normalizeSynonymMap(synonyms)
	for _, term := range baseTerms {
		expansion := SynonymExpansion{Term: term}
		for _, synonym := range lookup[term] {
			if _, ok := seen[synonym]; ok {
				continue
			}
			seen[synonym] = struct{}{}
			expansion.Synonyms = append(expansion.Synonyms, synonym)
			plan.Terms = append(plan.Terms, synonym)
		}
		if len(expansion.Synonyms) > 0 {
			plan.Expansions = append(plan.Expansions, expansion)
		}
	}

	return plan
}

func normalizeSynonymMap(synonyms map[string][]string) map[string][]string {
	lookup := make(map[string][]string, len(synonyms))
	for rawTerm, rawSynonyms := range synonyms {
		keys := NormalizeTerms([]string{rawTerm})
		if len(keys) == 0 {
			continue
		}
		key := keys[0]
		values := NormalizeTerms(rawSynonyms)
		if len(values) == 0 {
			continue
		}
		lookup[key] = append(lookup[key], values...)
	}
	return lookup
}

func highlightFragment(text string, terms []string, limit int) (string, string, string) {
	if limit <= 0 || utf8.RuneCountInString(text) <= limit {
		return text, "", ""
	}

	runes := []rune(text)
	start := 0
	if matches := findHighlightMatches(text, terms); len(matches) > 0 {
		center := (matches[0].start + matches[0].end) / 2
		start = center - limit/2
		if start < 0 {
			start = 0
		}
		if start+limit > len(runes) {
			start = len(runes) - limit
		}
	}

	end := start + limit
	if start > 0 {
		for start < end && isSpaceRune(runes[start]) {
			start++
		}
	}
	if end < len(runes) {
		for end > start && isSpaceRune(runes[end-1]) {
			end--
		}
	}

	prefix := ""
	if start > 0 {
		prefix = "..."
	}
	suffix := ""
	if end < len(runes) {
		suffix = "..."
	}
	return string(runes[start:end]), prefix, suffix
}

func renderHighlighted(text string, terms []string, preTag string, postTag string) string {
	matches := findHighlightMatches(text, terms)
	if len(matches) == 0 {
		return html.EscapeString(text)
	}

	runes := []rune(text)
	var b strings.Builder
	cursor := 0
	for _, match := range matches {
		b.WriteString(html.EscapeString(string(runes[cursor:match.start])))
		b.WriteString(preTag)
		b.WriteString(html.EscapeString(string(runes[match.start:match.end])))
		b.WriteString(postTag)
		cursor = match.end
	}
	b.WriteString(html.EscapeString(string(runes[cursor:])))
	return b.String()
}

func findHighlightMatches(text string, terms []string) []highlightMatch {
	if text == "" || len(terms) == 0 {
		return nil
	}

	runes := []rune(text)
	lowerRunes := []rune(strings.ToLower(text))
	termRunes := make([][]rune, 0, len(terms))
	for _, term := range terms {
		if term == "" {
			continue
		}
		termRunes = append(termRunes, []rune(strings.ToLower(term)))
	}
	if len(termRunes) == 0 {
		return nil
	}

	matches := make([]highlightMatch, 0)
	for i := 0; i < len(runes); {
		bestEnd := 0
		for _, term := range termRunes {
			end := i + len(term)
			if end > len(lowerRunes) {
				continue
			}
			if equalRunes(lowerRunes[i:end], term) && end > bestEnd {
				bestEnd = end
			}
		}
		if bestEnd > 0 {
			matches = append(matches, highlightMatch{start: i, end: bestEnd})
			i = bestEnd
			continue
		}
		i++
	}
	return matches
}

func equalRunes(a []rune, b []rune) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func isSpaceRune(r rune) bool {
	return r == ' ' || r == '\t' || r == '\n' || r == '\r'
}
