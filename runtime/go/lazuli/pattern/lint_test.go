package pattern

import (
	"reflect"
	"testing"
)

func TestLintGeneratedGoSourceAcceptsAnnotatedFunctions(t *testing.T) {
	source := `package customer

//lazuli:pattern command_pgx_insert v1
//line features/customer.lzi:42:1
func HandleCreateCustomer() {}

//lazuli:pattern query_pgx_lookup v2
func (s *Store) LookupCustomer() {}
`

	if got := LintGeneratedGoSource("customer.gen.go", source); len(got) != 0 {
		t.Fatalf("LintGeneratedGoSource() = %#v, want no diagnostics", got)
	}
}

func TestLintGeneratedGoSourceReportsMissingAnnotationsInSourceOrder(t *testing.T) {
	source := `package customer

func HandleCreateCustomer() {}

//lazuli:pattern query_pgx_lookup v1
func HandleLookupCustomer() {}

func HandleDeleteCustomer() {}
`

	got := LintGeneratedGoSource("customer.gen.go", source)
	want := []Diagnostic{
		{
			Code:    CodePatternAnnotationMissing,
			Message: "generated Go function lacks preceding //lazuli:pattern annotation",
			Path:    "customer.gen.go",
			Line:    3,
			Column:  1,
		},
		{
			Code:    CodePatternAnnotationMissing,
			Message: "generated Go function lacks preceding //lazuli:pattern annotation",
			Path:    "customer.gen.go",
			Line:    8,
			Column:  1,
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("LintGeneratedGoSource() = %#v, want %#v", got, want)
	}
}

func TestLintGeneratedGoSourceReportsInvalidAnnotation(t *testing.T) {
	source := `package customer

//lazuli:pattern custom v1
func HandleCreateCustomer() {}
`

	got := LintGeneratedGoSource("customer.gen.go", source)
	want := []Diagnostic{
		{
			Code:    CodePatternAnnotationMissing,
			Message: "generated Go function has invalid preceding //lazuli:pattern annotation",
			Path:    "customer.gen.go",
			Line:    4,
			Column:  1,
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("LintGeneratedGoSource() = %#v, want %#v", got, want)
	}
}

func TestLintGeneratedGoSourceRequiresContiguousHeader(t *testing.T) {
	source := "package customer\r\n\r\n//lazuli:pattern command_pgx_insert v1\r\n\r\nfunc HandleCreateCustomer() {}\r\n"

	got := LintGeneratedGoSource("", source)
	want := []Diagnostic{
		{
			Code:    CodePatternAnnotationMissing,
			Message: "generated Go function lacks preceding //lazuli:pattern annotation",
			Line:    5,
			Column:  1,
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("LintGeneratedGoSource() = %#v, want %#v", got, want)
	}
}

func TestLintGeneratedGoSourceIgnoresNonDeclarationFunctionText(t *testing.T) {
	source := `package customer

//lazuli:pattern command_pgx_insert v1
func HandleCreateCustomer() {
	callback := func() {}
	_ = callback
}

var body = "func NotADeclaration() {}"
`

	if got := LintGeneratedGoSource("", source); len(got) != 0 {
		t.Fatalf("LintGeneratedGoSource() = %#v, want no diagnostics", got)
	}
}
