package search

import (
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestPlanMeilisearchDescriptorNormalizesMetadata(t *testing.T) {
	descriptor := MeilisearchDescriptor{
		Host:      " https://user:not-real@Search.EXAMPLE.test:7700/path?token=placeholder#frag ",
		APIKeyEnv: " MEILI_MASTER_KEY ",
		Indexes: []MeilisearchIndexDescriptor{
			{
				UID:        "orders",
				PrimaryKey: " order_id ",
				Attributes: MeilisearchIndexAttributes{
					Filterable: []string{" status ", "tenant_id", "status"},
					Sortable:   []string{"created_at", " total "},
					Searchable: []string{"notes", " customer.name "},
				},
			},
			{
				UID:        "customers",
				PrimaryKey: "id",
			},
		},
		BatchSize: 250,
	}

	plan, err := PlanMeilisearchDescriptor(descriptor)
	if err != nil {
		t.Fatalf("PlanMeilisearchDescriptor() error = %v", err)
	}
	descriptor.Indexes[0].UID = "mutated"

	if plan.IndexCount != 2 || plan.APIKeySource != "env:MEILI_MASTER_KEY" {
		t.Fatalf("plan metadata = %#v, want two indexes and env api key source", plan)
	}
	if plan.Batch.Size != 250 || plan.Batch.Defaulted || plan.Batch.Min != 1 || plan.Batch.Max != 10000 {
		t.Fatalf("plan batch = %#v, want requested size with bounds", plan.Batch)
	}

	wantIndexes := []MeilisearchIndexDescriptor{
		{UID: "customers", PrimaryKey: "id"},
		{
			UID:        "orders",
			PrimaryKey: "order_id",
			Attributes: MeilisearchIndexAttributes{
				Filterable: []string{"status", "tenant_id"},
				Sortable:   []string{"created_at", "total"},
				Searchable: []string{"customer.name", "notes"},
			},
		},
	}
	if !reflect.DeepEqual(plan.Descriptor.Indexes, wantIndexes) {
		t.Fatalf("normalized indexes = %#v, want %#v", plan.Descriptor.Indexes, wantIndexes)
	}
	if descriptor.Indexes[0].UID != "mutated" {
		t.Fatalf("PlanMeilisearchDescriptor() retained shared index slice")
	}
}

func TestMeilisearchRedactedSummary(t *testing.T) {
	plan, err := PlanMeilisearchDescriptor(MeilisearchDescriptor{
		Host:      "https://user:not-real@search.example.test:7700/indexes?token=placeholder#frag",
		APIKey:    "not-a-real-meili-key",
		BatchSize: 0,
		Indexes: []MeilisearchIndexDescriptor{
			{
				UID:        "products",
				PrimaryKey: "id",
				Attributes: MeilisearchIndexAttributes{
					Filterable: []string{"brand", "category"},
					Sortable:   []string{"price"},
					Searchable: []string{"name", "description"},
				},
			},
		},
	})
	if err != nil {
		t.Fatalf("PlanMeilisearchDescriptor() error = %v", err)
	}

	summary := plan.RedactedSummary()
	if summary.HostRedacted != "https://redacted@search.example.test:7700/indexes" {
		t.Fatalf("HostRedacted = %q, want redacted host without query or fragment", summary.HostRedacted)
	}
	if summary.APIKey != "[redacted]" || summary.APIKeySource != "inline" {
		t.Fatalf("api key summary = %q/%q, want redacted inline", summary.APIKey, summary.APIKeySource)
	}
	if summary.BatchSize != DefaultMeilisearchBatchSize || summary.IndexCount != 1 {
		t.Fatalf("summary counts = %#v, want default batch and one index", summary)
	}
	wantIndexes := []MeilisearchIndexSummary{
		{UID: "products", PrimaryKey: "id", FilterableCount: 2, SortableCount: 1, SearchableCount: 2},
	}
	if !reflect.DeepEqual(summary.Indexes, wantIndexes) {
		t.Fatalf("summary indexes = %#v, want %#v", summary.Indexes, wantIndexes)
	}
	if strings.Contains(summary.HostRedacted, "not-real") || strings.Contains(summary.HostRedacted, "token=") {
		t.Fatalf("HostRedacted leaked sensitive material: %q", summary.HostRedacted)
	}
}

func TestMeilisearchIndexUIDValidation(t *testing.T) {
	valid := []string{"products", "tenant_7", "orders-2026", "A0"}
	for _, uid := range valid {
		if err := ValidateMeilisearchIndexUID(uid); err != nil {
			t.Fatalf("ValidateMeilisearchIndexUID(%q) error = %v", uid, err)
		}
	}

	invalid := []string{"", "bad uid", "bad.uid", "bad/uid", "bad\nuid"}
	for _, uid := range invalid {
		if err := ValidateMeilisearchIndexUID(uid); !errors.Is(err, ErrMeilisearchDescriptorInvalid) {
			t.Fatalf("ValidateMeilisearchIndexUID(%q) error = %v, want ErrMeilisearchDescriptorInvalid", uid, err)
		}
	}
}

func TestPlanMeilisearchBatchSize(t *testing.T) {
	tests := []struct {
		name string
		size int
		want MeilisearchBatchPlan
	}{
		{
			name: "default",
			size: 0,
			want: MeilisearchBatchPlan{Size: 1000, Defaulted: true, Min: 1, Max: 10000},
		},
		{
			name: "minimum",
			size: 1,
			want: MeilisearchBatchPlan{Size: 1, Min: 1, Max: 10000, AtMinimum: true},
		},
		{
			name: "maximum",
			size: 10000,
			want: MeilisearchBatchPlan{Size: 10000, Min: 1, Max: 10000, AtMaximum: true},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := PlanMeilisearchBatchSize(tt.size)
			if err != nil {
				t.Fatalf("PlanMeilisearchBatchSize(%d) error = %v", tt.size, err)
			}
			if got != tt.want {
				t.Fatalf("PlanMeilisearchBatchSize(%d) = %#v, want %#v", tt.size, got, tt.want)
			}
		})
	}

	for _, size := range []int{-1, 10001} {
		if _, err := PlanMeilisearchBatchSize(size); !errors.Is(err, ErrMeilisearchDescriptorInvalid) {
			t.Fatalf("PlanMeilisearchBatchSize(%d) error = %v, want ErrMeilisearchDescriptorInvalid", size, err)
		}
	}
}

