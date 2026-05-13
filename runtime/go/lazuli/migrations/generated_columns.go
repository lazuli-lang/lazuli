package migrations

import (
	"errors"
	"fmt"
	"strings"
)

var (
	// ErrGeneratedColumnDataTypeRequired is returned when a generated column
	// definition omits its PostgreSQL data type.
	ErrGeneratedColumnDataTypeRequired = errors.New("migrations: generated column data type required")
	// ErrGeneratedColumnExpressionRequired is returned when a generated column
	// definition omits its GENERATED ALWAYS AS expression.
	ErrGeneratedColumnExpressionRequired = errors.New("migrations: generated column expression required")
	// ErrInvalidGeneratedColumnDataType is returned when a generated column data
	// type contains characters outside Lazuli's generated DDL subset.
	ErrInvalidGeneratedColumnDataType = errors.New("migrations: invalid generated column data type")
)

// GeneratedColumn describes one PostgreSQL GENERATED ALWAYS AS column
// definition. Expression is inserted as SQL after trimming; use
// BuildGeneratedColumnIdentifierSQL when composing expression identifiers from
// generated names.
type GeneratedColumn struct {
	// Name is the generated column identifier.
	Name string
	// Column is accepted as an alias for Name.
	Column string
	// DataType is the PostgreSQL data type emitted before GENERATED.
	DataType string
	// Type is accepted as an alias for DataType.
	Type string
	// Expression is the PostgreSQL generation expression, without the wrapping
	// GENERATED ALWAYS AS parentheses.
	Expression string
	// NotNull appends NOT NULL to the generated column definition.
	NotNull bool
}

// AddGeneratedColumnOptions configures BuildAddGeneratedColumnSQL.
type AddGeneratedColumnOptions struct {
	// Table is the schema-qualified or unqualified table to alter.
	Table TableName
	// Column is the generated column definition to add.
	Column GeneratedColumn
	// IfNotExists adds IF NOT EXISTS to the ADD COLUMN clause.
	IfNotExists bool
}

// DropGeneratedColumnOptions configures BuildDropGeneratedColumnSQL.
type DropGeneratedColumnOptions struct {
	// Table is the schema-qualified or unqualified table to alter.
	Table TableName
	// Column is the generated column identifier to drop.
	Column string
	// Name is accepted as an alias for Column.
	Name string
	// IfExists adds IF EXISTS to the DROP COLUMN clause.
	IfExists bool
}

// BuildGeneratedColumnSQL returns a PostgreSQL generated column definition
// fragment. It only builds SQL; callers remain responsible for embedding the
// returned fragment in CREATE TABLE or ALTER TABLE DDL.
func BuildGeneratedColumnSQL(column GeneratedColumn) (string, error) {
	name, err := quoteSQLIdentifier("generated column name", column.columnName())
	if err != nil {
		return "", err
	}

	dataType, err := generatedColumnDataTypeSQL(column.dataType())
	if err != nil {
		return "", err
	}

	expression := strings.TrimSpace(column.Expression)
	if expression == "" {
		return "", ErrGeneratedColumnExpressionRequired
	}

	definition := name + " " + dataType + " GENERATED ALWAYS AS (" + expression + ") STORED"
	if column.NotNull {
		definition += " NOT NULL"
	}
	return definition, nil
}

// BuildGeneratedColumnDefinitionSQL is an explicit alias for
// BuildGeneratedColumnSQL for callers that need to distinguish a column
// definition fragment from a full DDL statement.
func BuildGeneratedColumnDefinitionSQL(column GeneratedColumn) (string, error) {
	return BuildGeneratedColumnSQL(column)
}

// BuildAddGeneratedColumnSQL returns an ALTER TABLE ADD COLUMN statement for a
// PostgreSQL generated column.
func BuildAddGeneratedColumnSQL(opts AddGeneratedColumnOptions) (string, error) {
	table, err := quoteTableName(opts.Table)
	if err != nil {
		return "", err
	}
	column, err := BuildGeneratedColumnSQL(opts.Column)
	if err != nil {
		return "", err
	}

	statement := "ALTER TABLE " + table + " ADD COLUMN "
	if opts.IfNotExists {
		statement += "IF NOT EXISTS "
	}
	return statement + column + ";", nil
}

// BuildDropGeneratedColumnSQL returns an ALTER TABLE DROP COLUMN statement for
// a generated column.
func BuildDropGeneratedColumnSQL(opts DropGeneratedColumnOptions) (string, error) {
	table, err := quoteTableName(opts.Table)
	if err != nil {
		return "", err
	}
	column, err := quoteSQLIdentifier("generated column name", opts.columnName())
	if err != nil {
		return "", err
	}

	statement := "ALTER TABLE " + table + " DROP COLUMN "
	if opts.IfExists {
		statement += "IF EXISTS "
	}
	return statement + column + ";", nil
}

// BuildGeneratedColumnIdentifierSQL validates and quotes one SQL identifier
// using the same strict generated identifier rules as the migration DDL
// helpers. It is useful when composing GeneratedColumn.Expression strings.
func BuildGeneratedColumnIdentifierSQL(name string) (string, error) {
	return quoteSQLIdentifier("generated column expression identifier", name)
}

func (column GeneratedColumn) columnName() string {
	if column.Name != "" {
		return column.Name
	}
	return column.Column
}

func (column GeneratedColumn) dataType() string {
	if column.DataType != "" {
		return column.DataType
	}
	return column.Type
}

func (opts DropGeneratedColumnOptions) columnName() string {
	if opts.Column != "" {
		return opts.Column
	}
	return opts.Name
}

func generatedColumnDataTypeSQL(dataType string) (string, error) {
	dataType = strings.TrimSpace(dataType)
	if dataType == "" {
		return "", ErrGeneratedColumnDataTypeRequired
	}
	if !validGeneratedColumnDataType(dataType) {
		return "", fmt.Errorf("%w %q", ErrInvalidGeneratedColumnDataType, dataType)
	}
	return dataType, nil
}

func validGeneratedColumnDataType(dataType string) bool {
	for i := 0; i < len(dataType); i++ {
		switch c := dataType[i]; {
		case isSQLIdentifierLetter(c),
			isSQLIdentifierDigit(c),
			c == '_',
			c == ' ',
			c == '(',
			c == ')',
			c == ',',
			c == '.',
			c == '[',
			c == ']':
			continue
		default:
			return false
		}
	}
	return true
}
