package security

import (
	"errors"
	"fmt"
	"strings"
)

// ErrSQLGuardRejected is returned when generated SQL fails SQLGuard
// validation. Use errors.Is to classify wrapped rejection reasons.
var ErrSQLGuardRejected = errors.New("lazuli/security: sql_guard_rejected")

// SQLGuard configures ValidateGeneratedSQL.
type SQLGuard struct {
	// AllowMutationWithoutWhere allows UPDATE and DELETE statements without a
	// WHERE token. Keep false for generated application SQL; set true only for
	// explicit maintenance statements that intentionally affect every row.
	AllowMutationWithoutWhere bool
}

// ValidateGeneratedSQL rejects SQL shapes that Lazuli generated SQL should not
// emit. This is intentionally a strict scanner, not a full SQL parser: it
// rejects multiple statements, SQL comments, unresolved interpolation markers,
// and UPDATE/DELETE without WHERE unless explicitly allowed.
func ValidateGeneratedSQL(sql string, guard SQLGuard) error {
	scan, err := scanGeneratedSQL(sql, true)
	if err != nil {
		return err
	}
	if len(scan.tokens) == 0 {
		return sqlGuardReject("empty SQL")
	}
	if guard.AllowMutationWithoutWhere {
		return nil
	}
	for i, token := range scan.tokens {
		if token != "UPDATE" && token != "DELETE" {
			continue
		}
		if !sqlGuardHasTokenAfter(scan.tokens, i+1, "WHERE") {
			return sqlGuardReject(strings.ToLower(token) + " without WHERE")
		}
	}
	return nil
}

// ValidateGeneratedSQLFragment rejects tokens that must never appear in a
// generated SQL fragment before it is embedded into a larger statement.
func ValidateGeneratedSQLFragment(fragment string) error {
	_, err := scanGeneratedSQL(fragment, false)
	return err
}

type generatedSQLScan struct {
	tokens []string
}

func scanGeneratedSQL(sql string, allowTrailingSemicolon bool) (generatedSQLScan, error) {
	var scan generatedSQLScan
	terminated := false

	for i := 0; i < len(sql); {
		c := sql[i]
		if c == '\'' {
			var ok bool
			i, ok = skipSQLSingleQuoted(sql, i)
			if !ok {
				return scan, sqlGuardReject("unterminated string literal")
			}
			continue
		}
		if c == '"' {
			var err error
			i, err = skipSQLQuotedIdentifier(sql, i, '"')
			if err != nil {
				return scan, err
			}
			continue
		}
		if c == '`' {
			var err error
			i, err = skipSQLQuotedIdentifier(sql, i, '`')
			if err != nil {
				return scan, err
			}
			continue
		}
		if end, ok, closed := skipSQLDollarQuoted(sql, i); ok {
			if !closed {
				return scan, sqlGuardReject("unterminated dollar-quoted string")
			}
			i = end
			continue
		}

		if marker := sqlInterpolationMarkerAt(sql, i); marker != "" {
			return scan, sqlGuardReject("unsafe interpolation marker " + marker)
		}
		if i+1 < len(sql) && sql[i] == '-' && sql[i+1] == '-' {
			return scan, sqlGuardReject("SQL line comment")
		}
		if i+1 < len(sql) && sql[i] == '/' && sql[i+1] == '*' {
			return scan, sqlGuardReject("SQL block comment")
		}
		if c == ';' {
			if !allowTrailingSemicolon || terminated {
				return scan, sqlGuardReject("multiple statements")
			}
			terminated = true
			i++
			continue
		}
		if sqlGuardIsSpace(c) {
			i++
			continue
		}
		if terminated {
			return scan, sqlGuardReject("multiple statements")
		}
		if sqlGuardIsIdentStart(c) {
			start := i
			i++
			for i < len(sql) && sqlGuardIsIdentPart(sql[i]) {
				i++
			}
			scan.tokens = append(scan.tokens, strings.ToUpper(sql[start:i]))
			continue
		}
		i++
	}

	return scan, nil
}

func skipSQLSingleQuoted(sql string, start int) (int, bool) {
	for i := start + 1; i < len(sql); i++ {
		if sql[i] != '\'' {
			continue
		}
		if i+1 < len(sql) && sql[i+1] == '\'' {
			i++
			continue
		}
		return i + 1, true
	}
	return len(sql), false
}

func skipSQLQuotedIdentifier(sql string, start int, quote byte) (int, error) {
	for i := start + 1; i < len(sql); i++ {
		if marker := sqlInterpolationMarkerAt(sql, i); marker != "" {
			return len(sql), sqlGuardReject("unsafe interpolation marker " + marker)
		}
		if sql[i] != quote {
			continue
		}
		if i+1 < len(sql) && sql[i+1] == quote {
			i++
			continue
		}
		return i + 1, nil
	}
	return len(sql), sqlGuardReject("unterminated quoted identifier")
}

func skipSQLDollarQuoted(sql string, start int) (end int, ok bool, closed bool) {
	if sql[start] != '$' {
		return 0, false, false
	}

	endTag := start + 1
	if endTag < len(sql) && sql[endTag] == '$' {
		endTag++
	} else {
		if endTag >= len(sql) || !sqlGuardIsIdentStart(sql[endTag]) {
			return 0, false, false
		}
		endTag++
		for endTag < len(sql) && sqlGuardIsIdentPart(sql[endTag]) {
			endTag++
		}
		if endTag >= len(sql) || sql[endTag] != '$' {
			return 0, false, false
		}
		endTag++
	}

	tag := sql[start:endTag]
	if close := strings.Index(sql[endTag:], tag); close >= 0 {
		return endTag + close + len(tag), true, true
	}
	return len(sql), true, false
}

func sqlInterpolationMarkerAt(sql string, i int) string {
	switch {
	case strings.HasPrefix(sql[i:], "${"):
		return "${"
	case strings.HasPrefix(sql[i:], "{{"):
		return "{{"
	case strings.HasPrefix(sql[i:], "}}"):
		return "}}"
	case sql[i] == '%' && i+1 < len(sql):
		switch sql[i+1] {
		case 's', 'q', 'v':
			return sql[i : i+2]
		case '[':
			return "%["
		}
	}
	return ""
}

func sqlGuardHasTokenAfter(tokens []string, start int, want string) bool {
	for i := start; i < len(tokens); i++ {
		if tokens[i] == want {
			return true
		}
	}
	return false
}

func sqlGuardIsSpace(c byte) bool {
	switch c {
	case ' ', '\t', '\n', '\r', '\f':
		return true
	default:
		return false
	}
}

func sqlGuardIsIdentStart(c byte) bool {
	return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_'
}

func sqlGuardIsIdentPart(c byte) bool {
	return sqlGuardIsIdentStart(c) || (c >= '0' && c <= '9')
}

func sqlGuardReject(reason string) error {
	return fmt.Errorf("%w: %s", ErrSQLGuardRejected, reason)
}
