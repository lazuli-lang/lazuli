package authz

import (
	"fmt"
	"sort"
	"strings"
)

// RoleInheritanceEdge links a child role to a parent role it inherits.
type RoleInheritanceEdge struct {
	Parent string
	Child  string
}

// RoleInheritanceGraph is an immutable role inheritance graph built from
// parent/child edges.
type RoleInheritanceGraph struct {
	parents  map[string][]string
	children map[string][]string
}

// NewRoleInheritanceGraph validates edges and returns an immutable inheritance
// graph. Duplicate edges are collapsed after trimming role names.
func NewRoleInheritanceGraph(edges ...RoleInheritanceEdge) (*RoleInheritanceGraph, error) {
	normalized, err := normalizeRoleInheritanceEdges(edges)
	if err != nil {
		return nil, err
	}

	graph := buildRoleInheritanceGraph(normalized)
	if err := graph.detectCycles(); err != nil {
		return nil, err
	}
	return graph, nil
}

// SortedRoleInheritanceEdges returns a normalized, deterministically sorted copy
// of edges. Empty parent or child names are invalid, and cyclic inheritance
// returns ErrRoleCycle.
func SortedRoleInheritanceEdges(edges []RoleInheritanceEdge) ([]RoleInheritanceEdge, error) {
	graph, err := NewRoleInheritanceGraph(edges...)
	if err != nil {
		return nil, err
	}
	return graph.Edges(), nil
}

// SortedRoleNames returns a trimmed, deduplicated, deterministically sorted copy
// of role names.
func SortedRoleNames(roles []string) []string {
	roles = cleanRoleNames(roles)
	sort.Strings(roles)
	return roles
}

// Roles returns all roles that appear in inheritance edges.
func (g *RoleInheritanceGraph) Roles() []string {
	if g == nil {
		return nil
	}

	seen := map[string]struct{}{}
	for role := range g.parents {
		seen[role] = struct{}{}
	}
	for role := range g.children {
		seen[role] = struct{}{}
	}
	return sortedRoleInheritanceKeys(seen)
}

// Edges returns all inheritance edges sorted by parent, then child.
func (g *RoleInheritanceGraph) Edges() []RoleInheritanceEdge {
	if g == nil {
		return nil
	}

	edges := make([]RoleInheritanceEdge, 0)
	for _, parent := range g.Roles() {
		for _, child := range g.children[parent] {
			edges = append(edges, RoleInheritanceEdge{Parent: parent, Child: child})
		}
	}
	sortRoleInheritanceEdges(edges)
	return edges
}

// Parents returns the sorted direct parent roles inherited by child.
func (g *RoleInheritanceGraph) Parents(child string) []string {
	if g == nil {
		return nil
	}
	return append([]string{}, g.parents[strings.TrimSpace(child)]...)
}

// Children returns the sorted direct child roles that inherit parent.
func (g *RoleInheritanceGraph) Children(parent string) []string {
	if g == nil {
		return nil
	}
	return append([]string{}, g.children[strings.TrimSpace(parent)]...)
}

// HasRole reports whether role appears in at least one inheritance edge.
func (g *RoleInheritanceGraph) HasRole(role string) bool {
	if g == nil {
		return false
	}

	role = strings.TrimSpace(role)
	if role == "" {
		return false
	}
	if _, ok := g.parents[role]; ok {
		return true
	}
	_, ok := g.children[role]
	return ok
}

// HasEdge reports whether child directly inherits parent.
func (g *RoleInheritanceGraph) HasEdge(parent, child string) bool {
	if g == nil {
		return false
	}

	parent = strings.TrimSpace(parent)
	child = strings.TrimSpace(child)
	if parent == "" || child == "" {
		return false
	}
	for _, candidate := range g.parents[child] {
		if candidate == parent {
			return true
		}
	}
	return false
}

// Inherits reports whether child inherits parent directly or transitively. A
// role is not considered to inherit itself.
func (g *RoleInheritanceGraph) Inherits(child, parent string) bool {
	if g == nil {
		return false
	}

	child = strings.TrimSpace(child)
	parent = strings.TrimSpace(parent)
	if child == "" || parent == "" || child == parent {
		return false
	}

	return g.roleInherits(child, parent, map[string]struct{}{})
}

