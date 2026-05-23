package lazuli

import (
	"fmt"

	"github.com/jackc/pgx/v5"
)

// Transition advances a lifecycle column with an optimistic state precondition.
func Transition(ctx *Ctx, tx pgx.Tx, table, field, fromState, toState string, whereField string, whereValue any) error {
	if tx == nil {
		return &Error{Status: 500, Code: CodeInternal, Message: "transition requires transaction"}
	}
	sql := fmt.Sprintf("UPDATE %s SET %s = $3 WHERE %s = $1 AND %s = $2",
		quoteIdent(table), quoteIdent(field), quoteIdent(field), quoteIdent(whereField))
	tag, err := tx.Exec(ctxContext(ctx), sql, fromState, whereValue, toState)
	if err != nil {
		return classifyDBError("lifecycle transition", err)
	}
	if tag.RowsAffected() == 0 {
		return &Error{
			Status:     409,
			Code:       CodeLifecycleStateMismatch,
			Message:    "lifecycle state mismatch",
			MessageKey: CodeLifecycleStateMismatch,
			Data:       map[string]any{"expected_state": fromState, "next_state": toState},
		}
	}
	return nil
}
