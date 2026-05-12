package migrations

import (
	"errors"
	"fmt"
	"io/fs"
	"path"
	"sort"
	"strings"
)

// RecipeRootDir is the canonical filesystem root for migration recipes.
const RecipeRootDir = "migrations/recipes"

// RecipeFSLoader discovers migration recipes from migrations/recipes.
type RecipeFSLoader struct {
	// FS contains migrations/recipes/<from>-to-<to>/<recipe>/ directories.
	FS fs.FS
}

// NewRecipeFSLoader returns a filesystem-backed migration recipe loader.
func NewRecipeFSLoader(source fs.FS) RecipeFSLoader {
	return RecipeFSLoader{FS: source}
}

// LoadRecipeManifests discovers and loads all recipe manifests from source.
func LoadRecipeManifests(source fs.FS) ([]RecipeManifest, error) {
	return NewRecipeFSLoader(source).LoadManifests()
}

// LoadRecipeDescriptors discovers all recipes from source and returns planner
// descriptors derived from their manifests.
func LoadRecipeDescriptors(source fs.FS) ([]UpgradeRecipeDescriptor, error) {
	return NewRecipeFSLoader(source).LoadDescriptors()
}

// LoadManifests discovers migrations/recipes/<from>-to-<to>/<recipe>
// directories, validates their path shape, and loads each recipe.toml.
func (l RecipeFSLoader) LoadManifests() ([]RecipeManifest, error) {
	if l.FS == nil {
		return nil, invalidRecipeManifest("FS is required")
	}

	dirs, err := recipeFSDiscoverManifestDirs(l.FS)
	if err != nil {
		return nil, err
	}

	manifests := make([]RecipeManifest, 0, len(dirs))
	for _, dir := range dirs {
		manifest, err := LoadRecipeManifest(l.FS, dir)
		if err != nil {
			return nil, err
		}
		manifests = append(manifests, manifest)
	}

	sort.SliceStable(manifests, func(i, j int) bool {
		return recipeFSManifestLess(manifests[i], manifests[j])
	})
	return manifests, nil
}

// LoadDescriptors discovers migration recipes and returns planner descriptors
// in the same deterministic order as LoadManifests.
func (l RecipeFSLoader) LoadDescriptors() ([]UpgradeRecipeDescriptor, error) {
	manifests, err := l.LoadManifests()
	if err != nil {
		return nil, err
	}

	descriptors := make([]UpgradeRecipeDescriptor, 0, len(manifests))
	for _, manifest := range manifests {
		descriptors = append(descriptors, recipeFSDescriptorFromManifest(manifest))
	}
	return descriptors, nil
}

func recipeFSDiscoverManifestDirs(source fs.FS) ([]string, error) {
	var dirs []string
	versionWindows := map[string]struct{}{}
	versionWindowsWithRecipes := map[string]struct{}{}

	err := fs.WalkDir(source, RecipeRootDir, func(name string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if name == RecipeRootDir {
			if !entry.IsDir() {
				return invalidRecipeManifest("recipe root %q must be a directory", RecipeRootDir)
			}
			return nil
		}

		rel := strings.TrimPrefix(name, RecipeRootDir+"/")
		depth := strings.Count(rel, "/") + 1
		if entry.IsDir() {
			switch depth {
			case 1:
				if err := recipeFSValidateVersionWindow(path.Base(name)); err != nil {
					return err
				}
				versionWindows[name] = struct{}{}
				return nil
			case 2:
				recipePath, err := ParseRecipePath(name)
				if err != nil {
					return err
				}
				versionWindow := path.Dir(recipePath.Dir)
				versionWindows[versionWindow] = struct{}{}
				versionWindowsWithRecipes[versionWindow] = struct{}{}
				dirs = append(dirs, recipePath.Dir)
				return fs.SkipDir
			default:
				return invalidRecipeManifest("recipe directory %q is nested too deeply", name)
			}
		}

		switch depth {
		case 1:
			return invalidRecipeManifest("recipe root %q contains file %q; expected version directories", RecipeRootDir, name)
		case 2:
			return invalidRecipeManifest("recipe version directory %q contains file %q; expected recipe directories", path.Dir(name), name)
		default:
			return nil
		}
	})
	if err != nil {
		if errors.Is(err, ErrInvalidRecipeManifest) {
			return nil, err
		}
		return nil, fmt.Errorf("%w: discover %s: %w", ErrInvalidRecipeManifest, RecipeRootDir, err)
	}

	for window := range versionWindows {
		if _, ok := versionWindowsWithRecipes[window]; !ok {
			return nil, invalidRecipeManifest("recipe version directory %q must contain at least one recipe directory", window)
		}
	}

	sort.Strings(dirs)
	return dirs, nil
}

func recipeFSValidateVersionWindow(window string) error {
	fromVersion, toVersion, ok := strings.Cut(window, "-to-")
	if !ok || fromVersion == "" || toVersion == "" {
		return invalidRecipeManifest("recipe version window %q must match <from>-to-<to>", window)
	}
	if err := validateRecipeVersion("from-version", fromVersion); err != nil {
		return err
	}
	if err := validateRecipeVersion("to-version", toVersion); err != nil {
		return err
	}
	return validateRecipeVersionOrder(fromVersion, toVersion)
}

func recipeFSManifestLess(a, b RecipeManifest) bool {
	if a.FromVersion != b.FromVersion {
		return upgradePlanVersionLess(a.FromVersion, b.FromVersion)
	}
	if a.ToVersion != b.ToVersion {
		return upgradePlanVersionLess(a.ToVersion, b.ToVersion)
	}
	if a.Name != b.Name {
		return a.Name < b.Name
	}
	return a.Dir < b.Dir
}

func recipeFSDescriptorFromManifest(manifest RecipeManifest) UpgradeRecipeDescriptor {
	return UpgradeRecipeDescriptor{
		Name:        manifest.Name,
		FromVersion: manifest.FromVersion,
		ToVersion:   manifest.ToVersion,
		Kind:        string(manifest.Kind),
		Path:        manifest.Dir,
	}
}
