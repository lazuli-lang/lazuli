package diagnostics_test

import (
	"reflect"
	"testing"

	"lazuli.dev/runtime/lazuli/diagnostics"
)

func TestCatalogContainsRequestedFamilies(t *testing.T) {
	t.Parallel()

	want := map[diagnostics.Code]struct {
		family   diagnostics.Family
		severity diagnostics.Severity
	}{
		diagnostics.CodeLazuliVersionMismatch:        {family: diagnostics.FamilyLazuliVersion, severity: diagnostics.SeverityWarning},
		diagnostics.CodeLazuliVersionNoMigrationPath: {family: diagnostics.FamilyLazuliVersion, severity: diagnostics.SeverityError},
		diagnostics.CodeLazuliVersionPatchPin:        {family: diagnostics.FamilyLazuliVersion, severity: diagnostics.SeverityError},
		diagnostics.CodeMigrationRecipeMissing:       {family: diagnostics.FamilyMigrationRecipe, severity: diagnostics.SeverityError},
		diagnostics.CodeMigrationRecipeRoundTrip:     {family: diagnostics.FamilyMigrationRecipe, severity: diagnostics.SeverityError},
		diagnostics.CodeCodegenSentinelUnknown:       {family: diagnostics.FamilyCodegenSentinel, severity: diagnostics.SeverityError},
	}

	if got := len(diagnostics.Catalog()); got != len(want) {
		t.Fatalf("catalog size = %d, want %d", got, len(want))
	}
	for code, expected := range want {
		definition, ok := diagnostics.Lookup(code)
		if !ok {
			t.Fatalf("Lookup(%s) missing", code)
		}
		if definition.Family != expected.family {
			t.Fatalf("%s family = %s, want %s", code, definition.Family, expected.family)
		}
		if definition.DefaultSeverity != expected.severity {
			t.Fatalf("%s severity = %s, want %s", code, definition.DefaultSeverity, expected.severity)
		}
		if definition.Surface == "" || definition.Summary == "" {
			t.Fatalf("%s has incomplete catalog metadata: %#v", code, definition)
		}
	}

	if _, ok := diagnostics.Lookup(diagnostics.Code("UNKNOWN-001")); ok {
		t.Fatal("Lookup returned a definition for an unknown code")
	}
}

func TestCatalogReturnsCopy(t *testing.T) {
	t.Parallel()

	catalog := diagnostics.Catalog()
	catalog[0].Code = diagnostics.Code("CHANGED-001")

	got := diagnostics.Catalog()
	if got[0].Code != diagnostics.CodeLazuliVersionMismatch {
		t.Fatalf("Catalog returned mutable backing storage: first code = %s", got[0].Code)
	}
}

func TestSeverityStringAndCodeFamily(t *testing.T) {
	t.Parallel()

	cases := []struct {
		severity diagnostics.Severity
		want     string
	}{
		{severity: diagnostics.SeverityError, want: "error"},
		{severity: diagnostics.SeverityWarning, want: "warning"},
		{severity: diagnostics.SeverityInfo, want: "info"},
		{severity: diagnostics.SeverityHint, want: "hint"},
		{severity: diagnostics.Severity(99), want: "unknown"},
	}
	for _, tc := range cases {
		if got := tc.severity.String(); got != tc.want {
			t.Fatalf("Severity(%d).String() = %q, want %q", tc.severity, got, tc.want)
		}
	}

	if got := diagnostics.Code("APP-URL-001").Family(); got != diagnostics.Family("APP-URL") {
		t.Fatalf("unknown code family = %s, want APP-URL", got)
	}
	if got := diagnostics.Code("").Family(); got != diagnostics.FamilyUnknown {
		t.Fatalf("empty code family = %s, want %s", got, diagnostics.FamilyUnknown)
	}
}

