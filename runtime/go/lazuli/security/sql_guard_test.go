package security

import (
	"errors"
	"testing"
)

func TestValidateGeneratedSQLAllowsSingleSafeStatements(t *testing.T) {
	t.Parallel()
	tests := []string{
		`SELECT id, email FROM "User" WHERE id = $1`,
		`SELECT '; -- /* ${not_marker}' AS literal`,
		`UPDATE "User" SET "name" = $1 WHERE "id" = $2 RETURNING *`,
		`DELETE FROM "Session" WHERE expires_at <= $1;`,
	}
	for _, sql := range tests {
		sql := sql
		t.Run(sql, func(t *testing.T) {
			t.Parallel()
			if err := ValidateGeneratedSQL(sql, SQLGuard{}); err != nil {
				t.Fatalf("ValidateGeneratedSQL() error = %v", err)
			}
		})
	}
}

func TestValidateGeneratedSQLRejectsMultipleStatements(t *testing.T) {
	t.Parallel()
	tests := []string{
		`SELECT 1; SELECT 2`,
		`SELECT 1;;`,
		`UPDATE users SET admin = true WHERE id = $1; DELETE FROM users WHERE id = $2`,
	}
	for _, sql := range tests {
		sql := sql
		t.Run(sql, func(t *testing.T) {
			t.Parallel()
			err := ValidateGeneratedSQL(sql, SQLGuard{})
			if !errors.Is(err, ErrSQLGuardRejected) {
				t.Fatalf("ValidateGeneratedSQL() error = %v, want ErrSQLGuardRejected", err)
			}
		})
	}
}

func TestValidateGeneratedSQLFragmentRejectsCommentsAndTerminators(t *testing.T) {
	t.Parallel()
	tests := []string{
		`id = $1 -- tenant scope`,
		`id = $1 /* tenant scope */`,
		`id = $1; DELETE FROM users`,
	}
	for _, fragment := range tests {
		fragment := fragment
		t.Run(fragment, func(t *testing.T) {
			t.Parallel()
			err := ValidateGeneratedSQLFragment(fragment)
			if !errors.Is(err, ErrSQLGuardRejected) {
				t.Fatalf("ValidateGeneratedSQLFragment() error = %v, want ErrSQLGuardRejected", err)
			}
		})
	}
}

func TestValidateGeneratedSQLRejectsInterpolationMarkers(t *testing.T) {
	t.Parallel()
	tests := []string{
		`SELECT * FROM ${table} WHERE id = $1`,
		`SELECT * FROM {{ .Table }} WHERE id = $1`,
		`SELECT * FROM "%s" WHERE id = $1`,
		`SELECT * FROM %[1]s WHERE id = $1`,
	}
	for _, sql := range tests {
		sql := sql
		t.Run(sql, func(t *testing.T) {
			t.Parallel()
			err := ValidateGeneratedSQL(sql, SQLGuard{})
			if !errors.Is(err, ErrSQLGuardRejected) {
				t.Fatalf("ValidateGeneratedSQL() error = %v, want ErrSQLGuardRejected", err)
			}
		})
	}
}

func TestValidateGeneratedSQLRejectsMutationWithoutWhere(t *testing.T) {
	t.Parallel()
	tests := []string{
		`UPDATE users SET admin = true`,
		`UPDATE users SET note = 'where'`,
		`DELETE FROM users`,
	}
	for _, sql := range tests {
		sql := sql
		t.Run(sql, func(t *testing.T) {
			t.Parallel()
			err := ValidateGeneratedSQL(sql, SQLGuard{})
			if !errors.Is(err, ErrSQLGuardRejected) {
				t.Fatalf("ValidateGeneratedSQL() error = %v, want ErrSQLGuardRejected", err)
			}
		})
	}
}

func TestValidateGeneratedSQLAllowsExplicitMutationWithoutWhere(t *testing.T) {
	t.Parallel()
	if err := ValidateGeneratedSQL(`DELETE FROM expired_sessions`, SQLGuard{
		AllowMutationWithoutWhere: true,
	}); err != nil {
		t.Fatalf("ValidateGeneratedSQL() error = %v", err)
	}
}
