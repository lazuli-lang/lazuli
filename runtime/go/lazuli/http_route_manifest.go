package lazuli

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"unicode"
)

// ErrInvalidHTTPRouteManifest is returned when a route manifest contains
// invalid route metadata or duplicate routes.
var ErrInvalidHTTPRouteManifest = errors.New("lazuli: invalid http route manifest")

// HTTPRoute describes one HTTP endpoint for inspection and generated server
// wiring. Middleware entries are names, not executable middleware functions.
type HTTPRoute struct {
	Method     string   `json:"method"`
	Path       string   `json:"path"`
	Name       string   `json:"name"`
	Middleware []string `json:"middleware,omitempty"`
}

// HTTPRouteManifest is a deterministic list of HTTP route metadata.
//
// Use NewHTTPRouteManifest to normalize, sort, and validate the route list.
type HTTPRouteManifest struct {
	Routes []HTTPRoute `json:"routes"`
}

// NewHTTPRouteManifest returns a validated manifest with routes sorted by
// method, path, and name. The input slice and middleware slices are copied.
func NewHTTPRouteManifest(routes []HTTPRoute) (HTTPRouteManifest, error) {
	normalized := make([]HTTPRoute, len(routes))
	for i, route := range routes {
		out, err := normalizeHTTPRoute(route)
		if err != nil {
			return HTTPRouteManifest{}, invalidHTTPRouteManifest("routes[%d]: %v", i, err)
		}
		normalized[i] = out
	}

	sort.SliceStable(normalized, func(i, j int) bool {
		return httpRouteLess(normalized[i], normalized[j])
	})
	if err := validateHTTPRouteManifestDuplicates(normalized); err != nil {
		return HTTPRouteManifest{}, err
	}

	return HTTPRouteManifest{Routes: normalized}, nil
}

// Validate checks that the manifest routes can be normalized without changing
// this manifest value.
func (m HTTPRouteManifest) Validate() error {
	_, err := NewHTTPRouteManifest(m.Routes)
	return err
}

// Lookup returns the route matching method and path after applying the same
// normalization used by NewHTTPRouteManifest.
func (m HTTPRouteManifest) Lookup(method, path string) (HTTPRoute, bool) {
	method, err := normalizeHTTPRouteMethod(method)
	if err != nil {
		return HTTPRoute{}, false
	}
	path, err = normalizeHTTPRoutePath(path)
	if err != nil {
		return HTTPRoute{}, false
	}

	for _, route := range m.Routes {
		normalized, err := normalizeHTTPRoute(route)
		if err != nil {
			continue
		}
		if normalized.Method == method && normalized.Path == path {
			return cloneHTTPRoute(normalized), true
		}
	}
	return HTTPRoute{}, false
}

// LookupName returns the route with name after applying the same name
// normalization used by NewHTTPRouteManifest.
func (m HTTPRouteManifest) LookupName(name string) (HTTPRoute, bool) {
	name, err := normalizeHTTPRouteName(name)
	if err != nil {
		return HTTPRoute{}, false
	}
	for _, route := range m.Routes {
		normalized, err := normalizeHTTPRoute(route)
		if err != nil {
			continue
		}
		if normalized.Name == name {
			return cloneHTTPRoute(normalized), true
		}
	}
	return HTTPRoute{}, false
}

func normalizeHTTPRoute(route HTTPRoute) (HTTPRoute, error) {
	method, err := normalizeHTTPRouteMethod(route.Method)
	if err != nil {
		return HTTPRoute{}, err
	}
	path, err := normalizeHTTPRoutePath(route.Path)
	if err != nil {
		return HTTPRoute{}, err
	}
	name, err := normalizeHTTPRouteName(route.Name)
	if err != nil {
		return HTTPRoute{}, err
	}
	middleware, err := normalizeHTTPRouteMiddleware(route.Middleware)
	if err != nil {
		return HTTPRoute{}, err
	}
	return HTTPRoute{
		Method:     method,
		Path:       path,
		Name:       name,
		Middleware: middleware,
	}, nil
}

