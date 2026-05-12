package search

import (
	"errors"
	"reflect"
	"testing"
)

func TestPatternEscapesLikeMetacharacters(t *testing.T) {
	tests := []struct {
		name  string
		query string
		want  string
	}{
		{name: "percent", query: "100%", want: `%100\%%`},
		{name: "underscore", query: "first_last", want: `%first\_last%`},
		{name: "backslash", query: `C:\tmp`, want: `%C:\\tmp%`},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := Pattern(tt.query, Contains); got != tt.want {
				t.Fatalf("Pattern(%q, Contains) = %q, want %q", tt.query, got, tt.want)
			}
		})
	}
}

func TestBuildLike(t *testing.T) {
	tests := []struct {
		name        string
		query       string
		placeholder int
		columns     []string
		want        Fragment
		wantErr     error
	}{
		{
			name:        "empty query returns no fragment",
			query:       "",
			placeholder: 1,
			columns:     []string{"name"},
			want:        Fragment{},
		},
		{
			name:        "whitespace query returns no fragment",
			query:       "   ",
			placeholder: 1,
			columns:     []string{"name"},
			want:        Fragment{},
		},
		{
			name:        "single column",
			query:       "beach",
			placeholder: 2,
			columns:     []string{"name"},
			want: Fragment{
				SQL:  `("name" LIKE $2 ESCAPE '\')`,
				Args: []any{"%beach%"},
			},
		},
		{
			name:        "multiple columns reuse one placeholder",
			query:       "garden",
			placeholder: 3,
			columns:     []string{"name", "description"},
			want: Fragment{
				SQL:  `("name" LIKE $3 ESCAPE '\' OR "description" LIKE $3 ESCAPE '\')`,
				Args: []any{"%garden%"},
			},
		},
		{
			name:        "missing columns",
			query:       "garden",
			placeholder: 1,
			wantErr:     errEmptyColumns,
		},
		{
			name:        "invalid column",
			query:       "garden",
			placeholder: 1,
			columns:     []string{"name; drop table properties"},
			wantErr:     errInvalidColumn,
		},
		{
			name:        "invalid placeholder",
			query:       "garden",
			placeholder: 0,
			columns:     []string{"name"},
			wantErr:     errInvalidPlaceholder,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := BuildLike(tt.query, tt.placeholder, tt.columns...)
			if tt.wantErr != nil {
				if !errors.Is(err, tt.wantErr) {
					t.Fatalf("BuildLike() error = %v, want %v", err, tt.wantErr)
				}
				return
			}
			if err != nil {
				t.Fatalf("BuildLike() error = %v", err)
			}
			if !reflect.DeepEqual(got, tt.want) {
				t.Fatalf("BuildLike() = %#v, want %#v", got, tt.want)
			}
		})
	}
}

func TestBuildLikeMode(t *testing.T) {
	tests := []struct {
		name string
		mode Mode
		want Fragment
	}{
		{
			name: "starts with",
			mode: StartsWith,
			want: Fragment{
				SQL:  `("name" LIKE $1 ESCAPE '\')`,
				Args: []any{`pool\_%`},
			},
		},
		{
			name: "exact",
			mode: Exact,
			want: Fragment{
				SQL:  `("name" LIKE $1 ESCAPE '\')`,
				Args: []any{`pool\_`},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := BuildLikeMode("pool_", 1, tt.mode, "name")
			if err != nil {
				t.Fatalf("BuildLikeMode() error = %v", err)
			}
			if !reflect.DeepEqual(got, tt.want) {
				t.Fatalf("BuildLikeMode() = %#v, want %#v", got, tt.want)
			}
		})
	}
}
