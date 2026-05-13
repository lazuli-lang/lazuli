package admin

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"unicode"
)

var (
	// ErrInvalidResourceSchema reports structurally invalid generated admin
	// resource schema metadata.
	ErrInvalidResourceSchema = errors.New("lazuli/admin: invalid resource schema")

	// ErrInvalidActionReference reports an unsafe or incomplete admin action
	// reference.
	ErrInvalidActionReference = errors.New("lazuli/admin: invalid action reference")
)

// FieldView names one generated admin surface a field can participate in.
type FieldView string

const (
	// FieldViewList marks fields rendered in collection/list views.
	FieldViewList FieldView = "list"

	// FieldViewDetail marks fields rendered in record/detail views.
	FieldViewDetail FieldView = "detail"

	// FieldViewForm marks fields rendered in create/update forms.
	FieldViewForm FieldView = "form"
)

// ResourceSchema is the generated admin-facing schema for one resource.
//
// It is derived from Resource metadata and separates fields by generated view
// while keeping filter metadata and action references in generator-neutral
// shapes.
type ResourceSchema struct {
	Name        string
	Label       string
	Group       string
	Description string

	ListFields   []Field
	DetailFields []Field
	FormFields   []Field
	Filters      []FilterMetadata
	Actions      []ResourceSchemaAction
}

// FilterMetadata describes one generated admin filter control.
type FilterMetadata struct {
	// Name is the stable filter identifier. By default it matches Field.
	Name string

	// Field is the source resource field this filter targets.
	Field string

	Label       string
	Type        string
	Description string
	Kind        FilterKind
	Required    bool
	Display     DisplayHint
}

// ResourceActionRef identifies an action on a resource without carrying an
// executable callback or arbitrary route string.
type ResourceActionRef struct {
	Resource string
	Action   string
}

// ActionReference is an alias for callers that prefer reference terminology.
type ActionReference = ResourceActionRef

// ResourceSchemaAction describes an action exposed by ResourceSchema.
type ResourceSchemaAction struct {
	Ref ResourceActionRef

	Name        string
	Label       string
	Description string
	Scope       ActionScope
	Display     DisplayHint
	Confirm     bool
	Danger      bool

	// InputSchemaRef optionally points at a generated input schema for action
	// form fields.
	InputSchemaRef InputSchemaRef
}

// ResourceField returns a field visible in the requested admin views.
func ResourceField(name, typ string, views ...FieldView) Field {
	return Field{Name: name, Type: typ}.WithViews(views...)
}

// ListField returns a field visible in generated list views.
func ListField(name, typ string) Field {
	return ResourceField(name, typ, FieldViewList)
}

// DetailField returns a field visible in generated detail views.
func DetailField(name, typ string) Field {
	return ResourceField(name, typ, FieldViewDetail)
}

// FormField returns a field visible in generated form views.
func FormField(name, typ string) Field {
	return ResourceField(name, typ, FieldViewForm)
}

// WithViews returns a copy visible in the requested admin views.
func (field Field) WithViews(views ...FieldView) Field {
	for _, view := range views {
		switch view {
		case FieldViewList:
			field.Display.List = true
		case FieldViewDetail:
			field.Display.Detail = true
		case FieldViewForm:
			field.Display.Form = true
		}
	}
	return field
}

// InList returns a copy visible in generated list views.
func (field Field) InList() Field {
	return field.WithViews(FieldViewList)
}

// InDetail returns a copy visible in generated detail views.
func (field Field) InDetail() Field {
	return field.WithViews(FieldViewDetail)
}

// InForm returns a copy visible in generated form views.
func (field Field) InForm() Field {
	return field.WithViews(FieldViewForm)
}

// WithLabel returns a copy with display label metadata.
func (field Field) WithLabel(label string) Field {
	field.Label = label
	return field
}

// WithDescription returns a copy with display description metadata.
func (field Field) WithDescription(description string) Field {
	field.Description = description
	return field
}

// WithGroup returns a copy assigned to a field display group.
func (field Field) WithGroup(group string) Field {
	field.Display.Group = group
	return field
}

// WithOrder returns a copy with field ordering metadata inside its group.
func (field Field) WithOrder(order int) Field {
	field.Display.Order = order
	return field
}

// WithWidth returns a copy with generator-defined sizing metadata.
func (field Field) WithWidth(width string) Field {
	field.Display.Width = width
	return field
}