// EffectiveRoles returns active roles plus all inherited parent roles. The
// returned slice is trimmed, deduplicated, and sorted.
func (g *RoleInheritanceGraph) EffectiveRoles(activeRoles ...string) []string {
	seen := map[string]struct{}{}
	for _, role := range cleanRoleNames(activeRoles) {
		seen[role] = struct{}{}
		if g != nil {
			g.collectEffectiveRoles(role, seen, map[string]struct{}{})
		}
	}
	return sortedRoleInheritanceKeys(seen)
}

func normalizeRoleInheritanceEdges(edges []RoleInheritanceEdge) ([]RoleInheritanceEdge, error) {
	seen := map[RoleInheritanceEdge]struct{}{}
	normalized := make([]RoleInheritanceEdge, 0, len(edges))
	for i, edge := range edges {
		edge.Parent = strings.TrimSpace(edge.Parent)
		edge.Child = strings.TrimSpace(edge.Child)
		if edge.Parent == "" {
			return nil, fmt.Errorf("%w: inheritance edge %d has empty parent role", ErrRoleGraphInvalid, i)
		}
		if edge.Child == "" {
			return nil, fmt.Errorf("%w: inheritance edge %d has empty child role", ErrRoleGraphInvalid, i)
		}
		if _, ok := seen[edge]; ok {
			continue
		}
		seen[edge] = struct{}{}
		normalized = append(normalized, edge)
	}
	sortRoleInheritanceEdges(normalized)
	return normalized, nil
}

func buildRoleInheritanceGraph(edges []RoleInheritanceEdge) *RoleInheritanceGraph {
	graph := &RoleInheritanceGraph{
		parents:  map[string][]string{},
		children: map[string][]string{},
	}
	for _, edge := range edges {
		graph.parents[edge.Child] = append(graph.parents[edge.Child], edge.Parent)
		graph.children[edge.Parent] = append(graph.children[edge.Parent], edge.Child)
		if _, ok := graph.parents[edge.Parent]; !ok {
			graph.parents[edge.Parent] = nil
		}
		if _, ok := graph.children[edge.Child]; !ok {
			graph.children[edge.Child] = nil
		}
	}
	return graph
}

func (g *RoleInheritanceGraph) detectCycles() error {
	const (
		unseen byte = iota
		visiting
		done
	)

	state := map[string]byte{}
	var stack []string

	var visit func(string) error
	visit = func(role string) error {
		switch state[role] {
		case visiting:
			return fmt.Errorf("%w: %s", ErrRoleCycle, formatCycle(stack, role))
		case done:
			return nil
		}

		state[role] = visiting
		stack = append(stack, role)
		for _, parent := range g.parents[role] {
			if err := visit(parent); err != nil {
				return err
			}
		}
		stack = stack[:len(stack)-1]
		state[role] = done
		return nil
	}

	for _, role := range g.Roles() {
		if err := visit(role); err != nil {
			return err
		}
	}
	return nil
}

func (g *RoleInheritanceGraph) roleInherits(child, parent string, visited map[string]struct{}) bool {
	if _, ok := visited[child]; ok {
		return false
	}
	visited[child] = struct{}{}

	for _, candidate := range g.parents[child] {
		if candidate == parent || g.roleInherits(candidate, parent, visited) {
			return true
		}
	}
	return false
}

func (g *RoleInheritanceGraph) collectEffectiveRoles(role string, seen, visited map[string]struct{}) {
	if _, ok := visited[role]; ok {
		return
	}
	visited[role] = struct{}{}

	for _, parent := range g.parents[role] {
		seen[parent] = struct{}{}
		g.collectEffectiveRoles(parent, seen, visited)
	}
}

func sortedRoleInheritanceKeys(keys map[string]struct{}) []string {
	out := make([]string, 0, len(keys))
	for key := range keys {
		out = append(out, key)
	}
	sort.Strings(out)
	return out
}

func sortRoleInheritanceEdges(edges []RoleInheritanceEdge) {
	sort.SliceStable(edges, func(i, j int) bool {
		if edges[i].Parent != edges[j].Parent {
			return edges[i].Parent < edges[j].Parent
		}
		return edges[i].Child < edges[j].Child
	})
}
