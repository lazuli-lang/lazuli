package search

import (
	"errors"
	"reflect"
	"testing"
)

func TestNormalizeFacetFieldsValidatesAllowedFieldsAndPreservesOrder(t *testing.T) {
	got, err := NormalizeFacetFields(
		[]FacetField{
			{Column: "lifecycle_stage"},
			{Name: "tier", Limit: 10},
		},
		"tier",
		"lifecycle_stage",
	)
	if err != nil {
		t.Fatalf("NormalizeFacetFields() error = %v", err)
	}

	want := []FacetField{
		{Name: "lifecycle_stage"},
		{Name: "tier", Limit: 10},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("NormalizeFacetFields() = %#v, want %#v", got, want)
	}
}

func TestNormalizeFacetFieldsRejectsInvalidInputs(t *testing.T) {
	tests := []struct {
		name    string
		fields  []FacetField
		allowed []string
		wantErr error
	}{
		{
			name:    "empty fields",
			wantErr: errEmptyFacetFields,
		},
		{
			name:    "invalid field",
			fields:  []FacetField{{Name: "tier;drop"}},
			wantErr: errInvalidColumn,
		},
		{
			name:    "unknown field",
			fields:  []FacetField{{Name: "tier"}},
			allowed: []string{"lifecycle_stage"},
			wantErr: errUnknownFacetField,
		},
		{
			name:    "duplicate field",
			fields:  []FacetField{{Name: "tier"}, {Column: "tier"}},
			wantErr: errDuplicateFacetField,
		},
		{
			name:    "negative limit",
			fields:  []FacetField{{Name: "tier", Limit: -1}},
			wantErr: errInvalidFacetLimit,
		},
		{
			name:    "invalid allowed field",
			fields:  []FacetField{{Name: "tier"}},
			allowed: []string{"not valid"},
			wantErr: errInvalidColumn,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateFacetFields(tt.fields, tt.allowed...)
			if !errors.Is(err, tt.wantErr) {
				t.Fatalf("ValidateFacetFields() error = %v, want %v", err, tt.wantErr)
			}
		})
	}
}

func TestBuildFacetAggregationBuildsDeterministicFragments(t *testing.T) {
	got, err := BuildFacetAggregation(FacetField{Name: "listing.tier", Limit: 5})
	if err != nil {
		t.Fatalf("BuildFacetAggregation() error = %v", err)
	}

	want := FacetAggregation{
		Field:      FacetField{Name: "listing.tier", Limit: 5},
		ValueSQL:   `"listing"."tier" AS facet_value`,
		CountSQL:   "COUNT(*) AS facet_count",
		GroupBySQL: `"listing"."tier"`,
		OrderBySQL: "facet_count DESC, facet_value ASC",
		Limit:      5,
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("BuildFacetAggregation() = %#v, want %#v", got, want)
	}
}

func TestBuildFacetAggregationQueryBuildsBucketSelect(t *testing.T) {
	where, err := BuildLike("beach", 1, "name")
	if err != nil {
		t.Fatalf("BuildLike() error = %v", err)
	}

	got, err := BuildFacetAggregationQuery(FacetAggregationOptions{
		Table: "customer",
		Field: FacetField{Name: "tier", Limit: 10},
		Where: where,
		Limit: 3,
	})
	if err != nil {
		t.Fatalf("BuildFacetAggregationQuery() error = %v", err)
	}

	want := Fragment{
		SQL:  `SELECT "tier" AS facet_value, COUNT(*) AS facet_count FROM "customer" WHERE ("name" LIKE $1 ESCAPE '\') GROUP BY "tier" ORDER BY facet_count DESC, facet_value ASC LIMIT 3`,
		Args: []any{"%beach%"},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("BuildFacetAggregationQuery() = %#v, want %#v", got, want)
	}
}

func TestBuildFacetAggregationQueryUsesDefaultLimit(t *testing.T) {
	got, err := BuildFacetAggregationQuery(FacetAggregationOptions{
		Table: "customer",
		Field: FacetField{Name: "tier"},
	})
	if err != nil {
		t.Fatalf("BuildFacetAggregationQuery() error = %v", err)
	}

	wantSQL := `SELECT "tier" AS facet_value, COUNT(*) AS facet_count FROM "customer" GROUP BY "tier" ORDER BY facet_count DESC, facet_value ASC LIMIT 20`
	if got.SQL != wantSQL {
		t.Fatalf("BuildFacetAggregationQuery().SQL = %q, want %q", got.SQL, wantSQL)
	}
}

