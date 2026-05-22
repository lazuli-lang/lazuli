package lazuli

import (
	"context"
	"errors"
	"fmt"

	"github.com/jackc/pgx/v5"
)

var ownedByActorQueryRow = defaultOwnedByActorQueryRow

func defaultOwnedByActorQueryRow(ctx context.Context, sql string, args ...any) (pgx.Row, error) {
	if dbPool == nil {
		return nil, &Error{Status: 500, Code: CodeInternal, Message: "database not initialized"}
	}
	return DB().QueryRow(ctx, sql, args...), nil
}

// OwnedByActor reports whether the selected row's relation column is ctx.User.ID.
func OwnedByActor(ctx *Ctx, resourceTable string, relation string, resourceID ID) (bool, error) {
	if err := AuthGuard(ctx); err != nil {
		return false, err
	}
	sql := fmt.Sprintf("SELECT %s FROM %s WHERE id = $1", quoteIdent(relation), quoteIdent(resourceTable))
	row, err := ownedByActorQueryRow(ctxContext(ctx), sql, resourceID)
	if err != nil {
		return false, err
	}
	var owner ID
	if err := row.Scan(&owner); err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			return false, nil
		}
		return false, classifyDBError("owner check", err)
	}
	return owner == ctx.User.ID, nil
}
