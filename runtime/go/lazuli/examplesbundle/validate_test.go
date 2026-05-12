package examplesbundle

import (
	"errors"
	"strings"
	"testing"
)

func TestValidateWithConfigChecksRequiredTagsAndContentBudget(t *testing.T) {
	entry := examplesBundleValidateTestEntry(t, examplesBundleTestExample("alpha", "examples/alpha.lzi"))
	maxTokens := TokenishContentLength(entry) - 1

	err := ValidateWithConfig([]Entry{entry}, ValidationConfig{
		RequiredTags:     []string{" runtime ", "curated"},
		MaxContentTokens: maxTokens,
	})
	if !errors.Is(err, ErrInvalidEntry) {
		t.Fatalf("ValidateWithConfig() error = %v, want ErrInvalidEntry", err)
	}

	for _, fragment := range []string{
		"entry[0]: tags",
		"missing required tags: curated",
		"entry[0]: content_tokens",
		"exceeds limit",
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateWithConfig() error = %v, want fragment %q", err, fragment)
		}
	}
}

func TestValidateDetectsDuplicateSourcePathsAndContentHashesInStableOrder(t *testing.T) {
	first := examplesBundleValidateTestEntry(t, Example{
		Name:       "first",
		Intent:     "first shared command",
		SourcePath: "examples/shared.lzi",
		Tags:       []string{"runtime", "command"},
		LZISource:  "command shared {}\n",
	})
	duplicate := examplesBundleValidateTestEntry(t, Example{
		Name:       "second",
		Intent:     "second shared command",
		SourcePath: "./examples/shared.lzi",
		Tags:       []string{"runtime", "command"},
		LZISource:  "command shared {}\n",
	})

	err := Validate([]Entry{first, duplicate})
	if !errors.Is(err, ErrInvalidEntry) {
		t.Fatalf("Validate() error = %v, want ErrInvalidEntry", err)
	}

	message := err.Error()
	sourcePathAt := strings.Index(message, "entry[1]: source_path")
	contentHashAt := strings.Index(message, "entry[1]: content_hash")
	if sourcePathAt < 0 {
		t.Fatalf("Validate() error = %v, want duplicate source_path", err)
	}
	if contentHashAt < 0 {
		t.Fatalf("Validate() error = %v, want duplicate content_hash", err)
	}
	if sourcePathAt > contentHashAt {
		t.Fatalf("Validate() error order = %v, want source_path before content_hash", err)
	}
}

func TestBuildRejectsDuplicateContentHashes(t *testing.T) {
	source := "command shared {}\n"
	first := examplesBundleTestExample("first", "examples/first.lzi")
	first.LZISource = source
	second := examplesBundleTestExample("second", "examples/second.lzi")
	second.LZISource = source

	_, err := Build([]Example{first, second})
	if !errors.Is(err, ErrInvalidEntry) {
		t.Fatalf("Build() error = %v, want ErrInvalidEntry", err)
	}
	if !strings.Contains(err.Error(), "duplicate content hash") {
		t.Fatalf("Build() error = %v, want duplicate content hash", err)
	}
}

func TestTokenishContentLengthCountsContentFields(t *testing.T) {
	entry := Entry{
		Name:       "Command",
		SourcePath: "examples/create.lzi",
		Tags:       []string{"runtime"},
		LZISource:  "command create_customer {}\n",
	}

	if got, want := TokenishContentLength(entry), 11; got != want {
		t.Fatalf("TokenishContentLength() = %d, want %d", got, want)
	}
}

func examplesBundleValidateTestEntry(t *testing.T, example Example) Entry {
	t.Helper()

	entry, err := NewEntry(example)
	if err != nil {
		t.Fatalf("NewEntry() error = %v", err)
	}
	return entry
}
