package admin

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"unicode"
)

// InputSchemaRef names an existing generated input schema used to render an
// admin action form.
type InputSchemaRef string

// String returns the raw schema reference.
func (r InputSchemaRef) String() string {
	return string(r)
}

// AdminAction describes a generator-neutral admin action definition.
//
// AdminAction is a richer helper shape for generated admin chrome readiness.
// It can be converted back to the existing Action metadata shape with Action.
type AdminAction struct {
	// Name is the stable generator identifier, for example "archive".
	Name string

	// Label is optional display text. Empty defaults to Name in normalized copies.
	Label string

	// Description is optional one-line display text for the action.
	Description string

	// Scope describes whether the action targets one record, selected records,
	// or the collection itself.
	Scope ActionScope

	// Display carries grouping and placement hints for generated action controls.
	Display DisplayHint

	// Confirm marks actions that should require explicit confirmation.
	Confirm bool

	// Destructive marks actions that should be presented as destructive.
	Destructive bool

	// InputSchemaRef optionally points at a generated input schema for action
	// form fields beyond the selected record or records.
	InputSchemaRef InputSchemaRef
}

// ActionDefinition is an alias kept for call sites that prefer definition
// terminology over chrome terminology.
type ActionDefinition = AdminAction

// AdminActionGroup is a deterministic group of admin action definitions.
type AdminActionGroup struct {
	Label   string
	Actions []AdminAction
}

// ActionDefinitionGroup is an alias for grouped action definitions.
type ActionDefinitionGroup = AdminActionGroup

// SingleAction returns an action definition for one selected record.
func SingleAction(name string) AdminAction {
	return AdminAction{Name: name, Scope: ActionScopeRecord}
}

// BulkAction returns an action definition for selected records.
func BulkAction(name string) AdminAction {
	return AdminAction{Name: name, Scope: ActionScopeBulk}
}

// CollectionAction returns an action definition that does not require a record
// selection.
func CollectionAction(name string) AdminAction {
	return AdminAction{Name: name, Scope: ActionScopeCollection}
}

// WithLabel returns a copy with display label metadata.
func (action AdminAction) WithLabel(label string) AdminAction {
	action.Label = label
	return action
}

// WithDescription returns a copy with display description metadata.
func (action AdminAction) WithDescription(description string) AdminAction {
	action.Description = description
	return action
}

// WithGroup returns a copy assigned to an action display group.
func (action AdminAction) WithGroup(group string) AdminAction {
	action.Display.Group = group
	return action
}

// WithOrder returns a copy with action ordering metadata inside its group.
func (action AdminAction) WithOrder(order int) AdminAction {
	action.Display.Order = order
	return action
}

// WithConfirmation returns a copy that requires explicit confirmation.
func (action AdminAction) WithConfirmation() AdminAction {
	action.Confirm = true
	return action
}

// AsDestructive returns a copy marked as destructive.
func (action AdminAction) AsDestructive() AdminAction {
	action.Destructive = true
	return action
}

// WithInputSchema returns a copy with an input schema reference.
func (action AdminAction) WithInputSchema(ref string) AdminAction {
	action.InputSchemaRef = InputSchemaRef(ref)
	return action
}

// WithInputSchemaRef returns a copy with an input schema reference.
func (action AdminAction) WithInputSchemaRef(ref InputSchemaRef) AdminAction {
	action.InputSchemaRef = ref
	return action
}

// RequiresConfirmation reports whether generated chrome should require
// confirmation before invoking the action.
func (action AdminAction) RequiresConfirmation() bool {
	return action.Confirm || action.Destructive
}

// Action returns the existing Action metadata shape for this admin action.
func (action AdminAction) Action() Action {
	return Action{
		Name:        action.Name,
		Label:       action.Label,
		Description: action.Description,
		Scope:       action.Scope,
		Display:     action.Display,
		Danger:      action.Destructive,
		Confirm:     action.Confirm,
	}
}

// ValidateAdminActions checks admin action definitions without mutating the
// input slice.
func ValidateAdminActions(actions []AdminAction) error {
	_, err := normalizeAdminActions(actions)
	return err
}

