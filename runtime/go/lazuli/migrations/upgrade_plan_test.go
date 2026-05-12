package migrations

import (
	"errors"
	"reflect"
	"testing"
)

func TestUpgradePlannerComposesVersionEdgesInOrder(t *testing.T) {
	recipes := []UpgradeRecipeDescriptor{
		{Name: "add_debug_bundle", FromVersion: "0.12", ToVersion: "0.13", Kind: "lzi"},
		{Name: "unrelated_future", FromVersion: "0.13", ToVersion: "0.14", Kind: "lzi"},
		{Name: "go_error_hierarchy", FromVersion: "0.12", ToVersion: "0.13", Kind: "go"},
		{Name: "rename_policies_to_rules", FromVersion: "0.11", ToVersion: "0.12", Kind: "lzi"},
	}

	plan, err := PlanUpgrade("0.11", "0.13", recipes)
	if err != nil {
		t.Fatalf("PlanUpgrade returned %v", err)
	}

	if plan.FromVersion != "0.11" || plan.ToVersion != "0.13" {
		t.Fatalf("plan versions = %q -> %q, want 0.11 -> 0.13", plan.FromVersion, plan.ToVersion)
	}
	if want := []string{"rename_policies_to_rules", "add_debug_bundle", "go_error_hierarchy"}; !reflect.DeepEqual(upgradePlanRecipeNames(plan), want) {
		t.Fatalf("plan recipes = %v, want %v", upgradePlanRecipeNames(plan), want)
	}
}

func TestUpgradePlannerChoosesShortestDeterministicPath(t *testing.T) {
	recipes := []UpgradeRecipeDescriptor{
		{Name: "step_b", FromVersion: "0.10", ToVersion: "0.11", Kind: "lzi"},
		{Name: "step_c", FromVersion: "0.11", ToVersion: "0.12", Kind: "lzi"},
		{Name: "direct", FromVersion: "0.10", ToVersion: "0.12", Kind: "lzi"},
	}

	plan, err := NewUpgradePlanner(recipes).Plan("0.10", "0.12")
	if err != nil {
		t.Fatalf("Plan returned %v", err)
	}

	if want := []string{"direct"}; !reflect.DeepEqual(upgradePlanRecipeNames(plan), want) {
		t.Fatalf("plan recipes = %v, want %v", upgradePlanRecipeNames(plan), want)
	}
}

func TestUpgradePlannerSameVersionReturnsEmptyPlan(t *testing.T) {
	plan, err := PlanUpgrade("0.12", "0.12", nil)
	if err != nil {
		t.Fatalf("PlanUpgrade returned %v", err)
	}
	if len(plan.Recipes) != 0 {
		t.Fatalf("plan recipes = %v, want none", plan.Recipes)
	}
}

func TestUpgradePlannerErrorsWhenPathMissing(t *testing.T) {
	recipes := []UpgradeRecipeDescriptor{
		{Name: "rename_policies_to_rules", FromVersion: "0.11", ToVersion: "0.12", Kind: "lzi"},
	}

	_, err := PlanUpgrade("0.11", "0.13", recipes)
	if !errors.Is(err, ErrUpgradePathNotFound) {
		t.Fatalf("expected ErrUpgradePathNotFound, got %v", err)
	}
}

func TestUpgradePlannerValidatesRecipeDescriptors(t *testing.T) {
	tests := []struct {
		name    string
		recipes []UpgradeRecipeDescriptor
		want    error
	}{
		{
			name:    "missing name",
			recipes: []UpgradeRecipeDescriptor{{FromVersion: "0.11", ToVersion: "0.12"}},
			want:    ErrUpgradeRecipeNameRequired,
		},
		{
			name:    "missing version",
			recipes: []UpgradeRecipeDescriptor{{Name: "rename_policies_to_rules", FromVersion: "0.11"}},
			want:    ErrUpgradeRecipeVersionRequired,
		},
		{
			name: "duplicate recipe",
			recipes: []UpgradeRecipeDescriptor{
				{Name: "rename_policies_to_rules", FromVersion: "0.11", ToVersion: "0.12"},
				{Name: "rename_policies_to_rules", FromVersion: "0.11", ToVersion: "0.12"},
			},
			want: ErrDuplicateUpgradeRecipe,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := PlanUpgrade("0.11", "0.12", tt.recipes)
			if !errors.Is(err, tt.want) {
				t.Fatalf("expected %v, got %v", tt.want, err)
			}
		})
	}
}

func TestUpgradePlannerRequiresPlanVersions(t *testing.T) {
	_, err := PlanUpgrade("", "0.12", nil)
	if !errors.Is(err, ErrUpgradePlanVersionRequired) {
		t.Fatalf("expected ErrUpgradePlanVersionRequired, got %v", err)
	}
}

func upgradePlanRecipeNames(plan UpgradePlan) []string {
	names := make([]string, len(plan.Recipes))
	for i, recipe := range plan.Recipes {
		names[i] = recipe.Name
	}
	return names
}
