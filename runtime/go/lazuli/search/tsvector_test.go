package search

import (
	"errors"
	"reflect"
	"testing"
)

func TestToTSVectorBuildsUnweightedExpression(t *testing.T) {
	got, err := ToTSVector("english", "title", "post.body")
	if err != nil {
		t.Fatalf("ToTSVector() error = %v", err)
	}

	want := `to_tsvector('english'::regconfig, coalesce("title"::text, '') || ' ' || coalesce("post"."body"::text, ''))`
	if got != want {
		t.Fatalf("ToTSVector() = %q, want %q", got, want)
	}
}

func TestWeightedToTSVectorBuildsWeightedExpression(t *testing.T) {
	got, err := WeightedToTSVector("portuguese",
		TSVectorColumn{Name: "title", Weight: "A"},
		TSVectorColumn{Column: "summary", Weight: "b"},
		TSVectorColumn{Name: "body"},
	)
	if err != nil {
		t.Fatalf("WeightedToTSVector() error = %v", err)
	}

	want := `setweight(to_tsvector('portuguese'::regconfig, coalesce("title"::text, '')), 'A') || setweight(to_tsvector('portuguese'::regconfig, coalesce("summary"::text, '')), 'B') || to_tsvector('portuguese'::regconfig, coalesce("body"::text, ''))`
	if got != want {
		t.Fatalf("WeightedToTSVector() = %q, want %q", got, want)
	}
}

func TestWeightedToTSVectorAcceptsColumnAliasWithoutWeights(t *testing.T) {
	got, err := WeightedToTSVector("english", TSVectorColumn{Column: "title"})
	if err != nil {
		t.Fatalf("WeightedToTSVector() error = %v", err)
	}

	want := `to_tsvector('english'::regconfig, coalesce("title"::text, ''))`
	if got != want {
		t.Fatalf("WeightedToTSVector() = %q, want %q", got, want)
	}
}

func TestTSQueryExpressions(t *testing.T) {
	plain, err := PlainToTSQuery("simple", 2)
	if err != nil {
		t.Fatalf("PlainToTSQuery() error = %v", err)
	}
	if want := `plainto_tsquery('simple'::regconfig, $2)`; plain != want {
		t.Fatalf("PlainToTSQuery() = %q, want %q", plain, want)
	}

	websearch, err := WebsearchToTSQuery("simple", 3)
	if err != nil {
		t.Fatalf("WebsearchToTSQuery() error = %v", err)
	}
	if want := `websearch_to_tsquery('simple'::regconfig, $3)`; websearch != want {
		t.Fatalf("WebsearchToTSQuery() = %q, want %q", websearch, want)
	}
}

func TestBuildTSVectorSearch(t *testing.T) {
	got, err := BuildTSVectorSearch(TSVectorOptions{
		Query:       "  beach house  ",
		Placeholder: 4,
		Language:    "english",
		QueryMode:   TSQueryWebsearch,
		Columns: []TSVectorColumn{
			{Name: "title", Weight: "A"},
			{Name: "body", Weight: "D"},
		},
	})
	if err != nil {
		t.Fatalf("BuildTSVectorSearch() error = %v", err)
	}

	want := Fragment{
		SQL:  `(setweight(to_tsvector('english'::regconfig, coalesce("title"::text, '')), 'A') || setweight(to_tsvector('english'::regconfig, coalesce("body"::text, '')), 'D') @@ websearch_to_tsquery('english'::regconfig, $4))`,
		Args: []any{"beach house"},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("BuildTSVectorSearch() = %#v, want %#v", got, want)
	}
}

func TestBuildTSVectorConvenienceBuilders(t *testing.T) {
	plain, err := BuildTSVector("pool", 1, "english", "title", "body")
	if err != nil {
		t.Fatalf("BuildTSVector() error = %v", err)
	}
	wantPlain := Fragment{
		SQL:  `(to_tsvector('english'::regconfig, coalesce("title"::text, '') || ' ' || coalesce("body"::text, '')) @@ plainto_tsquery('english'::regconfig, $1))`,
		Args: []any{"pool"},
	}
	if !reflect.DeepEqual(plain, wantPlain) {
		t.Fatalf("BuildTSVector() = %#v, want %#v", plain, wantPlain)
	}

	websearch, err := BuildWebsearchTSVector("pool deck", 2, "english", "title")
	if err != nil {
		t.Fatalf("BuildWebsearchTSVector() error = %v", err)
	}
	wantWebsearch := Fragment{
		SQL:  `(to_tsvector('english'::regconfig, coalesce("title"::text, '')) @@ websearch_to_tsquery('english'::regconfig, $2))`,
		Args: []any{"pool deck"},
	}
	if !reflect.DeepEqual(websearch, wantWebsearch) {
		t.Fatalf("BuildWebsearchTSVector() = %#v, want %#v", websearch, wantWebsearch)
	}
}

