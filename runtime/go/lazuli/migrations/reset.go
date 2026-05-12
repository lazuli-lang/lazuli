package migrations

import (
	"errors"
	"fmt"
)

var (
	// ErrInvalidSQLIdentifier is returned when reset/truncate helpers receive
	// an identifier outside Lazuli's safe ASCII SQL identifier subset.
	ErrInvalidSQLIdentifier = errors.New("migrations: invalid SQL identifier")
	// ErrInvalidResetMode is returned when BuildResetPlan receives an unknown
	// reset mode.
	ErrInvalidResetMode = errors.New("migrations: invalid reset mode")
	// ErrInvalidDropBehavior is returned when a drop plan asks for an unknown
	// DROP behavior.
	ErrInvalidDropBehavior = errors.New("migrations: invalid drop behavior")
)

// TableName identifies a table for reset/truncate planning. Schema is
// optional; when set, both Schema and Name are validated and quoted.
type TableName struct {
	// Schema is the optional schema qualifier.
	Schema string
	// Name is the table name.
	Name string
}

// DropBehavior controls the optional dependency clause appended to DROP
// statements. The zero value omits the clause and leaves provider defaults in
// effect.
type DropBehavior string

const (
	// DropBehaviorDefault leaves DROP behavior unspecified.
	DropBehaviorDefault DropBehavior = ""
	// DropBehaviorRestrict appends RESTRICT to DROP statements.
	DropBehaviorRestrict DropBehavior = "restrict"
	// DropBehaviorCascade appends CASCADE to DROP statements.
	DropBehaviorCascade DropBehavior = "cascade"
)

// ResetMode selects the destructive strategy used by BuildResetPlan.
type ResetMode string

const (
	// ResetModeTruncate emits TRUNCATE TABLE statements for Tables.
	ResetModeTruncate ResetMode = "truncate"
	// ResetModeDrop emits DROP statements for Tables and Schemas.
	ResetModeDrop ResetMode = "drop"
	// ResetModeDropCreate emits DROP statements, then CREATE SCHEMA statements.
	// Table creation is intentionally omitted because provider-neutral table
	// creation requires column definitions owned by generated migrations.
	ResetModeDropCreate ResetMode = "drop_create"
)

// TruncatePlanOptions configures BuildTruncatePlan.
type TruncatePlanOptions struct {
	// Tables are expected in creation/dependency order. BuildTruncatePlan emits
	// them in reverse order so dependent tables listed after parents are
	// cleared first where provider constraints allow it.
	Tables []TableName
}

// DropPlanOptions configures BuildDropPlan.
type DropPlanOptions struct {
	// Schemas are expected in creation order. BuildDropPlan emits them in
	// reverse order after table drops.
	Schemas []string
	// Tables are expected in creation/dependency order. BuildDropPlan emits
	// them in reverse order before schema drops.
	Tables []TableName
	// IfExists adds IF EXISTS to each DROP statement.
	IfExists bool
	// Behavior optionally appends RESTRICT or CASCADE.
	Behavior DropBehavior
}

// CreatePlanOptions configures BuildCreatePlan.
type CreatePlanOptions struct {
	// Schemas are emitted in the given order.
	Schemas []string
	// IfNotExists adds IF NOT EXISTS to each CREATE SCHEMA statement.
	IfNotExists bool
}

// ResetPlanOptions configures BuildResetPlan.
type ResetPlanOptions struct {
	// Mode selects truncate, drop, or drop/create behavior.
	Mode ResetMode
	// Schemas are used by drop and drop/create modes.
	Schemas []string
	// Tables are used by truncate, drop, and drop/create modes.
	Tables []TableName
	// DropIfExists adds IF EXISTS to DROP statements.
	DropIfExists bool
	// CreateIfNotExists adds IF NOT EXISTS to CREATE SCHEMA statements.
	CreateIfNotExists bool
	// DropBehavior optionally appends RESTRICT or CASCADE to DROP statements.
	DropBehavior DropBehavior
}

// BuildTruncatePlan returns provider-neutral TRUNCATE statements for tables.
// It only builds SQL; callers remain responsible for choosing the connection,
// transaction, and execution policy.
func BuildTruncatePlan(opts TruncatePlanOptions) ([]string, error) {
	tables, err := quoteTableNames(opts.Tables)
	if err != nil {
		return nil, err
	}

	statements := make([]string, 0, len(tables))
	for i := len(tables) - 1; i >= 0; i-- {
		statements = append(statements, "TRUNCATE TABLE "+tables[i]+";")
	}
	return statements, nil
}

