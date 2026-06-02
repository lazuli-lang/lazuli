package lazuli

import (
	"errors"
	"fmt"
	"os"
	"reflect"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
)

// RunList executes a `query.list` and returns the matching rows. The runtime
// builds a SELECT with optional filters, tenancy auto-injection, soft-delete
// scoping, ORDER BY, and LIMIT.
//
// When Cache is set, identical (tenant + args) calls are served from the
// in-memory query cache. Successful commands with matching `Invalidates`
// entries evict the cache.
func (q *Query[A, R]) RunList(ctx *Ctx, args A) ([]R, error) {
	if q.Kind != QueryList {
		return nil, &Error{Status: 500, Code: CodeInternal,
			Message: "RunList called on non-list query: " + q.Name}
	}
	if err := EvalPolicy(ctx, q.Policy); err != nil {
		return nil, err
	}
	if err := RunPrelude(ctx, q.Prelude); err != nil {
		return nil, err
	}

	if q.Cache != nil {
		if hit, ok := lookupQueryCache[[]R](ctx, q.Name, args); ok {
			return hit, nil
		}
	}

	sql, values, res, err := q.buildListSQL(ctx, args)
	if err != nil {
		return nil, err
	}

	rows, err := DB().Query(ctx, sql, values...)
	if err != nil {
		return nil, internalServerError(err, "list query failed")
	}
	defer rows.Close()

	out, err := pgx.CollectRows(rows, pgx.RowToStructByName[R])
	if err != nil {
		return nil, internalServerError(err, "list scan failed")
	}
	// Decrypt every row's server-readable encrypted fields before
	// returning. Resources without `@cap.Encrypted` skip the callback
	// (nil-guarded inside `decryptScannedRow`).
	for i := range out {
		if err := decryptScannedRow(ctx, res, &out[i]); err != nil {
			return nil, internalServerError(err, "list decrypt failed")
		}
	}
	if q.Cache != nil {
		storeQueryCache(ctx, q.Name, args, out, q.Cache.TTL)
	}
	_ = RunIncrement(ctx, q.Prelude)
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
	if err := EvalPolicy(ctx, q.Policy); err != nil {
		return zero, err
	}
	if err := RunPrelude(ctx, q.Prelude); err != nil {
		return zero, err
	}

	if q.Cache != nil {
		if hit, ok := lookupQueryCache[R](ctx, q.Name, args); ok {
			return hit, nil
		}
	}

	sql, values, res, err := q.buildLookupSQL(ctx, args)
	if err != nil {
		return zero, err
	}

	rows, err := DB().Query(ctx, sql, values...)
	if err != nil {
		return zero, internalServerError(err, "lookup query failed")
	}
	defer rows.Close()

	out, err := pgx.CollectOneRow(rows, pgx.RowToStructByName[R])
	if errors.Is(err, pgx.ErrNoRows) {
		return zero, &Error{Status: 404, Code: CodeNotFound,
			Message: "no row matches lookup keys"}
	}
	if err != nil {
		return zero, internalServerError(err, "lookup scan failed")
	}
	if err := decryptScannedRow(ctx, res, &out); err != nil {
		return zero, internalServerError(err, "lookup decrypt failed")
	}
	if q.Cache != nil {
		storeQueryCache(ctx, q.Name, args, out, q.Cache.TTL)
	}
	_ = RunIncrement(ctx, q.Prelude)
	return out, nil
}

// RunSQL executes a SQL-backed `query.view` and scans the result into the
// generated row shape. Codegen supplies SQLArgs in the same order as authored
// `params`; SQLMany selects list vs single-row scan.
func (q *Query[A, R]) RunSQL(ctx *Ctx, args A) (any, error) {
	if q.Kind != QueryView && q.Kind != QuerySQL {
		return nil, &Error{Status: 500, Code: CodeInternal,
			Message: "RunSQL called on non-SQL query: " + q.Name}
	}
	if err := EvalPolicy(ctx, q.Policy); err != nil {
		return nil, err
	}
	if err := RunPrelude(ctx, q.Prelude); err != nil {
		return nil, err
	}

	sqlText, err := q.sqlText()
	if err != nil {
		return nil, err
	}
	var values []any
	if q.SQLArgs != nil {
		values = q.SQLArgs(args)
	}

	rows, err := DB().Query(ctx, sqlText, values...)
	if err != nil {
		return nil, internalServerError(err, "sql query failed")
	}
	defer rows.Close()

	// Select the row collector by the destination type's Kind. A struct
	// `R` (the common `query.view` / record-returning `query.sql` shape)
	// maps column-by-name via `RowToStructByName`. A SCALAR `R` (e.g.
	// `query.sql ... returns int64 / Text / bool`) has no struct fields to
	// map and previously panicked here with
	// `reflect: NumField of non-struct type` — those single-column results
	// scan positionally via `pgx.RowTo[R]`. (W4-2 QUERY-SQL-SCALAR-PANIC.)
	scalar := sqlReturnIsScalar[R]()

	if q.SQLMany {
		var out any
		if scalar {
			out, err = pgx.CollectRows(rows, pgx.RowTo[R])
		} else {
			out, err = pgx.CollectRows(rows, pgx.RowToStructByName[R])
		}
		if err != nil {
			return nil, internalServerError(err, "sql scan failed")
		}
		_ = RunIncrement(ctx, q.Prelude)
		return out, nil
	}

	var out any
	if scalar {
		out, err = pgx.CollectOneRow(rows, pgx.RowTo[R])
	} else {
		out, err = pgx.CollectOneRow(rows, pgx.RowToStructByName[R])
	}
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, &Error{Status: 404, Code: CodeNotFound,
			Message: "no row matches SQL query"}
	}
	if err != nil {
		return nil, internalServerError(err, "sql scan failed")
	}
	_ = RunIncrement(ctx, q.Prelude)
	return out, nil
}

