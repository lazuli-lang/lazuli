package migrations

import (
	"errors"
	"testing"
)

func TestBuildTruncatePlanReversesTableOrder(t *testing.T) {
	statements, err := BuildTruncatePlan(TruncatePlanOptions{
		Tables: []TableName{
			{Schema: "app", Name: "accounts"},
			{Schema: "app", Name: "orders"},
			{Name: "audit_log"},
		},
	})
	if err != nil {
		t.Fatalf("BuildTruncatePlan returned %v", err)
	}

	want := []string{
		`TRUNCATE TABLE "audit_log";`,
		`TRUNCATE TABLE "app"."orders";`,
		`TRUNCATE TABLE "app"."accounts";`,
	}
	if !equal(statements, want) {
		t.Fatalf("statements = %v, want %v", statements, want)
	}
}

func TestBuildDropPlanDropsTablesBeforeSchemasWithOptions(t *testing.T) {
	statements, err := BuildDropPlan(DropPlanOptions{
		Schemas:  []string{"app", "audit"},
		Tables:   []TableName{{Schema: "app", Name: "accounts"}, {Schema: "app", Name: "orders"}},
		IfExists: true,
		Behavior: DropBehaviorCascade,
	})
	if err != nil {
		t.Fatalf("BuildDropPlan returned %v", err)
	}

	want := []string{
		`DROP TABLE IF EXISTS "app"."orders" CASCADE;`,
		`DROP TABLE IF EXISTS "app"."accounts" CASCADE;`,
		`DROP SCHEMA IF EXISTS "audit" CASCADE;`,
		`DROP SCHEMA IF EXISTS "app" CASCADE;`,
	}
	if !equal(statements, want) {
		t.Fatalf("statements = %v, want %v", statements, want)
	}
}

func TestBuildCreatePlanCreatesSchemasInOrder(t *testing.T) {
	statements, err := BuildCreatePlan(CreatePlanOptions{
		Schemas:     []string{"app", "audit"},
		IfNotExists: true,
	})
	if err != nil {
		t.Fatalf("BuildCreatePlan returned %v", err)
	}

	want := []string{
		`CREATE SCHEMA IF NOT EXISTS "app";`,
		`CREATE SCHEMA IF NOT EXISTS "audit";`,
	}
	if !equal(statements, want) {
		t.Fatalf("statements = %v, want %v", statements, want)
	}
}

func TestBuildResetPlanModes(t *testing.T) {
	tests := []struct {
		name string
		opts ResetPlanOptions
		want []string
	}{
		{
			name: "truncate",
			opts: ResetPlanOptions{
				Mode:   ResetModeTruncate,
				Tables: []TableName{{Name: "parents"}, {Name: "children"}},
			},
			want: []string{
				`TRUNCATE TABLE "children";`,
				`TRUNCATE TABLE "parents";`,
			},
		},
		{
			name: "drop",
			opts: ResetPlanOptions{
				Mode:         ResetModeDrop,
				Schemas:      []string{"app"},
				Tables:       []TableName{{Schema: "app", Name: "parents"}, {Schema: "app", Name: "children"}},
				DropIfExists: true,
			},
			want: []string{
				`DROP TABLE IF EXISTS "app"."children";`,
				`DROP TABLE IF EXISTS "app"."parents";`,
				`DROP SCHEMA IF EXISTS "app";`,
			},
		},
		{
			name: "drop create",
			opts: ResetPlanOptions{
				Mode:              ResetModeDropCreate,
				Schemas:           []string{"app"},
				Tables:            []TableName{{Schema: "app", Name: "parents"}, {Schema: "app", Name: "children"}},
				DropIfExists:      true,
				CreateIfNotExists: true,
				DropBehavior:      DropBehaviorRestrict,
			},
			want: []string{
				`DROP TABLE IF EXISTS "app"."children" RESTRICT;`,
				`DROP TABLE IF EXISTS "app"."parents" RESTRICT;`,
				`DROP SCHEMA IF EXISTS "app" RESTRICT;`,
				`CREATE SCHEMA IF NOT EXISTS "app";`,
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			statements, err := BuildResetPlan(tt.opts)
			if err != nil {
				t.Fatalf("BuildResetPlan returned %v", err)
			}
			if !equal(statements, tt.want) {
				t.Fatalf("statements = %v, want %v", statements, tt.want)
			}
		})
	}
}

func TestResetPlanBuildersRejectInvalidInput(t *testing.T) {
	tests := []struct {
		name string
		err  error
		run  func() error
	}{
		{
			name: "table name",
			err:  ErrInvalidSQLIdentifier,
			run: func() error {
				_, err := BuildTruncatePlan(TruncatePlanOptions{Tables: []TableName{{Name: "1users"}}})
				return err
			},
		},
		{
			name: "table schema",
			err:  ErrInvalidSQLIdentifier,
			run: func() error {
				_, err := BuildDropPlan(DropPlanOptions{Tables: []TableName{{Schema: "bad-schema", Name: "users"}}})
				return err
			},
		},
		{
			name: "schema name",
			err:  ErrInvalidSQLIdentifier,
			run: func() error {
				_, err := BuildCreatePlan(CreatePlanOptions{Schemas: []string{"public.users"}})
				return err
			},
		},
		{
			name: "drop behavior",
			err:  ErrInvalidDropBehavior,
			run: func() error {
				_, err := BuildDropPlan(DropPlanOptions{Behavior: DropBehavior("purge")})
				return err
			},
		},
		{
			name: "reset mode",
			err:  ErrInvalidResetMode,
			run: func() error {
				_, err := BuildResetPlan(ResetPlanOptions{Mode: ResetMode("replace")})
				return err
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := tt.run(); !errors.Is(err, tt.err) {
				t.Fatalf("error = %v, want %v", err, tt.err)
			}
		})
	}
}
