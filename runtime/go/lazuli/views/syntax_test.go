package views

import (
	"errors"
	"reflect"
	"testing"
)

func TestLookupSyntaxLanguageCanonicalizesAliasesAndCopies(t *testing.T) {
	descriptor, ok := LookupSyntaxLanguage(" .JS ")
	if !ok {
		t.Fatalf("LookupSyntaxLanguage(.JS) ok = false")
	}
	if descriptor.Name != SyntaxLanguageJavaScript {
		t.Fatalf("LookupSyntaxLanguage(.JS).Name = %q, want %q", descriptor.Name, SyntaxLanguageJavaScript)
	}
	if descriptor.Label != "JavaScript" {
		t.Fatalf("LookupSyntaxLanguage(.JS).Label = %q, want JavaScript", descriptor.Label)
	}

	descriptor.Aliases[0] = "mutated"
	next, ok := LookupSyntaxLanguage("javascript")
	if !ok {
		t.Fatalf("LookupSyntaxLanguage(javascript) ok = false")
	}
	if reflect.DeepEqual(next.Aliases, descriptor.Aliases) {
		t.Fatalf("LookupSyntaxLanguage returned aliases that can mutate catalog")
	}
	if got, ok := NormalizeSyntaxLanguage("tsx"); !ok || got != SyntaxLanguageTypeScript {
		t.Fatalf("NormalizeSyntaxLanguage(tsx) = %q, %v; want %q, true", got, ok, SyntaxLanguageTypeScript)
	}
}

func TestSyntaxLanguageDescriptorsSortedAndIndependent(t *testing.T) {
	descriptors := SyntaxLanguageDescriptors()
	if len(descriptors) == 0 {
		t.Fatalf("SyntaxLanguageDescriptors returned no descriptors")
	}
	for i := 1; i < len(descriptors); i++ {
		if descriptors[i-1].Name > descriptors[i].Name {
			t.Fatalf("SyntaxLanguageDescriptors not sorted at %d: %q before %q", i, descriptors[i-1].Name, descriptors[i].Name)
		}
	}

	descriptors[0].Aliases = append(descriptors[0].Aliases, "mutated")
	next := SyntaxLanguageDescriptors()
	if reflect.DeepEqual(next[0].Aliases, descriptors[0].Aliases) {
		t.Fatalf("SyntaxLanguageDescriptors returned mutable catalog aliases")
	}
}

func TestSyntaxThemeMetadataAliasesAndFallbacks(t *testing.T) {
	dark := ResolveSyntaxTheme("unknown", "dark", "light")
	if dark.Name != SyntaxThemeLazuliDark {
		t.Fatalf("ResolveSyntaxTheme fallback = %q, want %q", dark.Name, SyntaxThemeLazuliDark)
	}
	if !dark.Dark || dark.HighContrast {
		t.Fatalf("ResolveSyntaxTheme(dark) metadata = Dark:%v HighContrast:%v, want true false", dark.Dark, dark.HighContrast)
	}

	contrast := ResolveSyntaxTheme("high-contrast")
	if contrast.Name != SyntaxThemeContrastLight || !contrast.HighContrast || contrast.Dark {
		t.Fatalf("ResolveSyntaxTheme(high-contrast) = %#v", contrast)
	}

	fallback := ResolveSyntaxTheme("missing", "also-missing")
	if fallback.Name != DefaultSyntaxTheme {
		t.Fatalf("ResolveSyntaxTheme default = %q, want %q", fallback.Name, DefaultSyntaxTheme)
	}
	if err := ValidateSyntaxTheme("sepia"); !errors.Is(err, ErrInvalidSyntaxTheme) {
		t.Fatalf("ValidateSyntaxTheme(sepia) error = %v, want ErrInvalidSyntaxTheme", err)
	}
}

func TestNormalizeSyntaxTokenClassesValidatesDeduplicatesAndSorts(t *testing.T) {
	classes, err := NormalizeSyntaxTokenClasses([]string{
		" Keyword ",
		"syntax-string",
		"token-comment",
		"keyword",
		"bool",
	})
	if err != nil {
		t.Fatalf("NormalizeSyntaxTokenClasses() error = %v", err)
	}
	want := []SyntaxTokenClass{
		SyntaxTokenBoolean,
		SyntaxTokenComment,
		SyntaxTokenKeyword,
		SyntaxTokenString,
	}
	if !reflect.DeepEqual(classes, want) {
		t.Fatalf("NormalizeSyntaxTokenClasses() = %#v, want %#v", classes, want)
	}

	if err := ValidateSyntaxTokenClass("comment primary"); !errors.Is(err, ErrInvalidSyntaxTokenClass) {
		t.Fatalf("ValidateSyntaxTokenClass(whitespace) error = %v, want ErrInvalidSyntaxTokenClass", err)
	}
	if _, err := NormalizeSyntaxTokenClass("unknown"); !errors.Is(err, ErrInvalidSyntaxTokenClass) {
		t.Fatalf("NormalizeSyntaxTokenClass(unknown) error = %v, want ErrInvalidSyntaxTokenClass", err)
	}
}

func TestResolveSyntaxLanguageUsesFallbackOrder(t *testing.T) {
	language := ResolveSyntaxLanguage("missing", "also-missing", "sql", "go")
	if language.Name != SyntaxLanguageSQL {
		t.Fatalf("ResolveSyntaxLanguage fallback = %q, want %q", language.Name, SyntaxLanguageSQL)
	}

	defaulted := ResolveSyntaxLanguage("missing")
	if defaulted.Name != DefaultSyntaxLanguage {
		t.Fatalf("ResolveSyntaxLanguage default = %q, want %q", defaulted.Name, DefaultSyntaxLanguage)
	}

	if err := ValidateSyntaxLanguage("made-up"); !errors.Is(err, ErrInvalidSyntaxLanguage) {
		t.Fatalf("ValidateSyntaxLanguage(made-up) error = %v, want ErrInvalidSyntaxLanguage", err)
	}
}