func TestNormalizeRankBoostsSortsDeterministically(t *testing.T) {
	got, err := NormalizeRankBoosts(
		[]RankBoost{
			{Field: "email", Weight: 1},
			{Column: "name", Weight: 2},
			{Field: "nickname", Weight: 2},
		},
		"name",
		"email",
		"nickname",
	)
	if err != nil {
		t.Fatalf("NormalizeRankBoosts() error = %v", err)
	}

	want := []RankBoost{
		{Field: "name", Weight: 2},
		{Field: "nickname", Weight: 2},
		{Field: "email", Weight: 1},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("NormalizeRankBoosts() = %#v, want %#v", got, want)
	}
}

func TestBuildRankBoostBuildsStableCaseExpression(t *testing.T) {
	got, err := BuildRankBoost(RankBoostConfig{
		Query:       " pool_ ",
		Placeholder: 2,
		Mode:        StartsWith,
		Boosts: []RankBoost{
			{Field: "email", Weight: 1},
			{Field: "name", Weight: 2},
		},
	})
	if err != nil {
		t.Fatalf("BuildRankBoost() error = %v", err)
	}

	want := Fragment{
		SQL:  `(CASE WHEN "name" LIKE $2 ESCAPE '\' THEN 2 ELSE 0 END + CASE WHEN "email" LIKE $2 ESCAPE '\' THEN 1 ELSE 0 END)`,
		Args: []any{`pool\_%`},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("BuildRankBoost() = %#v, want %#v", got, want)
	}
}

func TestBuildRankBoostOrderAddsTieBreakers(t *testing.T) {
	got, err := BuildRankBoostOrder(RankBoostConfig{
		Query:       "garden",
		Placeholder: 4,
		Boosts: []RankBoost{
			{Field: "name", Weight: 2},
			{Field: "email", Weight: 1},
		},
		TieBreakers: []RankOrderField{
			{Field: "created_at", Desc: true},
			{Column: "id"},
		},
	})
	if err != nil {
		t.Fatalf("BuildRankBoostOrder() error = %v", err)
	}

	want := Fragment{
		SQL:  `(CASE WHEN "name" LIKE $4 ESCAPE '\' THEN 2 ELSE 0 END + CASE WHEN "email" LIKE $4 ESCAPE '\' THEN 1 ELSE 0 END) DESC, "created_at" DESC, "id" ASC`,
		Args: []any{"%garden%"},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("BuildRankBoostOrder() = %#v, want %#v", got, want)
	}
}

func TestRankBoostBuildersRejectInvalidInputs(t *testing.T) {
	tests := []struct {
		name    string
		build   func() (Fragment, error)
		wantErr error
	}{
		{
			name: "invalid placeholder",
			build: func() (Fragment, error) {
				return BuildRankBoost(RankBoostConfig{
					Query:       "garden",
					Placeholder: 0,
					Boosts:      []RankBoost{{Field: "name", Weight: 1}},
				})
			},
			wantErr: errInvalidPlaceholder,
		},
		{
			name: "empty boosts",
			build: func() (Fragment, error) {
				return BuildRankBoost(RankBoostConfig{
					Query:       "garden",
					Placeholder: 1,
				})
			},
			wantErr: errEmptyRankBoosts,
		},
		{
			name: "invalid boost field",
			build: func() (Fragment, error) {
				return BuildRankBoost(RankBoostConfig{
					Query:       "garden",
					Placeholder: 1,
					Boosts:      []RankBoost{{Field: "name;drop", Weight: 1}},
				})
			},
			wantErr: errInvalidColumn,
		},
		{
			name: "invalid boost weight",
			build: func() (Fragment, error) {
				return BuildRankBoost(RankBoostConfig{
					Query:       "garden",
					Placeholder: 1,
					Boosts:      []RankBoost{{Field: "name"}},
				})
			},
			wantErr: errInvalidRankBoost,
		},
		{
			name: "duplicate boost",
			build: func() (Fragment, error) {
				return BuildRankBoost(RankBoostConfig{
					Query:       "garden",
					Placeholder: 1,
					Boosts: []RankBoost{
						{Field: "name", Weight: 1},
						{Column: "name", Weight: 2},
					},
				})
			},
			wantErr: errDuplicateRankBoost,
		},
		{
			name: "invalid tie breaker",
			build: func() (Fragment, error) {
				return BuildRankBoostOrder(RankBoostConfig{
					Query:       "garden",
					Placeholder: 1,
					Boosts:      []RankBoost{{Field: "name", Weight: 1}},
					TieBreakers: []RankOrderField{{Field: "created-at"}},
				})
			},
			wantErr: errInvalidColumn,
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

func TestRankBoostEmptyQueryReturnsNoFragment(t *testing.T) {
	got, err := BuildRankBoost(RankBoostConfig{
		Query:       "   ",
		Placeholder: 0,
		Boosts:      []RankBoost{{Field: "not valid"}},
	})
	if err != nil {
		t.Fatalf("BuildRankBoost() error = %v", err)
	}
	if !reflect.DeepEqual(got, Fragment{}) {
		t.Fatalf("BuildRankBoost() = %#v, want empty fragment", got)
	}
}
