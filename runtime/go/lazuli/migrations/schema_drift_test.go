package migrations

import (
	"errors"
	"reflect"
	"testing"
)

func TestDiffSchemaDriftReportsDeterministicIssues(t *testing.T) {
	expected := SchemaSnapshot{
		Tables: []SchemaTable{
			{
				Name: TableName{Schema: "public", Name: "users"},
				Columns: []SchemaColumn{
					{Name: "email", Type: "text"},
					{Name: "id", Type: "bigint"},
					{Name: "created_at", Type: "timestamptz", Default: "now()"},
				},
				Indexes: []SchemaIndex{
					{Name: "users_email_idx", Columns: []string{"email"}, Unique: true, Method: "btree"},
				},
				Constraints: []SchemaConstraint{
					{Name: "users_org_fk", Type: SchemaConstraintForeignKey, Columns: []string{"org_id"}, ReferencedTable: TableName{Schema: "public", Name: "organizations"}, ReferencedColumns: []string{"id"}},
					{Name: "users_email_key", Type: SchemaConstraintUnique, Columns: []string{"email"}},
					{Name: "users_pkey", Type: SchemaConstraintPrimaryKey, Columns: []string{"id"}},
				},
			},
			{Name: TableName{Schema: "public", Name: "accounts"}},
		},
	}
	observed := SchemaSnapshot{
		Tables: []SchemaTable{
			{Name: TableName{Schema: "public", Name: "audit_events"}},
			{
				Name: TableName{Schema: "public", Name: "users"},
				Columns: []SchemaColumn{
					{Name: "name", Type: "text", Nullable: true},
					{Name: "id", Type: "bigint"},
					{Name: "email", Type: "varchar(255)", Nullable: true},
				},
				Indexes: []SchemaIndex{
					{Name: "users_name_idx", Columns: []string{"name"}},
					{Name: "users_email_idx", Columns: []string{"id", "email"}, Method: "hash", Predicate: "deleted_at IS NULL"},
				},
				Constraints: []SchemaConstraint{
					{Name: "users_name_check", Type: SchemaConstraintCheck, Expression: "name <> ''"},
					{Name: "users_pkey", Type: SchemaConstraintPrimaryKey, Columns: []string{"id"}},
					{Name: "users_org_fk", Type: SchemaConstraintForeignKey, Columns: []string{"organization_id"}, ReferencedTable: TableName{Schema: "identity", Name: "organizations"}, ReferencedColumns: []string{"uuid"}},
				},
			},
		},
	}

	got, err := DiffSchemaDrift(expected, observed)
	if err != nil {
		t.Fatalf("DiffSchemaDrift returned %v", err)
	}

	want := SchemaDriftReport{
		Issues: []SchemaDriftIssue{
			{
				Kind:   SchemaDriftMissingTable,
				Object: SchemaDriftObjectTable,
				Path:   "public.accounts",
				Table:  TableName{Schema: "public", Name: "accounts"},
				Name:   "public.accounts",
			},
			{
				Kind:   SchemaDriftUnexpectedTable,
				Object: SchemaDriftObjectTable,
				Path:   "public.audit_events",
				Table:  TableName{Schema: "public", Name: "audit_events"},
				Name:   "public.audit_events",
			},
			{
				Kind:   SchemaDriftMissingColumn,
				Object: SchemaDriftObjectColumn,
				Path:   "public.users.columns.created_at",
				Table:  TableName{Schema: "public", Name: "users"},
				Name:   "created_at",
			},
			{
				Kind:   SchemaDriftChangedColumn,
				Object: SchemaDriftObjectColumn,
				Path:   "public.users.columns.email",
				Table:  TableName{Schema: "public", Name: "users"},
				Name:   "email",
				Fields: []SchemaDriftFieldChange{
					{Field: "type", Expected: "text", Observed: "varchar(255)"},
					{Field: "nullable", Expected: "false", Observed: "true"},
				},
			},
			{
				Kind:   SchemaDriftUnexpectedColumn,
				Object: SchemaDriftObjectColumn,
				Path:   "public.users.columns.name",
				Table:  TableName{Schema: "public", Name: "users"},
				Name:   "name",
			},
			{
				Kind:   SchemaDriftMissingConstraint,
				Object: SchemaDriftObjectConstraint,
				Path:   "public.users.constraints.users_email_key",
				Table:  TableName{Schema: "public", Name: "users"},
				Name:   "users_email_key",
			},
			{
				Kind:   SchemaDriftUnexpectedConstraint,
				Object: SchemaDriftObjectConstraint,
				Path:   "public.users.constraints.users_name_check",
				Table:  TableName{Schema: "public", Name: "users"},
				Name:   "users_name_check",
			},
			{
				Kind:   SchemaDriftChangedConstraint,
				Object: SchemaDriftObjectConstraint,
				Path:   "public.users.constraints.users_org_fk",
				Table:  TableName{Schema: "public", Name: "users"},
				Name:   "users_org_fk",
				Fields: []SchemaDriftFieldChange{
					{Field: "columns", Expected: "org_id", Observed: "organization_id"},
					{Field: "referenced_table", Expected: "public.organizations", Observed: "identity.organizations"},
					{Field: "referenced_columns", Expected: "id", Observed: "uuid"},
				},
			},
			{
				Kind:   SchemaDriftChangedIndex,
				Object: SchemaDriftObjectIndex,
				Path:   "public.users.indexes.users_email_idx",
				Table:  TableName{Schema: "public", Name: "users"},
				Name:   "users_email_idx",
				Fields: []SchemaDriftFieldChange{
					{Field: "unique", Expected: "true", Observed: "false"},
					{Field: "method", Expected: "btree", Observed: "hash"},
					{Field: "columns", Expected: "email", Observed: "id, email"},
					{Field: "predicate", Expected: "", Observed: "deleted_at IS NULL"},
				},
			},
			{
				Kind:   SchemaDriftUnexpectedIndex,
				Object: SchemaDriftObjectIndex,
				Path:   "public.users.indexes.users_name_idx",
				Table:  TableName{Schema: "public", Name: "users"},
				Name:   "users_name_idx",
			},
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("DiffSchemaDrift() = %#v, want %#v", got, want)
	}
	if !got.HasDrift() {
		t.Fatal("DiffSchemaDrift() HasDrift() = false, want true")
	}
}

func TestDiffSchemaDriftNormalizesWithoutMutatingInput(t *testing.T) {
	expected := SchemaSnapshot{
		Tables: []SchemaTable{
			{
				Name: TableName{Schema: " public ", Name: " users "},
				Columns: []SchemaColumn{
					{Name: " email ", Type: " text ", Nullable: true, Default: " lower(email) "},
				},
				Indexes: []SchemaIndex{
					{Name: " users_email_idx ", Columns: []string{" email "}, Unique: true, Method: " btree "},
				},
				Constraints: []SchemaConstraint{
					{Name: " users_email_key ", Type: SchemaConstraintUnique, Columns: []string{" email "}},
				},
			},
		},
	}
	observed := SchemaSnapshot{
		Tables: []SchemaTable{
			{
				Name: TableName{Schema: "public", Name: "users"},
				Columns: []SchemaColumn{
					{Name: "email", Type: "text", Nullable: true, Default: "lower(email)"},
				},
				Indexes: []SchemaIndex{
					{Name: "users_email_idx", Columns: []string{"email"}, Unique: true, Method: "btree"},
				},
				Constraints: []SchemaConstraint{
					{Name: "users_email_key", Type: SchemaConstraintUnique, Columns: []string{"email"}},
				},
			},
		},
	}

	got, err := DiffSchemaDrift(expected, observed)
	if err != nil {
		t.Fatalf("DiffSchemaDrift returned %v", err)
	}
	if got.HasDrift() {
		t.Fatalf("DiffSchemaDrift() HasDrift() = true, issues = %#v", got.Issues)
	}
	if expected.Tables[0].Name.Name != " users " || expected.Tables[0].Columns[0].Name != " email " {
		t.Fatalf("DiffSchemaDrift() mutated expected input: %#v", expected.Tables[0])
	}
}

func TestDiffSchemaDriftValidatesNamesAndDuplicates(t *testing.T) {
	tests := []struct {
		name     string
		snapshot SchemaSnapshot
		want     error
	}{
		{
			name: "missing table name",
			snapshot: SchemaSnapshot{
				Tables: []SchemaTable{{Name: TableName{Schema: "public"}}},
			},
			want: ErrSchemaDriftNameRequired,
		},
		{
			name: "duplicate table",
			snapshot: SchemaSnapshot{
				Tables: []SchemaTable{
					{Name: TableName{Schema: " public ", Name: "users"}},
					{Name: TableName{Schema: "public", Name: " users "}},
				},
			},
			want: ErrDuplicateSchemaDriftObject,
		},
		{
			name: "missing column name",
			snapshot: SchemaSnapshot{
				Tables: []SchemaTable{
					{Name: TableName{Name: "users"}, Columns: []SchemaColumn{{Type: "bigint"}}},
				},
			},
			want: ErrSchemaDriftNameRequired,
		},
		{
			name: "duplicate scoped object",
			snapshot: SchemaSnapshot{
				Tables: []SchemaTable{
					{
						Name: TableName{Name: "users"},
						Indexes: []SchemaIndex{
							{Name: " users_email_idx ", Columns: []string{"email"}},
							{Name: "users_email_idx", Columns: []string{"email"}},
						},
					},
				},
			},
			want: ErrDuplicateSchemaDriftObject,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := DiffSchemaDrift(tt.snapshot, SchemaSnapshot{})
			if !errors.Is(err, tt.want) {
				t.Fatalf("DiffSchemaDrift error = %v, want %v", err, tt.want)
			}
		})
	}
}
