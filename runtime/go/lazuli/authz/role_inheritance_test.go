package authz

import (
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestRoleInheritanceGraphReportsParentChildEdges(t *testing.T) {
	t.Parallel()

	graph := mustRoleInheritanceGraph(t,
		RoleInheritanceEdge{Parent: "viewer", Child: "editor"},
		RoleInheritanceEdge{Parent: "editor", Child: "admin"},
		RoleInheritanceEdge{Parent: "auditor", Child: "admin"},
	)

	if got, want := graph.Roles(), []string{"admin", "auditor", "editor", "viewer"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Roles() = %#v, want %#v", got, want)
	}
	if got, want := graph.Parents(" admin "), []string{"auditor", "editor"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Parents(admin) = %#v, want %#v", got, want)
	}
	if got, want := graph.Children(" editor "), []string{"admin"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Children(editor) = %#v, want %#v", got, want)
	}
	if !graph.HasRole(" viewer ") {
		t.Fatal("HasRole(viewer) = false, want true")
	}
	if !graph.HasEdge("editor", "admin") {
		t.Fatal("HasEdge(editor, admin) = false, want true")
	}
	if graph.HasEdge("admin", "editor") {
		t.Fatal("HasEdge(admin, editor) = true, want false")
	}
}

func TestRoleInheritanceGraphReturnsDefensiveCopies(t *testing.T) {
	t.Parallel()

	graph := mustRoleInheritanceGraph(t,
		RoleInheritanceEdge{Parent: "viewer", Child: "editor"},
		RoleInheritanceEdge{Parent: "editor", Child: "admin"},
	)

	parents := graph.Parents("admin")
	parents[0] = "changed"
	if got, want := graph.Parents("admin"), []string{"editor"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Parents(admin) after mutation = %#v, want %#v", got, want)
	}

	children := graph.Children("editor")
	children[0] = "changed"
	if got, want := graph.Children("editor"), []string{"admin"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Children(editor) after mutation = %#v, want %#v", got, want)
	}
}

func TestRoleInheritanceGraphExpandsEffectiveRoles(t *testing.T) {
	t.Parallel()

	graph := mustRoleInheritanceGraph(t,
		RoleInheritanceEdge{Parent: "viewer", Child: "editor"},
		RoleInheritanceEdge{Parent: "editor", Child: "admin"},
		RoleInheritanceEdge{Parent: "auditor", Child: "admin"},
	)

	if !graph.Inherits("admin", "viewer") {
		t.Fatal("admin should transitively inherit viewer")
	}
	if graph.Inherits("viewer", "admin") {
		t.Fatal("viewer should not inherit admin")
	}
	if graph.Inherits("admin", "admin") {
		t.Fatal("role should not inherit itself")
	}

	got := graph.EffectiveRoles(" admin ", "support", "admin")
	want := []string{"admin", "auditor", "editor", "support", "viewer"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("EffectiveRoles() = %#v, want %#v", got, want)
	}
}

func TestSortedRoleInheritanceEdgesNormalizesSortsAndDoesNotMutate(t *testing.T) {
	t.Parallel()

	edges := []RoleInheritanceEdge{
		{Parent: " editor ", Child: " admin "},
		{Parent: "viewer", Child: "editor"},
		{Parent: "editor", Child: "admin"},
		{Parent: "auditor", Child: "admin"},
	}

	got, err := SortedRoleInheritanceEdges(edges)
	if err != nil {
		t.Fatalf("SortedRoleInheritanceEdges() error = %v", err)
	}

	want := []RoleInheritanceEdge{
		{Parent: "auditor", Child: "admin"},
		{Parent: "editor", Child: "admin"},
		{Parent: "viewer", Child: "editor"},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("SortedRoleInheritanceEdges() = %#v, want %#v", got, want)
	}
	if edges[0].Parent != " editor " || edges[0].Child != " admin " {
		t.Fatalf("SortedRoleInheritanceEdges() mutated input: %#v", edges[0])
	}
}

func TestSortedRoleNamesNormalizesAndSorts(t *testing.T) {
	t.Parallel()

	got := SortedRoleNames([]string{" editor ", "admin", "editor", " ", "viewer"})
	want := []string{"admin", "editor", "viewer"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("SortedRoleNames() = %#v, want %#v", got, want)
	}
}

func TestNewRoleInheritanceGraphDetectsCycle(t *testing.T) {
	t.Parallel()

	_, err := NewRoleInheritanceGraph(
		RoleInheritanceEdge{Parent: "editor", Child: "admin"},
		RoleInheritanceEdge{Parent: "viewer", Child: "editor"},
		RoleInheritanceEdge{Parent: "admin", Child: "viewer"},
	)
	if !errors.Is(err, ErrRoleCycle) {
		t.Fatalf("NewRoleInheritanceGraph() error = %v, want ErrRoleCycle", err)
	}
	if err == nil || !strings.Contains(err.Error(), "admin -> editor -> viewer -> admin") {
		t.Fatalf("NewRoleInheritanceGraph() error = %v, want cycle path", err)
	}
}

func TestNewRoleInheritanceGraphRejectsInvalidEdges(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		edge RoleInheritanceEdge
	}{
		{
			name: "empty parent",
			edge: RoleInheritanceEdge{Parent: " ", Child: "admin"},
		},
		{
			name: "empty child",
			edge: RoleInheritanceEdge{Parent: "viewer", Child: " "},
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			_, err := NewRoleInheritanceGraph(tt.edge)
			if !errors.Is(err, ErrRoleGraphInvalid) {
				t.Fatalf("NewRoleInheritanceGraph() error = %v, want ErrRoleGraphInvalid", err)
			}
		})
	}
}

func mustRoleInheritanceGraph(t *testing.T, edges ...RoleInheritanceEdge) *RoleInheritanceGraph {
	t.Helper()

	graph, err := NewRoleInheritanceGraph(edges...)
	if err != nil {
		t.Fatalf("NewRoleInheritanceGraph() error = %v", err)
	}
	return graph
}
