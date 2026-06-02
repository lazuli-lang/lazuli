package lazuli

import (
	"strings"
	"testing"
)

// TestSanitizeHTMLStrictStripsScript proves the `strict` profile strips
// ALL tags, including a `<script>` payload — the core stored-XSS guard.
func TestSanitizeHTMLStrictStripsScript(t *testing.T) {
	const input = `<script>alert(1)</script>hello`
	got := SanitizeHTML(SanitizeStrict, input)
	if got != "hello" {
		t.Fatalf("strict sanitize: got %q, want %q (script tag must be stripped)", got, "hello")
	}
}

// TestSanitizeHTMLBasicKeepsSafeFormattingDropsScript proves the `basic`
// profile keeps a safe formatting tag but strips script and event-handler
// attributes.
func TestSanitizeHTMLBasicKeepsSafeFormattingDropsScript(t *testing.T) {
	const input = `<b>bold</b><script>alert(1)</script><img src=x onerror=alert(2)>`
	got := SanitizeHTML(SanitizeBasic, input)
	if !strings.Contains(got, "<b>bold</b>") {
		t.Fatalf("basic sanitize: expected <b>bold</b> preserved, got %q", got)
	}
	if strings.Contains(got, "<script") || strings.Contains(got, "alert(1)") {
		t.Fatalf("basic sanitize: script must be stripped, got %q", got)
	}
	if strings.Contains(got, "onerror") {
		t.Fatalf("basic sanitize: onerror handler must be stripped, got %q", got)
	}
	// `<img>` is not in the basic allow-list, so it is dropped entirely.
	if strings.Contains(got, "<img") {
		t.Fatalf("basic sanitize: <img> not in allow-list, must be dropped, got %q", got)
	}
}

// TestSanitizeHTMLMarkdownSafeStripsScriptKeepsStructure proves the
// `markdown_safe` profile preserves rendered-markdown structure (headings,
// links) while still stripping script.
func TestSanitizeHTMLMarkdownSafeStripsScriptKeepsStructure(t *testing.T) {
	const input = `<h1>Title</h1><a href="https://x.test">link</a><script>steal()</script>`
	got := SanitizeHTML(SanitizeMarkdownSafe, input)
	if !strings.Contains(got, "<h1>Title</h1>") {
		t.Fatalf("markdown_safe: expected heading preserved, got %q", got)
	}
	if !strings.Contains(got, `href="https://x.test"`) {
		t.Fatalf("markdown_safe: expected safe link preserved, got %q", got)
	}
	if strings.Contains(got, "<script") || strings.Contains(got, "steal()") {
		t.Fatalf("markdown_safe: script must be stripped, got %q", got)
	}
}

// TestSanitizeHTMLUnknownProfileFailsClosed proves an unrecognised profile
// falls back to the safest (strict) policy rather than passing markup.
func TestSanitizeHTMLUnknownProfileFailsClosed(t *testing.T) {
	got := SanitizeHTML(SanitizeHTMLProfile("bogus"), `<script>x</script>keep`)
	if got != "keep" {
		t.Fatalf("unknown profile must fail closed to strict: got %q, want %q", got, "keep")
	}
}

// TestSanitizeColumnValuesStripsScriptOnDeclaredColumn proves the write-
// boundary glue rewrites a string binding for a `sanitize_html(strict)`
// column and leaves un-sanitized columns untouched.
func TestSanitizeColumnValuesStripsScriptOnDeclaredColumn(t *testing.T) {
	res := &resourceErased{
		Name: "post",
		SanitizeColumns: map[string]string{
			"body": "strict",
		},
	}
	cols := []string{quoteIdent("title"), quoteIdent("body")}
	values := []any{
		`<b>not sanitized</b>`,           // title — not in SanitizeColumns
		`<script>alert(1)</script>hello`, // body — strict
	}
	if err := sanitizeColumnValues(res, cols, values); err != nil {
		t.Fatalf("sanitizeColumnValues: %v", err)
	}
	if values[0] != `<b>not sanitized</b>` {
		t.Fatalf("un-sanitized column must be unchanged, got %v", values[0])
	}
	if values[1] != "hello" {
		t.Fatalf("sanitized column: got %v, want %q", values[1], "hello")
	}
}

// TestSanitizeColumnValuesPointerString covers the optional-field path
// where the binding is a *string.
func TestSanitizeColumnValuesPointerString(t *testing.T) {
	res := &resourceErased{
		SanitizeColumns: map[string]string{"bio": "strict"},
	}
	raw := `<iframe src=evil></iframe>safe`
	cols := []string{quoteIdent("bio")}
	values := []any{&raw}
	if err := sanitizeColumnValues(res, cols, values); err != nil {
		t.Fatalf("sanitizeColumnValues: %v", err)
	}
	out, ok := values[0].(*string)
	if !ok || out == nil {
		t.Fatalf("expected non-nil *string, got %T %v", values[0], values[0])
	}
	if *out != "safe" {
		t.Fatalf("pointer sanitize: got %q, want %q", *out, "safe")
	}
	// Caller's original string must not be mutated in place.
	if raw == "safe" {
		t.Fatalf("original input was mutated; expected fresh pointer")
	}
}

// TestSanitizeColumnValuesNoSanitizeColumns confirms the fast path: a
// resource with no sanitized columns leaves every value untouched.
func TestSanitizeColumnValuesNoSanitizeColumns(t *testing.T) {
	res := &resourceErased{}
	cols := []string{quoteIdent("body")}
	values := []any{`<script>x</script>`}
	if err := sanitizeColumnValues(res, cols, values); err != nil {
		t.Fatalf("should be a no-op: %v", err)
	}
	if values[0] != `<script>x</script>` {
		t.Fatalf("no-op path mutated value: %v", values[0])
	}
}

// TestSanitizeColumnValuesNonStringSkips proves a non-string binding on a
// declared column degrades to unchanged rather than panicking.
func TestSanitizeColumnValuesNonStringSkips(t *testing.T) {
	res := &resourceErased{SanitizeColumns: map[string]string{"n": "strict"}}
	cols := []string{quoteIdent("n")}
	values := []any{int64(42)}
	if err := sanitizeColumnValues(res, cols, values); err != nil {
		t.Fatalf("sanitizeColumnValues: %v", err)
	}
	if values[0] != int64(42) {
		t.Fatalf("non-string binding must pass through, got %v", values[0])
	}
}
