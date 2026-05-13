// Package views provides generator-neutral metadata helpers for template
// layouts and partials.
package views

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"unicode"
)

var (
	// ErrInvalidLayoutManifest is returned when layout or partial manifest
	// metadata is structurally invalid.
	ErrInvalidLayoutManifest = errors.New("lazuli/views: invalid layout manifest")

	// ErrDuplicateLayout reports duplicate layout names.
	ErrDuplicateLayout = errors.New("lazuli/views: duplicate layout")

	// ErrDuplicateLayoutSlot reports duplicate slot names within one layout.
	ErrDuplicateLayoutSlot = errors.New("lazuli/views: duplicate layout slot")

	// ErrDuplicatePartial reports duplicate partial names.
	ErrDuplicatePartial = errors.New("lazuli/views: duplicate partial")

	// ErrDuplicatePartialDependency reports duplicate dependency names within
	// one partial.
	ErrDuplicatePartialDependency = errors.New("lazuli/views: duplicate partial dependency")

	// ErrUnknownPartialDependency reports a partial dependency that is not
	// present in the manifest partial list.
	ErrUnknownPartialDependency = errors.New("lazuli/views: unknown partial dependency")

	// ErrPartialDependencyCycle reports a cycle in the partial dependency graph.
	ErrPartialDependencyCycle = errors.New("lazuli/views: partial dependency cycle")
)

// Layout describes one template layout and its named slots.
type Layout struct {
	// Name is the stable layout identifier, for example "app" or
	// "admin.shell".
	Name string `json:"name"`

	// Slots are named insertion points exposed by the layout.
	Slots []string `json:"slots,omitempty"`
}

// Partial describes one reusable template partial and other partials it needs.
type Partial struct {
	// Name is the stable partial identifier, for example "nav.primary".
	Name string `json:"name"`

	// Dependencies are partial names that must be available before this partial.
	Dependencies []string `json:"dependencies,omitempty"`
}

// LayoutManifest is a deterministic list of template layouts and partials.
//
// Use NewLayoutManifest to normalize, validate, and order manifest metadata.
type LayoutManifest struct {
	Layouts  []Layout  `json:"layouts,omitempty"`
	Partials []Partial `json:"partials,omitempty"`
}

// NewLayoutManifest returns a validated manifest with layouts sorted by name
// and partials ordered topologically with dependencies before dependents. The
// input slices and nested string slices are copied.
func NewLayoutManifest(layouts []Layout, partials []Partial) (LayoutManifest, error) {
	normalizedLayouts, layoutErr := normalizeLayouts(layouts)
	orderedPartials, partialErr := TopologicalPartials(partials)
	if err := errors.Join(layoutErr, partialErr); err != nil {
		return LayoutManifest{}, err
	}

	sortLayouts(normalizedLayouts)
	return LayoutManifest{
		Layouts:  normalizedLayouts,
		Partials: orderedPartials,
	}, nil
}

// Validate checks that the manifest can be normalized without changing this
// manifest value.
func (m LayoutManifest) Validate() error {
	_, err := NewLayoutManifest(m.Layouts, m.Partials)
	return err
}

// LayoutNames returns layout names in manifest order.
func (m LayoutManifest) LayoutNames() []string {
	names := make([]string, 0, len(m.Layouts))
	for _, layout := range m.Layouts {
		names = append(names, layout.Name)
	}
	return names
}

// Slots returns a copy of the slots for layoutName after normalizing the lookup
// name with the same rules used by NewLayoutManifest.
func (m LayoutManifest) Slots(layoutName string) ([]string, bool) {
	name, err := normalizeManifestName(layoutName, "layout name")
	if err != nil {
		return nil, false
	}

	for _, layout := range m.Layouts {
		clean, err := normalizeLayout(layout, -1)
		if err != nil {
			continue
		}
		if clean.Name == name {
			return append([]string(nil), clean.Slots...), true
		}
	}
	return nil, false
}

// PartialDependencies returns a copy of the dependencies for partialName after
// normalizing the lookup name with the same rules used by NewLayoutManifest.
func (m LayoutManifest) PartialDependencies(partialName string) ([]string, bool) {
	name, err := normalizeManifestName(partialName, "partial name")
	if err != nil {
		return nil, false
	}

	for _, partial := range m.Partials {
		clean, err := normalizePartial(partial, -1)
		if err != nil {
			continue
		}
		if clean.Name == name {
			return append([]string(nil), clean.Dependencies...), true
		}
	}
	return nil, false
}