// AsRequired returns a copy marked required.
func (field Field) AsRequired() Field {
	field.Required = true
	return field
}

// AsReadOnly returns a copy marked read-only for generated forms.
func (field Field) AsReadOnly() Field {
	field.ReadOnly = true
	return field
}

// AsHidden returns a copy hidden from generated default admin surfaces.
func (field Field) AsHidden() Field {
	field.Display.Hidden = true
	return field
}

// WithFilter returns a copy marked filterable with kind.
func (field Field) WithFilter(kind FilterKind) Field {
	field.Filter.Enabled = true
	field.Filter.Kind = kind
	return field
}

// WithSort returns a copy with default sort metadata.
func (field Field) WithSort(direction SortDirection, priority int) Field {
	field.Sort.Enabled = true
	field.Sort.Default = direction
	field.Sort.Priority = priority
	return field
}

// ActionRef returns a typed reference to an action on a resource.
func ActionRef(resource, action string) ResourceActionRef {
	return ResourceActionRef{Resource: resource, Action: action}
}

// String returns the canonical resource.action reference.
func (ref ResourceActionRef) String() string {
	switch {
	case ref.Resource == "":
		return ref.Action
	case ref.Action == "":
		return ref.Resource
	default:
		return ref.Resource + "." + ref.Action
	}
}

// Canonical returns the canonical resource.action reference.
func (ref ResourceActionRef) Canonical() string {
	return ref.String()
}

// ValidateResourceActionRef checks an action reference without mutating it.
func ValidateResourceActionRef(ref ResourceActionRef) error {
	_, err := NormalizeResourceActionRef(ref)
	return err
}

// NormalizeResourceActionRef returns a trimmed, validated action reference.
func NormalizeResourceActionRef(ref ResourceActionRef) (ResourceActionRef, error) {
	clean := ResourceActionRef{
		Resource: strings.TrimSpace(ref.Resource),
		Action:   strings.TrimSpace(ref.Action),
	}

	var errs []error
	errs = append(errs, validateActionRefPart("resource", clean.Resource)...)
	errs = append(errs, validateActionRefPart("action", clean.Action)...)
	if err := errors.Join(errs...); err != nil {
		return ResourceActionRef{}, err
	}
	return clean, nil
}

// RequiresConfirmation reports whether generated chrome should require
// confirmation before invoking the action.
func (action ResourceSchemaAction) RequiresConfirmation() bool {
	return action.Confirm || action.Danger
}

// Action returns the existing Action metadata shape for this schema action.
func (action ResourceSchemaAction) Action() Action {
	return Action{
		Name:        action.Name,
		Label:       action.Label,
		Description: action.Description,
		Scope:       action.Scope,
		Display:     action.Display,
		Danger:      action.Danger,
		Confirm:     action.Confirm,
	}
}

// AdminAction returns the richer AdminAction metadata shape for this schema
// action.
func (action ResourceSchemaAction) AdminAction() AdminAction {
	return AdminAction{
		Name:           action.Name,
		Label:          action.Label,
		Description:    action.Description,
		Scope:          action.Scope,
		Display:        action.Display,
		Confirm:        action.Confirm,
		Destructive:    action.Danger,
		InputSchemaRef: action.InputSchemaRef,
	}
}

// ActionRef returns the schema action reference for name.
func (schema ResourceSchema) ActionRef(name string) (ResourceActionRef, bool) {
	action, ok := schema.Action(name)
	if !ok {
		return ResourceActionRef{}, false
	}
	return action.Ref, true
}

// Action returns the schema action for name.
func (schema ResourceSchema) Action(name string) (ResourceSchemaAction, bool) {
	key := metadataKey(strings.TrimSpace(name))
	for _, action := range schema.Actions {
		if metadataKey(action.Name) == key || metadataKey(action.Ref.Action) == key {
			return action, true
		}
	}
	return ResourceSchemaAction{}, false
}

// BuildResourceSchema returns a validated, normalized schema for resource.
func BuildResourceSchema(resource Resource) (ResourceSchema, error) {
	normalized, err := normalizeResource(resource, 0)
	if err != nil {
		return ResourceSchema{}, err
	}
	sortFields(normalized.Fields)
	sortActions(normalized.Actions)

	return normalizeResourceSchema(resourceSchemaFromResource(
		normalized,
		resourceSchemaActionsFromActions(normalized.Name, normalized.Actions),
	))
}