func normalizeHTTPRouteMethod(method string) (string, error) {
	method = strings.ToUpper(strings.TrimSpace(method))
	if method == "" {
		return "", errors.New("method is required")
	}
	for _, r := range method {
		if !isHTTPRouteMethodRune(r) {
			return "", fmt.Errorf("method %q is not a valid HTTP token", method)
		}
	}
	return method, nil
}

func normalizeHTTPRoutePath(routePath string) (string, error) {
	routePath = strings.TrimSpace(routePath)
	if routePath == "" {
		return "", errors.New("path is required")
	}
	if strings.Contains(routePath, "://") || strings.HasPrefix(routePath, "//") {
		return "", errors.New("path must not be an absolute URL")
	}
	if strings.ContainsAny(routePath, "?#\\") {
		return "", errors.New("path must not contain query strings, fragments, or backslashes")
	}
	for _, r := range routePath {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return "", errors.New("path must not contain whitespace or control characters")
		}
	}
	return normalizeHTTPPathPattern(routePath), nil
}

func normalizeHTTPRouteName(name string) (string, error) {
	name = strings.TrimSpace(name)
	if name == "" {
		return "", errors.New("name is required")
	}
	if hasHTTPRouteUnsafeTextRune(name) {
		return "", fmt.Errorf("name %q must not contain whitespace or control characters", name)
	}
	return name, nil
}

func normalizeHTTPRouteMiddleware(values []string) ([]string, error) {
	if len(values) == 0 {
		return nil, nil
	}
	normalized := make([]string, len(values))
	for i, value := range values {
		value = strings.TrimSpace(value)
		if value == "" {
			return nil, fmt.Errorf("middleware[%d] is required", i)
		}
		if hasHTTPRouteUnsafeTextRune(value) {
			return nil, fmt.Errorf("middleware[%d] %q must not contain whitespace or control characters", i, value)
		}
		normalized[i] = value
	}
	return normalized, nil
}

func validateHTTPRouteManifestDuplicates(routes []HTTPRoute) error {
	seenRoutes := map[string]HTTPRoute{}
	seenNames := map[string]HTTPRoute{}
	for _, route := range routes {
		key := httpRouteKey(route.Method, route.Path)
		if previous, ok := seenRoutes[key]; ok {
			return invalidHTTPRouteManifest("duplicate route %s %s used by %q and %q", route.Method, route.Path, previous.Name, route.Name)
		}
		seenRoutes[key] = route

		if previous, ok := seenNames[route.Name]; ok {
			return invalidHTTPRouteManifest("duplicate route name %q for %s %s and %s %s", route.Name, previous.Method, previous.Path, route.Method, route.Path)
		}
		seenNames[route.Name] = route
	}
	return nil
}

func httpRouteLess(a, b HTTPRoute) bool {
	if a.Method != b.Method {
		return a.Method < b.Method
	}
	if a.Path != b.Path {
		return a.Path < b.Path
	}
	return a.Name < b.Name
}

func httpRouteKey(method, path string) string {
	return method + "\x00" + path
}

func cloneHTTPRoute(route HTTPRoute) HTTPRoute {
	route.Middleware = append([]string(nil), route.Middleware...)
	return route
}

func hasHTTPRouteUnsafeTextRune(value string) bool {
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func isHTTPRouteMethodRune(r rune) bool {
	if r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' {
		return true
	}
	switch r {
	case '!', '#', '$', '%', '&', '\'', '*', '+', '-', '.', '^', '_', '`', '|', '~':
		return true
	default:
		return false
	}
}

func invalidHTTPRouteManifest(format string, args ...any) error {
	return fmt.Errorf("%w: %s", ErrInvalidHTTPRouteManifest, fmt.Sprintf(format, args...))
}
