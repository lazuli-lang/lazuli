package version

import (
	"errors"
	"reflect"
	"testing"
)

func TestRuntimeCompatibilitySeverityString(t *testing.T) {
	tests := []struct {
		severity RuntimeCompatibilitySeverity
		want     string
	}{
		{severity: RuntimeCompatibilitySeverityError, want: "error"},
		{severity: RuntimeCompatibilitySeverityWarning, want: "warning"},
		{severity: RuntimeCompatibilitySeverityInfo, want: "info"},
		{severity: RuntimeCompatibilitySeverity(99), want: "unknown"},
	}

	for _, tt := range tests {
		if got := tt.severity.String(); got != tt.want {
			t.Fatalf("RuntimeCompatibilitySeverity(%d).String() = %q, want %q", tt.severity, got, tt.want)
		}
	}
}

func TestCompatibilityRangeContainsInclusive(t *testing.T) {
	r, err := NewCompatibilityRange(MinorPin{Major: 0, Minor: 11}, MinorPin{Major: 0, Minor: 13})
	if err != nil {
		t.Fatalf("NewCompatibilityRange() error = %v", err)
	}
	if got := r.String(); got != "0.11-0.13" {
		t.Fatalf("String() = %q, want 0.11-0.13", got)
	}

	for _, pin := range []MinorPin{
		{Major: 0, Minor: 11},
		{Major: 0, Minor: 12},
		{Major: 0, Minor: 13},
	} {
		if !r.Contains(pin) {
			t.Fatalf("range %s should contain %s", r, pin)
		}
	}
	for _, pin := range []MinorPin{
		{Major: 0, Minor: 10},
		{Major: 0, Minor: 14},
		{Major: 1, Minor: 0},
	} {
		if r.Contains(pin) {
			t.Fatalf("range %s should not contain %s", r, pin)
		}
	}
}

func TestCompatibilityRangeRejectsInvertedBounds(t *testing.T) {
	_, err := NewCompatibilityRange(MinorPin{Major: 0, Minor: 13}, MinorPin{Major: 0, Minor: 12})
	if !errors.Is(err, ErrInvalidCompatibilityRange) {
		t.Fatalf("NewCompatibilityRange() error = %v, want ErrInvalidCompatibilityRange", err)
	}
}

func TestEvaluateRuntimeCompatibilityNativeSkipsShims(t *testing.T) {
	shims := []CompatibilityShim{
		{
			ID:           "legacy-handler",
			Subject:      "legacy handler",
			APIRange:     CompatibilityRange{Min: MinorPin{Major: 0, Minor: 12}, Max: MinorPin{Major: 0, Minor: 12}},
			RuntimeRange: CompatibilityRange{Min: MinorPin{Major: 0, Minor: 12}, Max: MinorPin{Major: 0, Minor: 14}},
		},
	}

	evaluation, err := EvaluateRuntimeCompatibility(
		MinorPin{Major: 0, Minor: 12},
		SchemaVersion{Major: 0, Minor: 12, Patch: 9},
		shims,
	)
	if err != nil {
		t.Fatalf("EvaluateRuntimeCompatibility() error = %v", err)
	}
	if !evaluation.Compatible {
		t.Fatal("evaluation Compatible = false, want true")
	}
	if !evaluation.Native {
		t.Fatal("evaluation Native = false, want true")
	}
	if len(evaluation.Shims) != 0 || len(evaluation.Warnings) != 0 {
		t.Fatalf("native evaluation shims/warnings = %#v/%#v, want empty", evaluation.Shims, evaluation.Warnings)
	}
}

