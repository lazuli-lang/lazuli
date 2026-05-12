package pattern

import "strings"

const (
	// CodePatternAnnotationMissing reports a generated Go function without a
	// valid Lazuli pattern annotation header.
	CodePatternAnnotationMissing = "CODEGEN-PATTERN-001"
)

// Diagnostic is a deterministic source-text lint diagnostic.
type Diagnostic struct {
	Code    string
	Message string
	Path    string
	Line    int
	Column  int
}

// LintGeneratedGoSource verifies every generated Go function declaration is
// preceded by a canonical Lazuli pattern annotation.
//
// The lint is intentionally source-text based. It scans for generated
// top-level function lines beginning with "func " and allows an optional
// Go //line directive between the annotation and function.
func LintGeneratedGoSource(path, source string) []Diagnostic {
	lines := lintSplitLines(source)
	diagnostics := make([]Diagnostic, 0)

	for i, line := range lines {
		if !lintIsFunctionDeclaration(line) {
			continue
		}

		annotationLine, ok := lintPreviousAnnotationLine(lines, i)
		if !ok {
			diagnostics = append(diagnostics, lintDiagnostic(path, i+1, "generated Go function lacks preceding //lazuli:pattern annotation"))
			continue
		}

		if _, err := ParseAnnotation(annotationLine); err != nil {
			diagnostics = append(diagnostics, lintDiagnostic(path, i+1, "generated Go function has invalid preceding //lazuli:pattern annotation"))
		}
	}

	return diagnostics
}

func lintSplitLines(source string) []string {
	source = strings.ReplaceAll(source, "\r\n", "\n")
	source = strings.ReplaceAll(source, "\r", "\n")
	return strings.Split(source, "\n")
}

func lintIsFunctionDeclaration(line string) bool {
	return strings.HasPrefix(line, "func ")
}

func lintPreviousAnnotationLine(lines []string, funcIndex int) (string, bool) {
	index := funcIndex - 1
	for index >= 0 && lintIsLineDirective(lines[index]) {
		index--
	}
	if index < 0 {
		return "", false
	}

	line := strings.TrimSpace(lines[index])
	if !strings.HasPrefix(line, AnnotationPrefix) {
		return "", false
	}
	return line, true
}

func lintIsLineDirective(line string) bool {
	return strings.HasPrefix(strings.TrimSpace(line), "//line ")
}

func lintDiagnostic(path string, line int, message string) Diagnostic {
	return Diagnostic{
		Code:    CodePatternAnnotationMissing,
		Message: message,
		Path:    path,
		Line:    line,
		Column:  1,
	}
}
