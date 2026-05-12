package lazuli

import (
	"reflect"
	"testing"
)

func TestBuildDBResetSQLBuildsTruncateStatement(t *testing.T) {
	statements, err := BuildDBResetSQL([]string{"users", "audit_log"}, DBResetOptions{})
	if err != nil {
		t.Fatalf("BuildDBResetSQL returned error: %v", err)
	}

	want := []string{
		`TRUNCATE TABLE "users", "audit_log" RESTART IDENTITY CASCADE`,
	}
	assertDBResetStatements(t, statements, want)
}

func TestBuildDBResetSQLQuotesSchemaQualifiedTables(t *testing.T) {
	statements, err := BuildDBResetSQL([]string{"tenant_1.users", "public.Order_Events"}, DBResetOptions{})
	if err != nil {
		t.Fatalf("BuildDBResetSQL returned error: %v", err)
	}

	want := []string{
		`TRUNCATE TABLE "tenant_1"."users", "public"."Order_Events" RESTART IDENTITY CASCADE`,
	}
	assertDBResetStatements(t, statements, want)
}

func TestBuildDBResetSQLCanDisableTriggers(t *testing.T) {
	statements, err := BuildDBResetSQL([]string{"users", "app.orders"}, DBResetOptions{
		DisableTriggers: true,
	})
	if err != nil {
		t.Fatalf("BuildDBResetSQL returned error: %v", err)
	}

	want := []string{
		`ALTER TABLE "users" DISABLE TRIGGER ALL`,
		`ALTER TABLE "app"."orders" DISABLE TRIGGER ALL`,
		`TRUNCATE TABLE "users", "app"."orders" RESTART IDENTITY CASCADE`,
		`ALTER TABLE "users" ENABLE TRIGGER ALL`,
		`ALTER TABLE "app"."orders" ENABLE TRIGGER ALL`,
	}
	assertDBResetStatements(t, statements, want)
}

func TestBuildDBResetSQLRejectsNoTables(t *testing.T) {
	if statements, err := BuildDBResetSQL(nil, DBResetOptions{}); err == nil {
		t.Fatalf("BuildDBResetSQL returned nil error with statements %#v", statements)
	}
}

func TestBuildDBResetSQLRejectsInvalidTableNames(t *testing.T) {
	invalidTables := []string{
		"",
		" users",
		"users ",
		"1users",
		"tenant.1users",
		"tenant..users",
		"tenant.users.extra",
		"users;drop",
		`users"`,
		"users-name",
		"users name",
		"users\nname",
		"usérs",
	}

	for _, table := range invalidTables {
		t.Run(table, func(t *testing.T) {
			if statements, err := BuildDBResetSQL([]string{"users", table}, DBResetOptions{
				DisableTriggers: true,
			}); err == nil {
				t.Fatalf("BuildDBResetSQL returned nil error with statements %#v", statements)
			}
		})
	}
}

func TestBuildDBResetSQLAllowsStrictIdentifiers(t *testing.T) {
	tables := []string{"_", "_events", "Users_2", "app_1._audit"}

	statements, err := BuildDBResetSQL(tables, DBResetOptions{})
	if err != nil {
		t.Fatalf("BuildDBResetSQL returned error: %v", err)
	}

	want := []string{
		`TRUNCATE TABLE "_", "_events", "Users_2", "app_1"."_audit" RESTART IDENTITY CASCADE`,
	}
	assertDBResetStatements(t, statements, want)
}

func assertDBResetStatements(t *testing.T, got, want []string) {
	t.Helper()
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("statements = %#v, want %#v", got, want)
	}
}