func TestEvaluateRuntimeCompatibilityReturnsSortedActiveShims(t *testing.T) {
	shims := []CompatibilityShim{
		{
			ID:           "query-nullability",
			Subject:      "query nullability",
			Message:      "Generated query adapters are using legacy nullability rules.",
			APIRange:     CompatibilityRange{Min: MinorPin{Major: 0, Minor: 11}, Max: MinorPin{Major: 0, Minor: 11}},
			RuntimeRange: CompatibilityRange{Min: MinorPin{Major: 0, Minor: 14}, Max: MinorPin{Major: 0, Minor: 15}},
			Severity:     RuntimeCompatibilitySeverityInfo,
			References: []CompatibilityReference{
				{Label: " Query guide ", URL: " https://example.com/query "},
			},
		},
		{
			ID:           "handler-contract",
			Subject:      "handler contract",
			Message:      "Generated handlers are using compatibility adapters.",
			APIRange:     CompatibilityRange{Min: MinorPin{Major: 0, Minor: 10}, Max: MinorPin{Major: 0, Minor: 12}},
			RuntimeRange: CompatibilityRange{Min: MinorPin{Major: 0, Minor: 13}, Max: MinorPin{Major: 0, Minor: 14}},
		},
		{
			ID:           "future-runtime-only",
			Subject:      "future runtime only",
			APIRange:     CompatibilityRange{Min: MinorPin{Major: 0, Minor: 11}, Max: MinorPin{Major: 0, Minor: 12}},
			RuntimeRange: CompatibilityRange{Min: MinorPin{Major: 0, Minor: 15}, Max: MinorPin{Major: 0, Minor: 16}},
		},
	}

	evaluation, err := EvaluateRuntimeCompatibility(
		MinorPin{Major: 0, Minor: 11},
		SchemaVersion{Major: 0, Minor: 14, Patch: 3},
		shims,
	)
	if err != nil {
		t.Fatalf("EvaluateRuntimeCompatibility() error = %v", err)
	}
	if !evaluation.Compatible {
		t.Fatal("evaluation Compatible = false, want true")
	}
	if evaluation.Native {
		t.Fatal("evaluation Native = true, want false")
	}

	var shimIDs []string
	for _, shim := range evaluation.Shims {
		shimIDs = append(shimIDs, shim.ID)
	}
	if want := []string{"handler-contract", "query-nullability"}; !reflect.DeepEqual(shimIDs, want) {
		t.Fatalf("shim ids = %v, want %v", shimIDs, want)
	}

	var warningIDs []string
	var severities []RuntimeCompatibilitySeverity
	for _, warning := range evaluation.Warnings {
		warningIDs = append(warningIDs, warning.Shim.ID)
		severities = append(severities, warning.Severity)
		if warning.APIVersion != (MinorPin{Major: 0, Minor: 11}) {
			t.Fatalf("warning API version = %#v, want 0.11", warning.APIVersion)
		}
		if warning.RuntimeVersion != (SchemaVersion{Major: 0, Minor: 14, Patch: 3}) {
			t.Fatalf("warning runtime version = %#v, want 0.14.3", warning.RuntimeVersion)
		}
	}
	if want := []string{"handler-contract", "query-nullability"}; !reflect.DeepEqual(warningIDs, want) {
		t.Fatalf("warning ids = %v, want %v", warningIDs, want)
	}
	if want := []RuntimeCompatibilitySeverity{RuntimeCompatibilitySeverityWarning, RuntimeCompatibilitySeverityInfo}; !reflect.DeepEqual(severities, want) {
		t.Fatalf("warning severities = %v, want %v", severities, want)
	}
	if got := evaluation.Warnings[1].Shim.References[0]; got != (CompatibilityReference{Label: "Query guide", URL: "https://example.com/query"}) {
		t.Fatalf("normalized reference = %#v, want trimmed values", got)
	}

	if got := shims[0].References[0]; got != (CompatibilityReference{Label: " Query guide ", URL: " https://example.com/query "}) {
		t.Fatalf("input reference was mutated to %#v", got)
	}
	if shims[0].ID != "query-nullability" || shims[1].ID != "handler-contract" {
		t.Fatalf("input shims reordered or mutated: %#v", shims)
	}
}

