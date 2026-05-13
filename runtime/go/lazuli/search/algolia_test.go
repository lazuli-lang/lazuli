package search

import (
	"errors"
	"reflect"
	"testing"
)

func TestNormalizeAlgoliaDescriptorIsDeterministic(t *testing.T) {
	descriptor := AlgoliaDescriptor{
		AppID:         " demoapp ",
		IndexName:     " products ",
		APIKey:        " invalid-api-key ",
		EndpointURL:   "HTTPS://Example.INVALID/search/?debug=true#fragment",
		SyncBatchSize: 3,
		Ranking:       []string{" typo ", " custom(desc(popularity)) "},
		Replicas: []AlgoliaReplica{
			{Name: "products_price_desc", Ranking: []string{"desc(price)"}},
			{Name: " products_price_asc ", Ranking: []string{" asc(price) "}},
		},
		Facets: []AlgoliaFacet{
			{Attribute: "category", Searchable: true},
			{Attribute: "brand", FilterOnly: true},
		},
	}

	normalized, err := NormalizeAlgoliaDescriptor(descriptor)
	if err != nil {
		t.Fatalf("NormalizeAlgoliaDescriptor() error = %v", err)
	}
	descriptor.Ranking[0] = "mutated"
	descriptor.Replicas[0].Name = "mutated"
	descriptor.Facets[0].Attribute = "mutated"

	if normalized.AppID != "DEMOAPP" ||
		normalized.IndexName != "products" ||
		normalized.APIKey != "invalid-api-key" ||
		normalized.EndpointURL != "https://example.invalid/search" ||
		normalized.SyncBatchSize != 3 {
		t.Fatalf("normalized descriptor metadata = %#v, want canonical fields", normalized)
	}

	wantReplicas := []AlgoliaReplica{
		{Name: "products_price_asc", Ranking: []string{"asc(price)"}},
		{Name: "products_price_desc", Ranking: []string{"desc(price)"}},
	}
	if !reflect.DeepEqual(normalized.Replicas, wantReplicas) {
		t.Fatalf("Replicas = %#v, want %#v", normalized.Replicas, wantReplicas)
	}
	if !reflect.DeepEqual(normalized.Ranking, []string{"typo", "custom(desc(popularity))"}) {
		t.Fatalf("Ranking = %#v, want trimmed authored order", normalized.Ranking)
	}
	wantFacets := []AlgoliaFacet{
		{Attribute: "brand", FilterOnly: true},
		{Attribute: "category", Searchable: true},
	}
	if !reflect.DeepEqual(normalized.Facets, wantFacets) {
		t.Fatalf("Facets = %#v, want %#v", normalized.Facets, wantFacets)
	}
}

func TestAlgoliaDescriptorPlanSync(t *testing.T) {
	descriptor := AlgoliaDescriptor{
		AppID:         "demoapp",
		IndexName:     "products",
		APIKey:        "invalid-api-key",
		SyncBatchSize: 3,
	}

	plan, err := descriptor.PlanSync(7)
	if err != nil {
		t.Fatalf("PlanSync() error = %v", err)
	}

	want := AlgoliaSyncPlan{
		IndexName:     "products",
		TotalRecords:  7,
		SyncBatchSize: 3,
		BatchCount:    3,
		Batches: []AlgoliaSyncBatch{
			{Index: 1, Count: 3, Start: 0, End: 3, Offset: 0, Limit: 3},
			{Index: 2, Count: 3, Start: 3, End: 6, Offset: 3, Limit: 3},
			{Index: 3, Count: 3, Start: 6, End: 7, Offset: 6, Limit: 1},
		},
	}
	if !reflect.DeepEqual(plan, want) {
		t.Fatalf("PlanSync() = %#v, want %#v", plan, want)
	}

	if batches := PlanAlgoliaSyncBatches(0, 3); batches != nil {
		t.Fatalf("PlanAlgoliaSyncBatches(0, 3) = %#v, want nil", batches)
	}
	if size, err := NormalizeAlgoliaSyncBatchSize(0); err != nil || size != DefaultAlgoliaSyncBatchSize {
		t.Fatalf("NormalizeAlgoliaSyncBatchSize(0) = %d, %v; want default", size, err)
	}
}

