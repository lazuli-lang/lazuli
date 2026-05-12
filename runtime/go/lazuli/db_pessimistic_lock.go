package lazuli

import (
	"errors"
	"fmt"
	"strings"
)

var (
	errInvalidDBPessimisticLockMode       = errors.New("lazuli: invalid db pessimistic lock mode")
	errInvalidDBPessimisticLockOptions    = errors.New("lazuli: invalid db pessimistic lock options")
	errInvalidDBPessimisticLockIdentifier = errors.New("lazuli: invalid db pessimistic lock identifier")
	errInvalidDBPessimisticLockOrder      = errors.New("lazuli: invalid db pessimistic lock clause order")
)

// DBPessimisticLockMode names the row-level lock strength appended to a SELECT.
type DBPessimisticLockMode int

const (
	// DBPessimisticLockNone leaves the SELECT unlocked.
	DBPessimisticLockNone DBPessimisticLockMode = iota
	// DBPessimisticLockUpdate builds FOR UPDATE.
	DBPessimisticLockUpdate
	// DBPessimisticLockShare builds FOR SHARE.
	DBPessimisticLockShare
)

// DBPessimisticLockOptions configures a PostgreSQL row-level locking clause.
//
// Of is an optional list of table names or aliases for an OF clause. Each name
// may be schema-qualified with one dot; every identifier component is quoted.
// NoWait and SkipLocked are mutually exclusive because PostgreSQL accepts only
// one wait policy per locking clause.
type DBPessimisticLockOptions struct {
	Mode       DBPessimisticLockMode
	Of         []string
	NoWait     bool
	SkipLocked bool
}

// BuildDBPessimisticLockClause builds a PostgreSQL FOR UPDATE/FOR SHARE clause.
//
// The helper is pure: it validates generated identifiers, quotes every OF
// component, and never opens a database connection. The zero-value options
// produce an empty clause for generated repositories that select locking
// conditionally.
func BuildDBPessimisticLockClause(opts DBPessimisticLockOptions) (string, error) {
	if opts.NoWait && opts.SkipLocked {
		return "", errInvalidDBPessimisticLockOptions
	}

	var mode string
	switch opts.Mode {
	case DBPessimisticLockNone:
		if len(opts.Of) != 0 || opts.NoWait || opts.SkipLocked {
			return "", errInvalidDBPessimisticLockOptions
		}
		return "", nil
	case DBPessimisticLockUpdate:
		mode = "UPDATE"
	case DBPessimisticLockShare:
		mode = "SHARE"
	default:
		return "", errInvalidDBPessimisticLockMode
	}

	quotedOf, err := quoteDBPessimisticLockOf(opts.Of)
	if err != nil {
		return "", err
	}

	var b strings.Builder
	b.WriteString("FOR ")
	b.WriteString(mode)
	if len(quotedOf) > 0 {
		b.WriteString(" OF ")
		b.WriteString(strings.Join(quotedOf, ", "))
	}
	if opts.NoWait {
		b.WriteString(" NOWAIT")
	}
	if opts.SkipLocked {
		b.WriteString(" SKIP LOCKED")
	}
	return b.String(), nil
}

// AppendDBPessimisticLockClause appends a row-level locking clause to a SELECT.
//
// PostgreSQL locking clauses belong after WHERE/GROUP/ORDER/LIMIT/OFFSET. This
// helper appends the clause at the end of a single SELECT statement and rejects
// SQL that already contains a locking/wait clause or a statement terminator.
func AppendDBPessimisticLockClause(selectSQL string, opts DBPessimisticLockOptions) (string, error) {
	clause, err := BuildDBPessimisticLockClause(opts)
	if err != nil {
		return "", err
	}

	sql := strings.TrimSpace(selectSQL)
	if clause == "" {
		return sql, nil
	}
	if !validDBPessimisticLockSelectOrder(sql) {
		return "", errInvalidDBPessimisticLockOrder
	}
	return sql + " " + clause, nil
}

