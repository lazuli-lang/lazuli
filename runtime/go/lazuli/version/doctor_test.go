package version

import (
	"errors"
	"reflect"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/diagnostics"
	"lazuli.dev/runtime/lazuli/migrations"
)

func TestDiagnoseAppPinOKWhenMinorMatches(t *testing.T) {
	report, err := DiagnoseAppPin(DoctorRequest{
		Pin:    "0.12",
		Schema: "0.12.9",
	})
	if err != nil {
		t.Fatalf("DiagnoseAppPin() error = %v", err)
	}
	if !report.Check.OK() {
		t.Fatalf("DiagnoseAppPin() code = %q, want empty", report.Check.Code)
	}
	if len(report.Diagnostics) != 0 {
		t.Fatalf("DiagnoseAppPin() diagnostics = %v, want none", report.Diagnostics)
	}
	if len(report.UpgradePlan.Recipes) != 0 {
		t.Fatalf("DiagnoseAppPin() plan recipes = %v, want none", report.UpgradePlan.Recipes)
	}
}

func TestDiagnoseAppPinReportsMigrationRecipePath(t *testing.T) {
	report, err := DiagnoseAppPin(DoctorRequest{
		Pin:     "0.10",
		Schema:  "0.12.0",
		AppPath: "app.lzi",
		Line:    3,
		Column:  5,
		Recipes: []migrations.UpgradeRecipeDescriptor{
			{
				Name:        "rename-policies-to-rules",
				FromVersion: "0.10",
				ToVersion:   "0.11",
				Path:        "migrations/recipes/0.10-to-0.11/rename-policies-to-rules",
			},
			{
				Name:        "debug-loop",
				FromVersion: "0.11",
				ToVersion:   "0.12",
			},
		},
	})
	if err != nil {
		t.Fatalf("DiagnoseAppPin() error = %v", err)
	}

	diagnostic := versionDoctorTestSingleDiagnostic(t, report)
	if diagnostic.Code != diagnostics.CodeLazuliVersionMismatch {
		t.Fatalf("diagnostic code = %s, want %s", diagnostic.Code, diagnostics.CodeLazuliVersionMismatch)
	}
	if diagnostic.Severity != diagnostics.SeverityWarning {
		t.Fatalf("diagnostic severity = %s, want warning", diagnostic.Severity)
	}
	if diagnostic.Path != "app.lzi" || diagnostic.Line != 3 || diagnostic.Column != 5 {
		t.Fatalf("diagnostic location = %s:%d:%d, want app.lzi:3:5", diagnostic.Path, diagnostic.Line, diagnostic.Column)
	}
	for _, want := range []string{
		"migrations/recipes/0.10-to-0.11/rename-policies-to-rules",
		"migrations/recipes/0.11-to-0.12/debug-loop",
	} {
		if !strings.Contains(diagnostic.Message, want) {
			t.Fatalf("diagnostic message = %q, want it to mention %q", diagnostic.Message, want)
		}
	}

	if got, want := versionDoctorTestRecipeNames(report.UpgradePlan), []string{"rename-policies-to-rules", "debug-loop"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("plan recipes = %v, want %v", got, want)
	}
}

func TestDiagnoseAppPinReportsMajorMismatchAsError(t *testing.T) {
	report, err := DiagnoseAppPin(DoctorRequest{
		Pin:    "1.0",
		Schema: "0.12.0",
		Recipes: []migrations.UpgradeRecipeDescriptor{
			{Name: "future-major", FromVersion: "1.0", ToVersion: "0.12"},
		},
	})
	if err != nil {
		t.Fatalf("DiagnoseAppPin() error = %v", err)
	}

	diagnostic := versionDoctorTestSingleDiagnostic(t, report)
	if diagnostic.Code != diagnostics.CodeLazuliVersionMismatch {
		t.Fatalf("diagnostic code = %s, want %s", diagnostic.Code, diagnostics.CodeLazuliVersionMismatch)
	}
	if diagnostic.Severity != diagnostics.SeverityError {
		t.Fatalf("diagnostic severity = %s, want error", diagnostic.Severity)
	}
}