func TestAlgoliaRedactedSummary(t *testing.T) {
	descriptor := AlgoliaDescriptor{
		AppID:         "demoapp",
		IndexName:     "products",
		APIKey:        "invalid-api-key",
		EndpointURL:   "https://user:pass@example.invalid/search?apiKey=invalid",
		SyncBatchSize: 25,
		Ranking:       []string{"typo"},
		Replicas:      []AlgoliaReplica{{Name: "products_recent"}},
		Facets:        []AlgoliaFacet{{Attribute: "category"}},
	}

	summary := descriptor.RedactedSummary()
	if summary.AppIDRedacted != "[redacted]" || summary.APIKey != "[redacted]" {
		t.Fatalf("redacted fields = %q/%q, want [redacted]", summary.AppIDRedacted, summary.APIKey)
	}
	if !summary.HasAppID || !summary.HasAPIKey {
		t.Fatalf("presence flags = %v/%v, want true/true", summary.HasAppID, summary.HasAPIKey)
	}
	if summary.IndexName != "products" || summary.EndpointURL != "https://example.invalid/search" {
		t.Fatalf("summary metadata = %#v, want normalized non-secret fields", summary)
	}
	if summary.ReplicaCount != 1 || summary.RankingCount != 1 || summary.FacetCount != 1 || summary.SyncBatchSize != 25 {
		t.Fatalf("summary counts = %#v, want one replica/ranking/facet and batch 25", summary)
	}
	if !reflect.DeepEqual(summary.Replicas, []string{"products_recent"}) ||
		!reflect.DeepEqual(summary.Ranking, []string{"typo"}) ||
		!reflect.DeepEqual(summary.Facets, []AlgoliaFacet{{Attribute: "category"}}) {
		t.Fatalf("summary slices = %#v, want safe metadata copies", summary)
	}
}

func TestValidateAlgoliaDescriptorRejectsInvalidInput(t *testing.T) {
	valid := AlgoliaDescriptor{
		AppID:         "demoapp",
		IndexName:     "products",
		APIKey:        "invalid-api-key",
		SyncBatchSize: DefaultAlgoliaSyncBatchSize,
	}

	tests := []struct {
		name    string
		mutate  func(*AlgoliaDescriptor)
		wantErr error
	}{
		{
			name: "missing app id",
			mutate: func(d *AlgoliaDescriptor) {
				d.AppID = " "
			},
			wantErr: errAlgoliaAppIDRequired,
		},
		{
			name: "missing index",
			mutate: func(d *AlgoliaDescriptor) {
				d.IndexName = " "
			},
			wantErr: errAlgoliaIndexRequired,
		},
		{
			name: "missing api key",
			mutate: func(d *AlgoliaDescriptor) {
				d.APIKey = " "
			},
			wantErr: errAlgoliaAPIKeyRequired,
		},
		{
			name: "duplicate replica",
			mutate: func(d *AlgoliaDescriptor) {
				d.Replicas = []AlgoliaReplica{{Name: "products_by_price"}, {Name: " products_by_price "}}
			},
			wantErr: errDuplicateAlgoliaReplica,
		},
		{
			name: "duplicate facet",
			mutate: func(d *AlgoliaDescriptor) {
				d.Facets = []AlgoliaFacet{{Attribute: "brand"}, {Attribute: "Brand"}}
			},
			wantErr: errDuplicateAlgoliaFacet,
		},
		{
			name: "invalid facet attribute",
			mutate: func(d *AlgoliaDescriptor) {
				d.Facets = []AlgoliaFacet{{Attribute: "bad-name"}}
			},
			wantErr: errInvalidAlgoliaDescriptor,
		},
		{
			name: "duplicate ranking",
			mutate: func(d *AlgoliaDescriptor) {
				d.Ranking = []string{"typo", " TYPO "}
			},
			wantErr: errDuplicateAlgoliaRanking,
		},
		{
			name: "control character",
			mutate: func(d *AlgoliaDescriptor) {
				d.Ranking = []string{"typo\nwords"}
			},
			wantErr: errInvalidAlgoliaDescriptor,
		},
		{
			name: "batch too large",
			mutate: func(d *AlgoliaDescriptor) {
				d.SyncBatchSize = MaxAlgoliaSyncBatchSize + 1
			},
			wantErr: errInvalidAlgoliaSyncBatch,
		},
		{
			name: "invalid endpoint",
			mutate: func(d *AlgoliaDescriptor) {
				d.EndpointURL = "ftp://example.invalid"
			},
			wantErr: errInvalidAlgoliaEndpointURL,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			descriptor := valid
			tt.mutate(&descriptor)
			if err := ValidateAlgoliaDescriptor(descriptor); !errors.Is(err, tt.wantErr) {
				t.Fatalf("ValidateAlgoliaDescriptor() error = %v, want %v", err, tt.wantErr)
			}
		})
	}
}