func TestMeilisearchDescriptorValidationRejectsInvalidMetadata(t *testing.T) {
	valid := MeilisearchDescriptor{
		Host:      "https://search.example.test",
		APIKey:    "not-a-real-meili-key",
		Indexes:   []MeilisearchIndexDescriptor{{UID: "products", PrimaryKey: "id"}},
		BatchSize: 100,
	}

	tests := []struct {
		name       string
		descriptor MeilisearchDescriptor
		fragment   string
	}{
		{
			name: "missing host",
			descriptor: func() MeilisearchDescriptor {
				d := valid
				d.Host = ""
				return d
			}(),
			fragment: "host is required",
		},
		{
			name: "unsupported host scheme",
			descriptor: func() MeilisearchDescriptor {
				d := valid
				d.Host = "ftp://search.example.test"
				return d
			}(),
			fragment: "unsupported",
		},
		{
			name: "both api key sources",
			descriptor: func() MeilisearchDescriptor {
				d := valid
				d.APIKeyEnv = "MEILI_MASTER_KEY"
				return d
			}(),
			fragment: "mutually exclusive",
		},
		{
			name: "missing index",
			descriptor: func() MeilisearchDescriptor {
				d := valid
				d.Indexes = nil
				return d
			}(),
			fragment: "at least one index",
		},
		{
			name: "duplicate index",
			descriptor: func() MeilisearchDescriptor {
				d := valid
				d.Indexes = []MeilisearchIndexDescriptor{{UID: "products"}, {UID: " products "}}
				return d
			}(),
			fragment: "duplicate index uid",
		},
		{
			name: "bad attribute",
			descriptor: func() MeilisearchDescriptor {
				d := valid
				d.Indexes = []MeilisearchIndexDescriptor{{
					UID: "products",
					Attributes: MeilisearchIndexAttributes{
						Filterable: []string{"status\nbad"},
					},
				}}
				return d
			}(),
			fragment: "control character",
		},
		{
			name: "bad batch",
			descriptor: func() MeilisearchDescriptor {
				d := valid
				d.BatchSize = 10001
				return d
			}(),
			fragment: "batch size",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateMeilisearchDescriptor(tt.descriptor)
			if !errors.Is(err, ErrMeilisearchDescriptorInvalid) {
				t.Fatalf("ValidateMeilisearchDescriptor() error = %v, want ErrMeilisearchDescriptorInvalid", err)
			}
			if tt.fragment != "" && !strings.Contains(err.Error(), tt.fragment) {
				t.Fatalf("ValidateMeilisearchDescriptor() error = %q, want fragment %q", err, tt.fragment)
			}
		})
	}
}
