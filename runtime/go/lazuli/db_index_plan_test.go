package lazuli

import (
	"strings"
	"testing"
)

func TestBuildDBIndexPlanBuildsDeterministicSQL(t *testing.T) {
	opts := DBIndexPlanOptions{
		Table:       "app.customer_events",
		Columns:     []string{"org_id", "created_at"},
		Method:      "btree",
		IfNotExists: true,
	}
	plan, err := BuildDBIndexPlan(opts)
	if err != nil {
		t.Fatalf("BuildDBIndexPlan returned error: %v", err)
	}
	opts.Columns[0] = "mutated"

	if plan.Name != "idx_customer_events_org_id_created_at" {
		t.Fatalf("Name = %q, want deterministic index name", plan.Name)
	}
	wantSQL := `CREATE INDEX IF NOT EXISTS "idx_customer_events_org_id_created_at" ON "app"."customer_events" USING "btree" ("org_id", "created_at")`
	if plan.SQL != wantSQL {
		t.Fatalf("SQL = %q, want %q", plan.SQL, wantSQL)
	}
	wantDropSQL := `DROP INDEX "app"."idx_customer_events_org_id_created_at"`
	if plan.DropSQL != wantDropSQL {
		t.Fatalf("DropSQL = %q, want %q", plan.DropSQL, wantDropSQL)
	}
	if got := strings.Join(plan.Columns, ","); got != "org_id,created_at" {
		t.Fatalf("Columns = %v, want cloned input columns", plan.Columns)
	}
}

func TestBuildDBUniqueConstraintPlanBuildsSQL(t *testing.T) {
	plan, err := BuildDBUniqueConstraintPlan(DBUniqueConstraintPlanOptions{
		Table:   "users",
		Columns: []string{"org_id", "email"},
	})
	if err != nil {
		t.Fatalf("BuildDBUniqueConstraintPlan returned error: %v", err)
	}

	if plan.Kind != DBUniqueConstraint {
		t.Fatalf("Kind = %s, want %s", plan.Kind, DBUniqueConstraint)
	}
	if plan.Name != "uq_users_org_id_email" {
		t.Fatalf("Name = %q, want deterministic unique constraint name", plan.Name)
	}
	wantSQL := `ALTER TABLE "users" ADD CONSTRAINT "uq_users_org_id_email" UNIQUE ("org_id", "email")`
	if plan.SQL != wantSQL {
		t.Fatalf("SQL = %q, want %q", plan.SQL, wantSQL)
	}
	wantDropSQL := `ALTER TABLE "users" DROP CONSTRAINT "uq_users_org_id_email"`
	if plan.DropSQL != wantDropSQL {
		t.Fatalf("DropSQL = %q, want %q", plan.DropSQL, wantDropSQL)
	}
}

func TestBuildDBForeignKeyPlanBuildsSQL(t *testing.T) {
	plan, err := BuildDBForeignKeyPlan(DBForeignKeyPlanOptions{
		Table:             "app.orders",
		Columns:           []string{"customer_id"},
		ReferenceTable:    "app.customers",
		ReferenceColumns:  []string{"id"},
		OnDelete:          DBForeignKeyCascade,
		OnUpdate:          DBForeignKeyRestrict,
		Deferrable:        true,
		InitiallyDeferred: true,
	})
	if err != nil {
		t.Fatalf("BuildDBForeignKeyPlan returned error: %v", err)
	}

	if plan.Kind != DBForeignKeyConstraint {
		t.Fatalf("Kind = %s, want %s", plan.Kind, DBForeignKeyConstraint)
	}
	if plan.Name != "fk_orders_customer_id_customers_id" {
		t.Fatalf("Name = %q, want deterministic foreign key name", plan.Name)
	}
	wantSQL := `ALTER TABLE "app"."orders" ADD CONSTRAINT "fk_orders_customer_id_customers_id" FOREIGN KEY ("customer_id") REFERENCES "app"."customers" ("id") ON DELETE CASCADE ON UPDATE RESTRICT DEFERRABLE INITIALLY DEFERRED`
	if plan.SQL != wantSQL {
		t.Fatalf("SQL = %q, want %q", plan.SQL, wantSQL)
	}
	wantDropSQL := `ALTER TABLE "app"."orders" DROP CONSTRAINT "fk_orders_customer_id_customers_id"`
	if plan.DropSQL != wantDropSQL {
		t.Fatalf("DropSQL = %q, want %q", plan.DropSQL, wantDropSQL)
	}
}

func TestBuildDBCompositeKeyPlanBuildsSQL(t *testing.T) {
	plan, err := BuildDBCompositeKeyPlan(DBCompositeKeyPlanOptions{
		Table:   "invoice_lines",
		Columns: []string{"invoice_id", "line_no"},
	})
	if err != nil {
		t.Fatalf("BuildDBCompositeKeyPlan returned error: %v", err)
	}

	if plan.Kind != DBCompositeKeyConstraint {
		t.Fatalf("Kind = %s, want %s", plan.Kind, DBCompositeKeyConstraint)
	}
	if plan.Name != "pk_invoice_lines_invoice_id_line_no" {
		t.Fatalf("Name = %q, want deterministic composite key name", plan.Name)
	}
	wantSQL := `ALTER TABLE "invoice_lines" ADD CONSTRAINT "pk_invoice_lines_invoice_id_line_no" PRIMARY KEY ("invoice_id", "line_no")`
	if plan.SQL != wantSQL {
		t.Fatalf("SQL = %q, want %q", plan.SQL, wantSQL)
	}
}