// sqlReturnIsScalar reports whether the SQL row destination type R is a
// single scalar (string/number/bool/[]byte/time, possibly pointer-wrapped)
// rather than a struct that `pgx.RowToStructByName` can map by column name.
// Struct returns (records, resources) keep the by-name path; everything else
// scans the single result column positionally via `pgx.RowTo`.
//
// `time.Time` is a struct but is a scalar SQL value, so it is treated as
// scalar (RowToStructByName on it would map by exported field names, which is
// never what an authored `returns Timestamp` means).
func sqlReturnIsScalar[R any]() bool {
	var zero R
	t := reflect.TypeOf(&zero).Elem()
	for t.Kind() == reflect.Pointer {
		t = t.Elem()
	}
	if t.Kind() != reflect.Struct {
		return true
	}
	// time.Time is the one struct that represents a scalar SQL column.
	return t.PkgPath() == "time" && t.Name() == "Time"
}

func (q *Query[A, R]) sqlText() (string, error) {
	if strings.TrimSpace(q.SQLText) != "" {
		return q.SQLText, nil
	}
	if strings.TrimSpace(q.SQL) == "" {
		return "", &Error{Status: 500, Code: CodeInternal,
			Message: "SQL query has no SQL body or path: " + q.Name}
	}
	raw, err := os.ReadFile(q.SQL)
	if err != nil {
		return "", internalServerError(err, "read SQL query file failed")
	}
	return string(raw), nil
}

func (q *Query[A, R]) buildListSQL(ctx *Ctx, args A) (string, []any, *resourceErased, error) {
	res, err := q.resourceErased()
	if err != nil {
		return "", nil, nil, err
	}

	conds, values, err := baseScopeConditions(ctx, res, 1)
	if err != nil {
		return "", nil, nil, err
	}

	for _, f := range q.Filters {
		val, err := resolveSource(ctx, f.When, args)
		if err != nil {
			return "", nil, nil, err
		}
		if isNilOrZero(val) {
			continue // optional filter; skip when args don't carry it
		}
		values = append(values, val)
		conds = append(conds, whereConditionFragment(f.Column, f.When, len(values)))
	}

	if q.Search != nil && len(q.Search.Over) > 0 {
		val, err := resolveSource(ctx, q.Search.Source, args)
		if err != nil {
			return "", nil, nil, err
		}
		if !isNilOrZero(val) {
			pattern := buildSearchPattern(fmt.Sprint(val), q.Search.Mode)
			values = append(values, pattern)
			placeholder := fmt.Sprintf("$%d", len(values))
			ors := make([]string, 0, len(q.Search.Over))
			for _, col := range q.Search.Over {
				ors = append(ors, fmt.Sprintf("%s ILIKE %s", quoteIdent(col), placeholder))
			}
			conds = append(conds, "("+strings.Join(ors, " OR ")+")")
		}
	}

	// Resource.Name is the authored PascalCase (`User`, `UserSession`);
	// the migration emit snake_lowercases it (`user`, `user_session`).
	// `quoteResourceTable` applies the same transform so SELECT round-
	// trips with the migrated schema. The legacy `quoteIdent(res.Name)`
	// produced `"UserSession"` (case-sensitive, no table) → 42P01.
	sql := "SELECT * FROM " + quoteResourceTable(res.Name)
	if len(conds) > 0 {
		sql += " WHERE " + strings.Join(conds, " AND ")
	}
	sql += " " + buildOrder(q.Order)
	sql += fmt.Sprintf(" LIMIT %d", paginateOrDefault(q.Paginate))

	return sql, values, res, nil
}

