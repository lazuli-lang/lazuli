package auth

import (
	"strings"
	"testing"

	"github.com/jackc/pgx/v5"
)

// TestQuoteTableIdentEscapesEmbeddedQuote is a regression guard for the
// former naive `"` + table + `"` implementation, which would have
// emitted a syntactically broken / injection-prone identifier when the
// table name itself contained a double-quote. quoteTableIdent now
// delegates to pgx.Identifier.Sanitize(), so an embedded `"` MUST be
// doubled (the SQL escape), not passed through.
func TestQuoteTableIdentEscapesEmbeddedQuote(t *testing.T) {
	cases := []struct {
		name  string
		table string
	}{
		{name: "simple", table: "user_session"},
		{name: "single embedded quote", table: `ab"cd`},
		{name: "injection attempt", table: `x"; DROP TABLE users; --`},
		{name: "trailing quote", table: `tbl"`},
		{name: "only quote", table: `"`},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := quoteTableIdent(tc.table)

			// Must match the library-correct sanitizer exactly (wire,
			// not a reimplementation).
			want := pgx.Identifier{tc.table}.Sanitize()
			if got != want {
				t.Fatalf("quoteTableIdent(%q) = %q, want %q (pgx.Identifier.Sanitize)", tc.table, got, want)
			}

			// Result is always wrapped in double-quotes.
			if !strings.HasPrefix(got, `"`) || !strings.HasSuffix(got, `"`) {
				t.Fatalf("quoteTableIdent(%q) = %q, expected double-quoted identifier", tc.table, got)
			}

			// Crucially, the naive wrapper would have produced
			// `"` + table + `"` verbatim. Assert we do NOT do that
			// whenever the name carries a double-quote: every embedded
			// `"` in the raw name must appear doubled in the output, so
			// the naive form is strictly shorter and unequal.
			if strings.Contains(tc.table, `"`) {
				naive := `"` + tc.table + `"`
				if got == naive {
					t.Fatalf("quoteTableIdent(%q) = %q is the naive (unescaped) wrapping; embedded quote not sanitized", tc.table, got)
				}

				rawQuotes := strings.Count(tc.table, `"`)
				// Sanitize escapes each inner `"` as `""`, then adds the
				// two surrounding quotes: total quotes = rawQuotes*2 + 2.
				wantQuotes := rawQuotes*2 + 2
				if gotQuotes := strings.Count(got, `"`); gotQuotes != wantQuotes {
					t.Fatalf("quoteTableIdent(%q) = %q has %d double-quotes, want %d (each embedded quote doubled)", tc.table, got, gotQuotes, wantQuotes)
				}
			}
		})
	}
}
