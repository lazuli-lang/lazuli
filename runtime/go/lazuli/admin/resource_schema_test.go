package admin

import (
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestBuildResourceSchemaSplitsFieldsFiltersAndActionRefs(t *testing.T) {
	resource := Resource{
		Name:  " orders ",
		Label: " Orders ",
		Group: " Sales ",
		Fields: []Field{
			ListField(" total ", " decimal ").
				WithLabel(" Total ").
				InDetail().
				WithFilter(FilterRange).
				WithOrder(2),
			FormField(" status ", " string ").
				WithLabel(" Status ").
				InList().
				InDetail().
				WithFilter(FilterSelect).
				WithOrder(1).
				AsRequired(),
			DetailField("notes", "text").
				WithOrder(3),
			ResourceField("created_at", "datetime").
				AsReadOnly(),
			ResourceField("internal", "string").
				WithFilter(FilterContains).
				AsHidden(),
		},
		Actions: []Action{
			{Name: " refund ", Label: " Refund ", Scope: ActionScopeRecord, Confirm: true, Danger: true, Display: DisplayHint{Order: 2}},
			{Name: " export ", Scope: ActionScopeCollection},
		},
	}

	got, err := BuildResourceSchema(resource)
	if err != nil {
		t.Fatalf("BuildResourceSchema() error = %v", err)
	}

	if got.Name != "orders" || got.Label != "Orders" || got.Group != "Sales" {
		t.Fatalf("resource metadata was not normalized: %#v", got)
	}
	if gotNames := fieldNames(got.ListFields); !reflect.DeepEqual(gotNames, []string{"created_at", "status", "total"}) {
		t.Fatalf("ListFields names = %#v", gotNames)
	}
	if gotNames := fieldNames(got.DetailFields); !reflect.DeepEqual(gotNames, []string{"created_at", "status", "total", "notes"}) {
		t.Fatalf("DetailFields names = %#v", gotNames)
	}
	if gotNames := fieldNames(got.FormFields); !reflect.DeepEqual(gotNames, []string{"status"}) {
		t.Fatalf("FormFields names = %#v", gotNames)
	}

	if gotFilters := filterNames(got.Filters); !reflect.DeepEqual(gotFilters, []string{"status", "total"}) {
		t.Fatalf("Filters names = %#v", gotFilters)
	}
	if got.Filters[0].Kind != FilterSelect || got.Filters[0].Type != "string" || !got.Filters[0].Required {
		t.Fatalf("first filter metadata = %#v, want required select string filter", got.Filters[0])
	}

	ref, ok := got.ActionRef("Refund")
	if !ok {
		t.Fatal("ActionRef(\"Refund\") was not found")
	}
	if ref.String() != "orders.refund" {
		t.Fatalf("ActionRef(\"Refund\") = %q, want orders.refund", ref.String())
	}

	if gotActions := schemaActionNames(got.Actions); !reflect.DeepEqual(gotActions, []string{"export", "refund"}) {
		t.Fatalf("Actions names = %#v", gotActions)
	}
	if !got.Actions[1].RequiresConfirmation() {
		t.Fatal("RequiresConfirmation returned false for dangerous action")
	}

	if resource.Name != " orders " || resource.Fields[0].Name != " total " || resource.Actions[0].Name != " refund " {
		t.Fatal("BuildResourceSchema() mutated input metadata")
	}
}

func TestFieldViewHelpersDefaultToNonHiddenFieldsWhenViewIsNotExplicit(t *testing.T) {
	fields := []Field{
		ResourceField("id", "id").AsReadOnly(),
		ResourceField("email", "string").AsRequired(),
		ResourceField("internal", "string").AsHidden(),
	}

	listFields, err := ListFields(fields)
	if err != nil {
		t.Fatalf("ListFields() error = %v", err)
	}
	if got := fieldNames(listFields); !reflect.DeepEqual(got, []string{"email", "id"}) {
		t.Fatalf("ListFields() names = %#v", got)
	}

	detailFields, err := DetailFields(fields)
	if err != nil {
		t.Fatalf("DetailFields() error = %v", err)
	}
	if got := fieldNames(detailFields); !reflect.DeepEqual(got, []string{"email", "id"}) {
		t.Fatalf("DetailFields() names = %#v", got)
	}

	formFields, err := FormFields(fields)
	if err != nil {
		t.Fatalf("FormFields() error = %v", err)
	}
	if got := fieldNames(formFields); !reflect.DeepEqual(got, []string{"email"}) {
		t.Fatalf("FormFields() names = %#v", got)
	}
}

func TestBuildResourceSchemaWithActionsKeepsInputSchemaRefs(t *testing.T) {
	resource := Resource{
		Name: "orders",
		Fields: []Field{
			ResourceField("id", "id"),
		},
		Actions: []Action{
			{Name: "bad_scope", Scope: "somewhere"},
		},
	}
	actions := []AdminAction{
		BulkAction(" archive ").
			WithLabel(" Archive ").
			WithInputSchema(" orders.ArchiveInput ").
			WithConfirmation().
			AsDestructive(),
	}

	got, err := BuildResourceSchemaWithActions(resource, actions)
	if err != nil {
		t.Fatalf("BuildResourceSchemaWithActions() error = %v", err)
	}

	if len(got.Actions) != 1 {
		t.Fatalf("Actions length = %d, want 1", len(got.Actions))
	}
	action := got.Actions[0]
	if action.Name != "archive" || action.Label != "Archive" || action.Scope != ActionScopeBulk {
		t.Fatalf("schema action was not normalized: %#v", action)
	}
	if action.Ref.Canonical() != "orders.archive" {
		t.Fatalf("action ref = %q, want orders.archive", action.Ref.Canonical())
	}
	if action.InputSchemaRef != "orders.ArchiveInput" {
		t.Fatalf("InputSchemaRef = %q, want orders.ArchiveInput", action.InputSchemaRef)
	}
	if !action.Danger || !action.RequiresConfirmation() {
		t.Fatalf("action flags = Danger:%v RequiresConfirmation:%v, want true true", action.Danger, action.RequiresConfirmation())
	}

	if resource.Actions[0].Scope != "somewhere" {
		t.Fatal("BuildResourceSchemaWithActions() mutated resource actions")
	}
	if actions[0].Name != " archive " || actions[0].InputSchemaRef != " orders.ArchiveInput " {
		t.Fatal("BuildResourceSchemaWithActions() mutated admin actions")
	}
}

func TestValidateResourceSchemaRejectsInvalidFiltersAndUnsafeActionRefs(t *testing.T) {
	schema := ResourceSchema{
		Name: "orders",
		ListFields: []Field{
			ResourceField("id", "id"),
		},
		Filters: []FilterMetadata{
			{Name: "status", Field: "state", Type: "string", Kind: "custom"},
			{Name: "id", Field: "id", Type: "id"},
			{Name: "ID", Field: "ID", Type: "id"},
		},
		Actions: []ResourceSchemaAction{
			{Name: "refund", Ref: ActionRef("orders", "refund")},
			{Name: "Refund", Ref: ActionRef("orders", "Refund")},
			{Name: "archive", Ref: ActionRef("orders", "bad action")},
			{Name: "export", Ref: ActionRef("other", "export")},
			{Ref: ResourceActionRef{Resource: "orders"}},
		},
	}

	err := ValidateResourceSchema(schema)
	for _, wantErr := range []error{
		ErrInvalidResourceSchema,
		ErrInvalidActionReference,
		ErrDuplicateAction,
	} {
		if !errors.Is(err, wantErr) {
			t.Fatalf("ValidateResourceSchema() error = %v, want %v", err, wantErr)
		}
	}

	for _, want := range []string{
		"filter[0].field must match name",
		"filter[0].kind",
		"filter[2] \"ID\" also appears at filter[1]",
		"action[1] \"Refund\" also appears at action[0]",
		"action[2].ref",
		"action contains whitespace",
		"action[3].ref.resource",
		"action[4].name",
		"action is required",
	} {
		if !strings.Contains(err.Error(), want) {
			t.Fatalf("ValidateResourceSchema() error = %q, want substring %q", err.Error(), want)
		}
	}
}

func TestNormalizeResourceActionRefTrimsAndValidatesParts(t *testing.T) {
	ref, err := NormalizeResourceActionRef(ActionRef(" orders ", " refund "))
	if err != nil {
		t.Fatalf("NormalizeResourceActionRef() error = %v", err)
	}
	if ref.Canonical() != "orders.refund" {
		t.Fatalf("Canonical() = %q, want orders.refund", ref.Canonical())
	}

	err = ValidateResourceActionRef(ActionRef("orders", "bad action"))
	if !errors.Is(err, ErrInvalidActionReference) {
		t.Fatalf("ValidateResourceActionRef() error = %v, want %v", err, ErrInvalidActionReference)
	}
}

func filterNames(filters []FilterMetadata) []string {
	names := make([]string, 0, len(filters))
	for _, filter := range filters {
		names = append(names, filter.Name)
	}
	return names
}

func schemaActionNames(actions []ResourceSchemaAction) []string {
	names := make([]string, 0, len(actions))
	for _, action := range actions {
		names = append(names, action.Name)
	}
	return names
}
