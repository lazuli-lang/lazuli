// Package authz provides small authorization helpers that are independent of
// Lazuli's generated command/query policy enforcement.
package authz

import (
	"errors"
	"fmt"
	"sort"
	"strings"
)

const permissionSeparator = ":"

var (
	// ErrRoleGraphInvalid is returned when role definitions are malformed.
	ErrRoleGraphInvalid = errors.New("lazuli/authz: role_graph_invalid")

	// ErrRoleCycle is returned when role inheritance contains a cycle.
	ErrRoleCycle = errors.New("lazuli/authz: role_cycle")
)

// Permission is a stable permission token. Resource/action permissions use the
// "resource:action" form produced by PermissionFor.
type Permission string

// String returns the permission token.
func (p Permission) String() string {
	return string(p)
}

// PermissionFor builds a permission token from resource and action parts.
// Empty resource or action input returns an empty permission that never matches.
func PermissionFor(resource, action string) Permission {
	resource = strings.TrimSpace(resource)
	action = strings.TrimSpace(action)
	if resource == "" || action == "" ||
		strings.Contains(resource, permissionSeparator) ||
		strings.Contains(action, permissionSeparator) {
		return ""
	}
	return Permission(resource + permissionSeparator + action)
}

// SplitPermission splits a "resource:action" permission token into parts.
func SplitPermission(permission Permission) (resource, action string, ok bool) {
	text := strings.TrimSpace(permission.String())
	resource, action, ok = strings.Cut(text, permissionSeparator)
	if !ok || strings.Contains(action, permissionSeparator) {
		return "", "", false
	}
	resource = strings.TrimSpace(resource)
	action = strings.TrimSpace(action)
	if resource == "" || action == "" {
		return "", "", false
	}
	return resource, action, true
}

// Role defines one RBAC role.
//
// Inherits lists parent roles whose permissions are granted to this role. For
// example, an "admin" role that inherits "editor" receives editor permissions.
type Role struct {
	Name        string
	Inherits    []string
	Permissions []Permission
}

// RoleGraph is an immutable role inheritance graph.
type RoleGraph struct {
	roles map[string]Role
}

// NewRoleGraph validates roles and returns an immutable inheritance graph.
func NewRoleGraph(roles ...Role) (*RoleGraph, error) {
	graph := &RoleGraph{roles: make(map[string]Role, len(roles))}
	for i, role := range roles {
		role.Name = strings.TrimSpace(role.Name)
		if role.Name == "" {
			return nil, fmt.Errorf("%w: role %d has empty name", ErrRoleGraphInvalid, i)
		}
		if _, exists := graph.roles[role.Name]; exists {
			return nil, fmt.Errorf("%w: duplicate role %q", ErrRoleGraphInvalid, role.Name)
		}
		role.Inherits = cleanRoleNames(role.Inherits)
		role.Permissions = cleanPermissions(role.Permissions)
		graph.roles[role.Name] = role
	}
	if err := graph.validateInheritedRoles(); err != nil {
		return nil, err
	}
	if err := graph.detectCycles(); err != nil {
		return nil, err
	}
	return graph, nil
}

// HasRole reports whether role is present in the graph.
func (g *RoleGraph) HasRole(role string) bool {
	if g == nil {
		return false
	}
	_, ok := g.roles[strings.TrimSpace(role)]
	return ok
}

// HasPermission reports whether any active role grants permission directly or
// through inherited roles. Unknown roles and empty permissions do not match.
func (g *RoleGraph) HasPermission(activeRoles []string, permission Permission) bool {
	if g == nil {
		return false
	}
	permission = cleanPermission(permission)
	if permission == "" {
		return false
	}

	visited := map[string]struct{}{}
	for _, role := range cleanRoleNames(activeRoles) {
		if g.roleHasPermission(role, permission, visited) {
			return true
		}
	}
	return false
}

// Can reports whether any active role grants the resource/action permission.
func (g *RoleGraph) Can(activeRoles []string, resource, action string) bool {
	return g.HasPermission(activeRoles, PermissionFor(resource, action))
}

// Inherits reports whether role inherits inheritedRole directly or
// transitively. A role is not considered to inherit itself.
func (g *RoleGraph) Inherits(role, inheritedRole string) bool {
	if g == nil {
		return false
	}
	role = strings.TrimSpace(role)
	inheritedRole = strings.TrimSpace(inheritedRole)
	if role == "" || inheritedRole == "" || role == inheritedRole {
		return false
	}

	visited := map[string]struct{}{}
	return g.roleInherits(role, inheritedRole, visited)
}