func TestDBIndexPlanGeneratedNamesStayDeterministicUnderPostgresLimit(t *testing.T) {
	opts := DBIndexPlanOptions{
		Table: "customer_subscription_audit_events",
		Columns: []string{
			"tenant_organization_identifier",
			"external_subscription_reference",
			"effective_timestamp_microseconds",
		},
	}

	first, err := BuildDBIndexPlan(opts)
	if err != nil {
		t.Fatalf("BuildDBIndexPlan first returned error: %v", err)
	}
	second, err := BuildDBIndexPlan(opts)
	if err != nil {
		t.Fatalf("BuildDBIndexPlan second returned error: %v", err)
	}
	opts.Columns[2] = "different_timestamp_microseconds"
	changed, err := BuildDBIndexPlan(opts)
	if err != nil {
		t.Fatalf("BuildDBIndexPlan changed returned error: %v", err)
	}

	if first.Name != second.Name {
		t.Fatalf("generated names differ: %q != %q", first.Name, second.Name)
	}
	if len(first.Name) > maxDBIndexPlanIdentifierLength {
		t.Fatalf("Name length = %d, want <= %d", len(first.Name), maxDBIndexPlanIdentifierLength)
	}
	if first.Name == changed.Name {
		t.Fatalf("Name = %q after input change, want distinct deterministic suffix", first.Name)
	}
}

func TestDBIndexPlanHonorsExplicitNames(t *testing.T) {
	indexPlan, err := BuildDBIndexPlan(DBIndexPlanOptions{
		Name:    "custom_idx_name",
		Table:   "users",
		Columns: []string{"email"},
	})
	if err != nil {
		t.Fatalf("BuildDBIndexPlan returned error: %v", err)
	}
	if indexPlan.Name != "custom_idx_name" || !strings.Contains(indexPlan.SQL, `"custom_idx_name"`) {
		t.Fatalf("index explicit name not used in plan: %#v", indexPlan)
	}

	constraintPlan, err := BuildDBUniqueConstraintPlan(DBUniqueConstraintPlanOptions{
		Name:    "custom_unique_name",
		Table:   "users",
		Columns: []string{"email"},
	})
	if err != nil {
		t.Fatalf("BuildDBUniqueConstraintPlan returned error: %v", err)
	}
	if constraintPlan.Name != "custom_unique_name" || !strings.Contains(constraintPlan.SQL, `"custom_unique_name"`) {
		t.Fatalf("constraint explicit name not used in plan: %#v", constraintPlan)
	}
}

func TestDBIndexPlanBuildersRejectInvalidInput(t *testing.T) {
	tests := []struct {
		name  string
		build func() error
	}{
		{
			name: "index empty table",
			build: func() error {
				_, err := BuildDBIndexPlan(DBIndexPlanOptions{Columns: []string{"id"}})
				return err
			},
		},
		{
			name: "index invalid table",
			build: func() error {
				_, err := BuildDBIndexPlan(DBIndexPlanOptions{Table: "app.users.extra", Columns: []string{"id"}})
				return err
			},
		},
		{
			name: "index no columns",
			build: func() error {
				_, err := BuildDBIndexPlan(DBIndexPlanOptions{Table: "users"})
				return err
			},
		},
		{
			name: "index invalid column",
			build: func() error {
				_, err := BuildDBIndexPlan(DBIndexPlanOptions{Table: "users", Columns: []string{"email-address"}})
				return err
			},
		},
		{
			name: "index invalid method",
			build: func() error {
				_, err := BuildDBIndexPlan(DBIndexPlanOptions{Table: "users", Columns: []string{"email"}, Method: "bad method"})
				return err
			},
		},
		{
			name: "explicit name too long",
			build: func() error {
				_, err := BuildDBIndexPlan(DBIndexPlanOptions{Name: strings.Repeat("a", 64), Table: "users", Columns: []string{"email"}})
				return err
			},
		},
		{
			name: "unique empty columns",
			build: func() error {
				_, err := BuildDBUniqueConstraintPlan(DBUniqueConstraintPlanOptions{Table: "users"})
				return err
			},
		},
		{
			name: "foreign key missing reference table",
			build: func() error {
				_, err := BuildDBForeignKeyPlan(DBForeignKeyPlanOptions{
					Table:            "orders",
					Columns:          []string{"customer_id"},
					ReferenceColumns: []string{"id"},
				})
				return err
			},
		},
		{
			name: "foreign key mismatched columns",
			build: func() error {
				_, err := BuildDBForeignKeyPlan(DBForeignKeyPlanOptions{
					Table:            "orders",
					Columns:          []string{"customer_id", "tenant_id"},
					ReferenceTable:   "customers",
					ReferenceColumns: []string{"id"},
				})
				return err
			},
		},
		{
			name: "foreign key invalid action",
			build: func() error {
				_, err := BuildDBForeignKeyPlan(DBForeignKeyPlanOptions{
					Table:            "orders",
					Columns:          []string{"customer_id"},
					ReferenceTable:   "customers",
					ReferenceColumns: []string{"id"},
					OnDelete:         DBForeignKeyAction("DROP"),
				})
				return err
			},
		},
		{
			name: "foreign key initially deferred without deferrable",
			build: func() error {
				_, err := BuildDBForeignKeyPlan(DBForeignKeyPlanOptions{
					Table:             "orders",
					Columns:           []string{"customer_id"},
					ReferenceTable:    "customers",
					ReferenceColumns:  []string{"id"},
					InitiallyDeferred: true,
				})
				return err
			},
		},
		{
			name: "composite key one column",
			build: func() error {
				_, err := BuildDBCompositeKeyPlan(DBCompositeKeyPlanOptions{Table: "users", Columns: []string{"id"}})
				return err
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := tt.build(); err == nil {
				t.Fatal("builder returned nil error")
			}
		})
	}
}
