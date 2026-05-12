// Package admin provides generator-neutral metadata helpers for future Lazuli
// admin surfaces.
package admin

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"unicode"
)

const (
	// DefaultResourceGroup is used by GroupResources when Resource.Group is empty.
	DefaultResourceGroup = "Other"

	// DefaultFieldGroup is used by GroupFields when Field.Display.Group is empty.
	DefaultFieldGroup = "Main"

	// DefaultActionGroup is used by GroupActions when Action.Display.Group is empty.
	DefaultActionGroup = "Actions"
)

// SortDirection describes the default sort direction for a sortable field.
type SortDirection string

const (
	// SortAscending marks an ascending default sort.
	SortAscending SortDirection = "asc"

	// SortDescending marks a descending default sort.
	SortDescending SortDirection = "desc"
)

// FilterKind describes the shape of a generated admin filter control.
type FilterKind string

const (
	// FilterExact matches exact values.
	FilterExact FilterKind = "exact"

	// FilterContains matches substrings or searchable text.
	FilterContains FilterKind = "contains"

	// FilterRange matches ordered ranges.
	FilterRange FilterKind = "range"

	// FilterSelect matches one value from an option set.
	FilterSelect FilterKind = "select"
)

// ActionScope describes where an admin action is available.
type ActionScope string

const (
	// ActionScopeRecord applies to a single record.
	ActionScopeRecord ActionScope = "record"

	// ActionScopeCollection applies without selecting records.
	ActionScopeCollection ActionScope = "collection"

	// ActionScopeBulk applies to selected records.
	ActionScopeBulk ActionScope = "bulk"
)

var (
	// ErrInvalidResource reports structurally invalid resource metadata.
	ErrInvalidResource = errors.New("lazuli/admin: invalid resource")

	// ErrDuplicateResource reports duplicate resource names.
	ErrDuplicateResource = errors.New("lazuli/admin: duplicate resource")

	// ErrInvalidField reports structurally invalid field metadata.
	ErrInvalidField = errors.New("lazuli/admin: invalid field")

	// ErrDuplicateField reports duplicate field names within one field set.
	ErrDuplicateField = errors.New("lazuli/admin: duplicate field")

	// ErrInvalidAction reports structurally invalid action metadata.
	ErrInvalidAction = errors.New("lazuli/admin: invalid action")

	// ErrDuplicateAction reports duplicate action names within one action set.
	ErrDuplicateAction = errors.New("lazuli/admin: duplicate action")
)

// Resource describes one admin-manageable model or collection.
type Resource struct {
	// Name is the stable generator identifier, for example "customers".
	Name string

	// Label is optional display text. Empty defaults to Name in normalized copies.
	Label string

	// Group is an optional navigation grouping label.
	Group string

	// Description is optional one-line display text for the resource.
	Description string

	// Order sorts resources within a group. Lower values sort first.
	Order int

	// Fields describe record attributes exposed to the admin surface.
	Fields []Field

	// Actions describe commands exposed for this resource.
	Actions []Action
}

// Field describes one resource attribute for generated admin views and forms.
type Field struct {
	// Name is the stable generator identifier, for example "created_at".
	Name string

	// Label is optional display text. Empty defaults to Name in normalized copies.
	Label string

	// Type is the generator-neutral field type, for example "string" or "datetime".
	Type string

	// Description is optional one-line display text for the field.
	Description string

	// Required marks fields that should be validated as present.
	Required bool

	// ReadOnly marks fields that should not be editable in generated forms.
	ReadOnly bool

	// Sort carries sorting hints for list views.
	Sort SortHint

	// Filter carries filtering hints for list views.
	Filter FilterHint

	// Display carries grouping and placement hints for generated views.
	Display DisplayHint
}

// SortHint carries sorting hints for generated admin list views.
type SortHint struct {
	// Enabled marks the field as sortable.
	Enabled bool

	// Default optionally marks this field as part of the initial sort.
	Default SortDirection

	// Priority sorts multiple default sort fields. Lower values sort first.
	Priority int
}

// FilterHint carries filtering hints for generated admin list views.
type FilterHint struct {
	// Enabled marks the field as filterable.
	Enabled bool

	// Kind optionally selects the filter control shape. Enabled filters default to
	// FilterExact when Kind is empty.
	Kind FilterKind
}

// DisplayHint carries grouping and placement hints for generated admin surfaces.
type DisplayHint struct {
	// Group is an optional section label.
	Group string

	// Order sorts items within a group. Lower values sort first.
	Order int

	// List marks the item as visible in list views.
	List bool

	// Detail marks the item as visible in detail views.
	Detail bool

	// Form marks the item as visible in form views.
	Form bool

	// Hidden marks the item as hidden from generated default surfaces.
	Hidden bool

	// Width is an optional generator-defined sizing hint.
	Width string
}

