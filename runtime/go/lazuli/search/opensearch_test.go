package search

import (
	"errors"
	"reflect"
	"testing"
)

func TestOpenSearchDescriptorNormalizeValidateAndPlan(t *testing.T) {
	t.Parallel()

	descriptor := OpenSearchDescriptor{
		EndpointURL:   "https://user:pass@Search.EXAMPLE.test:9200/catalog/?secret=yes#fragment",
		AuthMode:      " BASIC ",
		Username:      " search-user ",
		Password:      " invalid-placeholder-password ",
		IndexName:     " Catalog-Products ",
		AliasName:     " Catalog-Read ",
		ShardCount:    3,
		ReplicaCount:  2,
		RefreshPolicy: "wait-for",
		BulkSize:      150,
		BulkBounds: OpenSearchBulkBounds{
			Min:     50,
			Default: 100,
			Max:     200,
		},
	}

	normalized, err := descriptor.Normalize()
	if err != nil {
		t.Fatalf("Normalize() error = %v", err)
	}
	if normalized.EndpointURL != "https://search.example.test:9200/catalog" {
		t.Fatalf("EndpointURL = %q, want safe canonical endpoint", normalized.EndpointURL)
	}
	if normalized.AuthMode != OpenSearchAuthBasic || normalized.Username != "search-user" || normalized.Password != "invalid-placeholder-password" {
		t.Fatalf("auth metadata = %#v, want normalized basic auth", normalized)
	}
	if normalized.IndexName != "catalog-products" || normalized.AliasName != "catalog-read" {
		t.Fatalf("index metadata = %q/%q, want catalog-products/catalog-read", normalized.IndexName, normalized.AliasName)
	}
	if normalized.RefreshPolicy != OpenSearchRefreshWaitFor {
		t.Fatalf("RefreshPolicy = %q, want wait_for", normalized.RefreshPolicy)
	}
	if normalized.BulkSize != 150 || normalized.BulkBounds != (OpenSearchBulkBounds{Min: 50, Default: 100, Max: 200}) {
		t.Fatalf("bulk metadata = %d/%#v, want requested size and bounds", normalized.BulkSize, normalized.BulkBounds)
	}
	if err := normalized.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	plan, err := normalized.PlanIndex(451)
	if err != nil {
		t.Fatalf("PlanIndex() error = %v", err)
	}
	if plan.EndpointURL != normalized.EndpointURL ||
		plan.IndexName != "catalog-products" ||
		plan.AliasName != "catalog-read" ||
		plan.ShardCount != 3 ||
		plan.ReplicaCount != 2 ||
		plan.RefreshPolicy != OpenSearchRefreshWaitFor ||
		plan.BulkSize != 150 ||
		plan.EstimatedDocuments != 451 ||
		plan.BulkBatchCount != 4 {
		t.Fatalf("PlanIndex() = %#v, want normalized index plan with four batches", plan)
	}
}

func TestOpenSearchDescriptorDefaults(t *testing.T) {
	t.Parallel()

	descriptor := OpenSearchDescriptor{
		EndpointURL: "http://localhost:9200",
		IndexName:   "events",
	}
	normalized, err := descriptor.Normalize()
	if err != nil {
		t.Fatalf("Normalize() error = %v", err)
	}
	if normalized.AuthMode != OpenSearchAuthNone {
		t.Fatalf("AuthMode = %q, want none", normalized.AuthMode)
	}
	if normalized.ShardCount != DefaultOpenSearchShardCount {
		t.Fatalf("ShardCount = %d, want default", normalized.ShardCount)
	}
	if normalized.RefreshPolicy != OpenSearchRefreshFalse {
		t.Fatalf("RefreshPolicy = %q, want false", normalized.RefreshPolicy)
	}
	wantBounds := OpenSearchBulkBounds{
		Min:     DefaultOpenSearchBulkMinSize,
		Default: DefaultOpenSearchBulkSize,
		Max:     DefaultOpenSearchBulkMaxSize,
	}
	if normalized.BulkSize != DefaultOpenSearchBulkSize || !reflect.DeepEqual(normalized.BulkBounds, wantBounds) {
		t.Fatalf("bulk defaults = %d/%#v, want %#v", normalized.BulkSize, normalized.BulkBounds, wantBounds)
	}
}

func TestOpenSearchRedactedSummary(t *testing.T) {
	t.Parallel()

	descriptor := OpenSearchDescriptor{
		EndpointURL:   "https://user:pass@search.example.test/index?api-key=placeholder#frag",
		AuthMode:      OpenSearchAuthAPIKey,
		APIKey:        "not-a-real-api-key",
		IndexName:     "logs",
		RefreshPolicy: OpenSearchRefreshTrue,
	}
	summary := descriptor.RedactedSummary()
	if summary.EndpointURL != "https://search.example.test/index" {
		t.Fatalf("EndpointURL = %q, want redacted endpoint URL", summary.EndpointURL)
	}
	if summary.APIKey != "[redacted]" || !summary.HasAPIKey {
		t.Fatalf("API key redaction = %q/%v, want redacted true", summary.APIKey, summary.HasAPIKey)
	}
	if summary.Username != "" || summary.Password != "" || summary.BearerToken != "" {
		t.Fatalf("empty secret redaction = %q/%q/%q, want empty", summary.Username, summary.Password, summary.BearerToken)
	}

	bad := OpenSearchDescriptor{
		EndpointURL: "://bad",
		AuthMode:    "bearer",
		BearerToken: "invalid-placeholder-token",
		IndexName:   "Logs",
	}
	badSummary := bad.RedactedSummary()
	if badSummary.EndpointURL != "[redacted]" || badSummary.BearerToken != "[redacted]" || !badSummary.HasBearerToken {
		t.Fatalf("bad summary = %#v, want endpoint and token redacted", badSummary)
	}
}

