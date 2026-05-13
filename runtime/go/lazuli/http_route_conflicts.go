package lazuli

import (
	"errors"
	"fmt"
	"net/http"
	"path"
	"sort"
	"strings"
)

var (
	// ErrInvalidRouteBinding is returned when generated route metadata cannot
	// be normalized into a route table entry.
	ErrInvalidRouteBinding = errors.New("lazuli: invalid http route binding")

	// ErrRouteConflict is returned when generated route metadata contains
	// duplicate method/path or route name bindings.
	ErrRouteConflict = errors.New("lazuli: http route conflict")
)

const (
	RouteConflictMethodPath = "method_path"
	RouteConflictName       = "name"
)

// RouteBinding describes one generated HTTP route registration.
type RouteBinding struct {
	Method string
	Path   string
	Name   string
}

// RouteConflict describes one deterministic conflict group in a route table.
type RouteConflict struct {
	Kind     string
	Key      string
	Bindings []RouteBinding
}

// RouteMethodNotAllowedMetadata describes the Allow metadata for one route
// path. Generated apps can use it to produce deterministic 405 responses.
type RouteMethodNotAllowedMetadata struct {
	Path            string
	Methods         []string
	Allow           string
	RouteNames      []string
	HasImplicitHEAD bool
}

// RouteConflictReport is the normalized route table and conflict metadata.
type RouteConflictReport struct {
	Bindings         []RouteBinding
	Conflicts        []RouteConflict
	MethodNotAllowed []RouteMethodNotAllowedMetadata
}

// NormalizeRouteBinding trims and canonicalizes generated route metadata.
func NormalizeRouteBinding(binding RouteBinding) (RouteBinding, error) {
	normalized := RouteBinding{
		Method: strings.ToUpper(strings.TrimSpace(binding.Method)),
		Path:   strings.TrimSpace(binding.Path),
		Name:   strings.TrimSpace(binding.Name),
	}
	if normalized.Method == "" || !validHTTPRouteToken(normalized.Method) {
		return RouteBinding{}, invalidRouteBinding("invalid method %q", binding.Method)
	}

	routePath, err := normalizeRoutePath(normalized.Path)
	if err != nil {
		return RouteBinding{}, invalidRouteBinding("invalid path %q: %v", binding.Path, err)
	}
	normalized.Path = routePath

	if normalized.Name == "" || strings.ContainsAny(normalized.Name, "\x00\r\n\t") {
		return RouteBinding{}, invalidRouteBinding("invalid name %q", binding.Name)
	}

	return normalized, nil
}

// DetectRouteConflicts normalizes route bindings, detects duplicate
// method/path and name bindings, and returns deterministic 405 metadata.
func DetectRouteConflicts(bindings []RouteBinding) (RouteConflictReport, error) {
	normalized := make([]RouteBinding, 0, len(bindings))
	for index, binding := range bindings {
		route, err := NormalizeRouteBinding(binding)
		if err != nil {
			return RouteConflictReport{}, fmt.Errorf("%w: route %d: %v", ErrInvalidRouteBinding, index, err)
		}
		normalized = append(normalized, route)
	}

	sortRouteBindings(normalized)

	report := RouteConflictReport{
		Bindings:         append([]RouteBinding(nil), normalized...),
		Conflicts:        detectRouteConflictGroups(normalized),
		MethodNotAllowed: buildMethodNotAllowedMetadata(normalized),
	}
	if len(report.Conflicts) > 0 {
		return report, routeConflictError{conflicts: report.Conflicts}
	}
	return report, nil
}

// OK reports whether the route table is conflict-free.
func (r RouteConflictReport) OK() bool {
	return len(r.Conflicts) == 0
}

// Summaries returns stable human-readable summaries for all conflicts.
func (r RouteConflictReport) Summaries() []string {
	summaries := make([]string, 0, len(r.Conflicts))
	for _, conflict := range r.Conflicts {
		summaries = append(summaries, conflict.Summary())
	}
	return summaries
}

