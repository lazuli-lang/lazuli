package migrations

import (
	"errors"
	"fmt"
	"sort"
	"strconv"
	"strings"
)

var (
	// ErrUpgradePlanVersionRequired is returned when a plan request omits the
	// source or target Lazuli version.
	ErrUpgradePlanVersionRequired = errors.New("migrations: upgrade plan version required")
	// ErrUpgradeRecipeNameRequired is returned when a recipe descriptor has no
	// stable recipe name.
	ErrUpgradeRecipeNameRequired = errors.New("migrations: upgrade recipe name required")
	// ErrUpgradeRecipeVersionRequired is returned when a recipe descriptor does
	// not declare both sides of its version edge.
	ErrUpgradeRecipeVersionRequired = errors.New("migrations: upgrade recipe version required")
	// ErrDuplicateUpgradeRecipe is returned when two descriptors describe the
	// same recipe on the same version edge.
	ErrDuplicateUpgradeRecipe = errors.New("migrations: duplicate upgrade recipe")
	// ErrUpgradePathNotFound is returned when no recipe edge path connects the
	// requested source and target versions.
	ErrUpgradePathNotFound = errors.New("migrations: upgrade path not found")
)

// UpgradeRecipeDescriptor is the in-memory metadata for one upgrade recipe.
//
// The planner treats recipes as directed edges from FromVersion to ToVersion.
// Name is the stable recipe directory name, Kind is adapter-defined metadata
// from recipe.toml, and Path is optional caller-owned context. The planner does
// not read recipe files from disk.
type UpgradeRecipeDescriptor struct {
	Name        string
	FromVersion string
	ToVersion   string
	Kind        string
	Path        string
}

// UpgradePlanner composes recipe descriptors into ordered upgrade plans.
type UpgradePlanner struct {
	// Recipes is the complete in-memory recipe catalog visible to the planner.
	Recipes []UpgradeRecipeDescriptor
}

// UpgradePlan is the ordered set of recipes needed to upgrade between two
// Lazuli versions. Recipes are ordered in application order.
type UpgradePlan struct {
	FromVersion string
	ToVersion   string
	Recipes     []UpgradeRecipeDescriptor
}

// NewUpgradePlanner returns a planner over the supplied in-memory recipe
// descriptors.
func NewUpgradePlanner(recipes []UpgradeRecipeDescriptor) UpgradePlanner {
	return UpgradePlanner{Recipes: recipes}
}

// PlanUpgrade composes the supplied recipe descriptors into an ordered upgrade
// plan from fromVersion to toVersion.
func PlanUpgrade(fromVersion, toVersion string, recipes []UpgradeRecipeDescriptor) (UpgradePlan, error) {
	return NewUpgradePlanner(recipes).Plan(fromVersion, toVersion)
}

// Plan composes the planner's recipe catalog into an ordered upgrade plan from
// fromVersion to toVersion.
func (p UpgradePlanner) Plan(fromVersion, toVersion string) (UpgradePlan, error) {
	fromVersion = upgradePlanNormalizeVersion(fromVersion)
	toVersion = upgradePlanNormalizeVersion(toVersion)
	plan := UpgradePlan{FromVersion: fromVersion, ToVersion: toVersion}

	if fromVersion == "" || toVersion == "" {
		return plan, ErrUpgradePlanVersionRequired
	}
	if fromVersion == toVersion {
		return plan, nil
	}

	graph, err := upgradePlanBuildGraph(p.Recipes)
	if err != nil {
		return plan, err
	}

	edges, ok := upgradePlanFindPath(graph, fromVersion, toVersion)
	if !ok {
		return plan, fmt.Errorf("%w from %q to %q", ErrUpgradePathNotFound, fromVersion, toVersion)
	}
	for _, edge := range edges {
		plan.Recipes = append(plan.Recipes, edge.recipes...)
	}
	return plan, nil
}

type upgradePlanEdge struct {
	fromVersion string
	toVersion   string
	recipes     []UpgradeRecipeDescriptor
}

type upgradePlanPath struct {
	version string
	edges   []upgradePlanEdge
}

