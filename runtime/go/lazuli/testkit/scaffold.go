package testkit

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"unicode"
)

// ScaffoldKind describes the test scaffold shape to plan.
type ScaffoldKind string

const (
	// ScaffoldKindUnit plans a same-package unit test scaffold.
	ScaffoldKindUnit ScaffoldKind = "unit"

	// ScaffoldKindIntegration plans an external-package integration test scaffold.
	ScaffoldKindIntegration ScaffoldKind = "integration"

	// ScaffoldKindSystem plans an external-package system test scaffold.
	ScaffoldKindSystem ScaffoldKind = "system"

	// ScaffoldKindRequest plans an external-package HTTP request test scaffold.
	ScaffoldKindRequest ScaffoldKind = "request"

	// ScaffoldKindJob plans an external-package background job test scaffold.
	ScaffoldKindJob ScaffoldKind = "job"

	// ScaffoldKindAPI plans an external-package API contract test scaffold.
	ScaffoldKindAPI ScaffoldKind = "api"
)

var (
	// ErrInvalidScaffold reports an invalid test scaffold specification.
	ErrInvalidScaffold = errors.New("lazuli/testkit: invalid scaffold")

	// ErrDuplicateScaffold reports duplicate scaffold slots, tables, or planned
	// file/package pairs.
	ErrDuplicateScaffold = errors.New("lazuli/testkit: duplicate scaffold")
)

// ScaffoldSpec describes a test scaffold to plan.
type ScaffoldSpec struct {
	// Kind selects the test scaffold defaults.
	Kind ScaffoldKind

	// Name is the generated test subject, for example "customer order".
	Name string

	// PackageName is the source package under test.
	PackageName string

	// TestPackageName optionally overrides the package declaration planned for
	// the test file. Empty uses the kind default.
	TestPackageName string

	// FileName optionally overrides the planned test file name. Empty uses the
	// kind default.
	FileName string

	// TemplateSlots adds caller-specific template slots. Built-in slot names
	// cannot be overridden.
	TemplateSlots []ScaffoldSlot

	// Tables names fixture or case tables referenced by the planned scaffold.
	Tables []ScaffoldTable
}

// ScaffoldPlan is the normalized, deterministic result of planning a scaffold.
type ScaffoldPlan struct {
	Kind              ScaffoldKind
	Name              string
	FileName          string
	PackageName       string
	SourcePackageName string
	TestName          string
	TemplateSlots     []ScaffoldSlot
	Tables            []ScaffoldTable
}

// ScaffoldSlot is one named value available to a scaffold template.
type ScaffoldSlot struct {
	Name  string
	Value string
}

// ScaffoldTable describes one table referenced by a test scaffold.
type ScaffoldTable struct {
	Name  string
	Alias string
}

// NormalizeScaffoldKind returns the canonical scaffold kind spelling.
func NormalizeScaffoldKind(kind ScaffoldKind) (ScaffoldKind, error) {
	clean := ScaffoldKind(strings.ToLower(strings.TrimSpace(string(kind))))
	switch clean {
	case ScaffoldKindUnit,
		ScaffoldKindIntegration,
		ScaffoldKindSystem,
		ScaffoldKindRequest,
		ScaffoldKindJob,
		ScaffoldKindAPI:
		return clean, nil
	default:
		return "", invalidScaffold("kind", "must be one of unit, integration, system, request, job, api")
	}
}

// ValidateScaffold checks spec without mutating it.
func ValidateScaffold(spec ScaffoldSpec) error {
	_, err := PlanScaffold(spec)
	return err
}

// PlanScaffold returns a normalized scaffold plan. It only returns data and
// never creates files on disk.
func PlanScaffold(spec ScaffoldSpec) (ScaffoldPlan, error) {
	return normalizeScaffoldSpec(spec)
}

// SortedScaffolds returns validated scaffold plans in deterministic order.
//
// Plans are sorted by scaffold kind, file name, package name, then scaffold
// name. Duplicate kind/file/package combinations are rejected.
func SortedScaffolds(specs []ScaffoldSpec) ([]ScaffoldPlan, error) {
	plans := make([]ScaffoldPlan, 0, len(specs))
	seen := make(map[string]int, len(specs))

	var errs []error
	for i, spec := range specs {
		plan, err := PlanScaffold(spec)
		if err != nil {
			errs = append(errs, fmt.Errorf("scaffolds[%d]: %w", i, err))
			continue
		}

		key := scaffoldPlanKey(plan)
		if first, ok := seen[key]; ok {
			errs = append(errs, fmt.Errorf("%w: scaffolds[%d] %s in package %s also appears at scaffolds[%d]",
				ErrDuplicateScaffold, i, plan.FileName, plan.PackageName, first))
			continue
		}
		seen[key] = i
		plans = append(plans, plan)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}

	sort.SliceStable(plans, func(i, j int) bool {
		return scaffoldPlanLess(plans[i], plans[j])
	})
	return plans, nil
}