func TestOpenSearchRefreshPolicyNormalization(t *testing.T) {
	t.Parallel()

	cases := []struct {
		raw  OpenSearchRefreshPolicy
		want OpenSearchRefreshPolicy
	}{
		{raw: "", want: OpenSearchRefreshFalse},
		{raw: " disabled ", want: OpenSearchRefreshFalse},
		{raw: "immediate", want: OpenSearchRefreshTrue},
		{raw: "wait", want: OpenSearchRefreshWaitFor},
	}
	for _, tc := range cases {
		got, err := NormalizeOpenSearchRefreshPolicy(tc.raw)
		if err != nil {
			t.Fatalf("NormalizeOpenSearchRefreshPolicy(%q) error = %v", tc.raw, err)
		}
		if got != tc.want {
			t.Fatalf("NormalizeOpenSearchRefreshPolicy(%q) = %q, want %q", tc.raw, got, tc.want)
		}
	}

	if _, err := NormalizeOpenSearchRefreshPolicy("eventual"); !errors.Is(err, ErrOpenSearchDescriptorInvalid) {
		t.Fatalf("NormalizeOpenSearchRefreshPolicy(invalid) error = %v, want ErrOpenSearchDescriptorInvalid", err)
	}
}

func TestOpenSearchAuthValidation(t *testing.T) {
	t.Parallel()

	valid := OpenSearchDescriptor{
		EndpointURL: "https://search.example.test",
		IndexName:   "orders",
	}
	cases := []struct {
		name       string
		descriptor OpenSearchDescriptor
	}{
		{
			name: "none with credentials",
			descriptor: func() OpenSearchDescriptor {
				d := valid
				d.Username = "user"
				return d
			}(),
		},
		{
			name: "basic missing password",
			descriptor: func() OpenSearchDescriptor {
				d := valid
				d.AuthMode = OpenSearchAuthBasic
				d.Username = "user"
				return d
			}(),
		},
		{
			name: "bearer missing token",
			descriptor: func() OpenSearchDescriptor {
				d := valid
				d.AuthMode = OpenSearchAuthBearer
				return d
			}(),
		},
		{
			name: "api key missing key",
			descriptor: func() OpenSearchDescriptor {
				d := valid
				d.AuthMode = OpenSearchAuthAPIKey
				return d
			}(),
		},
		{
			name: "unknown mode",
			descriptor: func() OpenSearchDescriptor {
				d := valid
				d.AuthMode = "signed"
				return d
			}(),
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := ValidateOpenSearchDescriptor(tc.descriptor)
			if !errors.Is(err, ErrOpenSearchDescriptorInvalid) {
				t.Fatalf("ValidateOpenSearchDescriptor() error = %v, want ErrOpenSearchDescriptorInvalid", err)
			}
		})
	}
}

func TestOpenSearchIndexNameValidation(t *testing.T) {
	t.Parallel()

	normalized, err := NormalizeOpenSearchIndexName(" Tenant.Events_2026 ")
	if err != nil {
		t.Fatalf("NormalizeOpenSearchIndexName() error = %v", err)
	}
	if normalized != "tenant.events_2026" {
		t.Fatalf("NormalizeOpenSearchIndexName() = %q, want tenant.events_2026", normalized)
	}

	for _, name := range []string{"", ". ", "..", "-logs", "_logs", "+logs", "Log", "logs data", "logs#data"} {
		if err := ValidateOpenSearchIndexName(name); !errors.Is(err, ErrOpenSearchDescriptorInvalid) {
			t.Fatalf("ValidateOpenSearchIndexName(%q) error = %v, want ErrOpenSearchDescriptorInvalid", name, err)
		}
	}
}

func TestOpenSearchBulkBoundsAndBatchCount(t *testing.T) {
	t.Parallel()

	bounds, err := NormalizeOpenSearchBulkBounds(OpenSearchBulkBounds{Min: 10, Max: 30})
	if err != nil {
		t.Fatalf("NormalizeOpenSearchBulkBounds(valid) error = %v", err)
	}
	if bounds != (OpenSearchBulkBounds{Min: 10, Default: 30, Max: 30}) {
		t.Fatalf("NormalizeOpenSearchBulkBounds() = %#v, want default clamped to max", bounds)
	}
	size, err := NormalizeOpenSearchBulkSize(0, bounds)
	if err != nil {
		t.Fatalf("NormalizeOpenSearchBulkSize(default) error = %v", err)
	}
	if size != 30 {
		t.Fatalf("NormalizeOpenSearchBulkSize(default) = %d, want 30", size)
	}
	if _, err := NormalizeOpenSearchBulkSize(31, bounds); !errors.Is(err, ErrOpenSearchDescriptorInvalid) {
		t.Fatalf("NormalizeOpenSearchBulkSize(out of bounds) error = %v, want ErrOpenSearchDescriptorInvalid", err)
	}
	if _, err := NormalizeOpenSearchBulkBounds(OpenSearchBulkBounds{Min: 40, Max: 30}); !errors.Is(err, ErrOpenSearchDescriptorInvalid) {
		t.Fatalf("NormalizeOpenSearchBulkBounds(invalid) error = %v, want ErrOpenSearchDescriptorInvalid", err)
	}
	if got := OpenSearchBulkBatchCount(61, 20); got != 4 {
		t.Fatalf("OpenSearchBulkBatchCount() = %d, want 4", got)
	}
	if got := OpenSearchBulkBatchCount(0, 20); got != 0 {
		t.Fatalf("OpenSearchBulkBatchCount(empty) = %d, want 0", got)
	}
}