func upgradePlanBuildGraph(recipes []UpgradeRecipeDescriptor) (map[string][]upgradePlanEdge, error) {
	edgesByKey := make(map[string]*upgradePlanEdge)
	seenRecipes := make(map[string]struct{}, len(recipes))

	for i, recipe := range recipes {
		recipe = upgradePlanNormalizeRecipe(recipe)
		if recipe.Name == "" {
			return nil, fmt.Errorf("migrations: upgrade recipe %d: %w", i, ErrUpgradeRecipeNameRequired)
		}
		if recipe.FromVersion == "" || recipe.ToVersion == "" {
			return nil, fmt.Errorf("migrations: upgrade recipe %q: %w", recipe.Name, ErrUpgradeRecipeVersionRequired)
		}

		recipeID := upgradePlanRecipeID(recipe)
		if _, exists := seenRecipes[recipeID]; exists {
			return nil, fmt.Errorf("%w %q", ErrDuplicateUpgradeRecipe, recipeID)
		}
		seenRecipes[recipeID] = struct{}{}

		edgeKey := upgradePlanEdgeKey(recipe.FromVersion, recipe.ToVersion)
		edge, exists := edgesByKey[edgeKey]
		if !exists {
			edge = &upgradePlanEdge{
				fromVersion: recipe.FromVersion,
				toVersion:   recipe.ToVersion,
			}
			edgesByKey[edgeKey] = edge
		}
		edge.recipes = append(edge.recipes, recipe)
	}

	graph := make(map[string][]upgradePlanEdge, len(edgesByKey))
	for _, edge := range edgesByKey {
		sort.Slice(edge.recipes, func(i, j int) bool {
			return upgradePlanRecipeLess(edge.recipes[i], edge.recipes[j])
		})
		graph[edge.fromVersion] = append(graph[edge.fromVersion], *edge)
	}
	for fromVersion := range graph {
		sort.Slice(graph[fromVersion], func(i, j int) bool {
			return upgradePlanEdgeLess(graph[fromVersion][i], graph[fromVersion][j])
		})
	}
	return graph, nil
}

func upgradePlanFindPath(graph map[string][]upgradePlanEdge, fromVersion, toVersion string) ([]upgradePlanEdge, bool) {
	queue := []upgradePlanPath{{version: fromVersion}}
	visited := map[string]struct{}{fromVersion: {}}

	for len(queue) > 0 {
		current := queue[0]
		queue = queue[1:]

		for _, edge := range graph[current.version] {
			if _, exists := visited[edge.toVersion]; exists {
				continue
			}

			nextEdges := make([]upgradePlanEdge, 0, len(current.edges)+1)
			nextEdges = append(nextEdges, current.edges...)
			nextEdges = append(nextEdges, edge)
			if edge.toVersion == toVersion {
				return nextEdges, true
			}

			visited[edge.toVersion] = struct{}{}
			queue = append(queue, upgradePlanPath{
				version: edge.toVersion,
				edges:   nextEdges,
			})
		}
	}
	return nil, false
}

func upgradePlanNormalizeRecipe(recipe UpgradeRecipeDescriptor) UpgradeRecipeDescriptor {
	recipe.Name = strings.TrimSpace(recipe.Name)
	recipe.FromVersion = upgradePlanNormalizeVersion(recipe.FromVersion)
	recipe.ToVersion = upgradePlanNormalizeVersion(recipe.ToVersion)
	recipe.Kind = strings.TrimSpace(recipe.Kind)
	recipe.Path = strings.TrimSpace(recipe.Path)
	return recipe
}

func upgradePlanNormalizeVersion(version string) string {
	return strings.TrimSpace(version)
}

func upgradePlanEdgeKey(fromVersion, toVersion string) string {
	return fromVersion + "\x00" + toVersion
}

func upgradePlanRecipeID(recipe UpgradeRecipeDescriptor) string {
	return upgradePlanEdgeKey(recipe.FromVersion, recipe.ToVersion) + "\x00" + recipe.Name
}

func upgradePlanEdgeLess(a, b upgradePlanEdge) bool {
	if a.toVersion != b.toVersion {
		return upgradePlanVersionLess(a.toVersion, b.toVersion)
	}
	if len(a.recipes) == 0 || len(b.recipes) == 0 {
		return len(a.recipes) < len(b.recipes)
	}
	return upgradePlanRecipeLess(a.recipes[0], b.recipes[0])
}

func upgradePlanRecipeLess(a, b UpgradeRecipeDescriptor) bool {
	if a.Name != b.Name {
		return a.Name < b.Name
	}
	if a.Kind != b.Kind {
		return a.Kind < b.Kind
	}
	if a.Path != b.Path {
		return a.Path < b.Path
	}
	if a.FromVersion != b.FromVersion {
		return upgradePlanVersionLess(a.FromVersion, b.FromVersion)
	}
	return upgradePlanVersionLess(a.ToVersion, b.ToVersion)
}

func upgradePlanVersionLess(a, b string) bool {
	aParts, aOK := upgradePlanVersionParts(a)
	bParts, bOK := upgradePlanVersionParts(b)
	if !aOK || !bOK {
		return a < b
	}

	common := len(aParts)
	if len(bParts) < common {
		common = len(bParts)
	}
	for i := 0; i < common; i++ {
		if aParts[i] != bParts[i] {
			return aParts[i] < bParts[i]
		}
	}
	return len(aParts) < len(bParts)
}

func upgradePlanVersionParts(version string) ([]int, bool) {
	segments := strings.Split(version, ".")
	if len(segments) == 0 {
		return nil, false
	}
	parts := make([]int, len(segments))
	for i, segment := range segments {
		if segment == "" {
			return nil, false
		}
		part, err := strconv.Atoi(segment)
		if err != nil || part < 0 {
			return nil, false
		}
		parts[i] = part
	}
	return parts, true
}
