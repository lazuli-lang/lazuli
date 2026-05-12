// Package diagnostics defines the stable diagnostic code catalog shared by
// Lazuli runtime tooling.
package diagnostics

import (
	"sort"
	"strings"
)

// Severity is the closed catalog of diagnostic severities.
type Severity uint8

const (
	SeverityError Severity = iota + 1
	SeverityWarning
	SeverityInfo
	SeverityHint
)

// String renders s as the stable lowercase token used in diagnostic output.
func (s Severity) String() string {
	switch s {
	case SeverityError:
		return "error"
	case SeverityWarning:
		return "warning"
	case SeverityInfo:
		return "info"
	case SeverityHint:
		return "hint"
	default:
		return "unknown"
	}
}

// Code is a stable diagnostic identifier such as LAZULI-VERSION-001.
type Code string

const (
	CodeLazuliVersionMismatch        Code = "LAZULI-VERSION-001"
	CodeLazuliVersionNoMigrationPath Code = "LAZULI-VERSION-002"
	CodeLazuliVersionPatchPin        Code = "LAZULI-VERSION-003"

	CodeMigrationRecipeMissing   Code = "MIGRATION-RECIPE-001"
	CodeMigrationRecipeRoundTrip Code = "MIGRATION-RECIPE-002"

	CodeCodegenSentinelUnknown Code = "CODEGEN-SENTINEL-001"
)

// String returns c as its stable diagnostic identifier.
func (c Code) String() string {
	return string(c)
}

// Family returns the code family prefix, for example LAZULI-VERSION.
func (c Code) Family() Family {
	if definition, ok := Lookup(c); ok {
		return definition.Family
	}
	value := string(c)
	if value == "" {
		return FamilyUnknown
	}
	if index := strings.LastIndex(value, "-"); index > 0 {
		return Family(value[:index])
	}
	return FamilyUnknown
}

// Family is a diagnostic code namespace.
type Family string

const (
	FamilyLazuliVersion   Family = "LAZULI-VERSION"
	FamilyMigrationRecipe Family = "MIGRATION-RECIPE"
	FamilyCodegenSentinel Family = "CODEGEN-SENTINEL"
	FamilyUnknown         Family = "UNKNOWN"
)

// Diagnostic is a single diagnostic emitted by Lazuli tooling.
type Diagnostic struct {
	Code     Code
	Severity Severity
	Message  string
	Path     string
	Line     int
	Column   int
}

// CodeDefinition describes one entry in the closed diagnostic catalog.
type CodeDefinition struct {
	Code            Code
	Family          Family
	DefaultSeverity Severity
	Surface         string
	Summary         string
}

// DiagnosticGroup is a stable group of diagnostics keyed by code, family, or
// severity.
type DiagnosticGroup struct {
	Key         string
	Diagnostics []Diagnostic
}

var catalogDefinitions = []CodeDefinition{
	{
		Code:            CodeLazuliVersionMismatch,
		Family:          FamilyLazuliVersion,
		DefaultSeverity: SeverityWarning,
		Surface:         "app.lzi:lazuli_version",
		Summary:         "lazuli_version pin is missing or does not match LZIR_SCHEMA at minor granularity",
	},
	{
		Code:            CodeLazuliVersionNoMigrationPath,
		Family:          FamilyLazuliVersion,
		DefaultSeverity: SeverityError,
		Surface:         "app.lzi:lazuli_version",
		Summary:         "pinned lazuli_version has no migration path to the current LZIR_SCHEMA",
	},
	{
		Code:            CodeLazuliVersionPatchPin,
		Family:          FamilyLazuliVersion,
		DefaultSeverity: SeverityError,
		Surface:         "app.lzi:lazuli_version",
		Summary:         "lazuli_version pin uses a patch-level three-segment form",
	},
	{
		Code:            CodeMigrationRecipeMissing,
		Family:          FamilyMigrationRecipe,
		DefaultSeverity: SeverityError,
		Surface:         "CI gate",
		Summary:         "LZIR_SCHEMA was bumped without a migration recipe directory",
	},
	{
		Code:            CodeMigrationRecipeRoundTrip,
		Family:          FamilyMigrationRecipe,
		DefaultSeverity: SeverityError,
		Surface:         "CI gate",
		Summary:         "migration recipe fixture does not round-trip through lazuli upgrade",
	},
	{
		Code:            CodeCodegenSentinelUnknown,
		Family:          FamilyCodegenSentinel,
		DefaultSeverity: SeverityError,
		Surface:         "codegen-go emitter",
		Summary:         "emitted handler returns a sentinel outside the codegen sentinel catalog",
	},
}

