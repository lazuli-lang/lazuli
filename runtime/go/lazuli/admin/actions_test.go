package admin

import (
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestAdminActionHelpersBuildSingleBulkAndFlags(t *testing.T) {
	single := SingleAction("refund")
	if single.Scope != ActionScopeRecord {
		t.Fatalf("SingleAction scope = %q, want %q", single.Scope, ActionScopeRecord)
	}

	bulk := BulkAction("archive").
		WithConfirmation().
		AsDestructive().
		WithInputSchema("orders.ArchiveInput")
	if bulk.Scope != ActionScopeBulk {
		t.Fatalf("BulkAction scope = %q, want %q", bulk.Scope, ActionScopeBulk)
	}
	if !bulk.Confirm {
		t.Fatal("WithConfirmation did not set Confirm")
	}
	if !bulk.Destructive {
		t.Fatal("AsDestructive did not set Destructive")
	}
	if !bulk.RequiresConfirmation() {
		t.Fatal("RequiresConfirmation returned false for destructive action")
	}
	if bulk.InputSchemaRef.String() != "orders.ArchiveInput" {
		t.Fatalf("InputSchemaRef = %q, want %q", bulk.InputSchemaRef, "orders.ArchiveInput")
	}

	metadata := bulk.Action()
	if !metadata.Danger || !metadata.Confirm {
		t.Fatalf("Action() flags = Danger:%v Confirm:%v, want true true", metadata.Danger, metadata.Confirm)
	}
}

func TestSortedAdminActionsNormalizesSortsAndDoesNotMutate(t *testing.T) {
	actions := []AdminAction{
		BulkAction(" archive ").
			WithLabel(" Archive ").
			WithGroup(" Danger ").
			WithOrder(2).
			WithInputSchema(" orders.ArchiveInput ").
			WithConfirmation().
			AsDestructive(),
		CollectionAction("export").WithOrder(1),
		SingleAction("refund").WithLabel("Refund").WithOrder(0),
	}

	got, err := SortedAdminActions(actions)
	if err != nil {
		t.Fatalf("SortedAdminActions() error = %v", err)
	}

	if gotNames := adminActionNames(got); !reflect.DeepEqual(gotNames, []string{"refund", "export", "archive"}) {
		t.Fatalf("SortedAdminActions() names = %#v", gotNames)
	}
	if got[2].Name != "archive" || got[2].Label != "Archive" {
		t.Fatalf("action was not trimmed and normalized: %#v", got[2])
	}
	if got[2].InputSchemaRef != "orders.ArchiveInput" {
		t.Fatalf("InputSchemaRef = %q, want %q", got[2].InputSchemaRef, "orders.ArchiveInput")
	}
	if got[0].Label != "Refund" || got[0].Scope != ActionScopeRecord {
		t.Fatalf("single action defaults changed: %#v", got[0])
	}

	if actions[0].Name != " archive " {
		t.Fatal("SortedAdminActions() mutated input name")
	}
	if actions[0].InputSchemaRef != " orders.ArchiveInput " {
		t.Fatal("SortedAdminActions() mutated input schema ref")
	}
}

func TestGroupAdminActionsIsDeterministic(t *testing.T) {
	groups, err := GroupAdminActions([]AdminAction{
		BulkAction("delete").WithGroup("Danger").WithOrder(2),
		CollectionAction("export"),
		BulkAction("merge").WithGroup("Danger").WithOrder(1),
	})
	if err != nil {
		t.Fatalf("GroupAdminActions() error = %v", err)
	}

	if got := adminActionGroupNames(groups); !reflect.DeepEqual(got, [][]string{{DefaultActionGroup, "export"}, {"Danger", "merge", "delete"}}) {
		t.Fatalf("GroupAdminActions() groups = %#v", got)
	}
}

func TestValidateAdminActionsRejectsInvalidDefinitions(t *testing.T) {
	actions := []AdminAction{
		SingleAction("archive"),
		BulkAction("Archive"),
		{
			Name:           "bad_scope",
			Scope:          "team",
			InputSchemaRef: "orders.Import Input",
		},
		{
			Name:           "bad_ref",
			Scope:          ActionScopeRecord,
			InputSchemaRef: "orders.\nImportInput",
		},
	}

	err := ValidateActionDefinitions(actions)
	for _, wantErr := range []error{
		ErrDuplicateAction,
		ErrInvalidAction,
	} {
		if !errors.Is(err, wantErr) {
			t.Fatalf("ValidateActionDefinitions() error = %v, want %v", err, wantErr)
		}
	}

	for _, want := range []string{
		"action[1] \"Archive\" also appears at action[0]",
		"action[2].scope",
		"action[2].input_schema_ref",
		"contains whitespace",
		"action[3].input_schema_ref",
		"contains control characters",
	} {
		if !strings.Contains(err.Error(), want) {
			t.Fatalf("ValidateActionDefinitions() error = %q, want substring %q", err.Error(), want)
		}
	}
}

func adminActionNames(actions []AdminAction) []string {
	names := make([]string, 0, len(actions))
	for _, action := range actions {
		names = append(names, action.Name)
	}
	return names
}

func adminActionGroupNames(groups []AdminActionGroup) [][]string {
	out := make([][]string, 0, len(groups))
	for _, group := range groups {
		row := []string{group.Label}
		row = append(row, adminActionNames(group.Actions)...)
		out = append(out, row)
	}
	return out
}
