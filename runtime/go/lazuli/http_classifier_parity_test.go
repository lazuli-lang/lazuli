package lazuli

import (
	"os"
	"path/filepath"
	"regexp"
	"runtime"
	"sort"
	"strings"
	"testing"
)

// §A1 COMMAND-WIRE-CLASSIFIER-SYNC — refuses to compile if the
// Rust-side `READ_VERB_PREFIXES` array in `lazuli_cli/src/main.rs`
// diverges from the Go-side `pureReadCommandPrefixes`. Both lists
// drive routing decisions (TS codegen emits `defineQuery` for
// names matching the Rust list → /q/; Go runtime mirrors those at
// /q/ for the same names). Drift between them re-opens the silent
// 404 class that closed in lazuli 7976c3ec.
//
// The test reads the Rust source as text — fragile against radical
// refactors of main.rs, but the prefix-array shape has been stable
// since the classifier landed and a refactor that breaks this test
// is exactly the moment to update both sides.

func TestPureReadClassifierParity(t *testing.T) {
	rustPath := locateRustClassifierSource(t)
	contents, err := os.ReadFile(rustPath)
	if err != nil {
		t.Fatalf("read %s: %v", rustPath, err)
	}
	rustPrefixes := extractRustReadVerbPrefixes(t, string(contents))

	goPrefixes := append([]string(nil), pureReadCommandPrefixes...)
	sort.Strings(goPrefixes)
	sort.Strings(rustPrefixes)

	if !equalStringSlices(goPrefixes, rustPrefixes) {
		t.Fatalf(
			"COMMAND-WIRE-CLASSIFIER-SYNC drift:\n  Go (runtime/go/lazuli/http.go) : %v\n  Rust (crates/lazuli_cli/src/main.rs::READ_VERB_PREFIXES) : %v\n\nA new pure-read prefix must be added to BOTH or removed from BOTH so the wire-routing stays consistent.",
			goPrefixes, rustPrefixes,
		)
	}
}

// locateRustClassifierSource finds the canonical Rust source. The
// runtime tests run from `runtime/go/lazuli`, and the crate sits at
// `../../../crates/lazuli_cli/src/main.rs` (sibling to runtime/).
func locateRustClassifierSource(t *testing.T) string {
	t.Helper()
	_, here, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller failed — can't locate Rust source from test binary")
	}
	root := filepath.Dir(filepath.Dir(filepath.Dir(filepath.Dir(here))))
	candidate := filepath.Join(root, "crates", "lazuli_cli", "src", "main.rs")
	if _, err := os.Stat(candidate); err != nil {
		t.Skipf("Rust source not found at %s (sibling crates/ dir missing) — skipping parity check", candidate)
	}
	return candidate
}

// extractRustReadVerbPrefixes scans the Rust source for the canonical
// declaration:
//
//	const READ_VERB_PREFIXES: &[&str] = &[
//	    "list_", "get_", "lookup_", "search_", "find_", "count_",
//	];
//
// and returns the inner string-literal list.
func extractRustReadVerbPrefixes(t *testing.T, source string) []string {
	t.Helper()
	declRe := regexp.MustCompile(`(?s)const\s+READ_VERB_PREFIXES\s*:\s*&\[&str\]\s*=\s*&\[(.*?)\]`)
	matches := declRe.FindStringSubmatch(source)
	if len(matches) < 2 {
		t.Fatal("could not locate `const READ_VERB_PREFIXES: &[&str] = &[...]` in Rust source — the test scraper needs an update")
	}
	body := matches[1]
	literalRe := regexp.MustCompile(`"([^"]+)"`)
	hits := literalRe.FindAllStringSubmatch(body, -1)
	out := make([]string, 0, len(hits))
	for _, h := range hits {
		out = append(out, h[1])
	}
	return out
}

func equalStringSlices(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

// Sanity: the helper isn't fooled by partial matches like
// `READ_VERB_PREFIXES_OLD` or comments containing the constant
// name. Asserts the regex pins on the exact declaration shape.
func TestExtractRustReadVerbPrefixes_HappyPath(t *testing.T) {
	src := `
// A comment mentioning READ_VERB_PREFIXES that shouldn't fool us.
const SOMETHING_ELSE: &[&str] = &["x"];
const READ_VERB_PREFIXES: &[&str] = &[
    "list_", "get_", "lookup_",
];
`
	got := extractRustReadVerbPrefixes(t, src)
	want := []string{"list_", "get_", "lookup_"}
	if !equalStringSlices(got, want) {
		t.Errorf("got %v want %v", got, want)
	}
	_ = strings.Join // keep strings import alive without an empty use
}