var catalogDefinitionsByCode = func() map[Code]CodeDefinition {
	index := make(map[Code]CodeDefinition, len(catalogDefinitions))
	for _, definition := range catalogDefinitions {
		index[definition.Code] = definition
	}
	return index
}()

// Catalog returns a copy of the closed diagnostic code catalog.
func Catalog() []CodeDefinition {
	out := make([]CodeDefinition, len(catalogDefinitions))
	copy(out, catalogDefinitions)
	return out
}

// Lookup returns the catalog definition for code.
func Lookup(code Code) (CodeDefinition, bool) {
	definition, ok := catalogDefinitionsByCode[code]
	return definition, ok
}

// SortStable sorts diagnostics by path, line, column, severity, and code while
// preserving input order for diagnostics with identical sort keys.
func SortStable(diagnostics []Diagnostic) {
	sort.SliceStable(diagnostics, func(i, j int) bool {
		return catalogLessDiagnostic(diagnostics[i], diagnostics[j])
	})
}

// Sorted returns a sorted copy of diagnostics.
func Sorted(diagnostics []Diagnostic) []Diagnostic {
	out := make([]Diagnostic, len(diagnostics))
	copy(out, diagnostics)
	SortStable(out)
	return out
}

// GroupByCode groups diagnostics by Code in first-seen group order.
func GroupByCode(diagnostics []Diagnostic) []DiagnosticGroup {
	return catalogGroupDiagnostics(diagnostics, func(diagnostic Diagnostic) string {
		return diagnostic.Code.String()
	})
}

// GroupByFamily groups diagnostics by Code family in first-seen group order.
func GroupByFamily(diagnostics []Diagnostic) []DiagnosticGroup {
	return catalogGroupDiagnostics(diagnostics, func(diagnostic Diagnostic) string {
		return string(diagnostic.Code.Family())
	})
}

// GroupBySeverity groups diagnostics by Severity in first-seen group order.
func GroupBySeverity(diagnostics []Diagnostic) []DiagnosticGroup {
	return catalogGroupDiagnostics(diagnostics, func(diagnostic Diagnostic) string {
		return diagnostic.Severity.String()
	})
}

func catalogLessDiagnostic(a, b Diagnostic) bool {
	if a.Path != b.Path {
		return a.Path < b.Path
	}
	if a.Line != b.Line {
		return a.Line < b.Line
	}
	if a.Column != b.Column {
		return a.Column < b.Column
	}
	if catalogSeverityRank(a.Severity) != catalogSeverityRank(b.Severity) {
		return catalogSeverityRank(a.Severity) < catalogSeverityRank(b.Severity)
	}
	if a.Code != b.Code {
		return a.Code < b.Code
	}
	return false
}

func catalogSeverityRank(severity Severity) int {
	switch severity {
	case SeverityError:
		return 0
	case SeverityWarning:
		return 1
	case SeverityInfo:
		return 2
	case SeverityHint:
		return 3
	default:
		return 4
	}
}

func catalogGroupDiagnostics(diagnostics []Diagnostic, keyFor func(Diagnostic) string) []DiagnosticGroup {
	groups := make([]DiagnosticGroup, 0)
	byKey := make(map[string]int)
	for _, diagnostic := range diagnostics {
		key := keyFor(diagnostic)
		index, ok := byKey[key]
		if !ok {
			index = len(groups)
			byKey[key] = index
			groups = append(groups, DiagnosticGroup{Key: key})
		}
		groups[index].Diagnostics = append(groups[index].Diagnostics, diagnostic)
	}
	return groups
}