// Action describes one command exposed by a generated admin surface.
type Action struct {
	// Name is the stable generator identifier, for example "refund".
	Name string

	// Label is optional display text. Empty defaults to Name in normalized copies.
	Label string

	// Description is optional one-line display text for the action.
	Description string

	// Scope describes where the action is available. Empty defaults to
	// ActionScopeRecord in normalized copies.
	Scope ActionScope

	// Display carries grouping and placement hints for generated action controls.
	Display DisplayHint

	// Danger marks actions that should be presented as destructive or sensitive.
	Danger bool

	// Confirm marks actions that should require explicit confirmation.
	Confirm bool
}

// ResourceGroup is a deterministic group of resources.
type ResourceGroup struct {
	Label     string
	Resources []Resource
}

// FieldGroup is a deterministic group of fields.
type FieldGroup struct {
	Label  string
	Fields []Field
}

// ActionGroup is a deterministic group of actions.
type ActionGroup struct {
	Label   string
	Actions []Action
}

// ValidateResources checks resource metadata without mutating the input slice.
func ValidateResources(resources []Resource) error {
	_, err := normalizeResources(resources)
	return err
}

// ValidateFields checks field metadata without mutating the input slice.
func ValidateFields(fields []Field) error {
	_, err := normalizeFields(fields, -1)
	return err
}

// ValidateActions checks action metadata without mutating the input slice.
func ValidateActions(actions []Action) error {
	_, err := normalizeActions(actions, -1)
	return err
}

// SortedResources returns a validated, normalized, deterministically sorted copy.
//
// Resources are sorted by group, order, label, then name. Fields and actions on
// each resource are sorted by display group, order, label, then name.
func SortedResources(resources []Resource) ([]Resource, error) {
	normalized, err := normalizeResources(resources)
	if err != nil {
		return nil, err
	}
	sortResources(normalized)
	return normalized, nil
}

// SortedFields returns a validated, normalized, deterministically sorted copy.
func SortedFields(fields []Field) ([]Field, error) {
	normalized, err := normalizeFields(fields, -1)
	if err != nil {
		return nil, err
	}
	sortFields(normalized)
	return normalized, nil
}

// SortedActions returns a validated, normalized, deterministically sorted copy.
func SortedActions(actions []Action) ([]Action, error) {
	normalized, err := normalizeActions(actions, -1)
	if err != nil {
		return nil, err
	}
	sortActions(normalized)
	return normalized, nil
}

// GroupResources returns validated resources grouped by Resource.Group.
//
// Empty groups are labeled DefaultResourceGroup. Groups and resources inside
// each group are sorted deterministically.
func GroupResources(resources []Resource) ([]ResourceGroup, error) {
	normalized, err := SortedResources(resources)
	if err != nil {
		return nil, err
	}
	return groupResources(normalized), nil
}

// GroupFields returns validated fields grouped by Field.Display.Group.
//
// Empty groups are labeled DefaultFieldGroup. Groups and fields inside each
// group are sorted deterministically.
func GroupFields(fields []Field) ([]FieldGroup, error) {
	normalized, err := SortedFields(fields)
	if err != nil {
		return nil, err
	}
	return groupFields(normalized), nil
}

// GroupActions returns validated actions grouped by Action.Display.Group.
//
// Empty groups are labeled DefaultActionGroup. Groups and actions inside each
// group are sorted deterministically.
func GroupActions(actions []Action) ([]ActionGroup, error) {
	normalized, err := SortedActions(actions)
	if err != nil {
		return nil, err
	}
	return groupActions(normalized), nil
}

