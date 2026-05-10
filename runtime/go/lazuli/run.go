package lazuli

import (
	"errors"
	"fmt"
	"reflect"
	"strings"

	"github.com/jackc/pgx/v5"
)

// RunList executes a `query.list` and returns the matching rows. The runtime
// builds a SELECT with optional filters, tenancy auto-injection, soft-delete
// scoping, ORDER BY, and LIMIT.
//
// Phase B spike: no search, no cache, no `scope override` semantics. Those
// arrive with later cuts; the type signature stays stable.
func (q *Query[A, R]) RunList(ctx *Ctx, args A) ([]R, error) {
	if q.Kind != QueryList {
		return nil, &Error{Status: 500, Code: CodeInternal,
			Message: "RunList called on non-list query: " + q.Name}
	}
	if err := enforcePolicy(ctx, q.Policy); err != nil {
		return nil, err
	}

	res, err := q.resourceErased()
	if err != nil {
		return nil, err
	}

	conds, values := baseScopeConditions(ctx, res)

	for _, f := range q.Filters {
		val, err := resolveSource(ctx, f.When, args)
		if err != nil {
			return nil, err
		}
		if isNilOrZero(val) {
			continue // optional filter; skip when args don't carry it
		}
		conds = append(conds, fmt.Sprintf("%s = $%d", quoteIdent(f.Column), len(values)+1))
		values = append(values, val)
	}

	sql := "SELECT * FROM " + quoteIdent(res.Name)
	if len(conds) > 0 {
		sql += " WHERE " + strings.Join(conds, " AND ")
	}
	sql += " " + buildOrder(q.Order)
	sql += fmt.Sprintf(" LIMIT %d", paginateOrDefault(q.Paginate))

	rows, err := DB().Query(ctx, sql, values...)
	if err != nil {
		return nil, &Error{Status: 500, Code: CodeInternal,
			Message: "list query failed: " + err.Error()}
	}
	defer rows.Close()

	out, err := pgx.CollectRows(rows, pgx.RowToStructByName[R])
	if err != nil {
		return nil, &Error{Status: 500, Code: CodeInternal,
			Message: "list scan failed: " + err.Error()}
	}
	return out, nil
}

// RunLookup executes a `query.lookup` and returns a single row, or an error
// envelope with `not_found` status when no row matches.
func (q *Query[A, R]) RunLookup(ctx *Ctx, args A) (R, error) {
	var zero R
	if q.Kind != QueryLookup {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "RunLookup called on non-lookup query: " + q.Name}
	}
	if err := enforcePolicy(ctx, q.Policy); err != nil {
		return zero, err
	}

	res, err := q.resourceErased()
	if err != nil {
		return zero, err
	}
	if len(q.LookupBy) == 0 {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "lookup query has no LookupBy keys: " + q.Name}
	}

	conds, values := baseScopeConditions(ctx, res)

	for _, k := range q.LookupBy {
		val, err := resolveSource(ctx, k.Source, args)
		if err != nil {
			return zero, err
		}
		if isNilOrZero(val) {
			return zero, &Error{Status: 400, Code: CodeBadRequest,
				Message: "lookup key " + k.Column + " is required"}
		}
		conds = append(conds, fmt.Sprintf("%s = $%d", quoteIdent(k.Column), len(values)+1))
		values = append(values, val)
	}

	sql := "SELECT * FROM " + quoteIdent(res.Name)
	if len(conds) > 0 {
		sql += " WHERE " + strings.Join(conds, " AND ")
	}
	sql += " LIMIT 1"

	rows, err := DB().Query(ctx, sql, values...)
	if err != nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "lookup query failed: " + err.Error()}
	}
	defer rows.Close()

	out, err := pgx.CollectOneRow(rows, pgx.RowToStructByName[R])
	if errors.Is(err, pgx.ErrNoRows) {
		return zero, &Error{Status: 404, Code: CodeNotFound,
			Message: "no row matches lookup keys"}
	}
	if err != nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "lookup scan failed: " + err.Error()}
	}
	return out, nil
}