func TestSortedCopyOrdersByLocationSeverityAndCode(t *testing.T) {
	t.Parallel()

	input := []diagnostics.Diagnostic{
		{Path: "b.lzi", Line: 1, Column: 1, Severity: diagnostics.SeverityWarning, Code: diagnostics.CodeMigrationRecipeMissing, Message: "b"},
		{Path: "a.lzi", Line: 2, Column: 1, Severity: diagnostics.SeverityWarning, Code: diagnostics.CodeMigrationRecipeMissing, Message: "a2"},
		{Path: "a.lzi", Line: 1, Column: 2, Severity: diagnostics.SeverityHint, Code: diagnostics.CodeLazuliVersionMismatch, Message: "hint"},
		{Path: "a.lzi", Line: 1, Column: 2, Severity: diagnostics.SeverityError, Code: diagnostics.CodeLazuliVersionNoMigrationPath, Message: "error-a"},
		{Path: "a.lzi", Line: 1, Column: 1, Severity: diagnostics.SeverityError, Code: diagnostics.CodeCodegenSentinelUnknown, Message: "first"},
		{Path: "a.lzi", Line: 1, Column: 2, Severity: diagnostics.SeverityError, Code: diagnostics.CodeLazuliVersionNoMigrationPath, Message: "error-b"},
	}

	got := diagnostics.Sorted(input)
	if input[0].Message != "b" {
		t.Fatal("Sorted mutated the input slice")
	}

	if messages := catalogTestMessages(got); !reflect.DeepEqual(messages, []string{"first", "error-a", "error-b", "hint", "a2", "b"}) {
		t.Fatalf("sorted messages = %v", messages)
	}
}

func TestGroupingPreservesFirstSeenGroupAndDiagnosticOrder(t *testing.T) {
	t.Parallel()

	input := []diagnostics.Diagnostic{
		{Code: diagnostics.CodeLazuliVersionMismatch, Severity: diagnostics.SeverityWarning, Message: "version-a"},
		{Code: diagnostics.CodeMigrationRecipeMissing, Severity: diagnostics.SeverityError, Message: "recipe"},
		{Code: diagnostics.CodeLazuliVersionNoMigrationPath, Severity: diagnostics.SeverityError, Message: "version-b"},
		{Code: diagnostics.CodeCodegenSentinelUnknown, Severity: diagnostics.SeverityError, Message: "sentinel"},
		{Code: diagnostics.CodeLazuliVersionMismatch, Severity: diagnostics.SeverityWarning, Message: "version-c"},
	}

	byCode := diagnostics.GroupByCode(input)
	if keys := catalogTestGroupKeys(byCode); !reflect.DeepEqual(keys, []string{
		"LAZULI-VERSION-001",
		"MIGRATION-RECIPE-001",
		"LAZULI-VERSION-002",
		"CODEGEN-SENTINEL-001",
	}) {
		t.Fatalf("code group keys = %v", keys)
	}
	if messages := catalogTestGroupMessages(byCode[0]); !reflect.DeepEqual(messages, []string{"version-a", "version-c"}) {
		t.Fatalf("first code group messages = %v", messages)
	}

	byFamily := diagnostics.GroupByFamily(input)
	if keys := catalogTestGroupKeys(byFamily); !reflect.DeepEqual(keys, []string{
		"LAZULI-VERSION",
		"MIGRATION-RECIPE",
		"CODEGEN-SENTINEL",
	}) {
		t.Fatalf("family group keys = %v", keys)
	}
	if messages := catalogTestGroupMessages(byFamily[0]); !reflect.DeepEqual(messages, []string{"version-a", "version-b", "version-c"}) {
		t.Fatalf("first family group messages = %v", messages)
	}

	bySeverity := diagnostics.GroupBySeverity(input)
	if keys := catalogTestGroupKeys(bySeverity); !reflect.DeepEqual(keys, []string{"warning", "error"}) {
		t.Fatalf("severity group keys = %v", keys)
	}
	if messages := catalogTestGroupMessages(bySeverity[0]); !reflect.DeepEqual(messages, []string{"version-a", "version-c"}) {
		t.Fatalf("first severity group messages = %v", messages)
	}
}

func catalogTestMessages(diagnostics []diagnostics.Diagnostic) []string {
	messages := make([]string, 0, len(diagnostics))
	for _, diagnostic := range diagnostics {
		messages = append(messages, diagnostic.Message)
	}
	return messages
}

func catalogTestGroupKeys(groups []diagnostics.DiagnosticGroup) []string {
	keys := make([]string, 0, len(groups))
	for _, group := range groups {
		keys = append(keys, group.Key)
	}
	return keys
}

func catalogTestGroupMessages(group diagnostics.DiagnosticGroup) []string {
	return catalogTestMessages(group.Diagnostics)
}
