package lazuli

import (
	"os"
	"strings"
	"testing"
)

// API-POLICY-UNENFORCED-001 — codegen↔runtime policy-enforcement parity.
//
// Every request-surface contract that carries a `Policy` field must reach
// an `EvalPolicy*` call on its runtime dispatch path. This is the static
// guard for the SEC-API-POLICY-NULL class: a future surface added with a
// `Policy` field but no evaluator (the exact shape of the api bug) trips
// this test at build time rather than shipping an unauthorized endpoint.
//
// Implemented as a source-parity invariant in the same family as
// http_classifier_parity_test.go: it reads the dispatch source files and
// asserts each one references EvalPolicy near its primary dispatch entry.
// Cheap, deterministic, no DB.
func TestPolicyEnforcementParity(t *testing.T) {
	// surface -> {file holding the dispatch entry, the dispatch func name}.
	cases := []struct {
		surface string
		file    string
		entry   string // dispatch function the Policy must be enforced from
	}{
		{"command", "handle.go", "func (c *Command[I, O]) Handle"},
		{"query.list", "run.go", "func (q *Query[A, R]) RunList"},
		{"query.lookup", "run.go", "func (q *Query[A, R]) RunLookup"},
		{"query.sql", "run.go", "func (q *Query[A, R]) RunSQL"},
		{"api", "api.go", "func (a *Api[I, O]) Invoke"},
	}

	for _, c := range cases {
		t.Run(c.surface, func(t *testing.T) {
			src, err := os.ReadFile(c.file)
			if err != nil {
				t.Fatalf("read %s: %v", c.file, err)
			}
			body := funcBody(string(src), c.entry)
			if body == "" {
				t.Fatalf("API-POLICY-UNENFORCED-001: could not locate %q in %s "+
					"(dispatch entry moved? update the parity table)", c.entry, c.file)
			}
			if !strings.Contains(body, "EvalPolicy") {
				t.Fatalf("API-POLICY-UNENFORCED-001: %s dispatch (%s in %s) does not "+
					"reference EvalPolicy — its Policy field is unenforced. Every "+
					"request surface carrying a Policy MUST call EvalPolicy*/fail closed.",
					c.surface, c.entry, c.file)
			}
		})
	}
}

// funcBody returns the text between the brace following `sig` and its
// matching close brace. Returns "" if `sig` is not found. Good enough for
// the well-formed runtime source this guard scans (no string literals
// containing unbalanced braces in these dispatch functions).
func funcBody(src, sig string) string {
	i := strings.Index(src, sig)
	if i < 0 {
		return ""
	}
	open := strings.IndexByte(src[i:], '{')
	if open < 0 {
		return ""
	}
	start := i + open
	depth := 0
	for j := start; j < len(src); j++ {
		switch src[j] {
		case '{':
			depth++
		case '}':
			depth--
			if depth == 0 {
				return src[start : j+1]
			}
		}
	}
	return ""
}
