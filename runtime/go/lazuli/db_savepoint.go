package lazuli

import (
	"context"
	"errors"
	"fmt"

	"github.com/jackc/pgx/v5/pgconn"
)

var (
	errNilSavepointExec = errors.New("lazuli: nil savepoint executor")
	errNilSavepointFunc = errors.New("lazuli: nil savepoint function")
)

// Exec is the minimal database executor required by WithSavepoint.
// It is compatible with pgx transaction Exec methods.
type Exec interface {
	Exec(context.Context, string, ...any) (pgconn.CommandTag, error)
}

// WithSavepoint runs fn inside a Postgres savepoint on tx.
//
// The savepoint name must be an unquoted SQL identifier made only of ASCII
// letters, numbers, and underscores, starting with a letter or underscore.
// On fn error, WithSavepoint rolls back to the savepoint and returns fn's
// error. On success, it releases the savepoint.
func WithSavepoint(ctx context.Context, tx Exec, name string, fn func(context.Context) error) error {
	if tx == nil {
		return errNilSavepointExec
	}
	if fn == nil {
		return errNilSavepointFunc
	}
	if !validSavepointIdentifier(name) {
		return fmt.Errorf("lazuli: invalid savepoint name %q", name)
	}

	if _, err := tx.Exec(ctx, "SAVEPOINT "+name); err != nil {
		return err
	}

	if err := fn(ctx); err != nil {
		_, _ = tx.Exec(ctx, "ROLLBACK TO SAVEPOINT "+name)
		return err
	}

	_, err := tx.Exec(ctx, "RELEASE SAVEPOINT "+name)
	return err
}

func validSavepointIdentifier(name string) bool {
	if name == "" {
		return false
	}

	for i := 0; i < len(name); i++ {
		c := name[i]
		if i == 0 {
			if !isSavepointLetter(c) && c != '_' {
				return false
			}
			continue
		}
		if !isSavepointLetter(c) && !isSavepointDigit(c) && c != '_' {
			return false
		}
	}

	return true
}

func isSavepointLetter(c byte) bool {
	return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')
}

func isSavepointDigit(c byte) bool {
	return c >= '0' && c <= '9'
}
