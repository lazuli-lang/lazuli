package search

import (
	"errors"
	"reflect"
	"testing"
)

func TestTypesenseDescriptorNormalizeValidateAndPlan(t *testing.T) {
	descriptor := TypesenseDescriptor{
		Nodes: []TypesenseNode{
			{URL: " HTTPS://Search.Example.test:8108/ "},
			{URL: "https://search.example.test:8108/?x=1"},
			{URL: "http://Backup.Example.test:8108/path/"},
		},
		APIKey:    " clearly-invalid-placeholder-key ",
		BatchSize: 4,
		Collections: []TypesenseCollection{
			{
				Name:                "orders",
				DefaultSortingField: "created_at",
				BatchSize:           3,
				Fields: []TypesenseField{
					{Name: "status", Type: " STRING ", Facet: true, Index: true},
					{Name: "created_at", Type: "INT64", Sort: true},
					{Name: "total", Type: "float", Optional: true, Sort: true},
				},
			},
			{
				Name: "customers",
				Fields: []TypesenseField{
					{Name: "name", Type: "string", Index: true},
				},
			},
		},
	}

	normalized, err := descriptor.Normalize()
	if err != nil {
		t.Fatalf("Normalize() error = %v", err)
	}
	descriptor.Collections[0].Fields[0].Name = "mutated"

	if err := normalized.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	wantNodes := []TypesenseNode{
		{URL: "http://backup.example.test:8108/path"},
		{URL: "https://search.example.test:8108"},
	}
	if !reflect.DeepEqual(normalized.Nodes, wantNodes) {
		t.Fatalf("Nodes = %#v, want %#v", normalized.Nodes, wantNodes)
	}
	if normalized.APIKey != "clearly-invalid-placeholder-key" || normalized.BatchSize != 4 {
		t.Fatalf("descriptor metadata = api %q batch %d, want trimmed api and batch 4", normalized.APIKey, normalized.BatchSize)
	}

	wantCollections := []TypesenseCollection{
		{
			Name:      "customers",
			BatchSize: 0,
			Fields: []TypesenseField{
				{Name: "name", Type: "string", Index: true},
			},
		},
		{
			Name:                "orders",
			DefaultSortingField: "created_at",
			BatchSize:           3,
			Fields: []TypesenseField{
				{Name: "created_at", Type: "int64", Sort: true},
				{Name: "status", Type: "string", Facet: true, Index: true},
				{Name: "total", Type: "float", Optional: true, Sort: true},
			},
		},
	}
	if !reflect.DeepEqual(normalized.Collections, wantCollections) {
		t.Fatalf("Collections = %#v, want %#v", normalized.Collections, wantCollections)
	}

	plan, err := normalized.PlanBatch("orders", 7)
	if err != nil {
		t.Fatalf("PlanBatch() error = %v", err)
	}
	wantPlan := TypesenseBatchPlan{
		Collection: "orders",
		BatchSize:  3,
		Total:      7,
		Windows: []TypesenseBatchWindow{
			{Index: 1, Count: 3, Start: 0, End: 3, Offset: 0, Limit: 3},
			{Index: 2, Count: 3, Start: 3, End: 6, Offset: 3, Limit: 3},
			{Index: 3, Count: 3, Start: 6, End: 7, Offset: 6, Limit: 1},
		},
	}
	if !reflect.DeepEqual(plan, wantPlan) {
		t.Fatalf("PlanBatch() = %#v, want %#v", plan, wantPlan)
	}

	customerPlan, err := normalized.PlanBatch("customers", 5)
	if err != nil {
		t.Fatalf("PlanBatch(customers) error = %v", err)
	}
	if customerPlan.BatchSize != 4 || len(customerPlan.Windows) != 2 {
		t.Fatalf("customer plan = %#v, want descriptor batch size fallback with two windows", customerPlan)
	}
}