// ValidateActionDefinitions checks action definitions without mutating the
// input slice.
func ValidateActionDefinitions(actions []ActionDefinition) error {
	return ValidateAdminActions(actions)
}

// SortedAdminActions returns a validated, normalized, deterministically sorted
// copy.
func SortedAdminActions(actions []AdminAction) ([]AdminAction, error) {
	normalized, err := normalizeAdminActions(actions)
	if err != nil {
		return nil, err
	}
	sortAdminActions(normalized)
	return normalized, nil
}

// SortedActionDefinitions returns a validated, normalized, deterministically
// sorted copy.
func SortedActionDefinitions(actions []ActionDefinition) ([]ActionDefinition, error) {
	return SortedAdminActions(actions)
}

// GroupAdminActions returns validated admin action definitions grouped by
// AdminAction.Display.Group.
//
// Empty groups are labeled DefaultActionGroup. Groups and actions inside each
// group are sorted deterministically.
func GroupAdminActions(actions []AdminAction) ([]AdminActionGroup, error) {
	normalized, err := SortedAdminActions(actions)
	if err != nil {
		return nil, err
	}
	return groupAdminActions(normalized), nil
}

// GroupActionDefinitions returns validated action definitions grouped by
// ActionDefinition.Display.Group.
func GroupActionDefinitions(actions []ActionDefinition) ([]ActionDefinitionGroup, error) {
	return GroupAdminActions(actions)
}

func normalizeAdminActions(actions []AdminAction) ([]AdminAction, error) {
	normalized := make([]AdminAction, 0, len(actions))
	seen := make(map[string]int, len(actions))

	var errs []error
	for i, action := range actions {
		clean, err := normalizeAdminAction(action, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		key := metadataKey(clean.Name)
		if first, ok := seen[key]; ok {
			errs = append(errs, fmt.Errorf("%w: action[%d] %q also appears at action[%d]", ErrDuplicateAction, i, clean.Name, first))
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

func normalizeAdminAction(action AdminAction, index int) (AdminAction, error) {
	base, baseErr := normalizeAction(action.Action(), -1, index)
	inputSchemaRef := InputSchemaRef(strings.TrimSpace(string(action.InputSchemaRef)))

	var errs []error
	if baseErr != nil {
		errs = append(errs, baseErr)
	}
	if err := validateInputSchemaRef(inputSchemaRef); err != nil {
		errs = append(errs, invalidActionField(-1, index, "input_schema_ref", err.Error()))
	}

	if err := errors.Join(errs...); err != nil {
		return AdminAction{}, err
	}
	return adminActionFromAction(base, inputSchemaRef), nil
}

func adminActionFromAction(action Action, inputSchemaRef InputSchemaRef) AdminAction {
	return AdminAction{
		Name:           action.Name,
		Label:          action.Label,
		Description:    action.Description,
		Scope:          action.Scope,
		Display:        action.Display,
		Confirm:        action.Confirm,
		Destructive:    action.Danger,
		InputSchemaRef: inputSchemaRef,
	}
}

func validateInputSchemaRef(ref InputSchemaRef) error {
	value := string(ref)
	if value == "" {
		return nil
	}

	var errs []error
	if hasControl(value) {
		errs = append(errs, errors.New("contains control characters"))
	}
	if strings.ContainsFunc(value, unicode.IsSpace) {
		errs = append(errs, errors.New("contains whitespace"))
	}
	return errors.Join(errs...)
}

func sortAdminActions(actions []AdminAction) {
	sort.SliceStable(actions, func(i, j int) bool {
		return compareAction(actions[i].Action(), actions[j].Action()) < 0
	})
}

func groupAdminActions(actions []AdminAction) []AdminActionGroup {
	byLabel := make(map[string][]AdminAction)
	for _, action := range actions {
		label := actionGroupLabel(action.Display.Group)
		byLabel[label] = append(byLabel[label], action)
	}

	labels := sortedLabels(byLabel)
	groups := make([]AdminActionGroup, 0, len(labels))
	for _, label := range labels {
		actions := append([]AdminAction(nil), byLabel[label]...)
		sortAdminActions(actions)
		groups = append(groups, AdminActionGroup{Label: label, Actions: actions})
	}
	return groups
}