func TestDiagnoseAppPinReportsMissingMigrationPath(t *testing.T) {
	report, err := DiagnoseAppPin(DoctorRequest{
		Pin:    "0.10",
		Schema: "0.12.0",
		Recipes: []migrations.UpgradeRecipeDescriptor{
			{Name: "rename-policies-to-rules", FromVersion: "0.10", ToVersion: "0.11"},
		},
	})
	if err != nil {
		t.Fatalf("DiagnoseAppPin() error = %v", err)
	}

	diagnostic := versionDoctorTestSingleDiagnostic(t, report)
	if diagnostic.Code != diagnostics.CodeLazuliVersionNoMigrationPath {
		t.Fatalf("diagnostic code = %s, want %s", diagnostic.Code, diagnostics.CodeLazuliVersionNoMigrationPath)
	}
	if diagnostic.Severity != diagnostics.SeverityError {
		t.Fatalf("diagnostic severity = %s, want error", diagnostic.Severity)
	}
	if !strings.Contains(diagnostic.Message, "no migration path") {
		t.Fatalf("diagnostic message = %q, want missing path text", diagnostic.Message)
	}
}

func TestDiagnoseAppPinMissingPinSeverityFollowsSchemaMajor(t *testing.T) {
	tests := []struct {
		name     string
		schema   string
		severity diagnostics.Severity
	}{
		{name: "pre 1.0", schema: "0.12.0", severity: diagnostics.SeverityWarning},
		{name: "1.0", schema: "1.0.0", severity: diagnostics.SeverityError},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			report, err := DiagnoseAppPin(DoctorRequest{Schema: tt.schema})
			if err != nil {
				t.Fatalf("DiagnoseAppPin() error = %v", err)
			}

			diagnostic := versionDoctorTestSingleDiagnostic(t, report)
			if diagnostic.Code != diagnostics.CodeLazuliVersionMismatch {
				t.Fatalf("diagnostic code = %s, want %s", diagnostic.Code, diagnostics.CodeLazuliVersionMismatch)
			}
			if diagnostic.Severity != tt.severity {
				t.Fatalf("diagnostic severity = %s, want %s", diagnostic.Severity, tt.severity)
			}
			if diagnostic.Line != 1 || diagnostic.Column != 1 {
				t.Fatalf("diagnostic location = %d:%d, want 1:1", diagnostic.Line, diagnostic.Column)
			}
		})
	}
}

func TestDiagnoseAppPinRejectsPatchPin(t *testing.T) {
	report, err := DiagnoseAppPin(DoctorRequest{
		Pin:    "0.12.0",
		Schema: "0.12.1",
	})
	if err != nil {
		t.Fatalf("DiagnoseAppPin() error = %v", err)
	}

	diagnostic := versionDoctorTestSingleDiagnostic(t, report)
	if diagnostic.Code != diagnostics.CodeLazuliVersionPatchPin {
		t.Fatalf("diagnostic code = %s, want %s", diagnostic.Code, diagnostics.CodeLazuliVersionPatchPin)
	}
	if diagnostic.Severity != diagnostics.SeverityError {
		t.Fatalf("diagnostic severity = %s, want error", diagnostic.Severity)
	}
	if !strings.Contains(diagnostic.Message, "MINOR-only") {
		t.Fatalf("diagnostic message = %q, want MINOR-only guidance", diagnostic.Message)
	}
}

func TestDiagnoseAppPinPropagatesInvalidRecipeCatalog(t *testing.T) {
	_, err := DiagnoseAppPin(DoctorRequest{
		Pin:    "0.10",
		Schema: "0.12.0",
		Recipes: []migrations.UpgradeRecipeDescriptor{
			{FromVersion: "0.10", ToVersion: "0.12"},
		},
	})
	if !errors.Is(err, migrations.ErrUpgradeRecipeNameRequired) {
		t.Fatalf("DiagnoseAppPin() error = %v, want ErrUpgradeRecipeNameRequired", err)
	}
}

func versionDoctorTestSingleDiagnostic(t *testing.T, report DoctorReport) diagnostics.Diagnostic {
	t.Helper()
	if len(report.Diagnostics) != 1 {
		t.Fatalf("diagnostics = %v, want exactly one", report.Diagnostics)
	}
	return report.Diagnostics[0]
}

func versionDoctorTestRecipeNames(plan migrations.UpgradePlan) []string {
	names := make([]string, len(plan.Recipes))
	for i, recipe := range plan.Recipes {
		names[i] = recipe.Name
	}
	return names
}