func TestTypesenseRedactedSummary(t *testing.T) {
	descriptor := TypesenseDescriptor{
		Nodes: []TypesenseNode{
			{URL: "https://name:password@Search.Example.test:8108/collections?api_key=query"},
			{URL: ":// bad url"},
		},
		APIKey:    " clearly-invalid-placeholder-key ",
		BatchSize: 2,
		Collections: []TypesenseCollection{
			{
				Name: "events",
				Fields: []TypesenseField{
					{Name: "created_at", Type: "int64", Sort: true},
				},
				DefaultSortingField: "created_at",
			},
		},
	}

	summary := descriptor.RedactedSummary()
	if !summary.HasAPIKey || summary.APIKey != "[redacted]" {
		t.Fatalf("api key summary = %q/%v, want redacted true", summary.APIKey, summary.HasAPIKey)
	}
	wantNodes := []string{"https://search.example.test:8108", "[redacted]"}
	if !reflect.DeepEqual(summary.Nodes, wantNodes) {
		t.Fatalf("Nodes = %#v, want %#v", summary.Nodes, wantNodes)
	}
	if len(summary.Collections) != 1 || summary.Collections[0].FieldCount != 1 || summary.Collections[0].Fields[0].Name != "created_at" {
		t.Fatalf("Collections summary = %#v, want one field summary", summary.Collections)
	}

	summary.Collections[0].Fields[0].Name = "mutated"
	if descriptor.Collections[0].Fields[0].Name != "created_at" {
		t.Fatalf("RedactedSummary() shared field slice with descriptor")
	}
}

func TestTypesenseNodeURLAndAPIKeyRedaction(t *testing.T) {
	normalized, err := NormalizeTypesenseNodeURL(" HTTPS://name:password@Example.test:8108/path/?key=value#fragment ")
	if err != nil {
		t.Fatalf("NormalizeTypesenseNodeURL() error = %v", err)
	}
	if normalized != "https://example.test:8108/path" {
		t.Fatalf("NormalizeTypesenseNodeURL() = %q, want canonical url", normalized)
	}

	if got := RedactTypesenseNodeURL("https://name:password@example.test:8108/path?api_key=placeholder"); got != "https://example.test:8108" {
		t.Fatalf("RedactTypesenseNodeURL() = %q, want host-only redacted url", got)
	}
	if got := RedactTypesenseAPIKey(" clearly-invalid-placeholder-key "); got != "[redacted]" {
		t.Fatalf("RedactTypesenseAPIKey(non-empty) = %q, want [redacted]", got)
	}
	if got := RedactTypesenseAPIKey(" "); got != "" {
		t.Fatalf("RedactTypesenseAPIKey(empty) = %q, want empty", got)
	}
}

func TestTypesenseBatchHelpers(t *testing.T) {
	if got, err := NormalizeTypesenseBatchSize(0); err != nil || got != DefaultTypesenseBatchSize {
		t.Fatalf("NormalizeTypesenseBatchSize(0) = %d, %v, want default nil", got, err)
	}
	if _, err := NormalizeTypesenseBatchSize(MaxTypesenseBatchSize + 1); !errors.Is(err, ErrTypesenseDescriptorInvalid) {
		t.Fatalf("NormalizeTypesenseBatchSize(too large) error = %v, want ErrTypesenseDescriptorInvalid", err)
	}

	windows := PlanTypesenseBatchWindows(5, 2)
	want := []TypesenseBatchWindow{
		{Index: 1, Count: 3, Start: 0, End: 2, Offset: 0, Limit: 2},
		{Index: 2, Count: 3, Start: 2, End: 4, Offset: 2, Limit: 2},
		{Index: 3, Count: 3, Start: 4, End: 5, Offset: 4, Limit: 1},
	}
	if !reflect.DeepEqual(windows, want) {
		t.Fatalf("PlanTypesenseBatchWindows() = %#v, want %#v", windows, want)
	}
	if got := PlanTypesenseBatchWindows(0, 2); got != nil {
		t.Fatalf("PlanTypesenseBatchWindows(0, 2) = %#v, want nil", got)
	}
}

