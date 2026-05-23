package lazuli

import (
	"fmt"
	"reflect"
	"sort"
	"strings"

	"github.com/jackc/pgx/v5"
)

// PartialUpdate updates non-nil fields on a row addressed by id.
func PartialUpdate(ctx *Ctx, tx pgx.Tx, table string, target ID, fields map[string]any) error {
	if tx == nil {
		return &Error{Status: 500, Code: CodeInternal, Message: "partial update requires transaction"}
	}
	cols := make([]string, 0, len(fields))
	for col, val := range fields {
		if !isNilUpdateValue(val) {
			cols = append(cols, col)
		}
	}
	if len(cols) == 0 {
		return nil
	}
	sort.Strings(cols)
	values := make([]any, 0, len(cols)+1)
	sets := make([]string, 0, len(cols))
	for _, col := range cols {
		values = append(values, fields[col])
		sets = append(sets, fmt.Sprintf("%s = $%d", quoteIdent(col), len(values)))
	}
	values = append(values, target)
	sql := fmt.Sprintf("UPDATE %s SET %s WHERE %s = $%d",
		quoteIdent(table), strings.Join(sets, ", "), quoteIdent("id"), len(values))
	tag, err := tx.Exec(ctxContext(ctx), sql, values...)
	if err != nil {
		return classifyDBError("partial update", err)
	}
	if tag.RowsAffected() == 0 {
		return &Error{Status: 404, Code: CodeNotFound, Message: "no row matches partial update target", MessageKey: CodeNotFound}
	}
	return nil
}

func isNilUpdateValue(v any) bool {
	if v == nil {
		return true
	}
	rv := reflect.ValueOf(v)
	switch rv.Kind() {
	case reflect.Chan, reflect.Func, reflect.Interface, reflect.Map, reflect.Pointer, reflect.Slice:
		return rv.IsNil()
	default:
		return false
	}
}
