package admin

import (
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestSortedResourcesNormalizesSortsAndDoesNotMutate(t *testing.T) {
	resources := []Resource{
		{
			Name:  "orders",
			Label: "Orders",
			Group: "Sales",
			Order: 2,
			Fields: []Field{
				{
					Name:    "total",
					Label:   "Total",
					Type:    "decimal",
					Filter:  FilterHint{Kind: FilterRange},
					Display: DisplayHint{Group: "Amounts", Order: 1, List: true},
				},
				{
					Name:    "id",
					Type:    "id",
					Filter:  FilterHint{Enabled: true},
					Display: DisplayHint{Order: 1, List: true, Detail: true},
				},
				{
					Name:    "created_at",
					Label:   "Created",
					Type:    "datetime",
					Sort:    SortHint{Default: SortAscending},
					Display: DisplayHint{Group: "Main"},
				},
			},
			Actions: []Action{
				{
					Name:    "archive",
					Scope:   ActionScopeBulk,
					Display: DisplayHint{Group: "Danger", Order: 2},
					Danger:  true,
					Confirm: true,
				},
				{
					Name:    "refund",
					Display: DisplayHint{Order: 1},
				},
			},
		},
		{
			Name:  "customers",
			Group: "Sales",
			Order: 1,
			Fields: []Field{
				{Name: "email", Type: "string"},
			},
		},
		{
			Name:  "audit",
			Label: "Audit Log",
			Fields: []Field{
				{Name: "event", Type: "string"},
			},
		},
	}

	got, err := SortedResources(resources)
	if err != nil {
		t.Fatalf("SortedResources() error = %v", err)
	}

	if gotNames := resourceNames(got); !reflect.DeepEqual(gotNames, []string{"audit", "customers", "orders"}) {
		t.Fatalf("SortedResources() names = %#v", gotNames)
	}

	orders := got[2]
	if gotNames := fieldNames(orders.Fields); !reflect.DeepEqual(gotNames, []string{"total", "created_at", "id"}) {
		t.Fatalf("SortedResources() field names = %#v", gotNames)
	}
	if gotNames := actionNames(orders.Actions); !reflect.DeepEqual(gotNames, []string{"refund", "archive"}) {
		t.Fatalf("SortedResources() action names = %#v", gotNames)
	}

	if !orders.Fields[0].Filter.Enabled || orders.Fields[0].Filter.Kind != FilterRange {
		t.Fatalf("Filter hint was not normalized: %#v", orders.Fields[0].Filter)
	}
	if !orders.Fields[1].Sort.Enabled || orders.Fields[1].Sort.Default != SortAscending {
		t.Fatalf("Sort hint was not normalized: %#v", orders.Fields[1].Sort)
	}
	if orders.Fields[2].Label != "id" || orders.Fields[2].Filter.Kind != FilterExact {
		t.Fatalf("Default field values were not normalized: %#v", orders.Fields[2])
	}
	if orders.Actions[0].Scope != ActionScopeRecord || orders.Actions[0].Label != "refund" {
		t.Fatalf("Default action values were not normalized: %#v", orders.Actions[0])
	}

	if resources[0].Fields[0].Filter.Enabled {
		t.Fatal("SortedResources() mutated input filter hint")
	}
	if resources[0].Actions[1].Scope != "" {
		t.Fatal("SortedResources() mutated input action scope")
	}
}

func TestGroupingHelpersAreDeterministic(t *testing.T) {
	resources := []Resource{
		{Name: "orders", Group: "Sales", Order: 2, Fields: []Field{{Name: "id", Type: "id"}}},
		{Name: "audit", Fields: []Field{{Name: "event", Type: "string"}}},
		{Name: "customers", Group: "Sales", Order: 1, Fields: []Field{{Name: "id", Type: "id"}}},
	}

	resourceGroups, err := GroupResources(resources)
	if err != nil {
		t.Fatalf("GroupResources() error = %v", err)
	}

	if got := resourceGroupNames(resourceGroups); !reflect.DeepEqual(got, [][]string{{DefaultResourceGroup, "audit"}, {"Sales", "customers", "orders"}}) {
		t.Fatalf("GroupResources() groups = %#v", got)
	}

	fieldGroups, err := GroupFields([]Field{
		{Name: "status", Type: "string", Display: DisplayHint{Group: "Workflow", Order: 2}},
		{Name: "id", Type: "id"},
		{Name: "assigned_to", Type: "relation", Display: DisplayHint{Group: "Workflow", Order: 1}},
	})
	if err != nil {
		t.Fatalf("GroupFields() error = %v", err)
	}
	if got := fieldGroupNames(fieldGroups); !reflect.DeepEqual(got, [][]string{{DefaultFieldGroup, "id"}, {"Workflow", "assigned_to", "status"}}) {
		t.Fatalf("GroupFields() groups = %#v", got)
	}

	actionGroups, err := GroupActions([]Action{
		{Name: "delete", Scope: ActionScopeBulk, Display: DisplayHint{Group: "Danger"}},
		{Name: "export", Scope: ActionScopeCollection},
		{Name: "merge", Scope: ActionScopeBulk, Display: DisplayHint{Group: "Danger", Order: 1}},
	})
	if err != nil {
		t.Fatalf("GroupActions() error = %v", err)
	}
	if got := actionGroupNames(actionGroups); !reflect.DeepEqual(got, [][]string{{DefaultActionGroup, "export"}, {"Danger", "delete", "merge"}}) {
		t.Fatalf("GroupActions() groups = %#v", got)
	}
}

func TestValidateResourcesRejectsInvalidAndDuplicateMetadata(t *testing.T) {
	resources := []Resource{
		{
			Name: "accounts",
			Fields: []Field{
				{Name: "id", Type: "id"},
			},
		},
		{
			Name: "Accounts",
			Fields: []Field{
				{Name: "id", Type: "id"},
			},
		},
		{
			Name:  "orders",
			Order: -1,
			Fields: []Field{
				{Name: "id", Type: "id"},
				{Name: "ID", Type: "id"},
				{
					Name:    "bad",
					Sort:    SortHint{Default: "sideways", Priority: -1},
					Filter:  FilterHint{Kind: "custom"},
					Display: DisplayHint{Order: -1},
				},
			},
			Actions: []Action{
				{Name: "ship", Scope: ActionScopeRecord},
				{Name: "SHIP", Scope: ActionScopeBulk},
				{Name: "bad_scope", Scope: "somewhere"},
				{Name: "", Scope: ActionScopeRecord},
			},
		},
	}

	err := ValidateResources(resources)
	for _, wantErr := range []error{
		ErrDuplicateResource,
		ErrInvalidResource,
		ErrDuplicateField,
		ErrInvalidField,
		ErrDuplicateAction,
		ErrInvalidAction,
	} {
		if !errors.Is(err, wantErr) {
			t.Fatalf("ValidateResources() error = %v, want %v", err, wantErr)
		}
	}

	for _, want := range []string{
		"resource[1] \"Accounts\" also appears at resource[0]",
		"resource[2].order",
		"resource[2].fields[1]",
		"resource[2].fields[2].type",
		"resource[2].fields[2].sort",
		"resource[2].fields[2].filter",
		"resource[2].fields[2].display",
		"resource[2].actions[1]",
		"resource[2].actions[2].scope",
		"resource[2].actions[3].name",
	} {
		if !strings.Contains(err.Error(), want) {
			t.Fatalf("ValidateResources() error = %q, want substring %q", err.Error(), want)
		}
	}
}

func resourceNames(resources []Resource) []string {
	names := make([]string, 0, len(resources))
	for _, resource := range resources {
		names = append(names, resource.Name)
	}
	return names
}

func fieldNames(fields []Field) []string {
	names := make([]string, 0, len(fields))
	for _, field := range fields {
		names = append(names, field.Name)
	}
	return names
}

func actionNames(actions []Action) []string {
	names := make([]string, 0, len(actions))
	for _, action := range actions {
		names = append(names, action.Name)
	}
	return names
}

func resourceGroupNames(groups []ResourceGroup) [][]string {
	out := make([][]string, 0, len(groups))
	for _, group := range groups {
		row := []string{group.Label}
		row = append(row, resourceNames(group.Resources)...)
		out = append(out, row)
	}
	return out
}

func fieldGroupNames(groups []FieldGroup) [][]string {
	out := make([][]string, 0, len(groups))
	for _, group := range groups {
		row := []string{group.Label}
		row = append(row, fieldNames(group.Fields)...)
		out = append(out, row)
	}
	return out
}

func actionGroupNames(groups []ActionGroup) [][]string {
	out := make([][]string, 0, len(groups))
	for _, group := range groups {
		row := []string{group.Label}
		row = append(row, actionNames(group.Actions)...)
		out = append(out, row)
	}
	return out
}
