// Package docgen provides small deterministic API reference generators for
// route metadata emitted by Lazuli codegen.
package docgen

import (
	"errors"
	"fmt"
	"net/http"
	"sort"
	"strings"
	"unicode"
)

const (
	// DefaultTitle is used by Markdown when MarkdownOptions.Title is empty.
	DefaultTitle = "API Reference"

	otherFeatureGroup = "Other"
	untaggedGroup     = "Untagged"
)

// GroupBy controls how Markdown sections are grouped.
type GroupBy string

const (
	// GroupByFeature groups routes by Route.Feature. Empty features are grouped
	// under "Other".
	GroupByFeature GroupBy = "feature"

	// GroupByTag groups routes by Route.Tags. Routes without tags are grouped
	// under "Untagged"; routes with multiple tags appear in each tag section.
	GroupByTag GroupBy = "tag"
)

var (
	// ErrInvalidRoute reports structurally invalid route metadata.
	ErrInvalidRoute = errors.New("lazuli/docgen: invalid route")

	// ErrDuplicateRoute reports duplicate method/path pairs.
	ErrDuplicateRoute = errors.New("lazuli/docgen: duplicate route")

	// ErrInvalidOptions reports unsupported rendering options.
	ErrInvalidOptions = errors.New("lazuli/docgen: invalid options")
)

// Route is the simple, generator-neutral metadata needed for API reference
// output.
type Route struct {
	// Name is the stable route name, for example "customer.get".
	Name string

	// Method is the HTTP method. It is normalized to uppercase.
	Method string

	// Path is the absolute HTTP route path.
	Path string

	// Feature is the owning feature used by GroupByFeature.
	Feature string

	// Tags are optional grouping labels used by GroupByTag.
	Tags []string

	// Summary is optional one-line display text for the route.
	Summary string
}

// MarkdownOptions configures Markdown API reference output.
type MarkdownOptions struct {
	// Title is rendered as the top-level heading. Empty uses DefaultTitle.
	Title string

	// GroupBy selects section grouping. Empty defaults to GroupByFeature.
	GroupBy GroupBy
}

// ValidateRoutes checks route metadata without mutating the input slice.
func ValidateRoutes(routes []Route) error {
	_, err := normalizeRoutes(routes)
	return err
}

// SortedRoutes returns a validated, normalized, stably sorted copy of routes.
//
// Sorting uses path, method, name, then feature. Exact ties keep input order.
func SortedRoutes(routes []Route) ([]Route, error) {
	normalized, err := normalizeRoutes(routes)
	if err != nil {
		return nil, err
	}
	sortRoutes(normalized)
	return normalized, nil
}

// Markdown renders routes as a deterministic Markdown API reference.
func Markdown(routes []Route, options MarkdownOptions) (string, error) {
	groupBy, err := normalizeGroupBy(options.GroupBy)
	if err != nil {
		return "", err
	}

	normalized, err := SortedRoutes(routes)
	if err != nil {
		return "", err
	}

	title := strings.TrimSpace(options.Title)
	if title == "" {
		title = DefaultTitle
	}

	var b strings.Builder
	b.WriteString("# ")
	b.WriteString(markdownHeadingText(title))
	b.WriteString("\n\n")

	if len(normalized) == 0 {
		b.WriteString("No routes.\n")
		return b.String(), nil
	}

	groups := groupRoutes(normalized, groupBy)
	for i, group := range groups {
		if i > 0 {
			b.WriteByte('\n')
		}
		b.WriteString("## ")
		b.WriteString(groupHeadingPrefix(groupBy))
		b.WriteString(markdownHeadingText(group.label))
		b.WriteString("\n\n")
		writeRouteTable(&b, group.routes)
	}

	return b.String(), nil
}

type routeGroup struct {
	label  string
	routes []Route
}

