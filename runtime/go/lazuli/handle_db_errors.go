package lazuli

import (
	"errors"
	"log/slog"
	"net/http"
	"sync"

	"github.com/jackc/pgx/v5/pgconn"
)

// uniqueViolationCodes maps a deterministically-named Postgres UNIQUE
// constraint (`<table>_<field_slug>_key` / `<table>_<field_slug>_uidx`) to the
// per-constraint domain error code authored via `unique a, b error <CODE>`.
// Generated per-feature glue (`<feature>/unique_codes.gen.go`) populates it in
// `init()`; `classifyDBError` consults it on a 23505 so the violation surfaces
// the pinned domain code instead of the generic `unique_violation`.
//
// This is the unique-constraint twin of `restrict on_delete ... error <CODE>`
// (which pins a referential-guard code via NewReferencedInUseError); both let a
// pilot replace a hand-written constraint→domain-code remap with one DSL clause.
var (
	uniqueViolationCodesMu sync.RWMutex
	uniqueViolationCodes   = map[string]string{}
)

// RegisterUniqueViolationCode binds a Postgres UNIQUE constraint name to the
// domain error code a 23505 violation on it should surface. Idempotent; called
// from generated `init()` glue. An empty name or code is ignored (defensive —
// codegen never emits a blank pair).
func RegisterUniqueViolationCode(constraintName, domainCode string) {
	if constraintName == "" || domainCode == "" {
		return
	}
	uniqueViolationCodesMu.Lock()
	defer uniqueViolationCodesMu.Unlock()
	uniqueViolationCodes[constraintName] = domainCode
}

// lookupUniqueViolationCode returns the registered domain code for a
// constraint name, or "" when none was authored.
func lookupUniqueViolationCode(constraintName string) string {
	if constraintName == "" {
		return ""
	}
	uniqueViolationCodesMu.RLock()
	defer uniqueViolationCodesMu.RUnlock()
	return uniqueViolationCodes[constraintName]
}

// resetUniqueViolationCodes clears the registry. Test-only.
func resetUniqueViolationCodes() {
	uniqueViolationCodesMu.Lock()
	defer uniqueViolationCodesMu.Unlock()
	uniqueViolationCodes = map[string]string{}
}

// classifyDBError maps a pgx/Postgres error to a typed Lazuli envelope so the
// client sees a stable wire `code` (e.g. `unique_violation`) and a localizable
// `message` instead of the raw SQLSTATE / constraint name. Falls back to
// `CodeInternal` / 500 when `err` is not a `*pgconn.PgError` — the legacy
// 500-internal behaviour for unclassified errors is preserved verbatim.
//
// SQLSTATE map (Postgres class 23 — integrity constraint violation):
//   - 23505 unique_violation       → 409
//   - 23503 foreign_key_violation  → 400
//   - 23502 not_null_violation     → 400
//   - 23514 check_violation        → 400
//
// `stage` is a short label (`"insert"`, `"update"`, `"delete"`) embedded in
// the raw Message so audits / 5xx-data-exposure consumers still see the
// originating site. The bare-code MessageKey wires the resolver chain to
// the builtin L3 fallback; feature overrides (`errors <code> message
// @translation.<key>`) light up at L2 of the resolver chain.
//
// DB-INTEGRITY-CATALOG-EXT (2026-05-19): extends the closed catalog of 8
// framework error codes with 4 new db-integrity codes. Wire-thin: ≤30 LOC
// of branching, no new go.mod deps (pgconn is transitive via pgx/v5).
func classifyDBError(stage string, err error) *Error {
	var pgErr *pgconn.PgError
	if errors.As(err, &pgErr) {
		// SECURITY: pgErr.Message must never reach the client envelope; it can
		// expose constraint, table, and column names. Keep details in logs.
		switch pgErr.Code {
		case "23505":
			// Per-constraint domain code (`unique a, b error <CODE>`): when the
			// violated constraint name was registered by generated glue, surface
			// the pinned domain code (still 409) so the client branches on the
			// meaningful code instead of the generic `unique_violation`. The
			// constraint name itself NEVER reaches the envelope Message (it can
			// leak table/column internals); only the registered code does.
			code := CodeUniqueViolation
			if domain := lookupUniqueViolationCode(pgErr.ConstraintName); domain != "" {
				code = domain
			}
			return &Error{Status: http.StatusConflict, Code: code,
				Message: stage + " failed", MessageKey: code}
		case "23503":
			return &Error{Status: http.StatusBadRequest, Code: CodeForeignKeyViolation,
				Message: stage + " failed", MessageKey: CodeForeignKeyViolation}
		case "23502":
			return &Error{Status: http.StatusBadRequest, Code: CodeNotNullViolation,
				Message: stage + " failed", MessageKey: CodeNotNullViolation}
		case "23514":
			return &Error{Status: http.StatusBadRequest, Code: CodeCheckViolation,
				Message: stage + " failed", MessageKey: CodeCheckViolation}
		default:
			slog.Error("lazuli: unmapped pg error",
				"stage", stage,
				"code", pgErr.Code,
				"message", pgErr.Message,
				"constraint", pgErr.ConstraintName,
				"column", pgErr.ColumnName,
			)
			return &Error{Status: http.StatusInternalServerError, Code: CodeInternal,
				Message: stage + " failed"}
		}
	}
	if err != nil {
		slog.Error("lazuli: unmapped db error", "stage", stage, "err", err.Error())
	}
	return &Error{Status: http.StatusInternalServerError, Code: CodeInternal,
		Message: stage + " failed"}
}

// ClassifyDBError exposes the shared pgconn classifier to runtime subpackages
// that sit outside package lazuli but still need the same wire codes.
func ClassifyDBError(stage string, err error) *Error { return classifyDBError(stage, err) }
