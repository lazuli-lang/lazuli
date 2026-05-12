package security

import (
	"errors"
	"testing"
)

func TestEscapeHTMLTextEscapesUnsafeInput(t *testing.T) {
	t.Parallel()

	got := EscapeHTMLText(`<span title="x&y">It's ok</span>`)
	const want = `&lt;span title=&#34;x&amp;y&#34;&gt;It&#39;s ok&lt;/span&gt;`
	if got.String() != want {
		t.Fatalf("EscapeHTMLText() = %q, want %q", got, want)
	}
}

func TestEscapeHTMLTextKeepsSafeString(t *testing.T) {
	t.Parallel()

	const trusted = SafeString(`<strong>trusted</strong>`)
	if got := EscapeHTMLText(trusted); got != trusted {
		t.Fatalf("EscapeHTMLText(SafeString) = %q, want unchanged", got)
	}
}

func TestEscapeHTMLAttributeEscapesQuotedContext(t *testing.T) {
	t.Parallel()

	got := EscapeHTMLAttribute(`a" b' & <tag>`)
	const want = `a&#34; b&#39; &amp; &lt;tag&gt;`
	if got.String() != want {
		t.Fatalf("EscapeHTMLAttribute() = %q, want %q", got, want)
	}
}

func TestEscapeHTMLURLEscapesSafeURL(t *testing.T) {
	t.Parallel()

	got, err := EscapeHTMLURL(`https://example.test/search?q=a&next=b`)
	if err != nil {
		t.Fatalf("EscapeHTMLURL() error = %v", err)
	}
	const want = `https://example.test/search?q=a&amp;next=b`
	if got.String() != want {
		t.Fatalf("EscapeHTMLURL() = %q, want %q", got, want)
	}
}

func TestValidateHTMLURLAllowsRelativeAndSafeAbsoluteURLs(t *testing.T) {
	t.Parallel()

	tests := []string{
		"/dashboard?tab=activity#latest",
		"settings/profile",
		"https://example.test/callback?state=ok",
		"mailto:support@example.test",
	}
	for _, raw := range tests {
		raw := raw
		t.Run(raw, func(t *testing.T) {
			t.Parallel()
			if err := ValidateHTMLURL(raw); err != nil {
				t.Fatalf("ValidateHTMLURL(%q) error = %v", raw, err)
			}
		})
	}
}

func TestEscapeHTMLURLRejectsUnsafeURLs(t *testing.T) {
	t.Parallel()

	tests := []string{
		"",
		"javascript:alert(1)",
		"JavaScript:alert(1)",
		"data:text/html,<script>alert(1)</script>",
		" javaScript:alert(1)",
		"java\nscript:alert(1)",
		"/search?q=two words",
	}
	for _, raw := range tests {
		raw := raw
		t.Run(raw, func(t *testing.T) {
			t.Parallel()
			got, err := EscapeHTMLURL(raw)
			if !errors.Is(err, ErrHTMLURLRejected) {
				t.Fatalf("EscapeHTMLURL(%q) = %q, %v; want ErrHTMLURLRejected", raw, got, err)
			}
		})
	}
}