// BuildResourceSchemaWithActions returns a validated, normalized schema for
// resource using richer admin action definitions instead of Resource.Actions.
func BuildResourceSchemaWithActions(resource Resource, actions []AdminAction) (ResourceSchema, error) {
	resource.Actions = nil

	normalizedResource, resourceErr := normalizeResource(resource, 0)
	normalizedActions, actionsErr := normalizeAdminActions(actions)
	if err := errors.Join(resourceErr, actionsErr); err != nil {
		return ResourceSchema{}, err
	}

	sortFields(normalizedResource.Fields)
	sortAdminActions(normalizedActions)
	return normalizeResourceSchema(resourceSchemaFromResource(
		normalizedResource,
		resourceSchemaActionsFromAdminActions(normalizedResource.Name, normalizedActions),
	))
}

// ValidateResourceSchema checks schema without mutating it.
func ValidateResourceSchema(schema ResourceSchema) error {
	_, err := normalizeResourceSchema(schema)
	return err
}

// ListFields returns validated, normalized list fields.
func ListFields(fields []Field) ([]Field, error) {
	normalized, err := SortedFields(fields)
	if err != nil {
		return nil, err
	}
	return fieldsForView(normalized, FieldViewList), nil
}

// DetailFields returns validated, normalized detail fields.
func DetailFields(fields []Field) ([]Field, error) {
	normalized, err := SortedFields(fields)
	if err != nil {
		return nil, err
	}
	return fieldsForView(normalized, FieldViewDetail), nil
}

// FormFields returns validated, normalized form fields.
func FormFields(fields []Field) ([]Field, error) {
	normalized, err := SortedFields(fields)
	if err != nil {
		return nil, err
	}
	return fieldsForView(normalized, FieldViewForm), nil
}

// FilterMetadataForFields returns validated, normalized filter metadata for
// filterable fields.
func FilterMetadataForFields(fields []Field) ([]FilterMetadata, error) {
	normalized, err := SortedFields(fields)
	if err != nil {
		return nil, err
	}
	return filterMetadataFromFields(normalized), nil
}

// ResourceListFields returns validated, normalized list fields for resource.
func ResourceListFields(resource Resource) ([]Field, error) {
	return ListFields(resource.Fields)
}

// ResourceDetailFields returns validated, normalized detail fields for resource.
func ResourceDetailFields(resource Resource) ([]Field, error) {
	return DetailFields(resource.Fields)
}

// ResourceFormFields returns validated, normalized form fields for resource.
func ResourceFormFields(resource Resource) ([]Field, error) {
	return FormFields(resource.Fields)
}

// ResourceFilterMetadata returns validated, normalized filter metadata for
// resource.
func ResourceFilterMetadata(resource Resource) ([]FilterMetadata, error) {
	return FilterMetadataForFields(resource.Fields)
}

func resourceSchemaFromResource(resource Resource, actions []ResourceSchemaAction) ResourceSchema {
	return ResourceSchema{
		Name:         resource.Name,
		Label:        resource.Label,
		Group:        resource.Group,
		Description:  resource.Description,
		ListFields:   fieldsForView(resource.Fields, FieldViewList),
		DetailFields: fieldsForView(resource.Fields, FieldViewDetail),
		FormFields:   fieldsForView(resource.Fields, FieldViewForm),
		Filters:      filterMetadataFromFields(resource.Fields),
		Actions:      actions,
	}
}

func fieldsForView(fields []Field, view FieldView) []Field {
	out := make([]Field, 0, len(fields))
	for _, field := range fields {
		if includeFieldInView(field, view) {
			out = append(out, field)
		}
	}
	return out
}

func includeFieldInView(field Field, view FieldView) bool {
	if field.Display.Hidden {
		return false
	}
	if hasAnyFieldView(field) {
		return fieldHasView(field, view)
	}

	switch view {
	case FieldViewList:
		return true
	case FieldViewDetail:
		return true
	case FieldViewForm:
		return !field.ReadOnly
	default:
		return false
	}
}

func hasAnyFieldView(field Field) bool {
	return field.Display.List || field.Display.Detail || field.Display.Form
}