// BuildDropPlan returns provider-neutral DROP TABLE and DROP SCHEMA statements.
// Tables are emitted before schemas, and both collections are emitted in
// reverse order to support callers that pass creation/dependency order.
func BuildDropPlan(opts DropPlanOptions) ([]string, error) {
	behavior, err := dropBehaviorSQL(opts.Behavior)
	if err != nil {
		return nil, err
	}
	tables, err := quoteTableNames(opts.Tables)
	if err != nil {
		return nil, err
	}
	schemas, err := quoteSchemaNames(opts.Schemas)
	if err != nil {
		return nil, err
	}

	statements := make([]string, 0, len(tables)+len(schemas))
	for i := len(tables) - 1; i >= 0; i-- {
		statement := "DROP TABLE "
		if opts.IfExists {
			statement += "IF EXISTS "
		}
		statements = append(statements, statement+tables[i]+behavior+";")
	}
	for i := len(schemas) - 1; i >= 0; i-- {
		statement := "DROP SCHEMA "
		if opts.IfExists {
			statement += "IF EXISTS "
		}
		statements = append(statements, statement+schemas[i]+behavior+";")
	}
	return statements, nil
}

// BuildCreatePlan returns provider-neutral CREATE SCHEMA statements. It does
// not emit CREATE TABLE statements because table DDL requires column
// definitions and constraints owned by generated migrations.
func BuildCreatePlan(opts CreatePlanOptions) ([]string, error) {
	schemas, err := quoteSchemaNames(opts.Schemas)
	if err != nil {
		return nil, err
	}

	statements := make([]string, 0, len(schemas))
	for _, schema := range schemas {
		statement := "CREATE SCHEMA "
		if opts.IfNotExists {
			statement += "IF NOT EXISTS "
		}
		statements = append(statements, statement+schema+";")
	}
	return statements, nil
}

// BuildResetPlan returns a reset plan using the selected ResetMode. It composes
// the truncate, drop, and create builders without executing any SQL.
func BuildResetPlan(opts ResetPlanOptions) ([]string, error) {
	switch opts.Mode {
	case ResetModeTruncate:
		return BuildTruncatePlan(TruncatePlanOptions{Tables: opts.Tables})
	case ResetModeDrop:
		return BuildDropPlan(DropPlanOptions{
			Schemas:  opts.Schemas,
			Tables:   opts.Tables,
			IfExists: opts.DropIfExists,
			Behavior: opts.DropBehavior,
		})
	case ResetModeDropCreate:
		drop, err := BuildDropPlan(DropPlanOptions{
			Schemas:  opts.Schemas,
			Tables:   opts.Tables,
			IfExists: opts.DropIfExists,
			Behavior: opts.DropBehavior,
		})
		if err != nil {
			return nil, err
		}
		create, err := BuildCreatePlan(CreatePlanOptions{
			Schemas:     opts.Schemas,
			IfNotExists: opts.CreateIfNotExists,
		})
		if err != nil {
			return nil, err
		}
		return append(drop, create...), nil
	default:
		return nil, fmt.Errorf("%w %q", ErrInvalidResetMode, opts.Mode)
	}
}

func quoteTableNames(tables []TableName) ([]string, error) {
	quoted := make([]string, len(tables))
	for i, table := range tables {
		name, err := quoteTableName(table)
		if err != nil {
			return nil, fmt.Errorf("migrations: table %d: %w", i, err)
		}
		quoted[i] = name
	}
	return quoted, nil
}

func quoteTableName(table TableName) (string, error) {
	name, err := quoteSQLIdentifier("table name", table.Name)
	if err != nil {
		return "", err
	}
	if table.Schema == "" {
		return name, nil
	}

	schema, err := quoteSQLIdentifier("table schema", table.Schema)
	if err != nil {
		return "", err
	}
	return schema + "." + name, nil
}

func quoteSchemaNames(schemas []string) ([]string, error) {
	quoted := make([]string, len(schemas))
	for i, schema := range schemas {
		name, err := quoteSQLIdentifier("schema name", schema)
		if err != nil {
			return nil, fmt.Errorf("migrations: schema %d: %w", i, err)
		}
		quoted[i] = name
	}
	return quoted, nil
}

func quoteSQLIdentifier(kind, name string) (string, error) {
	if !validSQLIdentifier(name) {
		return "", fmt.Errorf("%w: %s %q", ErrInvalidSQLIdentifier, kind, name)
	}
	return `"` + name + `"`, nil
}

func validSQLIdentifier(name string) bool {
	if name == "" {
		return false
	}
	for i := 0; i < len(name); i++ {
		c := name[i]
		if i == 0 {
			if !isSQLIdentifierLetter(c) && c != '_' {
				return false
			}
			continue
		}
		if !isSQLIdentifierLetter(c) && !isSQLIdentifierDigit(c) && c != '_' {
			return false
		}
	}
	return true
}

func isSQLIdentifierLetter(c byte) bool {
	return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')
}

func isSQLIdentifierDigit(c byte) bool {
	return c >= '0' && c <= '9'
}

func dropBehaviorSQL(behavior DropBehavior) (string, error) {
	switch behavior {
	case DropBehaviorDefault:
		return "", nil
	case DropBehaviorRestrict:
		return " RESTRICT", nil
	case DropBehaviorCascade:
		return " CASCADE", nil
	default:
		return "", fmt.Errorf("%w %q", ErrInvalidDropBehavior, behavior)
	}
}