func TestEvaluateRuntimeCompatibilityUnsupportedWithoutActiveShim(t *testing.T) {
	shims := []CompatibilityShim{
		{
			ID:           "old-runtime-only",
			Subject:      "old runtime only",
			APIRange:     CompatibilityRange{Min: MinorPin{Major: 0, Minor: 10}, Max: MinorPin{Major: 0, Minor: 10}},
			RuntimeRange: CompatibilityRange{Min: MinorPin{Major: 0, Minor: 12}, Max: MinorPin{Major: 0, Minor: 13}},
		},
	}

	evaluation, err := EvaluateRuntimeCompatibility(
		MinorPin{Major: 0, Minor: 10},
		SchemaVersion{Major: 0, Minor: 14},
		shims,
	)
	if err != nil {
		t.Fatalf("EvaluateRuntimeCompatibility() error = %v", err)
	}
	if evaluation.Compatible {
		t.Fatal("evaluation Compatible = true, want false")
	}
	if evaluation.Native {
		t.Fatal("evaluation Native = true, want false")
	}
	if len(evaluation.Shims) != 0 || len(evaluation.Warnings) != 0 {
		t.Fatalf("unsupported evaluation shims/warnings = %#v/%#v, want empty", evaluation.Shims, evaluation.Warnings)
	}
}

func TestEvaluateRuntimeCompatibilityRejectsInvalidShims(t *testing.T) {
	tests := []struct {
		name    string
		shims   []CompatibilityShim
		wantErr error
	}{
		{
			name: "missing id",
			shims: []CompatibilityShim{
				{
					APIRange:     CompatibilityRange{Min: MinorPin{Major: 0, Minor: 10}, Max: MinorPin{Major: 0, Minor: 12}},
					RuntimeRange: CompatibilityRange{Min: MinorPin{Major: 0, Minor: 13}, Max: MinorPin{Major: 0, Minor: 14}},
				},
			},
			wantErr: ErrInvalidCompatibilityShim,
		},
		{
			name: "invalid api range",
			shims: []CompatibilityShim{
				{
					ID:           "inverted-api",
					APIRange:     CompatibilityRange{Min: MinorPin{Major: 0, Minor: 12}, Max: MinorPin{Major: 0, Minor: 10}},
					RuntimeRange: CompatibilityRange{Min: MinorPin{Major: 0, Minor: 13}, Max: MinorPin{Major: 0, Minor: 14}},
				},
			},
			wantErr: ErrInvalidCompatibilityRange,
		},
		{
			name: "invalid severity",
			shims: []CompatibilityShim{
				{
					ID:           "bad-severity",
					APIRange:     CompatibilityRange{Min: MinorPin{Major: 0, Minor: 10}, Max: MinorPin{Major: 0, Minor: 12}},
					RuntimeRange: CompatibilityRange{Min: MinorPin{Major: 0, Minor: 13}, Max: MinorPin{Major: 0, Minor: 14}},
					Severity:     RuntimeCompatibilitySeverity(99),
				},
			},
			wantErr: ErrInvalidCompatibilityShim,
		},
		{
			name: "duplicate id",
			shims: []CompatibilityShim{
				{
					ID:           "shared",
					APIRange:     CompatibilityRange{Min: MinorPin{Major: 0, Minor: 10}, Max: MinorPin{Major: 0, Minor: 12}},
					RuntimeRange: CompatibilityRange{Min: MinorPin{Major: 0, Minor: 13}, Max: MinorPin{Major: 0, Minor: 14}},
				},
				{
					ID:           " shared ",
					APIRange:     CompatibilityRange{Min: MinorPin{Major: 0, Minor: 9}, Max: MinorPin{Major: 0, Minor: 10}},
					RuntimeRange: CompatibilityRange{Min: MinorPin{Major: 0, Minor: 13}, Max: MinorPin{Major: 0, Minor: 14}},
				},
			},
			wantErr: ErrDuplicateCompatibilityShim,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := EvaluateRuntimeCompatibility(MinorPin{Major: 0, Minor: 10}, SchemaVersion{Major: 0, Minor: 14}, tt.shims)
			if !errors.Is(err, tt.wantErr) {
				t.Fatalf("EvaluateRuntimeCompatibility() error = %v, want %v", err, tt.wantErr)
			}
		})
	}
}
