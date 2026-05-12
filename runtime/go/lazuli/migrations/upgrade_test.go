package migrations

import (
	"context"
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestUpgradeRunnerAppliesPlannedRecipeStepsInMemory(t *testing.T) {
	plan := UpgradePlan{
		FromVersion: "0.11",
		ToVersion:   "0.13",
		Recipes: []UpgradeRecipeDescriptor{
			{Name: "rename-policies-to-rules", FromVersion: "0.11", ToVersion: "0.12", Kind: "language"},
			{Name: "bump-version-pin", FromVersion: "0.12", ToVersion: "0.13", Kind: "language"},
		},
	}
	files := map[string]string{
		"app.lzi":               "app Acme\n  lazuli_version \"0.11\"\n",
		"features/customer.lzi": "policy allow_customer\n",
		"README.md":             "keep this\n",
	}
	recipes := []UpgradeRecipeSteps{
		{
			Descriptor: UpgradeRecipeDescriptor{Name: "bump-version-pin", FromVersion: "0.12", ToVersion: "0.13", Kind: "language"},
			Steps: []UpgradeStep{
				UpgradeStepFunc(func(_ context.Context, files *UpgradeFileSet) error {
					content, exists, err := files.ReadFile("app.lzi")
					if err != nil {
						return err
					}
					if !exists {
						return errors.New("app.lzi missing")
					}
					return files.WriteFile("app.lzi", strings.ReplaceAll(content, `lazuli_version "0.11"`, `lazuli_version "0.13"`))
				}),
			},
		},
		{
			Descriptor: UpgradeRecipeDescriptor{Name: "rename-policies-to-rules", FromVersion: "0.11", ToVersion: "0.12", Kind: "language"},
			Steps: []UpgradeStep{
				UpgradeStepFunc(func(_ context.Context, files *UpgradeFileSet) error {
					content, exists, err := files.ReadFile("features/customer.lzi")
					if err != nil {
						return err
					}
					if !exists {
						return errors.New("features/customer.lzi missing")
					}
					return files.WriteFile("features/customer.lzi", strings.ReplaceAll(content, "policy", "rule"))
				}),
			},
		},
	}

	result, err := NewUpgradeRunner(plan, recipes).Apply(context.Background(), files)
	if err != nil {
		t.Fatalf("Apply returned %v", err)
	}

	if files["app.lzi"] != "app Acme\n  lazuli_version \"0.11\"\n" {
		t.Fatalf("input file map was mutated: %q", files["app.lzi"])
	}
	snapshot := result.Files.Snapshot()
	wantFiles := map[string]string{
		"app.lzi":               "app Acme\n  lazuli_version \"0.13\"\n",
		"features/customer.lzi": "rule allow_customer\n",
		"README.md":             "keep this\n",
	}
	if !reflect.DeepEqual(snapshot, wantFiles) {
		t.Fatalf("files = %#v, want %#v", snapshot, wantFiles)
	}
	if want := []string{"app.lzi", "features/customer.lzi"}; !reflect.DeepEqual(result.ChangedFiles, want) {
		t.Fatalf("changed files = %v, want %v", result.ChangedFiles, want)
	}
	if want := []AppliedUpgradeRecipe{
		{
			Name:         "rename-policies-to-rules",
			FromVersion:  "0.11",
			ToVersion:    "0.12",
			ChangedFiles: []string{"features/customer.lzi"},
		},
		{
			Name:         "bump-version-pin",
			FromVersion:  "0.12",
			ToVersion:    "0.13",
			ChangedFiles: []string{"app.lzi"},
		},
	}; !reflect.DeepEqual(result.Applied, want) {
		t.Fatalf("applied = %#v, want %#v", result.Applied, want)
	}
}

func TestUpgradeRunnerReportsNewFilesAndIgnoresIdenticalWrites(t *testing.T) {
	plan := UpgradePlan{
		FromVersion: "0.11",
		ToVersion:   "0.12",
		Recipes: []UpgradeRecipeDescriptor{
			{Name: "add-upgrade-note", FromVersion: "0.11", ToVersion: "0.12"},
		},
	}
	files := map[string]string{"app.lzi": "app Acme\n"}
	recipes := []UpgradeRecipeSteps{
		{
			Descriptor: plan.Recipes[0],
			Steps: []UpgradeStep{
				UpgradeStepFunc(func(_ context.Context, files *UpgradeFileSet) error {
					if err := files.WriteFile("app.lzi", "app Acme\n"); err != nil {
						return err
					}
					return files.WriteFile("migrations/upgrade-note.md", "upgraded\n")
				}),
			},
		},
	}

	result, err := ApplyUpgrade(context.Background(), plan, files, recipes)
	if err != nil {
		t.Fatalf("ApplyUpgrade returned %v", err)
	}

	if want := []string{"migrations/upgrade-note.md"}; !reflect.DeepEqual(result.ChangedFiles, want) {
		t.Fatalf("changed files = %v, want %v", result.ChangedFiles, want)
	}
	if got := result.Files.Snapshot()["app.lzi"]; got != "app Acme\n" {
		t.Fatalf("app.lzi = %q, want preserved content", got)
	}
}

func TestUpgradeRunnerReturnsPartialResultOnStepError(t *testing.T) {
	plan := UpgradePlan{
		FromVersion: "0.11",
		ToVersion:   "0.12",
		Recipes: []UpgradeRecipeDescriptor{
			{Name: "partial", FromVersion: "0.11", ToVersion: "0.12"},
		},
	}
	sentinel := errors.New("step failed")
	files := map[string]string{"app.lzi": "app Acme\n"}
	recipes := []UpgradeRecipeSteps{
		{
			Descriptor: plan.Recipes[0],
			Steps: []UpgradeStep{
				UpgradeStepFunc(func(_ context.Context, files *UpgradeFileSet) error {
					return files.WriteFile("app.lzi", "app Acme\n  lazuli_version \"0.12\"\n")
				}),
				UpgradeStepFunc(func(context.Context, *UpgradeFileSet) error {
					return sentinel
				}),
			},
		},
	}

	result, err := NewUpgradeRunner(plan, recipes).Apply(context.Background(), files)
	if !errors.Is(err, sentinel) {
		t.Fatalf("Apply error = %v, want sentinel", err)
	}
	if files["app.lzi"] != "app Acme\n" {
		t.Fatalf("input file map was mutated: %q", files["app.lzi"])
	}
	if want := []string{"app.lzi"}; !reflect.DeepEqual(result.ChangedFiles, want) {
		t.Fatalf("changed files = %v, want %v", result.ChangedFiles, want)
	}
	if got := result.Files.Snapshot()["app.lzi"]; got != "app Acme\n  lazuli_version \"0.12\"\n" {
		t.Fatalf("partial app.lzi = %q", got)
	}
	if len(result.Applied) != 0 {
		t.Fatalf("applied recipes = %#v, want none after failed recipe", result.Applied)
	}
}

func TestUpgradeRunnerValidatesInputs(t *testing.T) {
	plan := UpgradePlan{
		FromVersion: "0.11",
		ToVersion:   "0.12",
		Recipes: []UpgradeRecipeDescriptor{
			{Name: "rename", FromVersion: "0.11", ToVersion: "0.12"},
		},
	}
	validStep := UpgradeStepFunc(func(context.Context, *UpgradeFileSet) error { return nil })

	tests := []struct {
		name    string
		files   map[string]string
		recipes []UpgradeRecipeSteps
		want    error
	}{
		{
			name:    "invalid file path",
			files:   map[string]string{"../app.lzi": "app Acme\n"},
			recipes: []UpgradeRecipeSteps{{Descriptor: plan.Recipes[0], Steps: []UpgradeStep{validStep}}},
			want:    ErrInvalidUpgradeFilePath,
		},
		{
			name:    "missing recipe steps",
			files:   map[string]string{"app.lzi": "app Acme\n"},
			recipes: nil,
			want:    ErrUpgradeRecipeStepsRequired,
		},
		{
			name:  "duplicate recipe steps",
			files: map[string]string{"app.lzi": "app Acme\n"},
			recipes: []UpgradeRecipeSteps{
				{Descriptor: plan.Recipes[0], Steps: []UpgradeStep{validStep}},
				{Descriptor: plan.Recipes[0], Steps: []UpgradeStep{validStep}},
			},
			want: ErrDuplicateUpgradeRecipe,
		},
		{
			name:    "nil step",
			files:   map[string]string{"app.lzi": "app Acme\n"},
			recipes: []UpgradeRecipeSteps{{Descriptor: plan.Recipes[0], Steps: []UpgradeStep{UpgradeStepFunc(nil)}}},
			want:    ErrUpgradeStepRequired,
		},
		{
			name:    "missing recipe name",
			files:   map[string]string{"app.lzi": "app Acme\n"},
			recipes: []UpgradeRecipeSteps{{Descriptor: UpgradeRecipeDescriptor{FromVersion: "0.11", ToVersion: "0.12"}, Steps: []UpgradeStep{validStep}}},
			want:    ErrUpgradeRecipeNameRequired,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := NewUpgradeRunner(plan, tt.recipes).Apply(context.Background(), tt.files)
			if !errors.Is(err, tt.want) {
				t.Fatalf("Apply error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestUpgradeFileSetRejectsUnsafeReadWritePaths(t *testing.T) {
	files, err := NewUpgradeFileSet(map[string]string{"app.lzi": "app Acme\n"})
	if err != nil {
		t.Fatalf("NewUpgradeFileSet returned %v", err)
	}

	if _, _, err := files.ReadFile("features/../app.lzi"); !errors.Is(err, ErrInvalidUpgradeFilePath) {
		t.Fatalf("ReadFile error = %v, want ErrInvalidUpgradeFilePath", err)
	}
	if err := files.WriteFile(`features\customer.lzi`, "resource Customer\n"); !errors.Is(err, ErrInvalidUpgradeFilePath) {
		t.Fatalf("WriteFile error = %v, want ErrInvalidUpgradeFilePath", err)
	}
	if want := []string{"app.lzi"}; !reflect.DeepEqual(files.Paths(), want) {
		t.Fatalf("paths = %v, want %v", files.Paths(), want)
	}
}
