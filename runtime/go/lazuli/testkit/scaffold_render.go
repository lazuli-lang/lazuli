package testkit

import (
	"errors"
	"fmt"
	"go/format"
	"sort"
	"strconv"
	"strings"
)

// RenderScaffold returns deterministic Go test source for a planned scaffold.
//
// The returned content is formatted Go source and is not written to disk.
func RenderScaffold(plan ScaffoldPlan) (string, error) {
	if err := validateScaffoldPlanForRender(plan); err != nil {
		return "", err
	}

	var b strings.Builder
	fmt.Fprintf(&b, "package %s\n\n", plan.PackageName)
	renderScaffoldImports(&b, scaffoldImports(plan.Kind))
	renderScaffoldTables(&b, plan)
	renderScaffoldTest(&b, plan)

	src, err := format.Source([]byte(b.String()))
	if err != nil {
		return "", fmt.Errorf("%w: render generated invalid Go source: %v", ErrInvalidScaffold, err)
	}
	return string(src), nil
}

// RenderedScaffold is one in-memory Go test file rendered from a scaffold plan.
type RenderedScaffold struct {
	FileName string
	Content  string
}

// RenderScaffolds returns deterministic in-memory Go test files for plans.
//
// The returned files are sorted by scaffold kind, file name, package name, and
// scaffold name. No files are written to disk.
func RenderScaffolds(plans []ScaffoldPlan) ([]RenderedScaffold, error) {
	sorted := append([]ScaffoldPlan(nil), plans...)
	sort.SliceStable(sorted, func(i, j int) bool {
		return scaffoldPlanLess(sorted[i], sorted[j])
	})

	rendered := make([]RenderedScaffold, 0, len(sorted))
	for i, plan := range sorted {
		if err := validateScaffoldFileName(plan.FileName); err != nil {
			return nil, fmt.Errorf("scaffolds[%d]: %w", i, err)
		}
		content, err := RenderScaffold(plan)
		if err != nil {
			return nil, fmt.Errorf("scaffolds[%d]: %w", i, err)
		}
		rendered = append(rendered, RenderedScaffold{
			FileName: plan.FileName,
			Content:  content,
		})
	}
	return rendered, nil
}

func validateScaffoldPlanForRender(plan ScaffoldPlan) error {
	var errs []error
	if _, err := NormalizeScaffoldKind(plan.Kind); err != nil {
		errs = append(errs, err)
	}
	switch plan.Kind {
	case ScaffoldKindUnit, ScaffoldKindIntegration, ScaffoldKindRequest, ScaffoldKindJob, ScaffoldKindAPI:
	case ScaffoldKindSystem:
		errs = append(errs, invalidScaffold("kind", "cannot render system scaffold"))
	}
	if err := validateGoPackageName("package_name", strings.TrimSpace(plan.PackageName)); err != nil {
		errs = append(errs, err)
	}
	if strings.TrimSpace(plan.TestName) == "" {
		errs = append(errs, invalidScaffold("test_name", "is required"))
	} else if !safeGoIdentifier(plan.TestName) {
		errs = append(errs, invalidScaffold("test_name", "must be a Go identifier"))
	}
	if _, err := SortedScaffoldTables(plan.Tables); err != nil {
		errs = append(errs, err)
	}
	return errors.Join(errs...)
}

func scaffoldImports(kind ScaffoldKind) []string {
	imports := []string{"testing"}
	switch kind {
	case ScaffoldKindIntegration, ScaffoldKindJob:
		imports = append(imports, "context")
	case ScaffoldKindRequest:
		imports = append(imports, "net/http", "net/http/httptest")
	case ScaffoldKindAPI:
		imports = append(imports, "encoding/json")
	}
	sort.Strings(imports)
	return imports
}

func renderScaffoldImports(b *strings.Builder, imports []string) {
	b.WriteString("import (\n")
	for _, path := range imports {
		fmt.Fprintf(b, "\t%q\n", path)
	}
	b.WriteString(")\n\n")
}

func renderScaffoldTables(b *strings.Builder, plan ScaffoldPlan) {
	if len(plan.Tables) == 0 {
		return
	}

	tables, _ := SortedScaffoldTables(plan.Tables)
	fmt.Fprintf(b, "var %sTables = []struct {\n", scaffoldCasePrefix(plan.TestName))
	b.WriteString("\tName string\n")
	b.WriteString("\tAlias string\n")
	b.WriteString("}{\n")
	for _, table := range tables {
		fmt.Fprintf(b, "\t{Name: %s, Alias: %s},\n", strconv.Quote(table.Name), strconv.Quote(table.Alias))
	}
	b.WriteString("}\n\n")
}

func renderScaffoldTest(b *strings.Builder, plan ScaffoldPlan) {
	fmt.Fprintf(b, "func %s(t *testing.T) {\n", plan.TestName)
	b.WriteString("\tt.Parallel()\n")
	renderScaffoldKindSetup(b, plan.Kind)
	fmt.Fprintf(b, "\ttests := []struct {\n")
	b.WriteString("\t\tname string\n")
	b.WriteString("\t}{\n")
	b.WriteString("\t\t{name: \"TODO: describe behavior\"},\n")
	b.WriteString("\t}\n\n")
	b.WriteString("\tfor _, tt := range tests {\n")
	b.WriteString("\t\tt.Run(tt.name, func(t *testing.T) {\n")
	fmt.Fprintf(b, "\t\t\tt.Skip(%q)\n", "TODO: implement "+string(plan.Kind)+" scaffold")
	b.WriteString("\t\t})\n")
	b.WriteString("\t}\n")
	b.WriteString("}\n")
}

func renderScaffoldKindSetup(b *strings.Builder, kind ScaffoldKind) {
	switch kind {
	case ScaffoldKindIntegration:
		b.WriteString("\tctx := context.Background()\n")
		b.WriteString("\t_ = ctx\n\n")
	case ScaffoldKindRequest:
		b.WriteString("\treq := httptest.NewRequest(http.MethodGet, \"/\", nil)\n")
		b.WriteString("\trec := httptest.NewRecorder()\n")
		b.WriteString("\t_ = req\n")
		b.WriteString("\t_ = rec\n\n")
	case ScaffoldKindJob:
		b.WriteString("\tctx := context.Background()\n")
		b.WriteString("\t_ = ctx\n\n")
	case ScaffoldKindAPI:
		b.WriteString("\tpayload := []byte(`{}`)\n")
		b.WriteString("\tif !json.Valid(payload) {\n")
		b.WriteString("\t\tt.Fatal(\"invalid placeholder JSON\")\n")
		b.WriteString("\t}\n\n")
	}
}

func scaffoldCasePrefix(testName string) string {
	clean := strings.TrimPrefix(testName, "Test")
	if clean == "" {
		clean = "Scaffold"
	}
	return strings.ToLower(clean[:1]) + clean[1:]
}