func fieldHasView(field Field, view FieldView) bool {
	switch view {
	case FieldViewList:
		return field.Display.List
	case FieldViewDetail:
		return field.Display.Detail
	case FieldViewForm:
		return field.Display.Form
	default:
		return false
	}
}

func filterMetadataFromFields(fields []Field) []FilterMetadata {
	filters := make([]FilterMetadata, 0, len(fields))
	for _, field := range fields {
		if field.Display.Hidden || !field.Filter.Enabled {
			continue
		}
		filters = append(filters, FilterMetadata{
			Name:        field.Name,
			Field:       field.Name,
			Label:       field.Label,
			Type:        field.Type,
			Description: field.Description,
			Kind:        field.Filter.Kind,
			Required:    field.Required,
			Display:     field.Display,
		})
	}
	sortFilterMetadata(filters)
	return filters
}

func resourceSchemaActionsFromActions(resource string, actions []Action) []ResourceSchemaAction {
	out := make([]ResourceSchemaAction, 0, len(actions))
	for _, action := range actions {
		out = append(out, ResourceSchemaAction{
			Ref:         ActionRef(resource, action.Name),
			Name:        action.Name,
			Label:       action.Label,
			Description: action.Description,
			Scope:       action.Scope,
			Display:     action.Display,
			Confirm:     action.Confirm,
			Danger:      action.Danger,
		})
	}
	return out
}

func resourceSchemaActionsFromAdminActions(resource string, actions []AdminAction) []ResourceSchemaAction {
	out := make([]ResourceSchemaAction, 0, len(actions))
	for _, action := range actions {
		out = append(out, ResourceSchemaAction{
			Ref:            ActionRef(resource, action.Name),
			Name:           action.Name,
			Label:          action.Label,
			Description:    action.Description,
			Scope:          action.Scope,
			Display:        action.Display,
			Confirm:        action.Confirm,
			Danger:         action.Destructive,
			InputSchemaRef: action.InputSchemaRef,
		})
	}
	return out
}

func normalizeResourceSchema(schema ResourceSchema) (ResourceSchema, error) {
	clean := ResourceSchema{
		Name:        strings.TrimSpace(schema.Name),
		Label:       strings.TrimSpace(schema.Label),
		Group:       strings.TrimSpace(schema.Group),
		Description: strings.TrimSpace(schema.Description),
	}
	if clean.Label == "" {
		clean.Label = clean.Name
	}

	var errs []error
	if clean.Name == "" {
		errs = append(errs, invalidResourceSchemaField("name", "is required"))
	} else if hasControl(clean.Name) {
		errs = append(errs, invalidResourceSchemaField("name", "contains control characters"))
	}
	if hasControl(clean.Label) {
		errs = append(errs, invalidResourceSchemaField("label", "contains control characters"))
	}
	if hasControl(clean.Group) {
		errs = append(errs, invalidResourceSchemaField("group", "contains control characters"))
	}
	if hasControl(clean.Description) {
		errs = append(errs, invalidResourceSchemaField("description", "contains control characters"))
	}

	var err error
	clean.ListFields, err = normalizeSchemaFields("list_fields", schema.ListFields)
	if err != nil {
		errs = append(errs, err)
	}
	clean.DetailFields, err = normalizeSchemaFields("detail_fields", schema.DetailFields)
	if err != nil {
		errs = append(errs, err)
	}
	clean.FormFields, err = normalizeSchemaFields("form_fields", schema.FormFields)
	if err != nil {
		errs = append(errs, err)
	}
	clean.Filters, err = normalizeFilterMetadataList(schema.Filters)
	if err != nil {
		errs = append(errs, err)
	}
	clean.Actions, err = normalizeResourceSchemaActions(clean.Name, schema.Actions)
	if err != nil {
		errs = append(errs, err)
	}

	if err := errors.Join(errs...); err != nil {
		return ResourceSchema{}, err
	}
	return clean, nil
}

func normalizeSchemaFields(section string, fields []Field) ([]Field, error) {
	normalized, err := normalizeFields(fields, -1)
	if err != nil {
		return nil, errors.Join(
			fmt.Errorf("%w: %s", ErrInvalidResourceSchema, section),
			err,
		)
	}
	sortFields(normalized)
	return normalized, nil
}

