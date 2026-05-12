package security

import (
	"encoding/base64"
	"errors"
	"testing"
)

func TestCSPBuilderRendersDeterministicHeader(t *testing.T) {
	t.Parallel()

	builder := NewCSPBuilder().
		Directive("SCRIPT-SRC", CSPSourceSelf, "https://cdn.example.com", CSPNonceSource("abc123"), CSPSourceSelf).
		Directive("object-src", CSPSourceNone).
		Directive("default-src", CSPSourceSelf).
		Directive("upgrade-insecure-requests").
		ReportOnly()

	name, value, err := builder.Header()
	if err != nil {
		t.Fatalf("Header() error = %v", err)
	}
	if name != HeaderContentSecurityPolicyReportOnly {
		t.Fatalf("header name = %q, want %q", name, HeaderContentSecurityPolicyReportOnly)
	}

	want := "default-src 'self'; object-src 'none'; script-src 'nonce-abc123' 'self' https://cdn.example.com; upgrade-insecure-requests"
	if value != want {
		t.Fatalf("header value = %q, want %q", value, want)
	}
}

func TestCSPPolicyHeaderNameDefaultsToEnforcing(t *testing.T) {
	t.Parallel()

	policy := CSPPolicy{
		Directives: []CSPDirective{
			{Name: "default-src", Sources: []string{CSPSourceSelf}},
		},
	}

	name, value, err := policy.Header()
	if err != nil {
		t.Fatalf("Header() error = %v", err)
	}
	if name != HeaderContentSecurityPolicy {
		t.Fatalf("header name = %q, want %q", name, HeaderContentSecurityPolicy)
	}
	if value != "default-src 'self'" {
		t.Fatalf("header value = %q, want default-src 'self'", value)
	}
}

func TestCSPBuilderMergesDuplicateDirectivesAndSources(t *testing.T) {
	t.Parallel()

	value, err := NewCSPBuilder().
		Directive("img-src", "https:", CSPSourceData).
		Directive("default-src", CSPSourceSelf).
		Directive("img-src", CSPSourceSelf, "https:").
		HeaderValue()
	if err != nil {
		t.Fatalf("HeaderValue() error = %v", err)
	}

	want := "default-src 'self'; img-src 'self' data: https:"
	if value != want {
		t.Fatalf("header value = %q, want %q", value, want)
	}
}

func TestCSPDirectiveValidationRejectsUnsafeInput(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name      string
		directive CSPDirective
	}{
		{
			name:      "empty directive name",
			directive: CSPDirective{Name: "", Sources: []string{CSPSourceSelf}},
		},
		{
			name:      "directive name with space",
			directive: CSPDirective{Name: "default src", Sources: []string{CSPSourceSelf}},
		},
		{
			name:      "directive name with semicolon",
			directive: CSPDirective{Name: "default-src;", Sources: []string{CSPSourceSelf}},
		},
		{
			name:      "empty source",
			directive: CSPDirective{Name: "script-src", Sources: []string{""}},
		},
		{
			name:      "source with semicolon",
			directive: CSPDirective{Name: "script-src", Sources: []string{"https://cdn.example.com; object-src 'none'"}},
		},
		{
			name:      "source with newline",
			directive: CSPDirective{Name: "script-src", Sources: []string{"https://cdn.example.com\nx"}},
		},
		{
			name:      "source with whitespace",
			directive: CSPDirective{Name: "script-src", Sources: []string{"two words"}},
		},
		{
			name:      "malformed quoted source",
			directive: CSPDirective{Name: "script-src", Sources: []string{"nonce-abc'"}},
		},
		{
			name:      "empty nonce source",
			directive: CSPDirective{Name: "script-src", Sources: []string{CSPNonceSource("")}},
		},
		{
			name:      "empty hash source",
			directive: CSPDirective{Name: "script-src", Sources: []string{CSPHashSource("sha256", "")}},
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			err := ValidateCSPDirective(tt.directive)
			if !errors.Is(err, ErrInvalidCSPDirective) {
				t.Fatalf("ValidateCSPDirective() error = %v, want ErrInvalidCSPDirective", err)
			}
		})
	}
}

func TestCSPPolicyValidationRejectsEmptyPolicyAndMixedNone(t *testing.T) {
	t.Parallel()

	if err := (CSPPolicy{}).Validate(); !errors.Is(err, ErrInvalidCSPDirective) {
		t.Fatalf("Validate(empty) error = %v, want ErrInvalidCSPDirective", err)
	}

	err := CSPPolicy{
		Directives: []CSPDirective{
			{Name: "default-src", Sources: []string{CSPSourceNone, CSPSourceSelf}},
		},
	}.Validate()
	if !errors.Is(err, ErrInvalidCSPDirective) {
		t.Fatalf("Validate(mixed none) error = %v, want ErrInvalidCSPDirective", err)
	}
}

func TestCSPNonceAndSourceHelpers(t *testing.T) {
	t.Parallel()

	nonce, err := GenerateCSPNonce()
	if err != nil {
		t.Fatalf("GenerateCSPNonce() error = %v", err)
	}
	decoded, err := base64.RawStdEncoding.DecodeString(nonce)
	if err != nil {
		t.Fatalf("nonce is not raw base64: %v", err)
	}
	if len(decoded) != CSPNonceBytes {
		t.Fatalf("nonce bytes = %d, want %d", len(decoded), CSPNonceBytes)
	}

	source := CSPNonceSource(" " + nonce + " ")
	if source != "'nonce-"+nonce+"'" {
		t.Fatalf("nonce source = %q, want wrapped nonce", source)
	}
	if err := ValidateCSPSource(source); err != nil {
		t.Fatalf("ValidateCSPSource(nonce) error = %v", err)
	}

	if got := CSPHashSource("SHA256", "abc123"); got != "'sha256-abc123'" {
		t.Fatalf("CSPHashSource() = %q, want 'sha256-abc123'", got)
	}
	if got := CSPSchemeSource("HTTPS:"); got != "https:" {
		t.Fatalf("CSPSchemeSource() = %q, want https:", got)
	}
}
