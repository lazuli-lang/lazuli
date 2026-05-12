package docgen

import (
	"strconv"
	"strings"
)

const (
	// DefaultChangelogTitle is used by MarkdownChangelog when
	// ChangelogMarkdownOptions.Title is empty.
	DefaultChangelogTitle = "API Changelog"
)

// RouteChangeKind identifies the kind of route metadata change.
type RouteChangeKind string

const (
	// RouteChangeAdded marks a route present only in the new snapshot.
	RouteChangeAdded RouteChangeKind = "added"

	// RouteChangeRemoved marks a route present only in the old snapshot.
	RouteChangeRemoved RouteChangeKind = "removed"

	// RouteChangeChanged marks a route whose method/path is unchanged but whose
	// route metadata changed.
	RouteChangeChanged RouteChangeKind = "changed"
)

// ChangeImpact classifies whether a change should be treated as breaking.
type ChangeImpact string

const (
	// ChangeImpactBreaking marks a removal or stable route name change.
	ChangeImpactBreaking ChangeImpact = "breaking"

	// ChangeImpactNonBreaking marks an additive route or documentation/grouping
	// metadata change.
	ChangeImpactNonBreaking ChangeImpact = "non-breaking"
)

// RouteFieldChange describes one changed field on an otherwise matching route.
type RouteFieldChange struct {
	Field  string
	Before string
	After  string
}

// RouteChange describes one route difference between two snapshots.
//
// For added routes, After is set. For removed routes, Before is set. For
// changed routes, both Before and After are set and Fields lists the changed
// route metadata fields.
type RouteChange struct {
	Kind   RouteChangeKind
	Impact ChangeImpact

	Before Route
	After  Route

	Fields []RouteFieldChange
}

// RouteChangelog is a deterministic comparison between two route snapshots.
type RouteChangelog struct {
	Added   []RouteChange
	Removed []RouteChange
	Changed []RouteChange
}

// ChangelogMarkdownOptions configures Markdown changelog output.
type ChangelogMarkdownOptions struct {
	// Title is rendered as the top-level heading. Empty uses
	// DefaultChangelogTitle.
	Title string
}

// CompareRouteSnapshots compares two route metadata snapshots.
//
// Routes are matched by normalized method and path. Removed routes are breaking.
// Added routes are non-breaking. Changes to the stable route name are breaking;
// changes to feature, tags, or summary metadata are non-breaking.
func CompareRouteSnapshots(before, after []Route) (RouteChangelog, error) {
	oldRoutes, err := SortedRoutes(before)
	if err != nil {
		return RouteChangelog{}, err
	}
	newRoutes, err := SortedRoutes(after)
	if err != nil {
		return RouteChangelog{}, err
	}

	oldByKey := routeSnapshotMap(oldRoutes)
	newByKey := routeSnapshotMap(newRoutes)

	changelog := RouteChangelog{}
	for _, route := range oldRoutes {
		key := routeSnapshotKey(route)
		next, ok := newByKey[key]
		if !ok {
			changelog.Removed = append(changelog.Removed, RouteChange{
				Kind:   RouteChangeRemoved,
				Impact: ChangeImpactBreaking,
				Before: route,
			})
			continue
		}

		fields := compareRouteFields(route, next)
		if len(fields) == 0 {
			continue
		}
		changelog.Changed = append(changelog.Changed, RouteChange{
			Kind:   RouteChangeChanged,
			Impact: routeFieldChangeImpact(fields),
			Before: route,
			After:  next,
			Fields: fields,
		})
	}

	for _, route := range newRoutes {
		if _, ok := oldByKey[routeSnapshotKey(route)]; ok {
			continue
		}
		changelog.Added = append(changelog.Added, RouteChange{
			Kind:   RouteChangeAdded,
			Impact: ChangeImpactNonBreaking,
			After:  route,
		})
	}

	return changelog, nil
}

// HasChanges reports whether the changelog contains any route changes.
func (changelog RouteChangelog) HasChanges() bool {
	return len(changelog.Added) > 0 || len(changelog.Removed) > 0 || len(changelog.Changed) > 0
}

// HasBreakingChanges reports whether any change is classified as breaking.
func (changelog RouteChangelog) HasBreakingChanges() bool {
	for _, change := range changelog.Changes() {
		if change.Impact == ChangeImpactBreaking {
			return true
		}
	}
	return false
}

// Changes returns all route changes in deterministic category order.
func (changelog RouteChangelog) Changes() []RouteChange {
	changes := make([]RouteChange, 0, len(changelog.Added)+len(changelog.Removed)+len(changelog.Changed))
	changes = append(changes, changelog.Added...)
	changes = append(changes, changelog.Removed...)
	changes = append(changes, changelog.Changed...)
	return changes
}

// BreakingChanges returns all changes classified as breaking.
func (changelog RouteChangelog) BreakingChanges() []RouteChange {
	return changelog.changesByImpact(ChangeImpactBreaking)
}

// NonBreakingChanges returns all changes classified as non-breaking.
func (changelog RouteChangelog) NonBreakingChanges() []RouteChange {
	return changelog.changesByImpact(ChangeImpactNonBreaking)
}