func normalizeFilterMetadataList(filters []FilterMetadata) ([]FilterMetadata, error) {
	normalized := make([]FilterMetadata, 0, len(filters))
	seen := make(map[string]int, len(filters))

	var errs []error
	for i, filter := range filters {
		clean, err := normalizeFilterMetadata(filter, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		key := metadataKey(clean.Name)
		if first, ok := seen[key]; ok {
			errs = append(errs, fmt.Errorf("%w: filter[%d] %q also appears at filter[%d]", ErrInvalidResourceSchema, i, clean.Name, first))
			continue
		}
		seen[key] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	sortFilterMetadata(normalized)
	return normalized, nil
}

func normalizeFilterMetadata(filter FilterMetadata, index int) (FilterMetadata, error) {
	clean := FilterMetadata{
		Name:        strings.TrimSpace(filter.Name),
		Field:       strings.TrimSpace(filter.Field),
		Label:       strings.TrimSpace(filter.Label),
		Type:        strings.TrimSpace(filter.Type),
		Description: strings.TrimSpace(filter.Description),
		Kind:        filter.Kind,
		Required:    filter.Required,
		Display:     normalizeDisplayHint(filter.Display),
	}
	if clean.Name == "" {
		clean.Name = clean.Field
	}
	if clean.Field == "" {
		clean.Field = clean.Name
	}
	if clean.Label == "" {
		clean.Label = clean.Name
	}
	if clean.Kind == "" {
		clean.Kind = FilterExact
	}

	var errs []error
	if clean.Name == "" {
		errs = append(errs, invalidFilterMetadataField(index, "name", "is required"))
	} else if hasControl(clean.Name) {
		errs = append(errs, invalidFilterMetadataField(index, "name", "contains control characters"))
	}
	if clean.Field == "" {
		errs = append(errs, invalidFilterMetadataField(index, "field", "is required"))
	} else if hasControl(clean.Field) {
		errs = append(errs, invalidFilterMetadataField(index, "field", "contains control characters"))
	}
	if metadataKey(clean.Name) != metadataKey(clean.Field) {
		errs = append(errs, invalidFilterMetadataField(index, "field", "must match name"))
	}
	if clean.Type == "" {
		errs = append(errs, invalidFilterMetadataField(index, "type", "is required"))
	} else if hasControl(clean.Type) {
		errs = append(errs, invalidFilterMetadataField(index, "type", "contains control characters"))
	}
	if hasControl(clean.Label) {
		errs = append(errs, invalidFilterMetadataField(index, "label", "contains control characters"))
	}
	if hasControl(clean.Description) {
		errs = append(errs, invalidFilterMetadataField(index, "description", "contains control characters"))
	}
	if err := validateFilterHint(FilterHint{Enabled: true, Kind: clean.Kind}); err != nil {
		errs = append(errs, invalidFilterMetadataField(index, "kind", err.Error()))
	}
	if err := validateDisplayHint(clean.Display); err != nil {
		errs = append(errs, invalidFilterMetadataField(index, "display", err.Error()))
	}

	if err := errors.Join(errs...); err != nil {
		return FilterMetadata{}, err
	}
	return clean, nil
}

func normalizeResourceSchemaActions(resource string, actions []ResourceSchemaAction) ([]ResourceSchemaAction, error) {
	normalized := make([]ResourceSchemaAction, 0, len(actions))
	seen := make(map[string]int, len(actions))

	var errs []error
	for i, action := range actions {
		clean, err := normalizeResourceSchemaAction(resource, action, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		key := metadataKey(clean.Ref.Action)
		if first, ok := seen[key]; ok {
			errs = append(errs, fmt.Errorf("%w: action[%d] %q also appears at action[%d]", ErrDuplicateAction, i, clean.Ref.Action, first))
			continue
		}
		seen[key] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	sortResourceSchemaActions(normalized)
	return normalized, nil
}

func normalizeResourceSchemaAction(resource string, action ResourceSchemaAction, index int) (ResourceSchemaAction, error) {
	clean := ResourceSchemaAction{
		Ref:            action.Ref,
		Name:           strings.TrimSpace(action.Name),
		Label:          strings.TrimSpace(action.Label),
		Description:    strings.TrimSpace(action.Description),
		Scope:          action.Scope,
		Display:        normalizeDisplayHint(action.Display),
		Confirm:        action.Confirm,
		Danger:         action.Danger,
		InputSchemaRef: InputSchemaRef(strings.TrimSpace(string(action.InputSchemaRef))),
	}
	clean.Ref.Resource = strings.TrimSpace(clean.Ref.Resource)
	clean.Ref.Action = strings.TrimSpace(clean.Ref.Action)
	if clean.Name == "" {
		clean.Name = clean.Ref.Action
	}
	if clean.Label == "" {
		clean.Label = clean.Name
	}
	if clean.Scope == "" {
		clean.Scope = ActionScopeRecord
	}
	if clean.Ref.Resource == "" {
		clean.Ref.Resource = resource
	}
	if clean.Ref.Action == "" {
		clean.Ref.Action = clean.Name
	}

	var errs []error
	if clean.Name == "" {
		errs = append(errs, invalidSchemaActionField(index, "name", "is required"))
	} else if hasControl(clean.Name) {
		errs = append(errs, invalidSchemaActionField(index, "name", "contains control characters"))
	}
	if hasControl(clean.Label) {
		errs = append(errs, invalidSchemaActionField(index, "label", "contains control characters"))
	}
	if hasControl(clean.Description) {
		errs = append(errs, invalidSchemaActionField(index, "description", "contains control characters"))
	}
	if err := validateActionScope(clean.Scope); err != nil {
		errs = append(errs, invalidSchemaActionField(index, "scope", err.Error()))
	}
	if err := validateDisplayHint(clean.Display); err != nil {
		errs = append(errs, invalidSchemaActionField(index, "display", err.Error()))
	}
	if err := validateInputSchemaRef(clean.InputSchemaRef); err != nil {
		errs = append(errs, invalidSchemaActionField(index, "input_schema_ref", err.Error()))
	}

	ref, err := NormalizeResourceActionRef(clean.Ref)
	if err != nil {
		errs = append(errs, fmt.Errorf("%w: action[%d].ref: %w", ErrInvalidResourceSchema, index, err))
	} else {
		clean.Ref = ref
		if resource != "" && metadataKey(clean.Ref.Resource) != metadataKey(resource) {
			errs = append(errs, invalidSchemaActionField(index, "ref.resource", "must match schema name"))
		}
		if metadataKey(clean.Ref.Action) != metadataKey(clean.Name) {
			errs = append(errs, invalidSchemaActionField(index, "ref.action", "must match name"))
		}
	}

	if err := errors.Join(errs...); err != nil {
		return ResourceSchemaAction{}, err
	}
	return clean, nil
}

func sortFilterMetadata(filters []FilterMetadata) {
	sort.SliceStable(filters, func(i, j int) bool {
		left, right := filters[i], filters[j]
		for _, cmp := range []int{
			compareFold(fieldGroupLabel(left.Display.Group), fieldGroupLabel(right.Display.Group)),
			compareInt(left.Display.Order, right.Display.Order),
			compareDisplayName(left.Label, left.Name, right.Label, right.Name),
			compareFold(left.Name, right.Name),
		} {
			if cmp != 0 {
				return cmp < 0
			}
		}
		return false
	})
}

func sortResourceSchemaActions(actions []ResourceSchemaAction) {
	sort.SliceStable(actions, func(i, j int) bool {
		return compareAction(actions[i].Action(), actions[j].Action()) < 0
	})
}

func validateActionRefPart(field, value string) []error {
	var errs []error
	if value == "" {
		errs = append(errs, fmt.Errorf("%w: %s is required", ErrInvalidActionReference, field))
	}
	if hasControl(value) {
		errs = append(errs, fmt.Errorf("%w: %s contains control characters", ErrInvalidActionReference, field))
	}
	if strings.ContainsFunc(value, unicode.IsSpace) {
		errs = append(errs, fmt.Errorf("%w: %s contains whitespace", ErrInvalidActionReference, field))
	}
	return errs
}

func invalidResourceSchemaField(field, reason string) error {
	return fmt.Errorf("%w: %s %s", ErrInvalidResourceSchema, field, reason)
}

func invalidFilterMetadataField(index int, field, reason string) error {
	return fmt.Errorf("%w: filter[%d].%s %s", ErrInvalidResourceSchema, index, field, reason)
}

func invalidSchemaActionField(index int, field, reason string) error {
	return fmt.Errorf("%w: action[%d].%s %s", ErrInvalidResourceSchema, index, field, reason)
}
