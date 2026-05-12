package migrations

import (
	"errors"
	"io/fs"
	"strings"
	"testing"
	"testing/fstest"
)

func TestRecipeLoadManifestParsesMetadataAndPath(t *testing.T) {
	source := fstest.MapFS{
		"migrations/recipes/0.11-to-0.12/rename-policies-to-rules/recipe.toml": &fstest.MapFile{Data: []byte(`
# Required migration metadata.
from-version = "0.11"
to-version = "0.12"
kind = "language" # root language surface
`)},
	}

	manifest, err := LoadRecipeManifest(source, "migrations/recipes/0.11-to-0.12/rename-policies-to-rules")
	if err != nil {
		t.Fatalf("LoadRecipeManifest returned %v", err)
	}

	if manifest.FromVersion != "0.11" {
		t.Fatalf("FromVersion = %q, want 0.11", manifest.FromVersion)
	}
	if manifest.ToVersion != "0.12" {
		t.Fatalf("ToVersion = %q, want 0.12", manifest.ToVersion)
	}
	if manifest.Kind != RecipeKindLanguage {
		t.Fatalf("Kind = %q, want %q", manifest.Kind, RecipeKindLanguage)
	}
	if manifest.Name != "rename-policies-to-rules" {
		t.Fatalf("Name = %q, want rename-policies-to-rules", manifest.Name)
	}
	if manifest.Dir != "migrations/recipes/0.11-to-0.12/rename-policies-to-rules" {
		t.Fatalf("Dir = %q", manifest.Dir)
	}
}

func TestRecipeParseManifestValidatesMetadata(t *testing.T) {
	tests := []struct {
		name string
		toml string
	}{
		{
			name: "missing field",
			toml: `
from-version = "0.11"
to-version = "0.12"
`,
		},
		{
			name: "unknown field",
			toml: `
from-version = "0.11"
to-version = "0.12"
kind = "language"
summary = "renames policies"
`,
		},
		{
			name: "patch version",
			toml: `
from-version = "0.11.0"
to-version = "0.12"
kind = "language"
`,
		},
		{
			name: "invalid kind",
			toml: `
from-version = "0.11"
to-version = "0.12"
kind = "runtime"
`,
		},
		{
			name: "trailing data",
			toml: `
from-version = "0.11" extra
to-version = "0.12"
kind = "language"
`,
		},
		{
			name: "duplicate key",
			toml: `
from-version = "0.11"
from-version = "0.10"
to-version = "0.12"
kind = "language"
`,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := ParseRecipeManifest(strings.NewReader(tt.toml))
			if !errors.Is(err, ErrInvalidRecipeManifest) {
				t.Fatalf("ParseRecipeManifest error = %v, want ErrInvalidRecipeManifest", err)
			}
		})
	}
}

func TestRecipeValidateManifestPathRejectsMismatch(t *testing.T) {
	manifest := RecipeManifest{
		FromVersion: "0.11",
		ToVersion:   "0.12",
		Kind:        RecipeKindLanguage,
	}

	err := ValidateRecipeManifestPath("migrations/recipes/0.10-to-0.12/rename-policies-to-rules", manifest)
	if !errors.Is(err, ErrInvalidRecipeManifest) {
		t.Fatalf("ValidateRecipeManifestPath error = %v, want ErrInvalidRecipeManifest", err)
	}
}

func TestRecipeParsePathRejectsUnsafeOrMalformedDirectories(t *testing.T) {
	tests := []string{
		"../migrations/recipes/0.11-to-0.12/rename",
		"migrations/recipes/0.11-to-0.12",
		"migrations/recipes/0.12-to-0.11/rename",
		"migrations/recipes/0.11-to-0.12/Rename",
		"migrations/recipes/0.11-to-0.12/-rename",
	}

	for _, dir := range tests {
		t.Run(dir, func(t *testing.T) {
			_, err := ParseRecipePath(dir)
			if !errors.Is(err, ErrInvalidRecipeManifest) {
				t.Fatalf("ParseRecipePath error = %v, want ErrInvalidRecipeManifest", err)
			}
		})
	}
}

func TestRecipeLoadManifestWrapsMissingFile(t *testing.T) {
	_, err := LoadRecipeManifest(fstest.MapFS{}, "migrations/recipes/0.11-to-0.12/rename")
	if !errors.Is(err, ErrInvalidRecipeManifest) {
		t.Fatalf("LoadRecipeManifest error = %v, want ErrInvalidRecipeManifest", err)
	}
	if !errors.Is(err, fs.ErrNotExist) {
		t.Fatalf("LoadRecipeManifest error = %v, want fs.ErrNotExist", err)
	}
}
