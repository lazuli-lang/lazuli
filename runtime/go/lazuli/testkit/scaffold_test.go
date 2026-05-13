package testkit_test

import (
	"errors"
	"reflect"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/testkit"
)

func TestPlanScaffoldDefaultsByKind(t *testing.T) {
	tests := []struct {
		name        string
		kind        testkit.ScaffoldKind
		wantFile    string
		wantPackage string
		wantTest    string
	}{
		{
			name:        "unit",
			kind:        testkit.ScaffoldKindUnit,
			wantFile:    "customer_order_test.go",
			wantPackage: "orders",
			wantTest:    "TestCustomerOrder",
		},
		{
			name:        "integration",
			kind:        testkit.ScaffoldKindIntegration,
			wantFile:    "customer_order_integration_test.go",
			wantPackage: "orders_test",
			wantTest:    "TestCustomerOrderIntegration",
		},
		{
			name:        "system",
			kind:        testkit.ScaffoldKindSystem,
			wantFile:    "customer_order_system_test.go",
			wantPackage: "orders_test",
			wantTest:    "TestCustomerOrderSystem",
		},
		{
			name:        "request",
			kind:        testkit.ScaffoldKindRequest,
			wantFile:    "customer_order_request_test.go",
			wantPackage: "orders_test",
			wantTest:    "TestCustomerOrderRequest",
		},
		{
			name:        "job",
			kind:        testkit.ScaffoldKindJob,
			wantFile:    "customer_order_job_test.go",
			wantPackage: "orders_test",
			wantTest:    "TestCustomerOrderJob",
		},
		{
			name:        "api",
			kind:        testkit.ScaffoldKindAPI,
			wantFile:    "customer_order_api_test.go",
			wantPackage: "orders_test",
			wantTest:    "TestCustomerOrderAPI",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			plan, err := testkit.PlanScaffold(testkit.ScaffoldSpec{
				Kind:        tt.kind,
				Name:        " Customer Order ",
				PackageName: "orders",
			})
			if err != nil {
				t.Fatalf("PlanScaffold() error = %v", err)
			}

			if plan.Kind != tt.kind {
				t.Fatalf("Kind = %q, want %q", plan.Kind, tt.kind)
			}
			if plan.Name != "Customer Order" {
				t.Fatalf("Name = %q, want trimmed subject", plan.Name)
			}
			if plan.FileName != tt.wantFile {
				t.Fatalf("FileName = %q, want %q", plan.FileName, tt.wantFile)
			}
			if plan.PackageName != tt.wantPackage {
				t.Fatalf("PackageName = %q, want %q", plan.PackageName, tt.wantPackage)
			}
			if plan.SourcePackageName != "orders" {
				t.Fatalf("SourcePackageName = %q, want orders", plan.SourcePackageName)
			}
			if plan.TestName != tt.wantTest {
				t.Fatalf("TestName = %q, want %q", plan.TestName, tt.wantTest)
			}

			slots := plan.SlotMap()
			for name, want := range map[string]string{
				"FileName":          tt.wantFile,
				"Kind":              string(tt.kind),
				"Name":              "Customer Order",
				"PackageName":       tt.wantPackage,
				"SourcePackageName": "orders",
				"TestName":          tt.wantTest,
			} {
				if got := slots[name]; got != want {
					t.Fatalf("slot %s = %q, want %q", name, got, want)
				}
			}
		})
	}
}

func TestPlanScaffoldSortsTablesAndSlotsAndDoesNotMutate(t *testing.T) {
	slots := []testkit.ScaffoldSlot{
		{Name: "Subject", Value: " customer "},
		{Name: "Expectation", Value: "ok"},
	}
	tables := []testkit.ScaffoldTable{
		{Name: "zeta", Alias: "z"},
		{Name: " alpha ", Alias: " a "},
	}

	plan, err := testkit.PlanScaffold(testkit.ScaffoldSpec{
		Kind:          testkit.ScaffoldKindRequest,
		Name:          "List Customers",
		PackageName:   "customers",
		TemplateSlots: slots,
		Tables:        tables,
	})
	if err != nil {
		t.Fatalf("PlanScaffold() error = %v", err)
	}

	wantTables := []testkit.ScaffoldTable{
		{Name: "alpha", Alias: "a"},
		{Name: "zeta", Alias: "z"},
	}
	if !reflect.DeepEqual(plan.Tables, wantTables) {
		t.Fatalf("Tables = %#v, want %#v", plan.Tables, wantTables)
	}

	var gotSlotNames []string
	for _, slot := range plan.TemplateSlots {
		gotSlotNames = append(gotSlotNames, slot.Name)
	}
	wantSlotNames := []string{
		"Expectation",
		"FileName",
		"Kind",
		"Name",
		"PackageName",
		"SourcePackageName",
		"Subject",
		"TestName",
	}
	if !reflect.DeepEqual(gotSlotNames, wantSlotNames) {
		t.Fatalf("slot names = %#v, want %#v", gotSlotNames, wantSlotNames)
	}
	if got := plan.SlotMap()["Subject"]; got != "customer" {
		t.Fatalf("Subject slot = %q, want trimmed custom value", got)
	}

	if slots[0].Value != " customer " {
		t.Fatalf("PlanScaffold mutated slot input to %q", slots[0].Value)
	}
	if tables[1].Alias != " a " {
		t.Fatalf("PlanScaffold mutated table input to %q", tables[1].Alias)
	}

	slotMap := plan.SlotMap()
	slotMap["Subject"] = "mutated"
	if got := plan.SlotMap()["Subject"]; got != "customer" {
		t.Fatalf("SlotMap returned mutable plan state, Subject = %q", got)
	}
}

