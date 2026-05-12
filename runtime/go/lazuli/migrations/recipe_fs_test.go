package migrations

import (
	"errors"
	"fmt"
	"io/fs"
	"reflect"
	"testing"
	"testing/fstest"
)

func TestRecipeFSLoadManifestsDiscoversRecipesInDeterministicOrder(t *testing.T) {
	source := fstest.MapFS{
		"migrations/recipes/0.10-to-0.11/b-rename/recipe.toml":   recipeFSTestManifest("0.10", "0.11", RecipeKindLanguage),
		"migrations/recipes/0.9-to-0.10/add-version/recipe.toml": recipeFSTestManifest("0.9", "0.10", RecipeKindLanguage),
		"migrations/recipes/0.10-to-0.11/a-first/recipe.toml":    recipeFSTestManifest("0.10", "0.11", RecipeKindGo),
		"migrations/recipes/0.10-to-0.11/a-first/input.lzi":      &fstest.MapFile{Data: []byte("app Before\n")},
		"migrations/recipes/0.10-to-0.11/a-first/output.lzi":     &fstest.MapFile{Data: []byte("app After\n")},
		"migrations/recipes/0.10-to-0.11/a-first/README.md":      &fstest.MapFile{Data: []byte("Adds the new app version pin.\n")},
		"migrations/recipes/0.10-to-0.11/a-first/go/fixture.go":  &fstest.MapFile{Data: []byte("package fixture\n")},
	}

	manifests, err := LoadRecipeManifests(source)
	if err != nil {
		t.Fatalf("LoadRecipeManifests returned %v", err)
	}

	wantDirs := []string{
		"migrations/recipes/0.9-to-0.10/add-version",
		"migrations/recipes/0.10-to-0.11/a-first",
		"migrations/recipes/0.10-to-0.11/b-rename",
	}
	if got := recipeFSTestManifestDirs(manifests); !reflect.DeepEqual(got, wantDirs) {
		t.Fatalf("manifest dirs = %v, want %v", got, wantDirs)
	}
	if manifests[1].Kind != RecipeKindGo {
		t.Fatalf("second manifest kind = %q, want %q", manifests[1].Kind, RecipeKindGo)
	}
}

func TestRecipeFSLoadDescriptorsUsesLoadedManifestFields(t *testing.T) {
	source := fstest.MapFS{
		"migrations/recipes/0.12-to-0.13/go-error-hierarchy/recipe.toml": recipeFSTestManifest("0.12", "0.13", RecipeKindGo),
	}

	descriptors, err := LoadRecipeDescriptors(source)
	if err != nil {
		t.Fatalf("LoadRecipeDescriptors returned %v", err)
	}

	want := []UpgradeRecipeDescriptor{
		{
			Name:        "go-error-hierarchy",
			FromVersion: "0.12",
			ToVersion:   "0.13",
			Kind:        "go",
			Path:        "migrations/recipes/0.12-to-0.13/go-error-hierarchy",
		},
	}
	if !reflect.DeepEqual(descriptors, want) {
		t.Fatalf("descriptors = %#v, want %#v", descriptors, want)
	}
}

func TestRecipeFSLoadManifestsRejectsMalformedShape(t *testing.T) {
	tests := []struct {
		name   string
		source fstest.MapFS
	}{
		{
			name: "file at recipe root",
			source: fstest.MapFS{
				"migrations/recipes/README.md": &fstest.MapFile{Data: []byte("recipes\n")},
			},
		},
		{
			name: "file at version level",
			source: fstest.MapFS{
				"migrations/recipes/0.11-to-0.12/README.md": &fstest.MapFile{Data: []byte("recipes\n")},
			},
		},
		{
			name: "invalid version window",
			source: fstest.MapFS{
				"migrations/recipes/0.11-0.12/rename/recipe.toml": recipeFSTestManifest("0.11", "0.12", RecipeKindLanguage),
			},
		},
		{
			name: "invalid recipe name",
			source: fstest.MapFS{
				"migrations/recipes/0.11-to-0.12/Rename/recipe.toml": recipeFSTestManifest("0.11", "0.12", RecipeKindLanguage),
			},
		},
		{
			name: "empty version window",
			source: fstest.MapFS{
				"migrations/recipes/0.11-to-0.12": &fstest.MapFile{Mode: fs.ModeDir},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := LoadRecipeManifests(tt.source)
			if !errors.Is(err, ErrInvalidRecipeManifest) {
				t.Fatalf("LoadRecipeManifests error = %v, want ErrInvalidRecipeManifest", err)
			}
		})
	}
}

func TestRecipeFSLoadManifestsRejectsRecipeWithoutManifest(t *testing.T) {
	source := fstest.MapFS{
		"migrations/recipes/0.11-to-0.12/rename/input.lzi": &fstest.MapFile{Data: []byte("app Before\n")},
	}

	_, err := LoadRecipeManifests(source)
	if !errors.Is(err, ErrInvalidRecipeManifest) {
		t.Fatalf("LoadRecipeManifests error = %v, want ErrInvalidRecipeManifest", err)
	}
	if !errors.Is(err, fs.ErrNotExist) {
		t.Fatalf("LoadRecipeManifests error = %v, want fs.ErrNotExist", err)
	}
}

func recipeFSTestManifest(fromVersion, toVersion string, kind RecipeKind) *fstest.MapFile {
	return &fstest.MapFile{Data: []byte(fmt.Sprintf(`
from-version = %q
to-version = %q
kind = %q
`, fromVersion, toVersion, kind))}
}

func recipeFSTestManifestDirs(manifests []RecipeManifest) []string {
	dirs := make([]string, len(manifests))
	for i, manifest := range manifests {
		dirs[i] = manifest.Dir
	}
	return dirs
}
