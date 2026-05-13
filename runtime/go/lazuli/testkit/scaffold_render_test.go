package testkit_test

import (
	"errors"
	"go/parser"
	"go/token"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/testkit"
)

func TestRenderScaffoldByKind(t *testing.T) {
	tests := []struct {
		name     string
		kind     testkit.ScaffoldKind
		contains []string
	}{
		{
			name: "unit",
			kind: testkit.ScaffoldKindUnit,
			contains: []string{
				"package orders",
				"\"testing\"",
				"func TestCustomerOrder(t *testing.T)",
				"t.Skip(\"TODO: implement unit scaffold\")",
			},
		},
		{
			name: "integration",
			kind: testkit.ScaffoldKindIntegration,
			contains: []string{
				"package orders_test",
				"\"context\"",
				"ctx := context.Background()",
				"func TestCustomerOrderIntegration(t *testing.T)",
			},
		},
		{
			name: "request",
			kind: testkit.ScaffoldKindRequest,
			contains: []string{
				"\"net/http\"",
				"\"net/http/httptest\"",
				"req := httptest.NewRequest(http.MethodGet, \"/\", nil)",
				"rec := httptest.NewRecorder()",
			},
		},
		{
			name: "job",
			kind: testkit.ScaffoldKindJob,
			contains: []string{
				"\"context\"",
				"t.Skip(\"TODO: implement job scaffold\")",
			},
		},
		{
			name: "api",
			kind: testkit.ScaffoldKindAPI,
			contains: []string{
				"\"encoding/json\"",
				"payload := []byte(`{}`)",
				"if !json.Valid(payload)",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			plan, err := testkit.PlanScaffold(testkit.ScaffoldSpec{
				Kind:        tt.kind,
				Name:        "Customer Order",
				PackageName: "orders",
			})
			if err != nil {
				t.Fatalf("PlanScaffold() error = %v", err)
			}

			src, err := testkit.RenderScaffold(plan)
			if err != nil {
				t.Fatalf("RenderScaffold() error = %v", err)
			}
			assertGoSource(t, src)
			for _, fragment := range tt.contains {
				if !strings.Contains(src, fragment) {
					t.Fatalf("RenderScaffold() missing %q in:\n%s", fragment, src)
				}
			}
		})
	}
}

func TestRenderScaffoldRendersSortedTables(t *testing.T) {
	plan, err := testkit.PlanScaffold(testkit.ScaffoldSpec{
		Kind:        testkit.ScaffoldKindAPI,
		Name:        "Charge Card",
		PackageName: "billing",
		Tables: []testkit.ScaffoldTable{
			{Name: "zeta", Alias: "z"},
			{Name: "alpha", Alias: "a"},
		},
	})
	if err != nil {
		t.Fatalf("PlanScaffold() error = %v", err)
	}

	src, err := testkit.RenderScaffold(plan)
	if err != nil {
		t.Fatalf("RenderScaffold() error = %v", err)
	}
	assertGoSource(t, src)

	alpha := strings.Index(src, `{Name: "alpha", Alias: "a"}`)
	zeta := strings.Index(src, `{Name: "zeta", Alias: "z"}`)
	if alpha == -1 || zeta == -1 || alpha > zeta {
		t.Fatalf("tables were not rendered in sorted order:\n%s", src)
	}
	if !strings.Contains(src, "var chargeCardAPITables = []struct") {
		t.Fatalf("RenderScaffold() missing table placeholder:\n%s", src)
	}
}

func TestRenderScaffoldIsDeterministic(t *testing.T) {
	planned, err := testkit.PlanScaffold(testkit.ScaffoldSpec{
		Kind:        testkit.ScaffoldKindRequest,
		Name:        "List Customers",
		PackageName: "customers",
		Tables: []testkit.ScaffoldTable{
			{Name: "requests"},
		},
	})
	if err != nil {
		t.Fatalf("PlanScaffold() error = %v", err)
	}
	plan := planned
	plan.Tables = []testkit.ScaffoldTable{
		{Name: "zeta"},
		{Name: "alpha"},
	}

	first, err := testkit.RenderScaffold(plan)
	if err != nil {
		t.Fatalf("RenderScaffold() first error = %v", err)
	}
	second, err := testkit.RenderScaffold(plan)
	if err != nil {
		t.Fatalf("RenderScaffold() second error = %v", err)
	}
	if first != second {
		t.Fatalf("RenderScaffold() is not deterministic\nfirst:\n%s\nsecond:\n%s", first, second)
	}
	if strings.Index(first, `{Name: "alpha", Alias: ""}`) > strings.Index(first, `{Name: "zeta", Alias: ""}`) {
		t.Fatalf("RenderScaffold() did not normalize table order:\n%s", first)
	}
}

func TestRenderScaffoldsSortsPlansAndReturnsFiles(t *testing.T) {
	requestPlan, err := testkit.PlanScaffold(testkit.ScaffoldSpec{
		Kind:        testkit.ScaffoldKindRequest,
		Name:        "List Customers",
		PackageName: "customers",
	})
	if err != nil {
		t.Fatalf("PlanScaffold(request) error = %v", err)
	}
	unitPlan, err := testkit.PlanScaffold(testkit.ScaffoldSpec{
		Kind:        testkit.ScaffoldKindUnit,
		Name:        "Alpha",
		PackageName: "customers",
	})
	if err != nil {
		t.Fatalf("PlanScaffold(unit) error = %v", err)
	}

	files, err := testkit.RenderScaffolds([]testkit.ScaffoldPlan{requestPlan, unitPlan})
	if err != nil {
		t.Fatalf("RenderScaffolds() error = %v", err)
	}
	if len(files) != 2 {
		t.Fatalf("RenderScaffolds() returned %d files, want 2", len(files))
	}
	if files[0].FileName != "alpha_test.go" || files[1].FileName != "list_customers_request_test.go" {
		t.Fatalf("RenderScaffolds() files = %#v, want sorted scaffold files", files)
	}
	for _, file := range files {
		assertGoSource(t, file.Content)
	}
}

func TestRenderScaffoldRejectsInvalidPlan(t *testing.T) {
	_, err := testkit.RenderScaffold(testkit.ScaffoldPlan{
		Kind:        testkit.ScaffoldKindSystem,
		PackageName: "orders_test",
		TestName:    "TestOrdersSystem",
	})
	if !errors.Is(err, testkit.ErrInvalidScaffold) {
		t.Fatalf("RenderScaffold() error = %v, want ErrInvalidScaffold", err)
	}
	if !strings.Contains(err.Error(), "kind cannot render system scaffold") {
		t.Fatalf("RenderScaffold() error = %q, want system unsupported context", err)
	}

	_, err = testkit.RenderScaffold(testkit.ScaffoldPlan{
		Kind:        testkit.ScaffoldKindUnit,
		PackageName: "bad-name",
		TestName:    "bad-name",
	})
	if !errors.Is(err, testkit.ErrInvalidScaffold) {
		t.Fatalf("RenderScaffold() error = %v, want ErrInvalidScaffold", err)
	}
	for _, fragment := range []string{
		"package_name must be a Go package identifier",
		"test_name must be a Go identifier",
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("RenderScaffold() error = %q, want fragment %q", err, fragment)
		}
	}
}

func assertGoSource(t *testing.T, src string) {
	t.Helper()
	if _, err := parser.ParseFile(token.NewFileSet(), "scaffold_test.go", src, parser.AllErrors); err != nil {
		t.Fatalf("generated source is not parseable: %v\n%s", err, src)
	}
}