// MarkdownChangelog renders a route changelog as deterministic Markdown.
func MarkdownChangelog(changelog RouteChangelog, options ChangelogMarkdownOptions) string {
	title := strings.TrimSpace(options.Title)
	if title == "" {
		title = DefaultChangelogTitle
	}

	breaking := changelog.BreakingChanges()
	nonBreaking := changelog.NonBreakingChanges()

	var b strings.Builder
	b.WriteString("# ")
	b.WriteString(markdownHeadingText(title))
	b.WriteString("\n\n")

	b.WriteString("| Impact | Count |\n")
	b.WriteString("| --- | --- |\n")
	writeChangelogSummaryRow(&b, "Breaking", len(breaking))
	writeChangelogSummaryRow(&b, "Non-breaking", len(nonBreaking))
	b.WriteByte('\n')

	writeChangelogTable(&b, "Breaking Changes", breaking, "No breaking changes.")
	b.WriteByte('\n')
	writeChangelogTable(&b, "Non-Breaking Changes", nonBreaking, "No non-breaking changes.")

	return b.String()
}

func (changelog RouteChangelog) changesByImpact(impact ChangeImpact) []RouteChange {
	changes := changelog.Changes()
	filtered := make([]RouteChange, 0, len(changes))
	for _, change := range changes {
		if change.Impact == impact {
			filtered = append(filtered, change)
		}
	}
	return filtered
}

func routeSnapshotMap(routes []Route) map[string]Route {
	byKey := make(map[string]Route, len(routes))
	for _, route := range routes {
		byKey[routeSnapshotKey(route)] = route
	}
	return byKey
}

func routeSnapshotKey(route Route) string {
	return route.Method + " " + route.Path
}

func compareRouteFields(before, after Route) []RouteFieldChange {
	var fields []RouteFieldChange
	if before.Name != after.Name {
		fields = append(fields, RouteFieldChange{Field: "Name", Before: before.Name, After: after.Name})
	}
	if before.Feature != after.Feature {
		fields = append(fields, RouteFieldChange{Field: "Feature", Before: before.Feature, After: after.Feature})
	}
	if !equalStrings(before.Tags, after.Tags) {
		fields = append(fields, RouteFieldChange{Field: "Tags", Before: strings.Join(before.Tags, ", "), After: strings.Join(after.Tags, ", ")})
	}
	if before.Summary != after.Summary {
		fields = append(fields, RouteFieldChange{Field: "Summary", Before: before.Summary, After: after.Summary})
	}
	return fields
}

func routeFieldChangeImpact(fields []RouteFieldChange) ChangeImpact {
	for _, field := range fields {
		if field.Field == "Name" {
			return ChangeImpactBreaking
		}
	}
	return ChangeImpactNonBreaking
}

func equalStrings(left, right []string) bool {
	if len(left) != len(right) {
		return false
	}
	for i := range left {
		if left[i] != right[i] {
			return false
		}
	}
	return true
}

func writeChangelogSummaryRow(b *strings.Builder, impact string, count int) {
	b.WriteString("| ")
	b.WriteString(markdownCell(impact))
	b.WriteString(" | ")
	b.WriteString(markdownCell(strconv.Itoa(count)))
	b.WriteString(" |\n")
}

func writeChangelogTable(b *strings.Builder, heading string, changes []RouteChange, emptyText string) {
	b.WriteString("## ")
	b.WriteString(markdownHeadingText(heading))
	b.WriteString("\n\n")
	if len(changes) == 0 {
		b.WriteString(emptyText)
		b.WriteByte('\n')
		return
	}

	b.WriteString("| Type | Method | Path | Name | Details |\n")
	b.WriteString("| --- | --- | --- | --- | --- |\n")
	for _, change := range changes {
		route := routeChangeDisplayRoute(change)
		b.WriteString("| ")
		b.WriteString(markdownCell(routeChangeKindLabel(change.Kind)))
		b.WriteString(" | ")
		b.WriteString(markdownCell(route.Method))
		b.WriteString(" | ")
		b.WriteString(markdownCell(route.Path))
		b.WriteString(" | ")
		b.WriteString(markdownCell(route.Name))
		b.WriteString(" | ")
		b.WriteString(markdownCell(routeChangeDetails(change)))
		b.WriteString(" |\n")
	}
}

func routeChangeDisplayRoute(change RouteChange) Route {
	if change.After.Method != "" || change.After.Path != "" || change.After.Name != "" {
		return change.After
	}
	return change.Before
}

func routeChangeKindLabel(kind RouteChangeKind) string {
	switch kind {
	case RouteChangeAdded:
		return "Added"
	case RouteChangeRemoved:
		return "Removed"
	case RouteChangeChanged:
		return "Changed"
	default:
		return string(kind)
	}
}

func routeChangeDetails(change RouteChange) string {
	switch change.Kind {
	case RouteChangeAdded:
		return "Route added"
	case RouteChangeRemoved:
		return "Route removed"
	}

	details := make([]string, 0, len(change.Fields))
	for _, field := range change.Fields {
		details = append(details, field.Field+": "+displayChangeValue(field.Before)+" -> "+displayChangeValue(field.After))
	}
	return strings.Join(details, "<br>")
}

func displayChangeValue(value string) string {
	value = strings.TrimSpace(value)
	if value == "" {
		return "(empty)"
	}
	return value
}