// SortedScaffoldTables returns a validated, normalized, deterministically
// sorted table copy.
func SortedScaffoldTables(tables []ScaffoldTable) ([]ScaffoldTable, error) {
	normalized := make([]ScaffoldTable, 0, len(tables))
	seen := make(map[string]int, len(tables))

	var errs []error
	for i, table := range tables {
		clean, err := normalizeScaffoldTable(table, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		key := strings.ToLower(clean.Name)
		if first, ok := seen[key]; ok {
			errs = append(errs, fmt.Errorf("%w: tables[%d] %q also appears at tables[%d]",
				ErrDuplicateScaffold, i, clean.Name, first))
			continue
		}
		seen[key] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}

	sort.SliceStable(normalized, func(i, j int) bool {
		if normalized[i].Name != normalized[j].Name {
			return normalized[i].Name < normalized[j].Name
		}
		return normalized[i].Alias < normalized[j].Alias
	})
	return normalized, nil
}

// SlotMap returns the plan template slots as a new map.
func (p ScaffoldPlan) SlotMap() map[string]string {
	slots := make(map[string]string, len(p.TemplateSlots))
	for _, slot := range p.TemplateSlots {
		slots[slot.Name] = slot.Value
	}
	return slots
}

func normalizeScaffoldSpec(spec ScaffoldSpec) (ScaffoldPlan, error) {
	var errs []error

	kind, err := NormalizeScaffoldKind(spec.Kind)
	kindValid := err == nil
	if err != nil {
		errs = append(errs, err)
	}

	name := strings.TrimSpace(spec.Name)
	if name == "" {
		errs = append(errs, invalidScaffold("name", "is required"))
	}

	sourcePackage := strings.TrimSpace(spec.PackageName)
	sourcePackageErr := validateGoPackageName("package_name", sourcePackage)
	sourcePackageValid := sourcePackageErr == nil
	if sourcePackageErr != nil {
		errs = append(errs, sourcePackageErr)
	}

	testPackage := strings.TrimSpace(spec.TestPackageName)
	if testPackage == "" && kindValid && sourcePackageValid {
		testPackage = defaultScaffoldPackageName(kind, sourcePackage)
	}
	if testPackage != "" || kindValid && sourcePackageValid {
		if err := validateGoPackageName("test_package_name", testPackage); err != nil {
			errs = append(errs, err)
		}
	}

	fileName := strings.TrimSpace(spec.FileName)
	if fileName == "" && name != "" && kindValid {
		fileName = defaultScaffoldFileName(kind, name)
	}
	if fileName != "" || kindValid && name != "" {
		if err := validateScaffoldFileName(fileName); err != nil {
			errs = append(errs, err)
		}
	}

	tables, err := SortedScaffoldTables(spec.Tables)
	if err != nil {
		errs = append(errs, err)
	}

	testName := ""
	if name != "" && kindValid {
		testName = defaultScaffoldTestName(kind, name)
	}
	slots, err := normalizeScaffoldSlots(defaultScaffoldSlots(kind, name, fileName, sourcePackage, testPackage, testName), spec.TemplateSlots)
	if err != nil {
		errs = append(errs, err)
	}

	if err := errors.Join(errs...); err != nil {
		return ScaffoldPlan{}, err
	}

	return ScaffoldPlan{
		Kind:              kind,
		Name:              name,
		FileName:          fileName,
		PackageName:       testPackage,
		SourcePackageName: sourcePackage,
		TestName:          testName,
		TemplateSlots:     slots,
		Tables:            tables,
	}, nil
}

func defaultScaffoldPackageName(kind ScaffoldKind, sourcePackage string) string {
	if kind == ScaffoldKindUnit || strings.HasSuffix(sourcePackage, "_test") {
		return sourcePackage
	}
	return sourcePackage + "_test"
}

func defaultScaffoldFileName(kind ScaffoldKind, name string) string {
	suffix := "_test.go"
	switch kind {
	case ScaffoldKindIntegration:
		suffix = "_integration_test.go"
	case ScaffoldKindSystem:
		suffix = "_system_test.go"
	case ScaffoldKindRequest:
		suffix = "_request_test.go"
	case ScaffoldKindJob:
		suffix = "_job_test.go"
	case ScaffoldKindAPI:
		suffix = "_api_test.go"
	}
	return scaffoldFileStem(name) + suffix
}

func defaultScaffoldTestName(kind ScaffoldKind, name string) string {
	testName := "Test" + exportedScaffoldName(name)
	switch kind {
	case ScaffoldKindIntegration:
		return testName + "Integration"
	case ScaffoldKindSystem:
		return testName + "System"
	case ScaffoldKindRequest:
		return testName + "Request"
	case ScaffoldKindJob:
		return testName + "Job"
	case ScaffoldKindAPI:
		return testName + "API"
	default:
		return testName
	}
}

func defaultScaffoldSlots(kind ScaffoldKind, name, fileName, sourcePackage, testPackage, testName string) []ScaffoldSlot {
	if kind == "" || name == "" || fileName == "" || sourcePackage == "" || testPackage == "" || testName == "" {
		return nil
	}
	return []ScaffoldSlot{
		{Name: "FileName", Value: fileName},
		{Name: "Kind", Value: string(kind)},
		{Name: "Name", Value: name},
		{Name: "PackageName", Value: testPackage},
		{Name: "SourcePackageName", Value: sourcePackage},
		{Name: "TestName", Value: testName},
	}
}

func normalizeScaffoldSlots(defaults, slots []ScaffoldSlot) ([]ScaffoldSlot, error) {
	combined := make([]ScaffoldSlot, 0, len(defaults)+len(slots))
	combined = append(combined, defaults...)
	combined = append(combined, slots...)

	normalized := make([]ScaffoldSlot, 0, len(combined))
	seen := make(map[string]scaffoldSlotPosition, len(combined))

	var errs []error
	for i, slot := range combined {
		callerSlot := i >= len(defaults)
		slotIndex := i
		if callerSlot {
			slotIndex -= len(defaults)
		}
		clean, err := normalizeScaffoldSlot(slot, slotIndex, callerSlot)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		if first, ok := seen[clean.Name]; ok {
			errs = append(errs, fmt.Errorf("%w: %s name %q also appears at %s",
				ErrDuplicateScaffold, scaffoldSlotField(slotIndex, callerSlot), clean.Name, scaffoldSlotField(first.index, first.callerSlot)))
			continue
		}
		seen[clean.Name] = scaffoldSlotPosition{index: slotIndex, callerSlot: callerSlot}
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}

	sort.SliceStable(normalized, func(i, j int) bool {
		return normalized[i].Name < normalized[j].Name
	})
	return normalized, nil
}

type scaffoldSlotPosition struct {
	index      int
	callerSlot bool
}

func normalizeScaffoldSlot(slot ScaffoldSlot, index int, callerSlot bool) (ScaffoldSlot, error) {
	clean := ScaffoldSlot{
		Name:  strings.TrimSpace(slot.Name),
		Value: strings.TrimSpace(slot.Value),
	}
	if clean.Name == "" {
		return ScaffoldSlot{}, invalidScaffold(scaffoldSlotField(index, callerSlot), "name is required")
	}
	if !safeGoIdentifier(clean.Name) {
		return ScaffoldSlot{}, invalidScaffold(scaffoldSlotField(index, callerSlot)+".name", "must be a Go identifier")
	}
	return clean, nil
}

func scaffoldSlotField(index int, callerSlot bool) string {
	if callerSlot {
		return fmt.Sprintf("template_slots[%d]", index)
	}
	return fmt.Sprintf("default_template_slots[%d]", index)
}

func normalizeScaffoldTable(table ScaffoldTable, index int) (ScaffoldTable, error) {
	clean := ScaffoldTable{
		Name:  strings.TrimSpace(table.Name),
		Alias: strings.TrimSpace(table.Alias),
	}

	var errs []error
	if clean.Name == "" {
		errs = append(errs, invalidScaffold(fmt.Sprintf("tables[%d].name", index), "is required"))
	} else if !safeScaffoldTableName(clean.Name) {
		errs = append(errs, invalidScaffold(fmt.Sprintf("tables[%d].name", index), "must contain only letters, digits, underscores, dots, or dashes"))
	}
	if clean.Alias != "" && !safeScaffoldTableName(clean.Alias) {
		errs = append(errs, invalidScaffold(fmt.Sprintf("tables[%d].alias", index), "must contain only letters, digits, underscores, dots, or dashes"))
	}

	if err := errors.Join(errs...); err != nil {
		return ScaffoldTable{}, err
	}
	return clean, nil
}

func scaffoldPlanKey(plan ScaffoldPlan) string {
	return string(plan.Kind) + "\x00" + plan.FileName + "\x00" + plan.PackageName
}

func scaffoldPlanLess(a, b ScaffoldPlan) bool {
	if scaffoldKindOrder(a.Kind) != scaffoldKindOrder(b.Kind) {
		return scaffoldKindOrder(a.Kind) < scaffoldKindOrder(b.Kind)
	}
	if a.FileName != b.FileName {
		return a.FileName < b.FileName
	}
	if a.PackageName != b.PackageName {
		return a.PackageName < b.PackageName
	}
	return a.Name < b.Name
}

func scaffoldKindOrder(kind ScaffoldKind) int {
	switch kind {
	case ScaffoldKindUnit:
		return 0
	case ScaffoldKindIntegration:
		return 1
	case ScaffoldKindSystem:
		return 2
	case ScaffoldKindRequest:
		return 3
	case ScaffoldKindJob:
		return 4
	case ScaffoldKindAPI:
		return 5
	default:
		return 99
	}
}

func validateGoPackageName(field, name string) error {
	if name == "" {
		return invalidScaffold(field, "is required")
	}
	if !safeGoIdentifier(name) || goKeyword(name) {
		return invalidScaffold(field, "must be a Go package identifier")
	}
	return nil
}

func validateScaffoldFileName(name string) error {
	if name == "" {
		return invalidScaffold("file_name", "is required")
	}
	if strings.ContainsAny(name, `/\`) || name == "." || name == ".." || strings.Contains(name, "..") {
		return invalidScaffold("file_name", "must be a file name, not a path")
	}
	if !strings.HasSuffix(name, "_test.go") {
		return invalidScaffold("file_name", "must end with _test.go")
	}
	for _, r := range name {
		if !(r == '.' || r == '-' || r == '_' || r >= '0' && r <= '9' || r >= 'A' && r <= 'Z' || r >= 'a' && r <= 'z') {
			return invalidScaffold("file_name", "must contain only letters, digits, dots, dashes, or underscores")
		}
	}
	return nil
}

func scaffoldFileStem(name string) string {
	var b strings.Builder
	lastUnderscore := false
	for _, r := range strings.TrimSpace(name) {
		switch {
		case r >= 'A' && r <= 'Z':
			b.WriteRune(r + ('a' - 'A'))
			lastUnderscore = false
		case r >= 'a' && r <= 'z' || r >= '0' && r <= '9':
			b.WriteRune(r)
			lastUnderscore = false
		default:
			if !lastUnderscore && b.Len() > 0 {
				b.WriteByte('_')
				lastUnderscore = true
			}
		}
	}
	stem := strings.Trim(b.String(), "_")
	if stem == "" {
		return "scaffold"
	}
	return stem
}

func exportedScaffoldName(name string) string {
	var b strings.Builder
	upperNext := true
	for _, r := range strings.TrimSpace(name) {
		if r >= 'A' && r <= 'Z' || r >= 'a' && r <= 'z' || r >= '0' && r <= '9' {
			if upperNext && r >= 'a' && r <= 'z' {
				r -= 'a' - 'A'
			}
			b.WriteRune(r)
			upperNext = false
			continue
		}
		upperNext = true
	}
	if b.Len() == 0 {
		return "Scaffold"
	}
	return b.String()
}

func safeGoIdentifier(name string) bool {
	if name == "" {
		return false
	}
	for i, r := range name {
		if i == 0 {
			if r != '_' && !unicode.IsLetter(r) {
				return false
			}
			continue
		}
		if r != '_' && !unicode.IsLetter(r) && !unicode.IsDigit(r) {
			return false
		}
	}
	return true
}

func safeScaffoldTableName(name string) bool {
	if name == "." || name == ".." || strings.Contains(name, "..") {
		return false
	}
	for _, r := range name {
		if r == '.' || r == '-' || r == '_' || unicode.IsLetter(r) || unicode.IsDigit(r) {
			continue
		}
		return false
	}
	return true
}

func goKeyword(name string) bool {
	switch name {
	case "break", "default", "func", "interface", "select",
		"case", "defer", "go", "map", "struct",
		"chan", "else", "goto", "package", "switch",
		"const", "fallthrough", "if", "range", "type",
		"continue", "for", "import", "return", "var":
		return true
	default:
		return false
	}
}

func invalidScaffold(field, detail string) error {
	return fmt.Errorf("%w: %s %s", ErrInvalidScaffold, field, detail)
}
