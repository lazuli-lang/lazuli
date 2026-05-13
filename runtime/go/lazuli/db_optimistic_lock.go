package lazuli

import (
	"errors"
	"fmt"
	"strings"
)

const (
	// DefaultDBOptimisticLockVersionColumn is the conventional integer version
	// column used when DBOptimisticLockVersion.Column is empty.
	DefaultDBOptimisticLockVersionColumn = "version"
	// DBOptimisticLockConflictReason is stored on repository conflict errors
	// produced from zero affected rows.
	DBOptimisticLockConflictReason = "version mismatch"
)

var (
	errInvalidDBOptimisticLockColumn       = errors.New("lazuli: invalid db optimistic lock column")
	errInvalidDBOptimisticLockPlaceholder  = errors.New("lazuli: invalid db optimistic lock placeholder")
	errInvalidDBOptimisticLockRowsAffected = errors.New("lazuli: invalid db optimistic lock rows affected")
)

// DBOptimisticLockVersion describes the version column and values for one
// optimistic-lock write.
//
// Column is a generated SQL identifier and defaults to "version" when empty.
// Expected is bound into the WHERE predicate. Next is bound into the SET
// assignment, letting generated code choose integer counters, timestamps, UUIDs,
// or any other storage representation.
type DBOptimisticLockVersion struct {
	Column   string
	Expected any
	Next     any
}

// DBOptimisticLockSQLFragment is a SQL fragment plus the bind arguments it
// references.
type DBOptimisticLockSQLFragment struct {
	SQL  string
	Args []any
}

// BuildDBOptimisticLockWhere builds a version equality predicate.
//
// The returned SQL does not include the leading "WHERE". placeholder is
// supplied by the caller so adapters can use "$1", "?", "@p1", ":version",
// or another generated bind token.
func BuildDBOptimisticLockWhere(version DBOptimisticLockVersion, placeholder string) (DBOptimisticLockSQLFragment, error) {
	column, placeholder, err := dbOptimisticLockColumnAndPlaceholder(version.Column, placeholder)
	if err != nil {
		return DBOptimisticLockSQLFragment{}, err
	}

	return DBOptimisticLockSQLFragment{
		SQL:  fmt.Sprintf("%s = %s", column, placeholder),
		Args: []any{version.Expected},
	}, nil
}

// BuildDBOptimisticLockNextVersionAssignment builds a SET assignment for the
// next version value.
//
// The returned SQL does not include the leading "SET". placeholder is supplied
// by the caller for adapter-neutral placeholder style.
func BuildDBOptimisticLockNextVersionAssignment(version DBOptimisticLockVersion, placeholder string) (DBOptimisticLockSQLFragment, error) {
	column, placeholder, err := dbOptimisticLockColumnAndPlaceholder(version.Column, placeholder)
	if err != nil {
		return DBOptimisticLockSQLFragment{}, err
	}

	return DBOptimisticLockSQLFragment{
		SQL:  fmt.Sprintf("%s = %s", column, placeholder),
		Args: []any{version.Next},
	}, nil
}

// CheckDBOptimisticLockRowsAffected converts an affected row count into an
// optimistic-lock conflict.
//
// Zero rows means the expected version did not match the stored version. Positive
// row counts succeed, allowing callers to use this helper for both single-row
// and batch writes. Negative row counts are treated as adapter bugs.
func CheckDBOptimisticLockRowsAffected(rowsAffected int64, resource string, id any) error {
	switch {
	case rowsAffected < 0:
		return fmt.Errorf("%w: %d", errInvalidDBOptimisticLockRowsAffected, rowsAffected)
	case rowsAffected == 0:
		return NewRepositoryConflict(resource, id, DBOptimisticLockConflictReason)
	default:
		return nil
	}
}

func dbOptimisticLockColumnAndPlaceholder(columnName, placeholder string) (string, string, error) {
	column, err := quoteDBOptimisticLockColumn(columnName)
	if err != nil {
		return "", "", err
	}

	placeholder = strings.TrimSpace(placeholder)
	if !validDBOptimisticLockPlaceholder(placeholder) {
		return "", "", errInvalidDBOptimisticLockPlaceholder
	}

	return column, placeholder, nil
}

func quoteDBOptimisticLockColumn(name string) (string, error) {
	if name == "" {
		name = DefaultDBOptimisticLockVersionColumn
	}
	if !validDBOptimisticLockIdentifier(name) {
		return "", fmt.Errorf("%w: %q", errInvalidDBOptimisticLockColumn, name)
	}
	return `"` + name + `"`, nil
}

func validDBOptimisticLockIdentifier(identifier string) bool {
	if identifier == "" {
		return false
	}
	for i := 0; i < len(identifier); i++ {
		c := identifier[i]
		if i == 0 {
			if !isDBOptimisticLockIdentifierLetter(c) && c != '_' {
				return false
			}
			continue
		}
		if !isDBOptimisticLockIdentifierLetter(c) && !isDBOptimisticLockIdentifierDigit(c) && c != '_' {
			return false
		}
	}
	return true
}

func validDBOptimisticLockPlaceholder(placeholder string) bool {
	if placeholder == "?" {
		return true
	}
	if len(placeholder) < 2 {
		return false
	}

	switch placeholder[0] {
	case '$', '@', ':':
	default:
		return false
	}

	for i := 1; i < len(placeholder); i++ {
		c := placeholder[i]
		if !isDBOptimisticLockIdentifierLetter(c) && !isDBOptimisticLockIdentifierDigit(c) && c != '_' {
			return false
		}
	}
	return true
}

func isDBOptimisticLockIdentifierLetter(c byte) bool {
	return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')
}

func isDBOptimisticLockIdentifierDigit(c byte) bool {
	return c >= '0' && c <= '9'
}
