package lazuli

import (
	"bytes"
	"encoding/json"
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
// the exit-criteria scenario for the the canonical pilot duplicate-email
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

// TestClassifyDBErrorRemapsRegisteredConstraintToDomainCode proves the
// per-constraint domain-code path (`unique a, b error <CODE>`): once codegen
// glue registers `<constraint_name> -> <CODE>`, a 23505 carrying that exact
// ConstraintName surfaces the pinned domain code (still 409), instead of the
// generic `unique_violation`. This is the gap that lets the pilot delete its
// hand-written `guard_member_not_in_job` / handle_db_errors remap.
func TestClassifyDBErrorRemapsRegisteredConstraintToDomainCode(t *testing.T) {
	const constraint = "job_member_job_id_user_id_key"
	const code = "MEMBER_ALREADY_IN_JOB"
	RegisterUniqueViolationCode(constraint, code)
	t.Cleanup(func() { resetUniqueViolationCodes() })

	pgErr := &pgconn.PgError{
		Code:           "23505",
		ConstraintName: constraint,
		Message:        `duplicate key value violates unique constraint "` + constraint + `"`,
	}
	lazErr := classifyDBError("insert", pgErr)
	if lazErr.Code != code {
		t.Fatalf("Code = %q, want domain code %q", lazErr.Code, code)
	}
	if lazErr.Status != http.StatusConflict {
		t.Fatalf("Status = %d, want 409", lazErr.Status)
	}
	if lazErr.MessageKey != code {
		t.Fatalf("MessageKey = %q, want %q (domain code → feature errors L2/L3)",
			lazErr.MessageKey, code)
	}
	if strings.Contains(lazErr.Message, constraint) {
		t.Fatalf("Message must not leak the constraint name, got %q", lazErr.Message)
	}
}

// TestClassifyDBErrorUnregisteredConstraintStaysGeneric asserts the additive
// discipline: a 23505 whose ConstraintName was NOT registered keeps the
// generic `unique_violation` wire code (back-compat for every constraint that
// did not author `error <CODE>`).
func TestClassifyDBErrorUnregisteredConstraintStaysGeneric(t *testing.T) {
	resetUniqueViolationCodes()
	pgErr := &pgconn.PgError{
		Code:           "23505",
		ConstraintName: "some_other_table_email_key",
		Message:        "dup",
	}
	lazErr := classifyDBError("insert", pgErr)
	if lazErr.Code != CodeUniqueViolation {
		t.Fatalf("Code = %q, want generic %q", lazErr.Code, CodeUniqueViolation)
	}
	if lazErr.Status != http.StatusConflict {
		t.Fatalf("Status = %d, want 409", lazErr.Status)
	}
}

// findSlogEntry parses the captured slog buffer line-by-line (slog emits one
// JSON object per line) and returns the first entry whose "msg" equals want.
func findSlogEntry(t *testing.T, buf *bytes.Buffer, want string) map[string]any {
	t.Helper()
	for _, line := range bytes.Split(bytes.TrimSpace(buf.Bytes()), []byte("\n")) {
		line = bytes.TrimSpace(line)
		if len(line) == 0 {
			continue
		}
		var entry map[string]any
		if err := json.Unmarshal(line, &entry); err != nil {
			t.Fatalf("slog line not JSON: %v; line=%q", err, line)
		}
		if entry["msg"] == want {
			return entry
		}
	}
	t.Fatalf("no slog entry with msg=%q; log=%q", want, buf.String())
	return nil
}

// TestClassifyDBErrorPopulatesBaseCauseWithSQLState is the regression guard
// for ERR-BASE-CAUSE-UNPOPULATED: classifyDBError MUST wrap the original pg
// error into Base.Cause so writeLazuliError's always-on 5xx server log
// ("cause" attr) and the dev-only `detail.cause` block carry the SQLSTATE +
// raw pg message. Before this fix Base.Cause was nil for DB-origin errors and
// the cause came up empty in the structured envelope.
func TestClassifyDBErrorPopulatesBaseCauseWithSQLState(t *testing.T) {
	cases := []struct {
		name     string
		sqlstate string
		// pgErr.Error() renders "<severity>: <message> (SQLSTATE <code>)".
		wantSubstrings []string
	}{
		{
			name:           "unmapped 5xx (the blind-diagnosis path)",
			sqlstate:       "42703", // undefined_column → 500/internal
			wantSubstrings: []string{"SQLSTATE 42703", "column"},
		},
		{
			name:           "mapped 4xx still carries cause",
			sqlstate:       "23505", // unique_violation → 409
			wantSubstrings: []string{"SQLSTATE 23505"},
		},
	}
	for _, c := range cases {
		c := c
		t.Run(c.name, func(t *testing.T) {
			pgErr := &pgconn.PgError{
				Severity: "ERROR",
				Code:     c.sqlstate,
				Message:  `column "org" of relation "reports_exports" does not exist`,
			}
			lazErr := classifyDBError("insert", pgErr)

			if lazErr.Base.Cause == nil {
				t.Fatalf("SQLSTATE %s: Base.Cause is nil; the SQLSTATE/detail is lost from the structured envelope", c.sqlstate)
			}
			// errors.As must walk through *Error.Unwrap to the wrapped pg error.
			var got *pgconn.PgError
			if !errors.As(lazErr, &got) {
				t.Fatalf("SQLSTATE %s: errors.As(*Error) did not reach the wrapped *pgconn.PgError", c.sqlstate)
			}
			causeStr := lazErr.Base.Cause.Error()
			for _, sub := range c.wantSubstrings {
				if !strings.Contains(causeStr, sub) {
					t.Fatalf("SQLSTATE %s: Base.Cause %q missing %q", c.sqlstate, causeStr, sub)
				}
			}
			// Client-facing masking is untouched: the masked Message must NOT
			// carry the raw SQLSTATE / column name.
			if strings.Contains(lazErr.Message, "SQLSTATE") || strings.Contains(lazErr.Message, "org") {
				t.Fatalf("SQLSTATE %s: client Message leaked pg internals: %q", c.sqlstate, lazErr.Message)
			}
		})
	}
}

// TestClassifyDBErrorCauseFlowsToDevDetailAndProdMasks walks the full HTTP
// boundary on the canonical DB-origin 500 (an INSERT into a non-existent
// column → SQLSTATE 42703). It proves the end-to-end ERR-BASE-CAUSE-UNPOPULATED
// fix: in dev the response `detail.cause` + the server log `cause` carry the
// SQLSTATE 42703 + "column ... does not exist", while in prod the whole detail
// block is masked (only the server log keeps the cause).
func TestClassifyDBErrorCauseFlowsToDevDetailAndProdMasks(t *testing.T) {
	const sqlstate = "42703"
	newPgErr := func() *pgconn.PgError {
		return &pgconn.PgError{
			Severity: "ERROR",
			Code:     sqlstate,
			Message:  `column "org" of relation "reports_exports" does not exist`,
		}
	}

	t.Run("dev exposes detail.cause + logs cause", func(t *testing.T) {
		t.Setenv("LAZULI_ENV", "dev")
		buf := captureSlog(t)

		lazErr := classifyDBError("insert", newPgErr())
		rec := httptest.NewRecorder()
		req := httptest.NewRequest(http.MethodPost, "/api/v1/c/reports_exports.request_billing_summary", nil)
		writeError(rec, req, lazErr)

		if rec.Code != http.StatusInternalServerError {
			t.Fatalf("status = %d, want 500", rec.Code)
		}

		// Server log carries the cause on the structured 5xx line. The buffer
		// may also hold the discrete "unmapped pg error" line, so scan for the
		// "request failed (5xx)" entry rather than unmarshaling the whole buffer.
		entry := findSlogEntry(t, buf, "lazuli: request failed (5xx)")
		cause, _ := entry["cause"].(string)
		if !strings.Contains(cause, "SQLSTATE "+sqlstate) || !strings.Contains(cause, "column") {
			t.Fatalf("server log cause = %q, want SQLSTATE %s + column detail", cause, sqlstate)
		}

		// Dev response detail.cause carries the same.
		var body map[string]any
		if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil {
			t.Fatalf("body not JSON: %v; body=%q", err, rec.Body.String())
		}
		detail, ok := body["detail"].(map[string]any)
		if !ok {
			t.Fatalf("dev body missing detail block: %q", rec.Body.String())
		}
		dCause, _ := detail["cause"].(string)
		if !strings.Contains(dCause, "SQLSTATE "+sqlstate) || !strings.Contains(dCause, "column") {
			t.Fatalf("dev detail.cause = %q, want SQLSTATE %s + column detail", dCause, sqlstate)
		}
		// Client message stays generic.
		if msg, _ := body["message"].(string); strings.Contains(msg, "SQLSTATE") || strings.Contains(msg, "org") {
			t.Fatalf("dev client message leaked pg internals: %q", msg)
		}
	})

	t.Run("prod masks detail but log keeps cause", func(t *testing.T) {
		t.Setenv("LAZULI_ENV", "production")
		buf := captureSlog(t)

		lazErr := classifyDBError("insert", newPgErr())
		rec := httptest.NewRecorder()
		req := httptest.NewRequest(http.MethodPost, "/api/v1/c/reports_exports.request_billing_summary", nil)
		writeError(rec, req, lazErr)

		var body map[string]any
		if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil {
			t.Fatalf("body not JSON: %v", err)
		}
		if _, ok := body["detail"]; ok {
			t.Fatalf("prod body leaked detail block: %q", rec.Body.String())
		}
		if !strings.Contains(buf.String(), "SQLSTATE "+sqlstate) {
			t.Fatalf("prod server log dropped the cause: %q", buf.String())
		}
	})
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