func TestTypesenseDescriptorRejectsInvalidInput(t *testing.T) {
	tests := []struct {
		name string
		run  func() error
	}{
		{
			name: "missing node",
			run: func() error {
				return ValidateTypesenseDescriptor(TypesenseDescriptor{
					APIKey: "clearly-invalid-placeholder-key",
					Collections: []TypesenseCollection{{
						Name:   "events",
						Fields: []TypesenseField{{Name: "title", Type: "string"}},
					}},
				})
			},
		},
		{
			name: "missing api key",
			run: func() error {
				return ValidateTypesenseDescriptor(TypesenseDescriptor{
					Nodes: []TypesenseNode{{URL: "https://search.example.test:8108"}},
					Collections: []TypesenseCollection{{
						Name:   "events",
						Fields: []TypesenseField{{Name: "title", Type: "string"}},
					}},
				})
			},
		},
		{
			name: "missing collection",
			run: func() error {
				return ValidateTypesenseDescriptor(TypesenseDescriptor{
					Nodes:  []TypesenseNode{{URL: "https://search.example.test:8108"}},
					APIKey: "clearly-invalid-placeholder-key",
				})
			},
		},
		{
			name: "invalid node scheme",
			run: func() error {
				_, err := NormalizeTypesenseNodeURL("ftp://search.example.test:8108")
				return err
			},
		},
		{
			name: "duplicate collection",
			run: func() error {
				_, err := NormalizeTypesenseCollections([]TypesenseCollection{
					{Name: "events", Fields: []TypesenseField{{Name: "title", Type: "string"}}},
					{Name: " events ", Fields: []TypesenseField{{Name: "created_at", Type: "int64"}}},
				})
				return err
			},
		},
		{
			name: "missing fields",
			run: func() error {
				_, err := NormalizeTypesenseCollections([]TypesenseCollection{{Name: "events"}})
				return err
			},
		},
		{
			name: "invalid field type",
			run: func() error {
				_, err := NormalizeTypesenseFields([]TypesenseField{{Name: "title", Type: "bytes"}})
				return err
			},
		},
		{
			name: "duplicate field",
			run: func() error {
				_, err := NormalizeTypesenseFields([]TypesenseField{
					{Name: "title", Type: "string"},
					{Name: " title ", Type: "string"},
				})
				return err
			},
		},
		{
			name: "unknown default sorting field",
			run: func() error {
				_, err := NormalizeTypesenseCollections([]TypesenseCollection{{
					Name:                "events",
					DefaultSortingField: "missing",
					Fields:              []TypesenseField{{Name: "title", Type: "string"}},
				}})
				return err
			},
		},
		{
			name: "non-sortable default sorting field",
			run: func() error {
				_, err := NormalizeTypesenseCollections([]TypesenseCollection{{
					Name:                "events",
					DefaultSortingField: "title",
					Fields:              []TypesenseField{{Name: "title", Type: "string"}},
				}})
				return err
			},
		},
		{
			name: "negative batch",
			run: func() error {
				_, err := NormalizeTypesenseBatchSize(-1)
				return err
			},
		},
		{
			name: "unknown plan collection",
			run: func() error {
				_, err := validTypesenseDescriptor().PlanBatch("missing", 1)
				return err
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := tt.run(); !errors.Is(err, ErrTypesenseDescriptorInvalid) {
				t.Fatalf("%s error = %v, want ErrTypesenseDescriptorInvalid", tt.name, err)
			}
		})
	}
}

func validTypesenseDescriptor() TypesenseDescriptor {
	return TypesenseDescriptor{
		Nodes:  []TypesenseNode{{URL: "https://search.example.test:8108"}},
		APIKey: "clearly-invalid-placeholder-key",
		Collections: []TypesenseCollection{{
			Name:   "events",
			Fields: []TypesenseField{{Name: "created_at", Type: "int64"}},
		}},
	}
}
