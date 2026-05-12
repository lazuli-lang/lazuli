package migrations

import (
	"context"
	"errors"
	"fmt"
	"io/fs"
	"path"
	"reflect"
	"sort"
	"strings"
)

var (
	// ErrInvalidUpgradeFilePath is returned when an in-memory upgrade file set
	// contains a path that is not safe for an fs.FS-style tree.
	ErrInvalidUpgradeFilePath = errors.New("migrations: invalid upgrade file path")
	// ErrUpgradeRecipeStepsRequired is returned when a planned recipe has no
	// registered upgrade steps.
	ErrUpgradeRecipeStepsRequired = errors.New("migrations: upgrade recipe steps required")
	// ErrUpgradeStepRequired is returned when a registered recipe step is nil.
	ErrUpgradeStepRequired = errors.New("migrations: upgrade step required")
)

// UpgradeStep rewrites an in-memory file set for one recipe step.
//
// Steps can read and write files through UpgradeFileSet. There is no delete
// operation: upgrade application is intentionally non-destructive and callers
// decide how, or whether, to persist the returned file set.
type UpgradeStep interface {
	ApplyUpgrade(ctx context.Context, files *UpgradeFileSet) error
}

// UpgradeStepFunc adapts a function to UpgradeStep.
type UpgradeStepFunc func(context.Context, *UpgradeFileSet) error

// ApplyUpgrade applies f to files.
func (f UpgradeStepFunc) ApplyUpgrade(ctx context.Context, files *UpgradeFileSet) error {
	return f(ctx, files)
}

// UpgradeRecipeSteps binds executable steps to one upgrade recipe descriptor.
type UpgradeRecipeSteps struct {
	Descriptor UpgradeRecipeDescriptor
	Steps      []UpgradeStep
}

// UpgradeFileSet is a mutable in-memory collection of slash-separated file
// paths to source contents.
type UpgradeFileSet struct {
	files map[string]string
}

// NewUpgradeFileSet validates and copies files into an in-memory upgrade file
// set.
func NewUpgradeFileSet(files map[string]string) (UpgradeFileSet, error) {
	copied := make(map[string]string, len(files))
	for name, content := range files {
		clean, ok := cleanUpgradeFilePath(name)
		if !ok {
			return UpgradeFileSet{}, fmt.Errorf("%w %q", ErrInvalidUpgradeFilePath, name)
		}
		copied[clean] = content
	}
	return UpgradeFileSet{files: copied}, nil
}

// ReadFile returns a file's current contents and whether it exists.
func (s *UpgradeFileSet) ReadFile(name string) (string, bool, error) {
	if s == nil {
		return "", false, nil
	}
	clean, ok := cleanUpgradeFilePath(name)
	if !ok {
		return "", false, fmt.Errorf("%w %q", ErrInvalidUpgradeFilePath, name)
	}
	content, exists := s.files[clean]
	return content, exists, nil
}

// WriteFile writes a file's contents into the in-memory set.
func (s *UpgradeFileSet) WriteFile(name, content string) error {
	clean, ok := cleanUpgradeFilePath(name)
	if !ok {
		return fmt.Errorf("%w %q", ErrInvalidUpgradeFilePath, name)
	}
	if s.files == nil {
		s.files = make(map[string]string)
	}
	s.files[clean] = content
	return nil
}

// Snapshot returns a copy of the file set contents keyed by slash-separated
// path.
func (s *UpgradeFileSet) Snapshot() map[string]string {
	if s == nil {
		return nil
	}
	return upgradeCopyFiles(s.files)
}

// Paths returns the file paths currently present in sorted order.
func (s *UpgradeFileSet) Paths() []string {
	if s == nil {
		return nil
	}
	paths := make([]string, 0, len(s.files))
	for name := range s.files {
		paths = append(paths, name)
	}
	sort.Strings(paths)
	return paths
}

// UpgradeRunner applies executable recipe steps in UpgradePlan order.
type UpgradeRunner struct {
	Plan    UpgradePlan
	Recipes []UpgradeRecipeSteps
}

// AppliedUpgradeRecipe records one recipe successfully applied by the runner.
type AppliedUpgradeRecipe struct {
	Name         string
	FromVersion  string
	ToVersion    string
	ChangedFiles []string
}

// UpgradeResult is the in-memory result of an upgrade run.
type UpgradeResult struct {
	Files        UpgradeFileSet
	ChangedFiles []string
	Applied      []AppliedUpgradeRecipe
}

// NewUpgradeRunner returns a runner that applies plan recipes using the
// supplied executable recipe steps.
func NewUpgradeRunner(plan UpgradePlan, recipes []UpgradeRecipeSteps) UpgradeRunner {
	return UpgradeRunner{Plan: plan, Recipes: recipes}
}

// ApplyUpgrade applies plan recipes to files in memory.
func ApplyUpgrade(ctx context.Context, plan UpgradePlan, files map[string]string, recipes []UpgradeRecipeSteps) (UpgradeResult, error) {
	return NewUpgradeRunner(plan, recipes).Apply(ctx, files)
}