func (q *Query[A, R]) buildLookupSQL(ctx *Ctx, args A) (string, []any, *resourceErased, error) {
	res, err := q.resourceErased()
	if err != nil {
		return "", nil, nil, err
	}
	if len(q.LookupBy) == 0 {
		return "", nil, nil, &Error{Status: 500, Code: CodeInternal,
			Message: "lookup query has no LookupBy keys: " + q.Name}
	}

	conds, values, err := baseScopeConditions(ctx, res, 1)
	if err != nil {
		return "", nil, nil, err
	}

	for _, k := range q.LookupBy {
		val, err := resolveSource(ctx, k.Source, args)
		if err != nil {
			return "", nil, nil, err
		}
		if isNilOrZero(val) {
			return "", nil, nil, &Error{Status: 400, Code: CodeBadRequest,
				Message: "lookup key " + k.Column + " is required"}
		}
		values = append(values, val)
		// `ir-resource-conventions-owner-scope.md` §8.3 — owner-scope
		// lookup keys carry `FromCtxOwnedVia` sources that must lower to
		// `<col> IN (SELECT id FROM <related> WHERE <through> = $N)`.
		// Route the cond construction through `whereConditionFragment`
		// so this shape expands consistently with `applyUpdates` /
		// `applyDeletes`. Direct scalar sources still collapse to the
		// `<col> = $N` shape (the fragment helper is a no-op for them).
		conds = append(conds, whereConditionFragment(k.Column, k.Source, len(values)))
	}

	// Same snake_case lowering as `RunList` — `Resource.Name` is the
	// authored PascalCase while the migrated table is snake_lower.
	// `quoteResourceTable` is the canonical transform; see comment
	// above on the list path.
	sql := "SELECT * FROM " + quoteResourceTable(res.Name)
	if len(conds) > 0 {
		sql += " WHERE " + strings.Join(conds, " AND ")
	}
	sql += " LIMIT 1"

	return sql, values, res, nil
}

// lookupQueryCache reads a cached value for the given query+args under the
// active tenant. The result is type-asserted back to T; mismatches indicate
// a programming error (cache shared across types) and treat as a miss.
func lookupQueryCache[T any](ctx *Ctx, name string, args any) (T, bool) {
	var zero T
	key, err := cacheKeyFor(ctx, name, args)
	if err != nil {
		return zero, false
	}
	now := time.Now()
	if ctx != nil && !ctx.Now.IsZero() {
		now = ctx.Now
	}
	v, ok := queryCache.get(key, now)
	if !ok {
		return zero, false
	}
	typed, ok := v.(T)
	if !ok {
		return zero, false
	}
	return typed, true
}

// storeQueryCache writes the given result under the canonical cache key for
// (query, tenant, args). Failures to compute the key (e.g. unencodable args)
// silently skip caching — correctness wins over speedup.
func storeQueryCache(ctx *Ctx, name string, args, value any, ttl time.Duration) {
	key, err := cacheKeyFor(ctx, name, args)
	if err != nil {
		return
	}
	now := time.Now()
	if ctx != nil && !ctx.Now.IsZero() {
		now = ctx.Now
	}
	queryCache.put(key, name, value, ttl, now)
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
// with their own filters / lookup keys. placeholderStart is the 1-based
// index for the first bound value this helper appends, so callers that already
// populated values can keep all $N references unique in a single statement.
func baseScopeConditions(ctx *Ctx, res *resourceErased, placeholderStart int) ([]string, []any, error) {
	var conds []string
	var values []any
	nextPlaceholder := placeholderStart

	if res.SoftDelete {
		conds = append(conds, "deleted_at IS NULL")
	}

	if res.Tenancy == TenancyOrg {
		// SECURITY (SEC-H2): TenancyOrg resources MUST have a tenant
		// attached to the request context. Nil-Tenant + TenancyOrg is
		// a programming/config error that previously caused silent
		// cross-org reads by dropping the org_id predicate. Fail closed.
		if ctx == nil || ctx.Tenant == nil {
			return nil, nil, tenantRequiredError()
		}
		conds = append(conds, fmt.Sprintf("org_id = $%d", nextPlaceholder))
		values = append(values, ctx.Tenant.OrgID)
		nextPlaceholder++
	}

	// TenancyNone is the explicit global opt-in: no scoping predicate.
	// Callers must declare global scope in .lzi.
	return conds, values, nil
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

// quoteResourceTable computes the SQL table name for a resource and
// quotes it. Resources are declared in PascalCase (e.g. `User`,
// `ServiceTransaction`); the migration emit lower-snakes them to
// `user`, `service_transaction`. The runtime applies the same
// transformation so INSERT / UPDATE / DELETE round-trip with the
// migrated schema.
func quoteResourceTable(name string) string {
	return quoteIdent(snakeCase(name))
}

// buildSearchPattern wraps the term according to SearchMode, escaping
// `%` and `_` so user-provided patterns don't leak SQL wildcards.
func buildSearchPattern(term string, mode SearchMode) string {
	escaped := strings.NewReplacer(`\`, `\\`, `%`, `\%`, `_`, `\_`).Replace(term)
	switch mode {
	case SearchStartsWith:
		return escaped + "%"
	case SearchExact:
		return escaped
	default: // SearchContains
		return "%" + escaped + "%"
	}
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
