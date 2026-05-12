package migrations

import (
	"errors"
	"strings"
)

var (
	// ErrNoTruncateTables is returned when a database lifecycle truncate
	// statement has no table targets.
	ErrNoTruncateTables = errors.New("migrations: truncate requires at least one table")
)

// DropDatabaseOptions configures BuildDropDatabaseSQL.
type DropDatabaseOptions struct {
	// IfExists adds IF EXISTS to the DROP DATABASE statement.
	IfExists bool
}

// TruncateTablesOptions configures BuildTruncateTablesSQL.
type TruncateTablesOptions struct {
	// Tables are schema-qualified or unqualified tables to truncate.
	Tables []TableName
	// RestartIdentity appends RESTART IDENTITY.
	RestartIdentity bool
	// Cascade appends CASCADE.
	Cascade bool
}

// BuildCreateDatabaseSQL returns a CREATE DATABASE statement for generated
// dev/test commands. It only builds SQL; callers remain responsible for
// choosing the connection and execution policy.
func BuildCreateDatabaseSQL(name string) (string, error) {
	database, err := quoteSQLIdentifier("database name", name)
	if err != nil {
		return "", err
	}
	return "CREATE DATABASE " + database + ";", nil
}

// BuildDropDatabaseSQL returns a DROP DATABASE statement for generated dev/test
// commands. It only builds SQL and never opens a database connection.
func BuildDropDatabaseSQL(name string, opts DropDatabaseOptions) (string, error) {
	database, err := quoteSQLIdentifier("database name", name)
	if err != nil {
		return "", err
	}

	statement := "DROP DATABASE "
	if opts.IfExists {
		statement += "IF EXISTS "
	}
	return statement + database + ";", nil
}

// BuildTruncateTablesSQL returns one TRUNCATE TABLE statement for schema-
// qualified or unqualified table targets. It only builds SQL and never opens a
// database connection.
func BuildTruncateTablesSQL(opts TruncateTablesOptions) (string, error) {
	if len(opts.Tables) == 0 {
		return "", ErrNoTruncateTables
	}

	tables, err := quoteTableNames(opts.Tables)
	if err != nil {
		return "", err
	}

	parts := []string{"TRUNCATE TABLE", strings.Join(tables, ", ")}
	if opts.RestartIdentity {
		parts = append(parts, "RESTART IDENTITY")
	}
	if opts.Cascade {
		parts = append(parts, "CASCADE")
	}
	return strings.Join(parts, " ") + ";", nil
}
