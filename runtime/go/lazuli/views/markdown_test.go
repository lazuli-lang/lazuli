package views

import (
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestParseMarkdownFrontmatterSortsMetadataAndReturnsBody(t *testing.T) {
	source := strings.Join([]string{
		"---",
		"Title: \"Getting Started\"",
		"# comment",
		"category: Guides",
		"slug: getting-started",
		"---",
		"# Getting Started",
		"",
		"Welcome.",
	}, "\r\n")

	metadata, body, err := ParseMarkdownFrontmatter(source)
	if err != nil {
		t.Fatalf("ParseMarkdownFrontmatter() error = %v", err)
	}

	wantMetadata := MarkdownFrontmatter{
		{Key: "category", Value: "Guides"},
		{Key: "slug", Value: "getting-started"},
		{Key: "title", Value: "Getting Started"},
	}
	if !reflect.DeepEqual(metadata, wantMetadata) {
		t.Fatalf("metadata = %#v, want %#v", metadata, wantMetadata)
	}
	if got, ok := metadata.Value(" TITLE "); !ok || got != "Getting Started" {
		t.Fatalf("Value(TITLE) = %q, %v", got, ok)
	}
	if body != "# Getting Started\n\nWelcome." {
		t.Fatalf("body = %q", body)
	}
}

func TestParseMarkdownFrontmatterRejectsMalformedMetadata(t *testing.T) {
	_, _, err := ParseMarkdownFrontmatter("---\ntitle: One\nTitle: Two\nbad line\n---\nBody")
	if !errors.Is(err, ErrInvalidMarkdownFrontmatter) {
		t.Fatalf("ParseMarkdownFrontmatter() error = %v, want ErrInvalidMarkdownFrontmatter", err)
	}
	for _, want := range []string{"line 2 duplicates line 1", "line 3 must be key: value"} {
		if !strings.Contains(err.Error(), want) {
			t.Fatalf("ParseMarkdownFrontmatter() error = %q, want substring %q", err.Error(), want)
		}
	}
}

func TestInspectMarkdownDetectsFeaturesWithoutRendering(t *testing.T) {
	source := strings.Join([]string{
		"---",
		"title: Demo",
		"---",
		"# Demo",
		"",
		"Intro with *emphasis*, **strong**, [site](https://example.com), and ![logo](/logo.png).",
		"",
		"> quoted",
		"",
		"- item",
		"",
		"| Name | Value |",
		"| --- | --- |",
		"| one | two |",
		"",
		"```html",
		"<script>alert(1)</script>",
		"```",
	}, "\n")

	document, err := InspectMarkdown(source)
	if err != nil {
		t.Fatalf("InspectMarkdown() error = %v", err)
	}

	wantFeatures := []MarkdownFeature{
		MarkdownFeatureHeading,
		MarkdownFeatureParagraph,
		MarkdownFeatureEmphasis,
		MarkdownFeatureStrong,
		MarkdownFeatureLink,
		MarkdownFeatureImage,
		MarkdownFeatureList,
		MarkdownFeatureBlockquote,
		MarkdownFeatureCode,
		MarkdownFeatureTable,
	}
	if !reflect.DeepEqual(document.Features, wantFeatures) {
		t.Fatalf("Features = %#v, want %#v", document.Features, wantFeatures)
	}
	if document.HasFeature(MarkdownFeatureRawHTML) {
		t.Fatal("InspectMarkdown() detected raw HTML inside a fenced code block")
	}
	wantLinks := []MarkdownLink{
		{Destination: "https://example.com", Scheme: "https"},
		{Destination: "/logo.png", Image: true},
	}
	if !reflect.DeepEqual(document.Links, wantLinks) {
		t.Fatalf("Links = %#v, want %#v", document.Links, wantLinks)
	}
}

func TestValidateMarkdownAppliesPolicyDeterministically(t *testing.T) {
	policy := MarkdownPolicy{
		AllowedFeatures: []MarkdownFeature{
			MarkdownFeatureHeading,
			MarkdownFeatureParagraph,
			MarkdownFeatureLink,
		},
		RequiredMetadata: []string{"title", "owner"},
	}
	source := strings.Join([]string{
		"---",
		"title: Demo",
		"---",
		"# Demo",
		"",
		"Read [docs](javascript:alert(1)).",
		"<section>unsafe</section>",
	}, "\n")

	_, err := ValidateMarkdown(source, policy)
	if !errors.Is(err, ErrInvalidMarkdownDocument) {
		t.Fatalf("ValidateMarkdown() error = %v, want ErrInvalidMarkdownDocument", err)
	}
	for _, want := range []string{
		`feature "raw_html" is not allowed`,
		`metadata "owner" is required`,
		`link[0] scheme "javascript" is not allowed`,
	} {
		if !strings.Contains(err.Error(), want) {
			t.Fatalf("ValidateMarkdown() error = %q, want substring %q", err.Error(), want)
		}
	}
}

func TestMarkdownPolicyNormalizesAndValidates(t *testing.T) {
	policy := MarkdownPolicy{
		AllowedFeatures: []MarkdownFeature{
			MarkdownFeatureLink,
			MarkdownFeatureHeading,
		},
		Sanitization: MarkdownSanitizationPolicy{
			AllowedURLSchemes: []string{"HTTPS:", "http"},
		},
		RequiredMetadata: []string{"Owner", "title"},
	}

	normalized := policy.Normalize()
	if got := normalized.AllowedFeatures; !reflect.DeepEqual(got, []MarkdownFeature{MarkdownFeatureHeading, MarkdownFeatureLink}) {
		t.Fatalf("AllowedFeatures = %#v", got)
	}
	if got := normalized.Sanitization.AllowedURLSchemes; !reflect.DeepEqual(got, []string{"http", "https"}) {
		t.Fatalf("AllowedURLSchemes = %#v", got)
	}
	if got := normalized.RequiredMetadata; !reflect.DeepEqual(got, []string{"owner", "title"}) {
		t.Fatalf("RequiredMetadata = %#v", got)
	}
	if !normalized.RequiresSanitization() {
		t.Fatal("RequiresSanitization() = false, want true")
	}
	if !policy.Allows(" heading ") {
		t.Fatal("Allows(heading) = false, want true")
	}
	if policy.AllowedFeatures[0] != MarkdownFeatureLink {
		t.Fatal("Normalize() mutated input features")
	}

	invalid := MarkdownPolicy{
		AllowedFeatures: []MarkdownFeature{
			MarkdownFeatureRawHTML,
			MarkdownFeatureRawHTML,
			MarkdownFeature("video"),
		},
		Sanitization: MarkdownSanitizationPolicy{
			Requirement:       MarkdownSanitizationTrusted,
			AllowRawHTML:      true,
			AllowedURLSchemes: []string{"javascript"},
		},
		RequiredMetadata: []string{"bad key"},
	}
	err := invalid.Validate()
	if !errors.Is(err, ErrInvalidMarkdownPolicy) {
		t.Fatalf("Validate() error = %v, want ErrInvalidMarkdownPolicy", err)
	}
	for _, want := range []string{
		"duplicates feature[0]",
		`allowed feature[2] "video" is unknown`,
		`url scheme[0] "javascript" is unsafe`,
		"raw HTML requires sanitizer enforcement",
		`required metadata[0] "bad key" is invalid`,
	} {
		if !strings.Contains(err.Error(), want) {
			t.Fatalf("Validate() error = %q, want substring %q", err.Error(), want)
		}
	}
}