func TestSortedScaffoldsOrdersByKindThenFileName(t *testing.T) {
	plans, err := testkit.SortedScaffolds([]testkit.ScaffoldSpec{
		{Kind: testkit.ScaffoldKindAPI, Name: "Charge", PackageName: "billing"},
		{Kind: testkit.ScaffoldKindUnit, Name: "Zeta", PackageName: "billing"},
		{Kind: testkit.ScaffoldKindIntegration, Name: "Alpha", PackageName: "billing"},
		{Kind: testkit.ScaffoldKindUnit, Name: "Alpha", PackageName: "billing"},
	})
	if err != nil {
		t.Fatalf("SortedScaffolds() error = %v", err)
	}

	gotFiles := make([]string, len(plans))
	for i, plan := range plans {
		gotFiles[i] = plan.FileName
	}
	wantFiles := []string{
		"alpha_test.go",
		"zeta_test.go",
		"alpha_integration_test.go",
		"charge_api_test.go",
	}
	if !reflect.DeepEqual(gotFiles, wantFiles) {
		t.Fatalf("files = %#v, want %#v", gotFiles, wantFiles)
	}
}

func TestNormalizeScaffoldKindCanonicalizesInput(t *testing.T) {
	got, err := testkit.NormalizeScaffoldKind(" API ")
	if err != nil {
		t.Fatalf("NormalizeScaffoldKind() error = %v", err)
	}
	if got != testkit.ScaffoldKindAPI {
		t.Fatalf("NormalizeScaffoldKind() = %q, want api", got)
	}
}

func TestValidateScaffoldRejectsInvalidInputs(t *testing.T) {
	err := testkit.ValidateScaffold(testkit.ScaffoldSpec{
		Kind:            "mystery",
		Name:            " ",
		PackageName:     "bad-name",
		TestPackageName: "for",
		FileName:        "../bad.go",
		TemplateSlots: []testkit.ScaffoldSlot{
			{Name: "bad-name"},
		},
		Tables: []testkit.ScaffoldTable{
			{Name: "bad/table"},
		},
	})
	if !errors.Is(err, testkit.ErrInvalidScaffold) {
		t.Fatalf("ValidateScaffold() error = %v, want ErrInvalidScaffold", err)
	}

	for _, fragment := range []string{
		"kind must be one of",
		"name is required",
		"package_name must be a Go package identifier",
		"test_package_name must be a Go package identifier",
		"file_name must be a file name",
		"template_slots[0].name must be a Go identifier",
		"tables[0].name must contain only",
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateScaffold() error = %q, want fragment %q", err, fragment)
		}
	}
}

func TestPlanScaffoldRejectsDuplicateSlotsTablesAndPlans(t *testing.T) {
	_, err := testkit.PlanScaffold(testkit.ScaffoldSpec{
		Kind:        testkit.ScaffoldKindUnit,
		Name:        "Orders",
		PackageName: "orders",
		TemplateSlots: []testkit.ScaffoldSlot{
			{Name: "Kind", Value: "override"},
		},
		Tables: []testkit.ScaffoldTable{
			{Name: "users"},
			{Name: " users "},
		},
	})
	if !errors.Is(err, testkit.ErrDuplicateScaffold) {
		t.Fatalf("PlanScaffold() error = %v, want ErrDuplicateScaffold", err)
	}
	for _, fragment := range []string{
		`tables[1] "users" also appears at tables[0]`,
		`template_slots[0] name "Kind" also appears at default_template_slots[1]`,
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("PlanScaffold() error = %q, want fragment %q", err, fragment)
		}
	}

	_, err = testkit.SortedScaffolds([]testkit.ScaffoldSpec{
		{Kind: testkit.ScaffoldKindUnit, Name: "Orders", PackageName: "orders"},
		{Kind: testkit.ScaffoldKindUnit, Name: "Orders", PackageName: "orders"},
	})
	if !errors.Is(err, testkit.ErrDuplicateScaffold) {
		t.Fatalf("SortedScaffolds() error = %v, want ErrDuplicateScaffold", err)
	}
	if !strings.Contains(err.Error(), "scaffolds[1] orders_test.go") {
		t.Fatalf("SortedScaffolds() error = %q, want duplicate file context", err)
	}
}