// resourceErased recovers the resource view a query reads from. The Query's
// Resource field is `any` so the registry can hold heterogeneous queries;
// every concrete value is a `*Resource[T]` for some T.
func (q *Query[A, R]) resourceErased() (*resourceErased, error) {
	if q.Resource == nil {
		return nil, &Error{Status: 500, Code: CodeInternal,
			Message: "query " + q.Name + " has no resource"}
	}
	switch r := q.Resource.(type) {
	case *Resource[R]:
		return r.erased(), nil
	default:
		// Use reflection as a fallback when Resource[T] != Resource[R].
		// Real generated code always uses matching types; this branch is
		// defensive for hand-written spike code.
		v := reflect.ValueOf(q.Resource)
		if v.Kind() == reflect.Pointer {
			v = v.Elem()
		}
		erased := v.MethodByName("erased")
		if !erased.IsValid() {
			return nil, &Error{Status: 500, Code: CodeInternal,
				Message: "query " + q.Name + " resource is not a *Resource[T]"}
		}
		out := erased.Call(nil)
		if len(out) != 1 {
			return nil, &Error{Status: 500, Code: CodeInternal,
				Message: "unexpected erased() arity"}
		}
		ret, ok := out[0].Interface().(*resourceErased)
		if !ok {
			return nil, &Error{Status: 500, Code: CodeInternal,
				Message: "erased() did not return *resourceErased"}
		}
		return ret, nil
	}
}

// baseScopeConditions returns the WHERE-clause fragments every query gets:
// soft-delete filter and tenant scoping. Generated queries can extend these
// with their own filters / lookup keys.
func baseScopeConditions(ctx *Ctx, res *resourceErased) ([]string, []any) {
	var conds []string
	var values []any

	if res.SoftDelete {
		conds = append(conds, "deleted_at IS NULL")
	}

	if res.Tenancy == TenancyOrg && ctx.Tenant != nil {
		conds = append(conds, fmt.Sprintf("org_id = $%d", len(values)+1))
		values = append(values, ctx.Tenant.OrgID)
	}

	return conds, values
}

// buildOrder produces the ORDER BY clause. Empty Order falls back to the
// DSL invariant `order created_at desc`.
func buildOrder(order []OrderClause) string {
	if len(order) == 0 {
		return "ORDER BY created_at DESC"
	}
	parts := make([]string, 0, len(order))
	for _, o := range order {
		dir := "ASC"
		if o.Desc {
			dir = "DESC"
		}
		parts = append(parts, quoteIdent(o.Column)+" "+dir)
	}
	return "ORDER BY " + strings.Join(parts, ", ")
}

// paginateOrDefault returns the configured page size, or 100 when omitted.
func paginateOrDefault(n int) int {
	if n <= 0 {
		return 100
	}
	return n
}

// quoteIdent wraps an identifier in double quotes for safe SQL composition.
// Generated code controls the column names so injection is not the threat
// model, but quoting protects against reserved words colliding.
func quoteIdent(name string) string {
	// Reject anything that isn't a safe ASCII identifier early — generated
	// code never produces those, so a violation is a programming error.
	for _, c := range name {
		ok := (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
			(c >= '0' && c <= '9') || c == '_'
		if !ok {
			panic("lazuli: refusing to quote suspicious identifier: " + name)
		}
	}
	return `"` + name + `"`
}

// isNilOrZero reports whether v is a nil pointer/interface or a zero value
// of its type. Used to decide when an optional filter should be skipped.
func isNilOrZero(v any) bool {
	if v == nil {
		return true
	}
	rv := reflect.ValueOf(v)
	switch rv.Kind() {
	case reflect.Pointer, reflect.Interface, reflect.Map, reflect.Slice, reflect.Chan, reflect.Func:
		return rv.IsNil()
	default:
		return rv.IsZero()
	}
}