func normalizeResources(resources []Resource) ([]Resource, error) {
	normalized := make([]Resource, 0, len(resources))
	seen := make(map[string]int, len(resources))

	var errs []error
	for i, resource := range resources {
		clean, err := normalizeResource(resource, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		key := metadataKey(clean.Name)
		if first, ok := seen[key]; ok {
			errs = append(errs, fmt.Errorf("%w: resource[%d] %q also appears at resource[%d]", ErrDuplicateResource, i, clean.Name, first))
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

func normalizeResource(resource Resource, index int) (Resource, error) {
	clean := Resource{
		Name:        strings.TrimSpace(resource.Name),
		Label:       strings.TrimSpace(resource.Label),
		Group:       strings.TrimSpace(resource.Group),
		Description: strings.TrimSpace(resource.Description),
		Order:       resource.Order,
	}
	if clean.Label == "" {
		clean.Label = clean.Name
	}

	var errs []error
	if clean.Name == "" {
		errs = append(errs, invalidResourceField(index, "name", "is required"))
	} else if hasControl(clean.Name) {
		errs = append(errs, invalidResourceField(index, "name", "contains control characters"))
	}
	if hasControl(clean.Label) {
		errs = append(errs, invalidResourceField(index, "label", "contains control characters"))
	}
	if hasControl(clean.Group) {
		errs = append(errs, invalidResourceField(index, "group", "contains control characters"))
	}
	if hasControl(clean.Description) {
		errs = append(errs, invalidResourceField(index, "description", "contains control characters"))
	}
	if clean.Order < 0 {
		errs = append(errs, invalidResourceField(index, "order", "must be non-negative"))
	}

	fields, err := normalizeFields(resource.Fields, index)
	if err != nil {
		errs = append(errs, err)
	}
	clean.Fields = fields

	actions, err := normalizeActions(resource.Actions, index)
	if err != nil {
		errs = append(errs, err)
	}
	clean.Actions = actions

	if err := errors.Join(errs...); err != nil {
		return Resource{}, err
	}
	return clean, nil
}

func normalizeFields(fields []Field, resourceIndex int) ([]Field, error) {
	normalized := make([]Field, 0, len(fields))
	seen := make(map[string]int, len(fields))

	var errs []error
	for i, field := range fields {
		clean, err := normalizeField(field, resourceIndex, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		key := metadataKey(clean.Name)
		if first, ok := seen[key]; ok {
			errs = append(errs, fmt.Errorf("%w: %s %q also appears at %s", ErrDuplicateField, fieldPath(resourceIndex, i), clean.Name, fieldPath(resourceIndex, first)))
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

func normalizeField(field Field, resourceIndex, fieldIndex int) (Field, error) {
	clean := Field{
		Name:        strings.TrimSpace(field.Name),
		Label:       strings.TrimSpace(field.Label),
		Type:        strings.TrimSpace(field.Type),
		Description: strings.TrimSpace(field.Description),
		Required:    field.Required,
		ReadOnly:    field.ReadOnly,
		Sort:        normalizeSortHint(field.Sort),
		Filter:      normalizeFilterHint(field.Filter),
		Display:     normalizeDisplayHint(field.Display),
	}
	if clean.Label == "" {
		clean.Label = clean.Name
	}

	var errs []error
	if clean.Name == "" {
		errs = append(errs, invalidFieldField(resourceIndex, fieldIndex, "name", "is required"))
	} else if hasControl(clean.Name) {
		errs = append(errs, invalidFieldField(resourceIndex, fieldIndex, "name", "contains control characters"))
	}
	if clean.Type == "" {
		errs = append(errs, invalidFieldField(resourceIndex, fieldIndex, "type", "is required"))
	} else if hasControl(clean.Type) {
		errs = append(errs, invalidFieldField(resourceIndex, fieldIndex, "type", "contains control characters"))
	}
	if hasControl(clean.Label) {
		errs = append(errs, invalidFieldField(resourceIndex, fieldIndex, "label", "contains control characters"))
	}
	if hasControl(clean.Description) {
		errs = append(errs, invalidFieldField(resourceIndex, fieldIndex, "description", "contains control characters"))
	}
	if err := validateSortHint(clean.Sort); err != nil {
		errs = append(errs, invalidFieldField(resourceIndex, fieldIndex, "sort", err.Error()))
	}
	if err := validateFilterHint(clean.Filter); err != nil {
		errs = append(errs, invalidFieldField(resourceIndex, fieldIndex, "filter", err.Error()))
	}
	if err := validateDisplayHint(clean.Display); err != nil {
		errs = append(errs, invalidFieldField(resourceIndex, fieldIndex, "display", err.Error()))
	}

	if err := errors.Join(errs...); err != nil {
		return Field{}, err
	}
	return clean, nil
}

func normalizeActions(actions []Action, resourceIndex int) ([]Action, error) {
	normalized := make([]Action, 0, len(actions))
	seen := make(map[string]int, len(actions))

	var errs []error
	for i, action := range actions {
		clean, err := normalizeAction(action, resourceIndex, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		key := metadataKey(clean.Name)
		if first, ok := seen[key]; ok {
			errs = append(errs, fmt.Errorf("%w: %s %q also appears at %s", ErrDuplicateAction, actionPath(resourceIndex, i), clean.Name, actionPath(resourceIndex, first)))
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

func normalizeAction(action Action, resourceIndex, actionIndex int) (Action, error) {
	clean := Action{
		Name:        strings.TrimSpace(action.Name),
		Label:       strings.TrimSpace(action.Label),
		Description: strings.TrimSpace(action.Description),
		Scope:       action.Scope,
		Display:     normalizeDisplayHint(action.Display),
		Danger:      action.Danger,
		Confirm:     action.Confirm,
	}
	if clean.Label == "" {
		clean.Label = clean.Name
	}
	if clean.Scope == "" {
		clean.Scope = ActionScopeRecord
	}

	var errs []error
	if clean.Name == "" {
		errs = append(errs, invalidActionField(resourceIndex, actionIndex, "name", "is required"))
	} else if hasControl(clean.Name) {
		errs = append(errs, invalidActionField(resourceIndex, actionIndex, "name", "contains control characters"))
	}
	if hasControl(clean.Label) {
		errs = append(errs, invalidActionField(resourceIndex, actionIndex, "label", "contains control characters"))
	}
	if hasControl(clean.Description) {
		errs = append(errs, invalidActionField(resourceIndex, actionIndex, "description", "contains control characters"))
	}
	if err := validateActionScope(clean.Scope); err != nil {
		errs = append(errs, invalidActionField(resourceIndex, actionIndex, "scope", err.Error()))
	}
	if err := validateDisplayHint(clean.Display); err != nil {
		errs = append(errs, invalidActionField(resourceIndex, actionIndex, "display", err.Error()))
	}

	if err := errors.Join(errs...); err != nil {
		return Action{}, err
	}
	return clean, nil
}

func normalizeSortHint(hint SortHint) SortHint {
	if hint.Default != "" {
		hint.Enabled = true
	}
	return hint
}

func validateSortHint(hint SortHint) error {
	var errs []error
	switch hint.Default {
	case "", SortAscending, SortDescending:
	default:
		errs = append(errs, fmt.Errorf("default must be %q or %q, got %q", SortAscending, SortDescending, hint.Default))
	}
	if hint.Priority < 0 {
		errs = append(errs, errors.New("priority must be non-negative"))
	}
	return errors.Join(errs...)
}

func normalizeFilterHint(hint FilterHint) FilterHint {
	if hint.Kind != "" {
		hint.Enabled = true
	}
	if hint.Enabled && hint.Kind == "" {
		hint.Kind = FilterExact
	}
	return hint
}

func validateFilterHint(hint FilterHint) error {
	switch hint.Kind {
	case "", FilterExact, FilterContains, FilterRange, FilterSelect:
		return nil
	default:
		return fmt.Errorf("kind must be a known filter kind, got %q", hint.Kind)
	}
}

func normalizeDisplayHint(hint DisplayHint) DisplayHint {
	hint.Group = strings.TrimSpace(hint.Group)
	hint.Width = strings.TrimSpace(hint.Width)
	return hint
}

func validateDisplayHint(hint DisplayHint) error {
	var errs []error
	if hasControl(hint.Group) {
		errs = append(errs, errors.New("group contains control characters"))
	}
	if hint.Order < 0 {
		errs = append(errs, errors.New("order must be non-negative"))
	}
	if hasControl(hint.Width) {
		errs = append(errs, errors.New("width contains control characters"))
	}
	return errors.Join(errs...)
}

func validateActionScope(scope ActionScope) error {
	switch scope {
	case ActionScopeRecord, ActionScopeCollection, ActionScopeBulk:
		return nil
	default:
		return fmt.Errorf("must be a known action scope, got %q", scope)
	}
}

func sortResources(resources []Resource) {
	for i := range resources {
		sortFields(resources[i].Fields)
		sortActions(resources[i].Actions)
	}
	sort.SliceStable(resources, func(i, j int) bool {
		return compareResource(resources[i], resources[j]) < 0
	})
}

func sortFields(fields []Field) {
	sort.SliceStable(fields, func(i, j int) bool {
		return compareField(fields[i], fields[j]) < 0
	})
}

func sortActions(actions []Action) {
	sort.SliceStable(actions, func(i, j int) bool {
		return compareAction(actions[i], actions[j]) < 0
	})
}

func compareResource(left, right Resource) int {
	for _, cmp := range []int{
		compareFold(resourceGroupLabel(left.Group), resourceGroupLabel(right.Group)),
		compareInt(left.Order, right.Order),
		compareDisplayName(left.Label, left.Name, right.Label, right.Name),
		compareFold(left.Name, right.Name),
	} {
		if cmp != 0 {
			return cmp
		}
	}
	return 0
}

func compareField(left, right Field) int {
	for _, cmp := range []int{
		compareFold(fieldGroupLabel(left.Display.Group), fieldGroupLabel(right.Display.Group)),
		compareInt(left.Display.Order, right.Display.Order),
		compareDisplayName(left.Label, left.Name, right.Label, right.Name),
		compareFold(left.Name, right.Name),
	} {
		if cmp != 0 {
			return cmp
		}
	}
	return 0
}

func compareAction(left, right Action) int {
	for _, cmp := range []int{
		compareFold(actionGroupLabel(left.Display.Group), actionGroupLabel(right.Display.Group)),
		compareInt(left.Display.Order, right.Display.Order),
		compareDisplayName(left.Label, left.Name, right.Label, right.Name),
		compareFold(left.Name, right.Name),
	} {
		if cmp != 0 {
			return cmp
		}
	}
	return 0
}

func groupResources(resources []Resource) []ResourceGroup {
	byLabel := make(map[string][]Resource)
	for _, resource := range resources {
		label := resourceGroupLabel(resource.Group)
		byLabel[label] = append(byLabel[label], resource)
	}

	labels := sortedLabels(byLabel)
	groups := make([]ResourceGroup, 0, len(labels))
	for _, label := range labels {
		resources := append([]Resource(nil), byLabel[label]...)
		sortResources(resources)
		groups = append(groups, ResourceGroup{Label: label, Resources: resources})
	}
	return groups
}

func groupFields(fields []Field) []FieldGroup {
	byLabel := make(map[string][]Field)
	for _, field := range fields {
		label := fieldGroupLabel(field.Display.Group)
		byLabel[label] = append(byLabel[label], field)
	}

	labels := sortedLabels(byLabel)
	groups := make([]FieldGroup, 0, len(labels))
	for _, label := range labels {
		fields := append([]Field(nil), byLabel[label]...)
		sortFields(fields)
		groups = append(groups, FieldGroup{Label: label, Fields: fields})
	}
	return groups
}

func groupActions(actions []Action) []ActionGroup {
	byLabel := make(map[string][]Action)
	for _, action := range actions {
		label := actionGroupLabel(action.Display.Group)
		byLabel[label] = append(byLabel[label], action)
	}

	labels := sortedLabels(byLabel)
	groups := make([]ActionGroup, 0, len(labels))
	for _, label := range labels {
		actions := append([]Action(nil), byLabel[label]...)
		sortActions(actions)
		groups = append(groups, ActionGroup{Label: label, Actions: actions})
	}
	return groups
}

func sortedLabels[T any](groups map[string][]T) []string {
	labels := make([]string, 0, len(groups))
	for label := range groups {
		labels = append(labels, label)
	}
	sortStringsStable(labels)
	return labels
}

func resourceGroupLabel(group string) string {
	if group == "" {
		return DefaultResourceGroup
	}
	return group
}

func fieldGroupLabel(group string) string {
	if group == "" {
		return DefaultFieldGroup
	}
	return group
}

func actionGroupLabel(group string) string {
	if group == "" {
		return DefaultActionGroup
	}
	return group
}

func invalidResourceField(index int, field, reason string) error {
	return fmt.Errorf("%w: resource[%d].%s %s", ErrInvalidResource, index, field, reason)
}

func invalidFieldField(resourceIndex, fieldIndex int, field, reason string) error {
	return fmt.Errorf("%w: %s.%s %s", ErrInvalidField, fieldPath(resourceIndex, fieldIndex), field, reason)
}

func invalidActionField(resourceIndex, actionIndex int, field, reason string) error {
	return fmt.Errorf("%w: %s.%s %s", ErrInvalidAction, actionPath(resourceIndex, actionIndex), field, reason)
}

func fieldPath(resourceIndex, fieldIndex int) string {
	if resourceIndex >= 0 {
		return fmt.Sprintf("resource[%d].fields[%d]", resourceIndex, fieldIndex)
	}
	return fmt.Sprintf("field[%d]", fieldIndex)
}

func actionPath(resourceIndex, actionIndex int) string {
	if resourceIndex >= 0 {
		return fmt.Sprintf("resource[%d].actions[%d]", resourceIndex, actionIndex)
	}
	return fmt.Sprintf("action[%d]", actionIndex)
}

func compareDisplayName(leftLabel, leftName, rightLabel, rightName string) int {
	leftDisplay := leftLabel
	if leftDisplay == "" {
		leftDisplay = leftName
	}
	rightDisplay := rightLabel
	if rightDisplay == "" {
		rightDisplay = rightName
	}
	return compareFold(leftDisplay, rightDisplay)
}

func compareInt(left, right int) int {
	switch {
	case left < right:
		return -1
	case left > right:
		return 1
	default:
		return 0
	}
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

func metadataKey(value string) string {
	return strings.ToLower(value)
}

func hasControl(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}