// Apply copies files, applies the runner's planned recipe steps to the copy,
// and reports files whose final contents differ from the input.
func (r UpgradeRunner) Apply(ctx context.Context, files map[string]string) (UpgradeResult, error) {
	working, err := NewUpgradeFileSet(files)
	if err != nil {
		return UpgradeResult{}, err
	}
	original := upgradeCopyFiles(working.files)
	result := UpgradeResult{Files: working}

	recipes, err := upgradeRecipeStepRegistry(r.Recipes)
	if err != nil {
		result.Files = working
		return result, err
	}

	for _, plannedRecipe := range r.Plan.Recipes {
		if err := ctx.Err(); err != nil {
			result.Files = working
			result.ChangedFiles = upgradeChangedFiles(original, working.files)
			return result, err
		}

		normalizedRecipe, key, err := upgradeRecipeStepKey(plannedRecipe)
		if err != nil {
			result.Files = working
			result.ChangedFiles = upgradeChangedFiles(original, working.files)
			return result, err
		}
		recipe, exists := recipes[key]
		if !exists {
			result.Files = working
			result.ChangedFiles = upgradeChangedFiles(original, working.files)
			return result, fmt.Errorf("%w for %q", ErrUpgradeRecipeStepsRequired, key)
		}

		beforeRecipe := upgradeCopyFiles(working.files)
		for i, step := range recipe.Steps {
			if err := ctx.Err(); err != nil {
				result.Files = working
				result.ChangedFiles = upgradeChangedFiles(original, working.files)
				return result, err
			}
			if err := step.ApplyUpgrade(ctx, &working); err != nil {
				result.Files = working
				result.ChangedFiles = upgradeChangedFiles(original, working.files)
				return result, fmt.Errorf("migrations: apply upgrade recipe %q step %d: %w", normalizedRecipe.Name, i+1, err)
			}
		}

		result.Applied = append(result.Applied, AppliedUpgradeRecipe{
			Name:         normalizedRecipe.Name,
			FromVersion:  normalizedRecipe.FromVersion,
			ToVersion:    normalizedRecipe.ToVersion,
			ChangedFiles: upgradeChangedFiles(beforeRecipe, working.files),
		})
	}

	result.Files = working
	result.ChangedFiles = upgradeChangedFiles(original, working.files)
	return result, nil
}

func upgradeRecipeStepRegistry(recipes []UpgradeRecipeSteps) (map[string]UpgradeRecipeSteps, error) {
	registry := make(map[string]UpgradeRecipeSteps, len(recipes))
	for _, recipe := range recipes {
		normalizedRecipe, key, err := upgradeRecipeStepKey(recipe.Descriptor)
		if err != nil {
			return nil, err
		}
		if _, exists := registry[key]; exists {
			return nil, fmt.Errorf("%w %q", ErrDuplicateUpgradeRecipe, key)
		}
		if len(recipe.Steps) == 0 {
			return nil, fmt.Errorf("%w for %q", ErrUpgradeRecipeStepsRequired, key)
		}
		for i, step := range recipe.Steps {
			if upgradeStepIsNil(step) {
				return nil, fmt.Errorf("%w for %q step %d", ErrUpgradeStepRequired, key, i+1)
			}
		}
		recipe.Descriptor = normalizedRecipe
		registry[key] = recipe
	}
	return registry, nil
}

func upgradeRecipeStepKey(recipe UpgradeRecipeDescriptor) (UpgradeRecipeDescriptor, string, error) {
	recipe = upgradePlanNormalizeRecipe(recipe)
	if recipe.Name == "" {
		return recipe, "", ErrUpgradeRecipeNameRequired
	}
	if recipe.FromVersion == "" || recipe.ToVersion == "" {
		return recipe, "", ErrUpgradeRecipeVersionRequired
	}
	return recipe, upgradePlanRecipeID(recipe), nil
}

func upgradeStepIsNil(step UpgradeStep) bool {
	if step == nil {
		return true
	}
	value := reflect.ValueOf(step)
	switch value.Kind() {
	case reflect.Chan, reflect.Func, reflect.Interface, reflect.Map, reflect.Pointer, reflect.Slice:
		return value.IsNil()
	default:
		return false
	}
}

func upgradeCopyFiles(files map[string]string) map[string]string {
	copied := make(map[string]string, len(files))
	for name, content := range files {
		copied[name] = content
	}
	return copied
}

func upgradeChangedFiles(before, after map[string]string) []string {
	changed := make([]string, 0)
	for name, afterContent := range after {
		beforeContent, exists := before[name]
		if !exists || beforeContent != afterContent {
			changed = append(changed, name)
		}
	}
	sort.Strings(changed)
	return changed
}

func cleanUpgradeFilePath(name string) (string, bool) {
	if name == "" || strings.ContainsAny(name, "\x00\\") {
		return "", false
	}
	clean := strings.TrimPrefix(path.Clean("/"+name), "/")
	if clean == "." || clean != name || !fs.ValidPath(clean) {
		return "", false
	}
	return clean, true
}
