package lazuli

import (
	"errors"
	"reflect"
	"testing"
	"time"
)

func TestRetentionCutoffSupportsExactAndCalendarWindows(t *testing.T) {
	now := time.Date(2026, 5, 12, 15, 30, 0, 0, time.FixedZone("BRT", -3*60*60))

	tests := []struct {
		name   string
		window Duration
		want   time.Time
	}{
		{
			name:   "exact",
			window: "36h",
			want:   now.UTC().Add(-36 * time.Hour),
		},
		{
			name:   "days",
			window: "7d",
			want:   now.UTC().AddDate(0, 0, -7),
		},
		{
			name:   "years",
			window: "7y",
			want:   now.UTC().AddDate(-7, 0, 0),
		},
		{
			name:   "months",
			window: "2mo",
			want:   now.UTC().AddDate(0, -2, 0),
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := RetentionCutoff(RetentionSpec{Window: tt.window, Then: RetentionDelete}, now)
			if err != nil {
				t.Fatalf("RetentionCutoff() error = %v", err)
			}
			if !got.Equal(tt.want) {
				t.Fatalf("RetentionCutoff() = %s, want %s", got, tt.want)
			}
		})
	}
}

func TestBuildRetentionAnonymizationPlanNormalizesSortsAndSummarizes(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	deleteSpec := Retention("90d").Then(Delete)
	anonymizeSpec := Retention("7y").Then(Anonymize)
	archiveSpec := Retention("30d").Then(Archive)
	unplannedSpec := (*RetentionSpec)(nil)

	customer := Resource[struct{}]{
		Name:       " customer ",
		Feature:    " people ",
		SoftDelete: true,
		Retention:  &anonymizeSpec,
	}

	fields := []RetentionField{
		{Name: " full_name ", Action: RetentionFieldRedact, Reason: " pii "},
		{Name: " email ", Action: RetentionFieldNull, Reason: " pii "},
	}
	resources := []RetentionResourceMetadata{
		{Name: "invoice", Feature: "billing", SoftDelete: true, Retention: &deleteSpec},
		{Name: "audit_log", Feature: "ops", SoftDelete: true, Retention: unplannedSpec},
		{Name: "export", Feature: "billing", SoftDelete: true, Retention: &archiveSpec},
		NewRetentionResourceMetadata(&customer, fields...),
	}

	plan, err := BuildRetentionAnonymizationPlan(resources, now)
	if err != nil {
		t.Fatalf("BuildRetentionAnonymizationPlan() error = %v", err)
	}

	if !plan.DryRun {
		t.Fatal("plan DryRun = false, want true")
	}
	if !plan.GeneratedAt.Equal(now) {
		t.Fatalf("plan GeneratedAt = %s, want %s", plan.GeneratedAt, now)
	}
	wantSummary := RetentionAnonymizationSummary{
		ResourceCount:    4,
		PlannedCount:     3,
		SkippedCount:     1,
		DeleteCount:      1,
		AnonymizeCount:   1,
		ArchiveCount:     1,
		FieldActionCount: 2,
	}
	if !reflect.DeepEqual(plan.Summary, wantSummary) {
		t.Fatalf("plan Summary = %#v, want %#v", plan.Summary, wantSummary)
	}

	gotOrder := retentionPlanEntryKeys(plan.Entries)
	wantOrder := []string{"billing/export/archive", "billing/invoice/delete", "people/customer/anonymize"}
	if !reflect.DeepEqual(gotOrder, wantOrder) {
		t.Fatalf("entry order = %#v, want %#v", gotOrder, wantOrder)
	}

	customerEntry := plan.Entries[2]
	if !customerEntry.Cutoff.Equal(now.AddDate(-7, 0, 0)) {
		t.Fatalf("customer cutoff = %s, want %s", customerEntry.Cutoff, now.AddDate(-7, 0, 0))
	}
	gotFields := retentionFieldNames(customerEntry.Fields)
	wantFields := []string{"email", "full_name"}
	if !reflect.DeepEqual(gotFields, wantFields) {
		t.Fatalf("customer fields = %#v, want %#v", gotFields, wantFields)
	}
	if customerEntry.Fields[0].Action != RetentionFieldNull || customerEntry.Fields[0].Reason != "pii" {
		t.Fatalf("first field = %#v, want normalized null pii field", customerEntry.Fields[0])
	}

	fields[0].Name = "mutated"
	if customerEntry.Fields[1].Name != "full_name" {
		t.Fatal("plan field mutated after caller slice changed")
	}
}

func TestBuildRetentionAnonymizationPlanValidatesMetadata(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	validSpec := Retention("7d").Then(Delete)

	tests := []struct {
		name      string
		resources []RetentionResourceMetadata
		want      error
	}{
		{
			name: "missing name",
			resources: []RetentionResourceMetadata{{
				SoftDelete: true,
				Retention:  &validSpec,
			}},
			want: ErrRetentionPlanInvalid,
		},
		{
			name: "retention without soft delete",
			resources: []RetentionResourceMetadata{{
				Name:      "customer",
				Retention: &validSpec,
			}},
			want: ErrRetentionPlanInvalid,
		},
		{
			name: "invalid duration",
			resources: []RetentionResourceMetadata{{
				Name:       "customer",
				SoftDelete: true,
				Retention:  &RetentionSpec{Window: "7q", Then: Delete},
			}},
			want: ErrRetentionPlanInvalid,
		},
		{
			name: "duplicate resource",
			resources: []RetentionResourceMetadata{
				{Name: "Customer", Feature: "CRM", SoftDelete: true, Retention: &validSpec},
				{Name: " customer ", Feature: "crm", SoftDelete: true, Retention: &validSpec},
			},
			want: ErrRetentionPlanInvalid,
		},
		{
			name: "duplicate field",
			resources: []RetentionResourceMetadata{{
				Name:       "customer",
				SoftDelete: true,
				Retention:  &RetentionSpec{Window: "7d", Then: Anonymize},
				Fields: []RetentionField{
					{Name: "email", Action: RetentionFieldNull},
					{Name: " email ", Action: RetentionFieldRedact},
				},
			}},
			want: ErrRetentionPlanInvalid,
		},
		{
			name: "unknown action",
			resources: []RetentionResourceMetadata{{
				Name:       "customer",
				SoftDelete: true,
				Retention:  &RetentionSpec{Window: "7d", Then: RetentionAction(99)},
			}},
			want: ErrRetentionPlanInvalid,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := BuildRetentionAnonymizationPlan(tt.resources, now)
			if !errors.Is(err, tt.want) {
				t.Fatalf("BuildRetentionAnonymizationPlan() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestRetentionCutoffRejectsInvalidWindows(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)

	for _, window := range []Duration{"", "-1h", "1.5d", "7q"} {
		t.Run(string(window), func(t *testing.T) {
			_, err := RetentionCutoff(RetentionSpec{Window: window, Then: Delete}, now)
			if !errors.Is(err, ErrRetentionDurationInvalid) {
				t.Fatalf("RetentionCutoff(%q) error = %v, want %v", window, err, ErrRetentionDurationInvalid)
			}
		})
	}
}

func retentionPlanEntryKeys(entries []RetentionAnonymizationPlanEntry) []string {
	keys := make([]string, 0, len(entries))
	for _, entry := range entries {
		keys = append(keys, entry.Feature+"/"+entry.Resource+"/"+entry.Action.String())
	}
	return keys
}

func retentionFieldNames(fields []RetentionField) []string {
	names := make([]string, 0, len(fields))
	for _, field := range fields {
		names = append(names, field.Name)
	}
	return names
}