// Summary returns a stable human-readable summary for a conflict group.
func (c RouteConflict) Summary() string {
	bindings := append([]RouteBinding(nil), c.Bindings...)
	sortRouteBindings(bindings)

	parts := make([]string, 0, len(bindings))
	for _, binding := range bindings {
		parts = append(parts, binding.Method+" "+binding.Path+" ("+binding.Name+")")
	}

	switch c.Kind {
	case RouteConflictMethodPath:
		return fmt.Sprintf("method/path %q is bound by %s", c.Key, strings.Join(parts, ", "))
	case RouteConflictName:
		return fmt.Sprintf("name %q is bound by %s", c.Key, strings.Join(parts, ", "))
	default:
		return fmt.Sprintf("%s %q is bound by %s", c.Kind, c.Key, strings.Join(parts, ", "))
	}
}

type routeConflictError struct {
	conflicts []RouteConflict
}

func (e routeConflictError) Error() string {
	summaries := make([]string, 0, len(e.conflicts))
	for _, conflict := range e.conflicts {
		summaries = append(summaries, conflict.Summary())
	}
	return fmt.Sprintf("%s: %s", ErrRouteConflict, strings.Join(summaries, "; "))
}

func (e routeConflictError) Unwrap() error {
	return ErrRouteConflict
}

func detectRouteConflictGroups(bindings []RouteBinding) []RouteConflict {
	byMethodPath := make(map[string][]RouteBinding, len(bindings))
	byName := make(map[string][]RouteBinding, len(bindings))
	for _, binding := range bindings {
		byMethodPath[binding.Method+" "+binding.Path] = append(byMethodPath[binding.Method+" "+binding.Path], binding)
		byName[binding.Name] = append(byName[binding.Name], binding)
	}

	conflicts := make([]RouteConflict, 0)
	conflicts = append(conflicts, routeConflictGroups(RouteConflictMethodPath, byMethodPath)...)
	conflicts = append(conflicts, routeConflictGroups(RouteConflictName, byName)...)
	sort.Slice(conflicts, func(i, j int) bool {
		if conflicts[i].Kind != conflicts[j].Kind {
			return conflicts[i].Kind < conflicts[j].Kind
		}
		return conflicts[i].Key < conflicts[j].Key
	})
	return conflicts
}

func routeConflictGroups(kind string, groups map[string][]RouteBinding) []RouteConflict {
	keys := make([]string, 0, len(groups))
	for key, bindings := range groups {
		if len(bindings) > 1 {
			keys = append(keys, key)
		}
	}
	sort.Strings(keys)

	conflicts := make([]RouteConflict, 0, len(keys))
	for _, key := range keys {
		bindings := append([]RouteBinding(nil), groups[key]...)
		sortRouteBindings(bindings)
		conflicts = append(conflicts, RouteConflict{
			Kind:     kind,
			Key:      key,
			Bindings: bindings,
		})
	}
	return conflicts
}

func buildMethodNotAllowedMetadata(bindings []RouteBinding) []RouteMethodNotAllowedMetadata {
	type pathState struct {
		methods map[string]struct{}
		names   map[string]struct{}
	}

	byPath := make(map[string]*pathState, len(bindings))
	for _, binding := range bindings {
		state := byPath[binding.Path]
		if state == nil {
			state = &pathState{
				methods: map[string]struct{}{},
				names:   map[string]struct{}{},
			}
			byPath[binding.Path] = state
		}
		state.methods[binding.Method] = struct{}{}
		state.names[binding.Name] = struct{}{}
	}

	paths := make([]string, 0, len(byPath))
	for routePath := range byPath {
		paths = append(paths, routePath)
	}
	sort.Strings(paths)

	metadata := make([]RouteMethodNotAllowedMetadata, 0, len(paths))
	for _, routePath := range paths {
		state := byPath[routePath]
		methods, hasImplicitHEAD := allowedRouteMethods(state.methods)
		names := sortedRouteSet(state.names)
		metadata = append(metadata, RouteMethodNotAllowedMetadata{
			Path:            routePath,
			Methods:         methods,
			Allow:           strings.Join(methods, ", "),
			RouteNames:      names,
			HasImplicitHEAD: hasImplicitHEAD,
		})
	}
	return metadata
}