// PartialOrder returns partial names in topological order, with dependencies
// before dependents.
func (m LayoutManifest) PartialOrder() ([]string, error) {
	partials, err := m.OrderedPartials()
	if err != nil {
		return nil, err
	}

	names := make([]string, 0, len(partials))
	for _, partial := range partials {
		names = append(names, partial.Name)
	}
	return names, nil
}

// OrderedPartials returns a topologically ordered copy of the manifest
// partials, with dependencies before dependents.
func (m LayoutManifest) OrderedPartials() ([]Partial, error) {
	return TopologicalPartials(m.Partials)
}

// TopologicalPartials returns a validated, normalized, topologically ordered
// copy of partials. Dependencies appear before dependents, and independent
// partials are ordered deterministically by name.
func TopologicalPartials(partials []Partial) ([]Partial, error) {
	normalized, err := normalizePartials(partials)
	if err != nil {
		return nil, err
	}
	if err := validatePartialDependencies(normalized); err != nil {
		return nil, err
	}
	return orderPartials(normalized)
}

func normalizeLayouts(layouts []Layout) ([]Layout, error) {
	normalized := make([]Layout, 0, len(layouts))
	seen := make(map[string]int, len(layouts))

	var errs []error
	for i, layout := range layouts {
		clean, err := normalizeLayout(layout, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		if first, ok := seen[clean.Name]; ok {
			errs = append(errs, invalidLayoutManifest(ErrDuplicateLayout, "layout[%d] %q also appears at layout[%d]", i, clean.Name, first))
			continue
		}
		seen[clean.Name] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	return normalized, nil
}

func normalizeLayout(layout Layout, index int) (Layout, error) {
	name, err := normalizeManifestName(layout.Name, "layout name")
	if err != nil {
		return Layout{}, invalidLayoutManifest(nil, "%s.name %v", layoutPath(index), err)
	}

	slots, err := normalizeLayoutSlots(layout.Slots, index)
	if err != nil {
		return Layout{}, err
	}
	return Layout{
		Name:  name,
		Slots: slots,
	}, nil
}

func normalizeLayoutSlots(slots []string, layoutIndex int) ([]string, error) {
	normalized := make([]string, 0, len(slots))
	seen := make(map[string]int, len(slots))

	var errs []error
	for i, slot := range slots {
		clean, err := normalizeManifestName(slot, "slot name")
		if err != nil {
			errs = append(errs, invalidLayoutManifest(nil, "%s.slots[%d] %v", layoutPath(layoutIndex), i, err))
			continue
		}
		if first, ok := seen[clean]; ok {
			errs = append(errs, invalidLayoutManifest(ErrDuplicateLayoutSlot, "%s.slots[%d] %q also appears at %s.slots[%d]", layoutPath(layoutIndex), i, clean, layoutPath(layoutIndex), first))
			continue
		}
		seen[clean] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	return normalized, nil
}

func normalizePartials(partials []Partial) ([]Partial, error) {
	normalized := make([]Partial, 0, len(partials))
	seen := make(map[string]int, len(partials))

	var errs []error
	for i, partial := range partials {
		clean, err := normalizePartial(partial, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		if first, ok := seen[clean.Name]; ok {
			errs = append(errs, invalidLayoutManifest(ErrDuplicatePartial, "partial[%d] %q also appears at partial[%d]", i, clean.Name, first))
			continue
		}
		seen[clean.Name] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	return normalized, nil
}

func normalizePartial(partial Partial, index int) (Partial, error) {
	name, err := normalizeManifestName(partial.Name, "partial name")
	if err != nil {
		return Partial{}, invalidLayoutManifest(nil, "%s.name %v", partialPath(index), err)
	}

	dependencies, err := normalizePartialDependencies(partial.Dependencies, index)
	if err != nil {
		return Partial{}, err
	}
	sortNames(dependencies)
	return Partial{
		Name:         name,
		Dependencies: dependencies,
	}, nil
}

func normalizePartialDependencies(dependencies []string, partialIndex int) ([]string, error) {
	normalized := make([]string, 0, len(dependencies))
	seen := make(map[string]int, len(dependencies))

	var errs []error
	for i, dependency := range dependencies {
		clean, err := normalizeManifestName(dependency, "dependency name")
		if err != nil {
			errs = append(errs, invalidLayoutManifest(nil, "%s.dependencies[%d] %v", partialPath(partialIndex), i, err))
			continue
		}
		if first, ok := seen[clean]; ok {
			errs = append(errs, invalidLayoutManifest(ErrDuplicatePartialDependency, "%s.dependencies[%d] %q also appears at %s.dependencies[%d]", partialPath(partialIndex), i, clean, partialPath(partialIndex), first))
			continue
		}
		seen[clean] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	return normalized, nil
}

func validatePartialDependencies(partials []Partial) error {
	names := make(map[string]struct{}, len(partials))
	for _, partial := range partials {
		names[partial.Name] = struct{}{}
	}

	var errs []error
	for i, partial := range partials {
		for _, dependency := range partial.Dependencies {
			if _, ok := names[dependency]; !ok {
				errs = append(errs, invalidLayoutManifest(ErrUnknownPartialDependency, "partial[%d] %q depends on unknown partial %q", i, partial.Name, dependency))
			}
		}
	}
	return errors.Join(errs...)
}

func orderPartials(partials []Partial) ([]Partial, error) {
	byName := make(map[string]Partial, len(partials))
	names := make([]string, 0, len(partials))
	for _, partial := range partials {
		byName[partial.Name] = clonePartial(partial)
		names = append(names, partial.Name)
	}
	sortNames(names)

	state := make(map[string]int, len(partials))
	ordered := make([]Partial, 0, len(partials))
	var stack []string

	var visit func(string) error
	visit = func(name string) error {
		switch state[name] {
		case 1:
			return invalidLayoutManifest(ErrPartialDependencyCycle, "%s", formatPartialCycle(stack, name))
		case 2:
			return nil
		}

		state[name] = 1
		stack = append(stack, name)
		for _, dependency := range byName[name].Dependencies {
			if err := visit(dependency); err != nil {
				return err
			}
		}
		stack = stack[:len(stack)-1]
		state[name] = 2
		ordered = append(ordered, clonePartial(byName[name]))
		return nil
	}

	for _, name := range names {
		if err := visit(name); err != nil {
			return nil, err
		}
	}
	return ordered, nil
}

func normalizeManifestName(name, field string) (string, error) {
	name = strings.TrimSpace(name)
	if name == "" {
		return "", fmt.Errorf("%s is required", field)
	}
	for _, r := range name {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return "", fmt.Errorf("%s %q must not contain whitespace or control characters", field, name)
		}
	}
	return name, nil
}

func sortLayouts(layouts []Layout) {
	sort.SliceStable(layouts, func(i, j int) bool {
		return compareName(layouts[i].Name, layouts[j].Name) < 0
	})
}

func sortNames(names []string) {
	sort.SliceStable(names, func(i, j int) bool {
		return compareName(names[i], names[j]) < 0
	})
}

func compareName(left, right string) int {
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

func layoutPath(index int) string {
	if index >= 0 {
		return fmt.Sprintf("layout[%d]", index)
	}
	return "layout"
}

func partialPath(index int) string {
	if index >= 0 {
		return fmt.Sprintf("partial[%d]", index)
	}
	return "partial"
}

func clonePartial(partial Partial) Partial {
	partial.Dependencies = append([]string(nil), partial.Dependencies...)
	return partial
}

func formatPartialCycle(stack []string, repeated string) string {
	start := 0
	for i, name := range stack {
		if name == repeated {
			start = i
			break
		}
	}
	cycle := append(append([]string(nil), stack[start:]...), repeated)
	return strings.Join(cycle, " -> ")
}

func invalidLayoutManifest(cause error, format string, args ...any) error {
	message := fmt.Sprintf(format, args...)
	if cause == nil {
		return fmt.Errorf("%w: %s", ErrInvalidLayoutManifest, message)
	}
	return fmt.Errorf("%w: %w: %s", ErrInvalidLayoutManifest, cause, message)
}
