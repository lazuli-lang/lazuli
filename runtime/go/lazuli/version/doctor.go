package version

import (
	"errors"
	"fmt"
	"strings"

	"lazuli.dev/runtime/lazuli/diagnostics"
	"lazuli.dev/runtime/lazuli/migrations"
)

// DoctorRequest is the app.lzi lazuli_version state visible to doctor.
type DoctorRequest struct {
	// Pin is the raw lazuli_version value from app.lzi. Empty means absent.
	Pin string
	// Schema is the runtime LZIR_SCHEMA value in MAJOR.MINOR.PATCH form.
	Schema string
	// AppPath is copied onto emitted diagnostics.
	AppPath string
	// Line and Column identify the lazuli_version token when known. Values
	// less than one default to 1.
	Line   int
	Column int
	// Recipes are the available migration recipes doctor may suggest.
	Recipes []migrations.UpgradeRecipeDescriptor
}

// DoctorReport is the doctor-facing verdict for an app.lzi lazuli_version pin.
type DoctorReport struct {
	Check       CheckResult
	Diagnostics []diagnostics.Diagnostic
	UpgradePlan migrations.UpgradePlan
}

// DiagnoseAppPin evaluates an app.lzi lazuli_version pin against the runtime
// schema and available migration recipes.
func DiagnoseAppPin(request DoctorRequest) (DoctorReport, error) {
	planner := migrations.NewUpgradePlanner(request.Recipes)
	var planErr error

	check, err := CheckPin(request.Pin, request.Schema, func(from, to MinorPin) bool {
		_, err := planner.Plan(from.String(), to.String())
		if err == nil {
			return true
		}
		if errors.Is(err, migrations.ErrUpgradePathNotFound) {
			return false
		}
		planErr = err
		return false
	})
	report := DoctorReport{Check: check}
	if err != nil {
		return report, err
	}
	if planErr != nil {
		return report, planErr
	}

	if check.Code == CodePinMismatch && strings.TrimSpace(request.Pin) != "" {
		plan, err := planner.Plan(check.Pin.String(), check.Schema.MinorPin().String())
		if err != nil {
			return report, err
		}
		report.UpgradePlan = plan
	}

	if !check.OK() {
		report.Diagnostics = append(report.Diagnostics, versionDoctorDiagnostic(request, check, report.UpgradePlan))
	}
	return report, nil
}

func versionDoctorDiagnostic(request DoctorRequest, check CheckResult, plan migrations.UpgradePlan) diagnostics.Diagnostic {
	return diagnostics.Diagnostic{
		Code:     diagnostics.Code(check.Code),
		Severity: versionDoctorSeverity(check),
		Message:  versionDoctorMessage(request, check, plan),
		Path:     request.AppPath,
		Line:     versionDoctorPositiveLocation(request.Line),
		Column:   versionDoctorPositiveLocation(request.Column),
	}
}

func versionDoctorSeverity(check CheckResult) diagnostics.Severity {
	switch check.Code {
	case CodePinMismatch:
		if check.Schema.Major >= 1 || check.Pin.Major != check.Schema.Major {
			return diagnostics.SeverityError
		}
		return diagnostics.SeverityWarning
	case CodeMigrationPathMissing, CodePatchPinRejected:
		return diagnostics.SeverityError
	default:
		return diagnostics.SeverityInfo
	}
}

func versionDoctorMessage(request DoctorRequest, check CheckResult, plan migrations.UpgradePlan) string {
	pin := strings.TrimSpace(request.Pin)
	schemaPin := check.Schema.MinorPin().String()

	switch check.Code {
	case CodePinMismatch:
		if pin == "" {
			return fmt.Sprintf("app.lzi is missing lazuli_version; write lazuli_version %q to match LZIR_SCHEMA %s.", schemaPin, check.Schema)
		}

		recipePaths := versionDoctorPlanRecipePaths(plan)
		if len(recipePaths) == 0 {
			return fmt.Sprintf("lazuli_version %q does not match LZIR_SCHEMA %s; run the matching migration recipe, then update the pin to %q.", pin, check.Schema, schemaPin)
		}
		return fmt.Sprintf("lazuli_version %q does not match LZIR_SCHEMA %s; run migration recipe path(s) %s, then update the pin to %q.", pin, check.Schema, strings.Join(recipePaths, ", "), schemaPin)
	case CodeMigrationPathMissing:
		return fmt.Sprintf("lazuli_version %q has no migration path to LZIR_SCHEMA %s.", pin, check.Schema)
	case CodePatchPinRejected:
		return fmt.Sprintf("lazuli_version %q uses patch-level form; write MINOR-only %q.", pin, schemaPin)
	default:
		return "lazuli_version policy check failed."
	}
}

func versionDoctorPlanRecipePaths(plan migrations.UpgradePlan) []string {
	paths := make([]string, 0, len(plan.Recipes))
	for _, recipe := range plan.Recipes {
		paths = append(paths, versionDoctorRecipePath(recipe))
	}
	return paths
}

func versionDoctorRecipePath(recipe migrations.UpgradeRecipeDescriptor) string {
	if path := strings.TrimSpace(recipe.Path); path != "" {
		return path
	}
	fromVersion := strings.TrimSpace(recipe.FromVersion)
	toVersion := strings.TrimSpace(recipe.ToVersion)
	name := strings.TrimSpace(recipe.Name)
	if fromVersion == "" || toVersion == "" || name == "" {
		return name
	}
	return fmt.Sprintf("migrations/recipes/%s-to-%s/%s", fromVersion, toVersion, name)
}

func versionDoctorPositiveLocation(value int) int {
	if value < 1 {
		return 1
	}
	return value
}