func quoteDBPessimisticLockOf(names []string) ([]string, error) {
	quoted := make([]string, 0, len(names))
	for _, name := range names {
		quotedName, err := quoteDBPessimisticLockName(name)
		if err != nil {
			return nil, err
		}
		quoted = append(quoted, quotedName)
	}
	return quoted, nil
}

func quoteDBPessimisticLockName(name string) (string, error) {
	parts := strings.Split(name, ".")
	if len(parts) > 2 {
		return "", fmt.Errorf("%w: %q", errInvalidDBPessimisticLockIdentifier, name)
	}

	quoted := make([]string, 0, len(parts))
	for _, part := range parts {
		if !validDBPessimisticLockIdentifier(part) {
			return "", fmt.Errorf("%w: %q", errInvalidDBPessimisticLockIdentifier, name)
		}
		quoted = append(quoted, `"`+part+`"`)
	}
	return strings.Join(quoted, "."), nil
}

func validDBPessimisticLockIdentifier(identifier string) bool {
	if identifier == "" {
		return false
	}
	for i := 0; i < len(identifier); i++ {
		c := identifier[i]
		if i == 0 {
			if !isDBPessimisticLockIdentifierLetter(c) && c != '_' {
				return false
			}
			continue
		}
		if !isDBPessimisticLockIdentifierLetter(c) && !isDBPessimisticLockIdentifierDigit(c) && c != '_' {
			return false
		}
	}
	return true
}

func isDBPessimisticLockIdentifierLetter(c byte) bool {
	return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')
}

func isDBPessimisticLockIdentifierDigit(c byte) bool {
	return c >= '0' && c <= '9'
}

func validDBPessimisticLockSelectOrder(sql string) bool {
	if sql == "" || strings.Contains(sql, ";") {
		return false
	}

	words := dbPessimisticLockSQLWords(sql)
	if len(words) == 0 || words[0] != "SELECT" {
		return false
	}

	for i := 0; i < len(words); i++ {
		switch words[i] {
		case "NOWAIT":
			return false
		case "SKIP":
			if i+1 < len(words) && words[i+1] == "LOCKED" {
				return false
			}
		case "FOR":
			if dbPessimisticLockWordStartsClause(words, i+1) {
				return false
			}
		}
	}
	return true
}

func dbPessimisticLockWordStartsClause(words []string, i int) bool {
	if i >= len(words) {
		return false
	}
	switch words[i] {
	case "UPDATE", "SHARE":
		return true
	case "NO":
		return i+2 < len(words) && words[i+1] == "KEY" && words[i+2] == "UPDATE"
	case "KEY":
		return i+1 < len(words) && words[i+1] == "SHARE"
	default:
		return false
	}
}

func dbPessimisticLockSQLWords(sql string) []string {
	words := make([]string, 0, 8)
	for i := 0; i < len(sql); {
		switch c := sql[i]; {
		case c == '\'':
			i = skipDBPessimisticLockQuoted(sql, i, '\'')
		case c == '"':
			i = skipDBPessimisticLockQuoted(sql, i, '"')
		case isDBPessimisticLockIdentifierLetter(c) || c == '_':
			start := i
			i++
			for i < len(sql) {
				next := sql[i]
				if !isDBPessimisticLockIdentifierLetter(next) && !isDBPessimisticLockIdentifierDigit(next) && next != '_' {
					break
				}
				i++
			}
			words = append(words, strings.ToUpper(sql[start:i]))
		default:
			i++
		}
	}
	return words
}

func skipDBPessimisticLockQuoted(sql string, start int, quote byte) int {
	for i := start + 1; i < len(sql); i++ {
		if sql[i] != quote {
			continue
		}
		if i+1 < len(sql) && sql[i+1] == quote {
			i++
			continue
		}
		return i + 1
	}
	return len(sql)
}
