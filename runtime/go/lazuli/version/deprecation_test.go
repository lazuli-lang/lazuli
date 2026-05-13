package version

import (
	"errors"
	"reflect"
	"testing"
)

func TestDeprecationSeverityString(t *testing.T) {
	tests := []struct {
		severity DeprecationSeverity
		want     string
	}{
		{severity: DeprecationSeverityError, want: "error"},
		{severity: DeprecationSeverityWarning, want: "warning"},
		{severity: DeprecationSeverityInfo, want: "info"},
		{severity: DeprecationSeverity(99), want: "unknown"},
	}

	for _, tt := range tests {
		if got := tt.severity.String(); got != tt.want {
			t.Fatalf("DeprecationSeverity(%d).String() = %q, want %q", tt.severity, got, tt.want)
		}
	}
}

func TestEvaluateDeprecationSkipsBeforeDeprecatedSince(t *testing.T) {
	rule := DeprecationRule{
		ID:              "lazy-handler-v1",
		DeprecatedSince: MinorPin{Major: 0, Minor: 12},
		RemoveAfter:     MinorPin{Major: 0, Minor: 14},
	}

	warning, active, err := EvaluateDeprecation(SchemaVersion{Major: 0, Minor: 11, Patch: 9}, rule)
	if err != nil {
		t.Fatalf("EvaluateDeprecation() error = %v", err)
	}
	if active {
		t.Fatalf("EvaluateDeprecation() active = true, warning = %#v", warning)
	}
}

func TestEvaluateDeprecationWarnsDuringWindow(t *testing.T) {
	links := []ReplacementLink{
		{Label: "Upgrade guide", URL: "https://example.com/lazuli/upgrade"},
		{Label: "Replacement API", URL: "https://example.com/lazuli/api"},
	}
	rule := DeprecationRule{
		ID:               "lazy-handler-v1",
		Subject:          "lazy handler v1",
		Message:          "Use lazy handler v2.",
		DeprecatedSince:  MinorPin{Major: 0, Minor: 12},
		RemoveAfter:      MinorPin{Major: 0, Minor: 14},
		Severity:         DeprecationSeverityInfo,
		ReplacementLinks: links,
	}

	warning, active, err := EvaluateDeprecation(SchemaVersion{Major: 0, Minor: 12, Patch: 9}, rule)
	if err != nil {
		t.Fatalf("EvaluateDeprecation() error = %v", err)
	}
	if !active {
		t.Fatal("EvaluateDeprecation() active = false, want true")
	}
	if warning.Current != (SchemaVersion{Major: 0, Minor: 12, Patch: 9}) {
		t.Fatalf("warning current = %#v, want 0.12.9", warning.Current)
	}
	if warning.Severity != DeprecationSeverityInfo {
		t.Fatalf("warning severity = %s, want info", warning.Severity)
	}
	if warning.Removed {
		t.Fatal("warning Removed = true, want false during deprecation window")
	}
	if !reflect.DeepEqual(warning.Rule.ReplacementLinks, links) {
		t.Fatalf("warning links = %#v, want %#v", warning.Rule.ReplacementLinks, links)
	}
}

func TestEvaluateDeprecationDefaultsSeverityToWarning(t *testing.T) {
	rule := DeprecationRule{
		ID:              "lazy-handler-v1",
		DeprecatedSince: MinorPin{Major: 0, Minor: 12},
		RemoveAfter:     MinorPin{Major: 0, Minor: 14},
	}

	warning, active, err := EvaluateDeprecation(SchemaVersion{Major: 0, Minor: 13}, rule)
	if err != nil {
		t.Fatalf("EvaluateDeprecation() error = %v", err)
	}
	if !active {
		t.Fatal("EvaluateDeprecation() active = false, want true")
	}
	if warning.Severity != DeprecationSeverityWarning {
		t.Fatalf("warning severity = %s, want warning", warning.Severity)
	}
}