func normalizeRoutes(routes []Route) ([]Route, error) {
	normalized := make([]Route, 0, len(routes))
	seen := make(map[string]int, len(routes))

	var errs []error
	for i, route := range routes {
		clean, err := normalizeRoute(route, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		key := clean.Method + " " + clean.Path
		if first, ok := seen[key]; ok {
			errs = append(errs, fmt.Errorf("%w: route[%d] %s also appears at route[%d]", ErrDuplicateRoute, i, key, first))
			continue
		}
		seen[key] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	return normalized, nil
}

func normalizeRoute(route Route, index int) (Route, error) {
	clean := Route{
		Name:    strings.TrimSpace(route.Name),
		Method:  strings.ToUpper(strings.TrimSpace(route.Method)),
		Path:    strings.TrimSpace(route.Path),
		Feature: strings.TrimSpace(route.Feature),
		Summary: strings.TrimSpace(route.Summary),
	}

	var errs []error
	if clean.Name == "" {
		errs = append(errs, invalidRouteField(index, "name", "is required"))
	} else if hasControl(clean.Name) {
		errs = append(errs, invalidRouteField(index, "name", "contains control characters"))
	}

	if err := validateMethod(clean.Method); err != nil {
		errs = append(errs, invalidRouteField(index, "method", err.Error()))
	}
	if err := validatePath(clean.Path); err != nil {
		errs = append(errs, invalidRouteField(index, "path", err.Error()))
	}
	if clean.Feature != "" && hasControl(clean.Feature) {
		errs = append(errs, invalidRouteField(index, "feature", "contains control characters"))
	}
	if hasControl(clean.Summary) {
		errs = append(errs, invalidRouteField(index, "summary", "contains control characters"))
	}

	tags, err := normalizeTags(route.Tags, index)
	if err != nil {
		errs = append(errs, err)
	}
	clean.Tags = tags

	if err := errors.Join(errs...); err != nil {
		return Route{}, err
	}
	return clean, nil
}

func invalidRouteField(index int, field, reason string) error {
	return fmt.Errorf("%w: route[%d].%s %s", ErrInvalidRoute, index, field, reason)
}

func validateMethod(method string) error {
	switch method {
	case http.MethodGet, http.MethodPost, http.MethodPut, http.MethodPatch,
		http.MethodDelete, http.MethodHead, http.MethodOptions:
		return nil
	case "":
		return errors.New("is required")
	default:
		return fmt.Errorf("must be a supported HTTP method, got %q", method)
	}
}

func validatePath(routePath string) error {
	if routePath == "" {
		return errors.New("is required")
	}
	if !strings.HasPrefix(routePath, "/") {
		return errors.New("must be absolute")
	}
	if strings.ContainsAny(routePath, "\x00\\?#") {
		return errors.New("contains unsafe characters")
	}
	if hasControl(routePath) {
		return errors.New("contains control characters")
	}
	for _, segment := range strings.Split(routePath, "/") {
		if segment == "." || segment == ".." {
			return errors.New("must not contain path traversal segments")
		}
	}
	return nil
}

func normalizeTags(tags []string, routeIndex int) ([]string, error) {
	if len(tags) == 0 {
		return nil, nil
	}

	seen := make(map[string]struct{}, len(tags))
	normalized := make([]string, 0, len(tags))
	var errs []error
	for i, tag := range tags {
		tag = strings.TrimSpace(tag)
		switch {
		case tag == "":
			errs = append(errs, fmt.Errorf("%w: route[%d].tags[%d] is empty", ErrInvalidRoute, routeIndex, i))
		case hasControl(tag):
			errs = append(errs, fmt.Errorf("%w: route[%d].tags[%d] contains control characters", ErrInvalidRoute, routeIndex, i))
		default:
			key := strings.ToLower(tag)
			if _, ok := seen[key]; ok {
				continue
			}
			seen[key] = struct{}{}
			normalized = append(normalized, tag)
		}
	}
	if err := errors.Join(errs...); err != nil {
		return nil, err
	}

	sortStringsStable(normalized)
	return normalized, nil
}

func normalizeGroupBy(groupBy GroupBy) (GroupBy, error) {
	switch groupBy {
	case "", GroupByFeature:
		return GroupByFeature, nil
	case GroupByTag:
		return GroupByTag, nil
	default:
		return "", fmt.Errorf("%w: unsupported group %q", ErrInvalidOptions, groupBy)
	}
}

func sortRoutes(routes []Route) {
	sort.SliceStable(routes, func(i, j int) bool {
		left := routes[i]
		right := routes[j]
		return compareRoute(left, right) < 0
	})
}

func compareRoute(left, right Route) int {
	for _, cmp := range []int{
		compareFold(left.Path, right.Path),
		compareMethod(left.Method, right.Method),
		compareFold(left.Name, right.Name),
		compareFold(left.Feature, right.Feature),
	} {
		if cmp != 0 {
			return cmp
		}
	}
	return 0
}

func compareMethod(left, right string) int {
	leftRank := methodRank(left)
	rightRank := methodRank(right)
	if leftRank != rightRank {
		if leftRank < rightRank {
			return -1
		}
		return 1
	}
	return compareFold(left, right)
}

func methodRank(method string) int {
	switch method {
	case http.MethodGet:
		return 0
	case http.MethodPost:
		return 1
	case http.MethodPut:
		return 2
	case http.MethodPatch:
		return 3
	case http.MethodDelete:
		return 4
	case http.MethodHead:
		return 5
	case http.MethodOptions:
		return 6
	default:
		return 100
	}
}

func groupRoutes(routes []Route, groupBy GroupBy) []routeGroup {
	byLabel := map[string][]Route{}
	for _, route := range routes {
		labels := routeGroupLabels(route, groupBy)
		for _, label := range labels {
			byLabel[label] = append(byLabel[label], route)
		}
	}

	labels := make([]string, 0, len(byLabel))
	for label := range byLabel {
		labels = append(labels, label)
	}
	sortStringsStable(labels)

	groups := make([]routeGroup, 0, len(labels))
	for _, label := range labels {
		routes := append([]Route(nil), byLabel[label]...)
		sortRoutes(routes)
		groups = append(groups, routeGroup{label: label, routes: routes})
	}
	return groups
}

func routeGroupLabels(route Route, groupBy GroupBy) []string {
	switch groupBy {
	case GroupByTag:
		if len(route.Tags) == 0 {
			return []string{untaggedGroup}
		}
		return route.Tags
	default:
		if route.Feature == "" {
			return []string{otherFeatureGroup}
		}
		return []string{route.Feature}
	}
}

func groupHeadingPrefix(groupBy GroupBy) string {
	if groupBy == GroupByTag {
		return "Tag: "
	}
	return "Feature: "
}

func writeRouteTable(b *strings.Builder, routes []Route) {
	b.WriteString("| Method | Path | Name | Feature | Tags | Summary |\n")
	b.WriteString("| --- | --- | --- | --- | --- | --- |\n")
	for _, route := range routes {
		b.WriteString("| ")
		b.WriteString(markdownCell(route.Method))
		b.WriteString(" | ")
		b.WriteString(markdownCell(route.Path))
		b.WriteString(" | ")
		b.WriteString(markdownCell(route.Name))
		b.WriteString(" | ")
		b.WriteString(markdownCell(route.Feature))
		b.WriteString(" | ")
		b.WriteString(markdownCell(strings.Join(route.Tags, ", ")))
		b.WriteString(" | ")
		b.WriteString(markdownCell(route.Summary))
		b.WriteString(" |\n")
	}
}

func markdownCell(value string) string {
	value = strings.TrimSpace(value)
	value = strings.ReplaceAll(value, "\r\n", "\n")
	value = strings.ReplaceAll(value, "\r", "\n")
	value = strings.ReplaceAll(value, "\n", "<br>")
	value = strings.ReplaceAll(value, "|", `\|`)
	return value
}

func markdownHeadingText(value string) string {
	return strings.Join(strings.Fields(value), " ")
}

func sortStringsStable(values []string) {
	sort.SliceStable(values, func(i, j int) bool {
		return compareFold(values[i], values[j]) < 0
	})
}

func compareFold(left, right string) int {
	leftFold := strings.ToLower(left)
	rightFold := strings.ToLower(right)
	switch {
	case leftFold < rightFold:
		return -1
	case leftFold > rightFold:
		return 1
	case left < right:
		return -1
	case left > right:
		return 1
	default:
		return 0
	}
}

func hasControl(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}