// EffectivePermissions returns all permissions granted to role directly or
// through inherited roles. The returned slice is sorted and deduplicated.
func (g *RoleGraph) EffectivePermissions(role string) []Permission {
	if g == nil {
		return nil
	}

	seen := map[Permission]struct{}{}
	visited := map[string]struct{}{}
	g.collectPermissions(strings.TrimSpace(role), seen, visited)

	out := make([]Permission, 0, len(seen))
	for permission := range seen {
		out = append(out, permission)
	}
	sort.Slice(out, func(i, j int) bool {
		return out[i] < out[j]
	})
	return out
}

func (g *RoleGraph) validateInheritedRoles() error {
	for _, roleName := range g.roleNames() {
		role := g.roles[roleName]
		for _, inherited := range role.Inherits {
			if _, ok := g.roles[inherited]; !ok {
				return fmt.Errorf("%w: role %q inherits unknown role %q", ErrRoleGraphInvalid, role.Name, inherited)
			}
		}
	}
	return nil
}

func (g *RoleGraph) detectCycles() error {
	const (
		unseen byte = iota
		visiting
		done
	)

	state := map[string]byte{}
	var stack []string

	var visit func(string) error
	visit = func(roleName string) error {
		switch state[roleName] {
		case visiting:
			return fmt.Errorf("%w: %s", ErrRoleCycle, formatCycle(stack, roleName))
		case done:
			return nil
		}

		state[roleName] = visiting
		stack = append(stack, roleName)
		for _, inherited := range g.roles[roleName].Inherits {
			if err := visit(inherited); err != nil {
				return err
			}
		}
		stack = stack[:len(stack)-1]
		state[roleName] = done
		return nil
	}

	for _, roleName := range g.roleNames() {
		if err := visit(roleName); err != nil {
			return err
		}
	}
	return nil
}

func (g *RoleGraph) roleHasPermission(roleName string, permission Permission, visited map[string]struct{}) bool {
	if _, ok := visited[roleName]; ok {
		return false
	}
	visited[roleName] = struct{}{}

	role, ok := g.roles[roleName]
	if !ok {
		return false
	}
	for _, candidate := range role.Permissions {
		if candidate == permission {
			return true
		}
	}
	for _, inherited := range role.Inherits {
		if g.roleHasPermission(inherited, permission, visited) {
			return true
		}
	}
	return false
}

func (g *RoleGraph) roleInherits(roleName, inheritedRole string, visited map[string]struct{}) bool {
	if _, ok := visited[roleName]; ok {
		return false
	}
	visited[roleName] = struct{}{}

	role, ok := g.roles[roleName]
	if !ok {
		return false
	}
	for _, inherited := range role.Inherits {
		if inherited == inheritedRole || g.roleInherits(inherited, inheritedRole, visited) {
			return true
		}
	}
	return false
}

func (g *RoleGraph) collectPermissions(roleName string, seen map[Permission]struct{}, visited map[string]struct{}) {
	if _, ok := visited[roleName]; ok {
		return
	}
	visited[roleName] = struct{}{}

	role, ok := g.roles[roleName]
	if !ok {
		return
	}
	for _, permission := range role.Permissions {
		seen[permission] = struct{}{}
	}
	for _, inherited := range role.Inherits {
		g.collectPermissions(inherited, seen, visited)
	}
}

func (g *RoleGraph) roleNames() []string {
	names := make([]string, 0, len(g.roles))
	for name := range g.roles {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}

func cleanRoleNames(names []string) []string {
	seen := map[string]struct{}{}
	out := make([]string, 0, len(names))
	for _, name := range names {
		name = strings.TrimSpace(name)
		if name == "" {
			continue
		}
		if _, ok := seen[name]; ok {
			continue
		}
		seen[name] = struct{}{}
		out = append(out, name)
	}
	return out
}

func cleanPermissions(permissions []Permission) []Permission {
	seen := map[Permission]struct{}{}
	out := make([]Permission, 0, len(permissions))
	for _, permission := range permissions {
		permission = cleanPermission(permission)
		if permission == "" {
			continue
		}
		if _, ok := seen[permission]; ok {
			continue
		}
		seen[permission] = struct{}{}
		out = append(out, permission)
	}
	return out
}

func cleanPermission(permission Permission) Permission {
	return Permission(strings.TrimSpace(permission.String()))
}

func formatCycle(stack []string, repeated string) string {
	start := 0
	for i, role := range stack {
		if role == repeated {
			start = i
			break
		}
	}
	cycle := append(append([]string(nil), stack[start:]...), repeated)
	return strings.Join(cycle, " -> ")
}
