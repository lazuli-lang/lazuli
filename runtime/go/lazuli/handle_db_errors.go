package lazuli

import (
	"errors"
	"net/http"

	"github.com/jackc/pgx/v5/pgconn"
)

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
		switch pgErr.Code {
		case "23505":
			return &Error{Status: http.StatusConflict, Code: CodeUniqueViolation,
				Message: stage + " failed: " + pgErr.Message, MessageKey: CodeUniqueViolation}
		case "23503":
			return &Error{Status: http.StatusBadRequest, Code: CodeForeignKeyViolation,
				Message: stage + " failed: " + pgErr.Message, MessageKey: CodeForeignKeyViolation}
		case "23502":
			return &Error{Status: http.StatusBadRequest, Code: CodeNotNullViolation,
				Message: stage + " failed: " + pgErr.Message, MessageKey: CodeNotNullViolation}
		case "23514":
			return &Error{Status: http.StatusBadRequest, Code: CodeCheckViolation,
				Message: stage + " failed: " + pgErr.Message, MessageKey: CodeCheckViolation}
		}
	}
	return &Error{Status: http.StatusInternalServerError, Code: CodeInternal,
		Message: stage + " failed: " + err.Error()}
}

// ClassifyDBError exposes the shared pgconn classifier to runtime subpackages
// that sit outside package lazuli but still need the same wire codes.
func ClassifyDBError(stage string, err error) *Error { return classifyDBError(stage, err) }
