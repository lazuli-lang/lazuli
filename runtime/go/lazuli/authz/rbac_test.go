package authz

import (
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestPermissionForBuildsResourceActionPermission(t *testing.T) {
	t.Parallel()

	permission := PermissionFor(" customers ", " read ")
	if permission != Permission("customers:read") {
		t.Fatalf("PermissionFor() = %q, want customers:read", permission)
	}

	resource, action, ok := SplitPermission(permission)
	if !ok {
		t.Fatal("SplitPermission() ok = false, want true")
	}
	if resource != "customers" || action != "read" {
		t.Fatalf("SplitPermission() = %q, %q, want customers, read", resource, action)
	}
}

func TestPermissionForRejectsEmptyParts(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name       string
		resource   string
		action     string
		permission Permission
	}{
		{name: "empty resource", action: "read"},
		{name: "empty action", resource: "customers"},
		{name: "separator in resource", resource: "customers:private", action: "read"},
		{name: "separator in action", resource: "customers", action: "read:all"},
		{name: "missing separator", permission: Permission("customers.read")},
		{name: "extra separator", permission: Permission("customers:read:all")},
		{name: "blank resource", permission: Permission(" :read")},
		{name: "blank action", permission: Permission("customers: ")},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if tt.resource != "" || tt.action != "" {
				if got := PermissionFor(tt.resource, tt.action); got != "" {
					t.Fatalf("PermissionFor() = %q, want empty", got)
				}
				return
			}
			if resource, action, ok := SplitPermission(tt.permission); ok {
				t.Fatalf("SplitPermission() = %q, %q, true; want false", resource, action)
			}
		})
	}
}

func TestRoleGraphChecksInheritedPermissions(t *testing.T) {
	t.Parallel()

	graph := mustRoleGraph(t,
		Role{Name: "viewer", Permissions: []Permission{PermissionFor("customers", "read")}},
		Role{Name: "editor", Inherits: []string{"viewer"}, Permissions: []Permission{PermissionFor("customers", "update")}},
		Role{Name: "admin", Inherits: []string{"editor"}, Permissions: []Permission{PermissionFor("customers", "delete")}},
	)

	if !graph.Can([]string{"admin"}, "customers", "read") {
		t.Fatal("admin cannot read customers through inherited viewer role")
	}
	if !graph.Can([]string{" editor "}, "customers", "read") {
		t.Fatal("editor cannot read customers through inherited viewer role")
	}
	if !graph.HasPermission([]string{"admin"}, Permission("customers:update")) {
		t.Fatal("admin does not have inherited customers:update permission")
	}
	if graph.Can([]string{"viewer"}, "customers", "delete") {
		t.Fatal("viewer can delete customers without permission")
	}
	if graph.Can([]string{"missing"}, "customers", "read") {
		t.Fatal("unknown role can read customers")
	}
}

func TestRoleGraphReportsRoleInheritance(t *testing.T) {
	t.Parallel()

	graph := mustRoleGraph(t,
		Role{Name: "viewer"},
		Role{Name: "editor", Inherits: []string{"viewer"}},
		Role{Name: "admin", Inherits: []string{"editor"}},
	)

	if !graph.HasRole(" admin ") {
		t.Fatal("HasRole(admin) = false, want true")
	}
	if !graph.Inherits("admin", "viewer") {
		t.Fatal("admin should transitively inherit viewer")
	}
	if graph.Inherits("admin", "admin") {
		t.Fatal("role should not inherit itself")
	}
	if graph.Inherits("viewer", "admin") {
		t.Fatal("viewer should not inherit admin")
	}
}

func TestRoleGraphEffectivePermissionsAreSortedAndDeduplicated(t *testing.T) {
	t.Parallel()

	graph := mustRoleGraph(t,
		Role{Name: "viewer", Permissions: []Permission{
			PermissionFor("customers", "read"),
			PermissionFor("customers", "read"),
		}},
		Role{Name: "auditor", Permissions: []Permission{PermissionFor("reports", "read")}},
		Role{Name: "admin", Inherits: []string{"viewer", "auditor"}, Permissions: []Permission{PermissionFor("customers", "delete")}},
	)

	got := graph.EffectivePermissions("admin")
	want := []Permission{
		Permission("customers:delete"),
		Permission("customers:read"),
		Permission("reports:read"),
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("EffectivePermissions() = %#v, want %#v", got, want)
	}
}

func TestNewRoleGraphDetectsInheritanceCycle(t *testing.T) {
	t.Parallel()

	_, err := NewRoleGraph(
		Role{Name: "admin", Inherits: []string{"editor"}},
		Role{Name: "editor", Inherits: []string{"viewer"}},
		Role{Name: "viewer", Inherits: []string{"admin"}},
	)
	if !errors.Is(err, ErrRoleCycle) {
		t.Fatalf("NewRoleGraph() error = %v, want ErrRoleCycle", err)
	}
	if err == nil || !strings.Contains(err.Error(), "admin -> editor -> viewer -> admin") {
		t.Fatalf("NewRoleGraph() error = %v, want cycle path", err)
	}
}

func TestNewRoleGraphRejectsInvalidDefinitions(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		roles []Role
	}{
		{
			name:  "empty name",
			roles: []Role{{Name: " "}},
		},
		{
			name: "duplicate role",
			roles: []Role{
				{Name: "admin"},
				{Name: " admin "},
			},
		},
		{
			name:  "unknown inherited role",
			roles: []Role{{Name: "admin", Inherits: []string{"missing"}}},
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			_, err := NewRoleGraph(tt.roles...)
			if !errors.Is(err, ErrRoleGraphInvalid) {
				t.Fatalf("NewRoleGraph() error = %v, want ErrRoleGraphInvalid", err)
			}
		})
	}
}

func mustRoleGraph(t *testing.T, roles ...Role) *RoleGraph {
	t.Helper()

	graph, err := NewRoleGraph(roles...)
	if err != nil {
		t.Fatalf("NewRoleGraph() error = %v", err)
	}
	return graph
}
