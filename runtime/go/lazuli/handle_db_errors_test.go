package lazuli

import (
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/jackc/pgx/v5/pgconn"
)

// TestClassifyDBErrorMapsSQLState23xxToTypedCodes is the load-bearing
// regression guard for DB-INTEGRITY-CATALOG-EXT (2026-05-19): every
// Postgres class-23 SQLSTATE the runtime expects to see must map to
// its typed Lazuli code + HTTP status. A failure here means a future
// edit clobbered the classifier and the wire would regress to
// `code:"internal"` + raw SQLSTATE strings — the field bug this whole
// extension exists to close.
func TestClassifyDBErrorMapsSQLState23xxToTypedCodes(t *testing.T) {
	cases := []struct {
		sqlstate   string
		wantCode   string
		wantStatus int
	}{
		{"23505", CodeUniqueViolation, http.StatusConflict},
		{"23503", CodeForeignKeyViolation, http.StatusBadRequest},
		{"23502", CodeNotNullViolation, http.StatusBadRequest},
		{"23514", CodeCheckViolation, http.StatusBadRequest},
	}
	for _, c := range cases {
		c := c
		t.Run(c.sqlstate, func(t *testing.T) {
			pgErr := &pgconn.PgError{
				Code:    c.sqlstate,
				Message: "constraint blah failed",
			}
			lazErr := classifyDBError("insert", pgErr)
			if lazErr.Code != c.wantCode {
				t.Fatalf("SQLSTATE %s: Code = %q, want %q",
					c.sqlstate, lazErr.Code, c.wantCode)
			}
			if lazErr.Status != c.wantStatus {
				t.Fatalf("SQLSTATE %s: Status = %d, want %d",
					c.sqlstate, lazErr.Status, c.wantStatus)
			}
			if lazErr.MessageKey != c.wantCode {
				t.Fatalf("SQLSTATE %s: MessageKey = %q, want %q (bare-code → builtin L3)",
					c.sqlstate, lazErr.MessageKey, c.wantCode)
			}
			if !strings.Contains(lazErr.Message, "insert failed") {
				t.Fatalf("SQLSTATE %s: Message must carry the stage label, got %q",
					c.sqlstate, lazErr.Message)
			}
		})
	}
}

// TestClassifyDBErrorPreservesInternalFallbackForUnclassified asserts
// the wire-thin discipline: only Postgres class-23 errors are typed,
// every other err shape (driver-level, non-pgconn, even a non-23
// SQLSTATE) lands on the existing 500-internal behaviour. This
// preserves the pre-extension contract for anything the classifier
// doesn't recognize — additive, never lossy.
func TestClassifyDBErrorPreservesInternalFallbackForUnclassified(t *testing.T) {
	// Non-pgconn err -> internal.
	plain := errors.New("connection reset")
	lazErr := classifyDBError("update", plain)
	if lazErr.Code != CodeInternal {
		t.Fatalf("plain err: Code = %q, want %q", lazErr.Code, CodeInternal)
	}
	if lazErr.Status != http.StatusInternalServerError {
		t.Fatalf("plain err: Status = %d, want 500", lazErr.Status)
	}
	if lazErr.MessageKey != "" {
		t.Fatalf("plain err: MessageKey must stay empty (no localizable code), got %q",
			lazErr.MessageKey)
	}

	// Non-23 SQLSTATE -> still internal (we only widened class 23).
	pgErr := &pgconn.PgError{Code: "42P01", Message: "table does not exist"}
	lazErr = classifyDBError("insert scan", pgErr)
	if lazErr.Code != CodeInternal {
		t.Fatalf("SQLSTATE 42P01: Code = %q, want %q", lazErr.Code, CodeInternal)
	}
}

// TestClassifyDBErrorUniqueViolationWireBodyIsLocalized walks the
// full HTTP boundary on a `23505` to prove the wire client sees the
// localized layperson message — NEVER the raw Postgres SQLSTATE,
// constraint name, or the `code:"internal"` legacy shape. This is
// the exit-criteria scenario for the hostpoint duplicate-email
// field incident.
func TestClassifyDBErrorUniqueViolationWireBodyIsLocalized(t *testing.T) {
	installZeroAuthoringFixture(t)

	pgErr := &pgconn.PgError{
		Code:    "23505",
		Message: `duplicate key value violates unique constraint "user_email_unique"`,
	}
	lazErr := classifyDBError("insert scan", pgErr)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/account.register", nil)
	req.Header.Set("Accept-Language", "pt-BR")
	writeError(rec, req, lazErr)

	if rec.Code != http.StatusConflict {
		t.Fatalf("HTTP status = %d, want 409 (unique_violation)", rec.Code)
	}
	body := rec.Body.String()
	for _, banned := range []string{
		`SQLSTATE`,
		`user_email_unique`,
		`duplicate key value`,
		`unique constraint`,
		`"code":"internal"`,
	} {
		if strings.Contains(body, banned) {
			t.Fatalf("wire payload leaks raw Postgres internals %q: %s", banned, body)
		}
	}
	payload := decodePayload(t, rec.Body.Bytes())
	if got, want := payload["code"], CodeUniqueViolation; got != want {
		t.Fatalf("code = %q, want %q", got, want)
	}
	wantMessage := "Este item já existe. Tente outro valor."
	if got, ok := payload["message"].(string); !ok || got != wantMessage {
		t.Fatalf("message = %v, want %q (builtin pt-BR floor)",
			payload["message"], wantMessage)
	}
}

// TestClassifyDBErrorWrappedPgErrorStillClassifies covers the
// errors.As traversal — pgx may wrap PgError inside transactional
// adapters; the classifier walks the chain rather than asserting
// equality, so wrapped forms still resolve correctly.
func TestClassifyDBErrorWrappedPgErrorStillClassifies(t *testing.T) {
	inner := &pgconn.PgError{Code: "23505", Message: "dup"}
	wrapped := fmt.Errorf("query failed: %w", inner)
	lazErr := classifyDBError("insert", wrapped)
	if lazErr.Code != CodeUniqueViolation {
		t.Fatalf("wrapped pgErr: Code = %q, want %q", lazErr.Code, CodeUniqueViolation)
	}
	if lazErr.Status != http.StatusConflict {
		t.Fatalf("wrapped pgErr: Status = %d, want 409", lazErr.Status)
	}
}