func TestEvaluateDeprecationRemoveAfterIsInclusive(t *testing.T) {
	rule := DeprecationRule{
		ID:              "lazy-handler-v1",
		DeprecatedSince: MinorPin{Major: 0, Minor: 12},
		RemoveAfter:     MinorPin{Major: 0, Minor: 14},
	}

	warning, active, err := EvaluateDeprecation(SchemaVersion{Major: 0, Minor: 14}, rule)
	if err != nil {
		t.Fatalf("EvaluateDeprecation() error = %v", err)
	}
	if !active {
		t.Fatal("EvaluateDeprecation() active = false, want true")
	}
	if warning.Removed {
		t.Fatal("warning Removed = true, want false at remove-after version")
	}
	if warning.Severity != DeprecationSeverityWarning {
		t.Fatalf("warning severity = %s, want warning", warning.Severity)
	}
}

func TestEvaluateDeprecationEscalatesAfterRemoveAfter(t *testing.T) {
	rule := DeprecationRule{
		ID:              "lazy-handler-v1",
		DeprecatedSince: MinorPin{Major: 0, Minor: 12},
		RemoveAfter:     MinorPin{Major: 0, Minor: 14},
		Severity:        DeprecationSeverityInfo,
	}

	warning, active, err := EvaluateDeprecation(SchemaVersion{Major: 0, Minor: 15}, rule)
	if err != nil {
		t.Fatalf("EvaluateDeprecation() error = %v", err)
	}
	if !active {
		t.Fatal("EvaluateDeprecation() active = false, want true")
	}
	if !warning.Removed {
		t.Fatal("warning Removed = false, want true after remove-after version")
	}
	if warning.Severity != DeprecationSeverityError {
		t.Fatalf("warning severity = %s, want error", warning.Severity)
	}
}

func TestEvaluateDeprecationsReturnsOnlyActiveWarnings(t *testing.T) {
	rules := []DeprecationRule{
		{
			ID:              "old",
			DeprecatedSince: MinorPin{Major: 0, Minor: 9},
			RemoveAfter:     MinorPin{Major: 0, Minor: 11},
		},
		{
			ID:              "current",
			DeprecatedSince: MinorPin{Major: 0, Minor: 12},
			RemoveAfter:     MinorPin{Major: 0, Minor: 14},
		},
		{
			ID:              "future",
			DeprecatedSince: MinorPin{Major: 0, Minor: 13},
			RemoveAfter:     MinorPin{Major: 0, Minor: 15},
		},
	}

	warnings, err := EvaluateDeprecations(SchemaVersion{Major: 0, Minor: 12}, rules)
	if err != nil {
		t.Fatalf("EvaluateDeprecations() error = %v", err)
	}

	var ids []string
	for _, warning := range warnings {
		ids = append(ids, warning.Rule.ID)
	}
	if want := []string{"old", "current"}; !reflect.DeepEqual(ids, want) {
		t.Fatalf("warning ids = %v, want %v", ids, want)
	}
	if !warnings[0].Removed || warnings[0].Severity != DeprecationSeverityError {
		t.Fatalf("old warning = %#v, want removed error", warnings[0])
	}
}

func TestEvaluateDeprecationRejectsInvalidRules(t *testing.T) {
	tests := []struct {
		name string
		rule DeprecationRule
	}{
		{
			name: "remove after before deprecated since",
			rule: DeprecationRule{
				DeprecatedSince: MinorPin{Major: 0, Minor: 12},
				RemoveAfter:     MinorPin{Major: 0, Minor: 11},
			},
		},
		{
			name: "unknown severity",
			rule: DeprecationRule{
				DeprecatedSince: MinorPin{Major: 0, Minor: 12},
				RemoveAfter:     MinorPin{Major: 0, Minor: 14},
				Severity:        DeprecationSeverity(99),
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, _, err := EvaluateDeprecation(SchemaVersion{Major: 0, Minor: 12}, tt.rule)
			if !errors.Is(err, ErrInvalidDeprecationRule) {
				t.Fatalf("EvaluateDeprecation() error = %v, want ErrInvalidDeprecationRule", err)
			}
		})
	}
}