func TestTSVectorRankExpressions(t *testing.T) {
	opts := TSVectorOptions{
		Query:       "garden",
		Placeholder: 5,
		Language:    "pg_catalog.english",
		QueryMode:   TSQueryPlain,
		Columns:     []TSVectorColumn{{Name: "listing.title"}},
	}

	rank, err := BuildTSVectorRank(opts)
	if err != nil {
		t.Fatalf("BuildTSVectorRank() error = %v", err)
	}
	wantRank := Fragment{
		SQL:  `ts_rank(to_tsvector('pg_catalog.english'::regconfig, coalesce("listing"."title"::text, '')), plainto_tsquery('pg_catalog.english'::regconfig, $5))`,
		Args: []any{"garden"},
	}
	if !reflect.DeepEqual(rank, wantRank) {
		t.Fatalf("BuildTSVectorRank() = %#v, want %#v", rank, wantRank)
	}

	order, err := BuildTSVectorOrderByRank(opts)
	if err != nil {
		t.Fatalf("BuildTSVectorOrderByRank() error = %v", err)
	}
	wantOrder := Fragment{
		SQL:  wantRank.SQL + " DESC",
		Args: []any{"garden"},
	}
	if !reflect.DeepEqual(order, wantOrder) {
		t.Fatalf("BuildTSVectorOrderByRank() = %#v, want %#v", order, wantOrder)
	}
}

func TestBuildTSVectorSearchEmptyQueryReturnsNoFragment(t *testing.T) {
	got, err := BuildTSVectorSearch(TSVectorOptions{
		Query:       "   ",
		Placeholder: 0,
		Language:    "not valid",
	})
	if err != nil {
		t.Fatalf("BuildTSVectorSearch() error = %v", err)
	}
	if !reflect.DeepEqual(got, Fragment{}) {
		t.Fatalf("BuildTSVectorSearch() = %#v, want empty fragment", got)
	}
}

func TestTSVectorBuildersRejectInvalidInputs(t *testing.T) {
	tests := []struct {
		name    string
		build   func() (Fragment, error)
		wantErr error
	}{
		{
			name: "missing columns",
			build: func() (Fragment, error) {
				return BuildTSVector("pool", 1, "english")
			},
			wantErr: errEmptyColumns,
		},
		{
			name: "invalid column",
			build: func() (Fragment, error) {
				return BuildTSVector("pool", 1, "english", "title;drop")
			},
			wantErr: errInvalidColumn,
		},
		{
			name: "empty dotted column segment",
			build: func() (Fragment, error) {
				return BuildTSVector("pool", 1, "english", "listing..title")
			},
			wantErr: errInvalidColumn,
		},
		{
			name: "invalid language",
			build: func() (Fragment, error) {
				return BuildTSVector("pool", 1, "english';drop", "title")
			},
			wantErr: errInvalidTSLanguage,
		},
		{
			name: "invalid placeholder",
			build: func() (Fragment, error) {
				return BuildTSVector("pool", 0, "english", "title")
			},
			wantErr: errInvalidPlaceholder,
		},
		{
			name: "invalid weight",
			build: func() (Fragment, error) {
				return BuildTSVectorSearch(TSVectorOptions{
					Query:       "pool",
					Placeholder: 1,
					Language:    "english",
					Columns:     []TSVectorColumn{{Name: "title", Weight: "Z"}},
				})
			},
			wantErr: errInvalidTSWeight,
		},
		{
			name: "invalid query mode",
			build: func() (Fragment, error) {
				return BuildTSVectorSearch(TSVectorOptions{
					Query:       "pool",
					Placeholder: 1,
					Language:    "english",
					QueryMode:   TSQueryMode(99),
					Columns:     []TSVectorColumn{{Name: "title"}},
				})
			},
			wantErr: errInvalidTSQueryMode,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := tt.build()
			if !errors.Is(err, tt.wantErr) {
				t.Fatalf("%s error = %v, want %v", tt.name, err, tt.wantErr)
			}
		})
	}
}
