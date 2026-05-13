package migrations

import (
	"errors"
	"testing"
)

func TestBuildGeneratedColumnSQLBuildsDefinition(t *testing.T) {
	score, err := BuildGeneratedColumnIdentifierSQL("score")
	if err != nil {
		t.Fatalf("BuildGeneratedColumnIdentifierSQL returned %v", err)
	}

	definition, err := BuildGeneratedColumnSQL(GeneratedColumn{
		Name:       "is_high_value",
		DataType:   "BOOLEAN",
		Expression: score + " > 80",
		NotNull:    true,
	})
	if err != nil {
		t.Fatalf("BuildGeneratedColumnSQL returned %v", err)
	}

	want := `"is_high_value" BOOLEAN GENERATED ALWAYS AS ("score" > 80) STORED NOT NULL`
	if definition != want {
		t.Fatalf("definition = %q, want %q", definition, want)
	}
}

func TestBuildAddGeneratedColumnSQLQuotesIdentifiers(t *testing.T) {
	statement, err := BuildAddGeneratedColumnSQL(AddGeneratedColumnOptions{
		Table: TableName{Schema: "crm", Name: "customer"},
		Column: GeneratedColumn{
			Column:     "normalized_email",
			Type:       "TEXT",
			Expression: "lower(email)",
		},
		IfNotExists: true,
	})
	if err != nil {
		t.Fatalf("BuildAddGeneratedColumnSQL returned %v", err)
	}

	want := `ALTER TABLE "crm"."customer" ADD COLUMN IF NOT EXISTS "normalized_email" TEXT GENERATED ALWAYS AS (lower(email)) STORED;`
	if statement != want {
		t.Fatalf("statement = %q, want %q", statement, want)
	}
}

func TestBuildDropGeneratedColumnSQLQuotesIdentifiers(t *testing.T) {
	statement, err := BuildDropGeneratedColumnSQL(DropGeneratedColumnOptions{
		Table:    TableName{Name: "select"},
		Name:     "Derived_1",
		IfExists: true,
	})
	if err != nil {
		t.Fatalf("BuildDropGeneratedColumnSQL returned %v", err)
	}

	want := `ALTER TABLE "select" DROP COLUMN IF EXISTS "Derived_1";`
	if statement != want {
		t.Fatalf("statement = %q, want %q", statement, want)
	}
}

func TestGeneratedColumnSQLAllowsDataTypeDDLSubset(t *testing.T) {
	definition, err := BuildGeneratedColumnDefinitionSQL(GeneratedColumn{
		Name:       "search_tokens",
		DataType:   "pg_catalog.tsvector",
		Expression: "to_tsvector('english'::regconfig, name)",
	})
	if err != nil {
		t.Fatalf("BuildGeneratedColumnDefinitionSQL returned %v", err)
	}

	want := `"search_tokens" pg_catalog.tsvector GENERATED ALWAYS AS (to_tsvector('english'::regconfig, name)) STORED`
	if definition != want {
		t.Fatalf("definition = %q, want %q", definition, want)
	}
}

func TestGeneratedColumnSQLRequiresDataTypeAndExpression(t *testing.T) {
	tests := []struct {
		name   string
		column GeneratedColumn
		want   error
	}{
		{
			name: "data type",
			column: GeneratedColumn{
				Name:       "derived",
				Expression: "score > 80",
			},
			want: ErrGeneratedColumnDataTypeRequired,
		},
		{
			name: "expression",
			column: GeneratedColumn{
				Name:     "derived",
				DataType: "BOOLEAN",
			},
			want: ErrGeneratedColumnExpressionRequired,
		},
		{
			name: "invalid data type",
			column: GeneratedColumn{
				Name:       "derived",
				DataType:   "BOOLEAN; DROP TABLE users",
				Expression: "score > 80",
			},
			want: ErrInvalidGeneratedColumnDataType,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if _, err := BuildGeneratedColumnSQL(tt.column); !errors.Is(err, tt.want) {
				t.Fatalf("error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestGeneratedColumnSQLRejectsInvalidIdentifiers(t *testing.T) {
	tests := []struct {
		name string
		run  func() error
	}{
		{
			name: "add table",
			run: func() error {
				_, err := BuildAddGeneratedColumnSQL(AddGeneratedColumnOptions{
					Table: TableName{Schema: "bad-schema", Name: "customer"},
					Column: GeneratedColumn{
						Name:       "derived",
						DataType:   "BOOLEAN",
						Expression: "score > 80",
					},
				})
				return err
			},
		},
		{
			name: "add column",
			run: func() error {
				_, err := BuildAddGeneratedColumnSQL(AddGeneratedColumnOptions{
					Table: TableName{Name: "customer"},
					Column: GeneratedColumn{
						Name:       "1derived",
						DataType:   "BOOLEAN",
						Expression: "score > 80",
					},
				})
				return err
			},
		},
		{
			name: "drop column",
			run: func() error {
				_, err := BuildDropGeneratedColumnSQL(DropGeneratedColumnOptions{
					Table:  TableName{Name: "customer"},
					Column: "derived;drop",
				})
				return err
			},
		},
		{
			name: "expression identifier",
			run: func() error {
				_, err := BuildGeneratedColumnIdentifierSQL("score-raw")
				return err
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := tt.run(); !errors.Is(err, ErrInvalidSQLIdentifier) {
				t.Fatalf("error = %v, want %v", err, ErrInvalidSQLIdentifier)
			}
		})
	}
}