func allowedRouteMethods(methodSet map[string]struct{}) ([]string, bool) {
	allowed := make(map[string]struct{}, len(methodSet)+1)
	for method := range methodSet {
		allowed[method] = struct{}{}
	}

	hasImplicitHEAD := false
	if _, hasGET := methodSet[http.MethodGet]; hasGET {
		if _, hasHEAD := methodSet[http.MethodHead]; !hasHEAD {
			allowed[http.MethodHead] = struct{}{}
			hasImplicitHEAD = true
		}
	}

	methods := sortedRouteSet(allowed)
	sort.SliceStable(methods, func(i, j int) bool {
		return routeMethodLess(methods[i], methods[j])
	})
	return methods, hasImplicitHEAD
}

func sortedRouteSet(set map[string]struct{}) []string {
	values := make([]string, 0, len(set))
	for value := range set {
		values = append(values, value)
	}
	sort.Strings(values)
	return values
}

func sortRouteBindings(bindings []RouteBinding) {
	sort.Slice(bindings, func(i, j int) bool {
		if bindings[i].Path != bindings[j].Path {
			return bindings[i].Path < bindings[j].Path
		}
		if bindings[i].Method != bindings[j].Method {
			return routeMethodLess(bindings[i].Method, bindings[j].Method)
		}
		return bindings[i].Name < bindings[j].Name
	})
}

func routeMethodLess(left, right string) bool {
	leftRank := routeMethodRank(left)
	rightRank := routeMethodRank(right)
	if leftRank != rightRank {
		return leftRank < rightRank
	}
	return left < right
}

func routeMethodRank(method string) int {
	switch method {
	case http.MethodGet:
		return 0
	case http.MethodHead:
		return 1
	case http.MethodPost:
		return 2
	case http.MethodPut:
		return 3
	case http.MethodPatch:
		return 4
	case http.MethodDelete:
		return 5
	case http.MethodOptions:
		return 6
	default:
		return 1000
	}
}

func normalizeRoutePath(routePath string) (string, error) {
	if routePath == "" {
		return "", errors.New("path is required")
	}
	for _, r := range routePath {
		if r < 0x20 || r == 0x7f {
			return "", errors.New("control characters are not allowed")
		}
	}
	if strings.ContainsAny(routePath, "\x00\\?#") {
		return "", errors.New("path must not contain NUL, backslash, query, or fragment syntax")
	}
	for _, segment := range strings.Split(routePath, "/") {
		if segment == ".." {
			return "", errors.New("path traversal is not allowed")
		}
	}

	if !strings.HasPrefix(routePath, "/") {
		routePath = "/" + routePath
	}

	trailingSlash := strings.HasSuffix(routePath, "/") && routePath != "/"
	cleaned := path.Clean(routePath)
	if trailingSlash && cleaned != "/" {
		cleaned += "/"
	}
	return cleaned, nil
}

func validHTTPRouteToken(token string) bool {
	for i := 0; i < len(token); i++ {
		c := token[i]
		switch {
		case c >= 'A' && c <= 'Z':
		case c >= '0' && c <= '9':
		case c == '!' || c == '#' || c == '$' || c == '%' || c == '&' || c == '\'' || c == '*':
		case c == '+' || c == '-' || c == '.' || c == '^' || c == '_' || c == '`' || c == '|' || c == '~':
		default:
			return false
		}
	}
	return true
}

func invalidRouteBinding(format string, args ...any) error {
	return fmt.Errorf("%w: %s", ErrInvalidRouteBinding, fmt.Sprintf(format, args...))
}
