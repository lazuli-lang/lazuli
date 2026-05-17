package lazuli

import (
	"context"
	"errors"
	"fmt"
	"reflect"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
)

// Handle executes the command transactionally. The runtime calls this from
// the HTTP dispatcher (and later from job triggers and webhook callbacks).
//
// Pipeline:
//  1. enforce policy
//  2. run validators (placeholder; Phase F)
//  3. open transaction for SQL effects
//  4. apply effect (insert / update / delete / return)
//  5. publish events (post-commit, best-effort)
//  6. write audit (placeholder; later phase)
//  7. invalidate caches (placeholder; Phase H)
//
// The placeholders exist so the type signatures stay stable; later cuts
// fill them in without changing the generated dist code.
func (c *Command[I, O]) Handle(ctx *Ctx, input I) (O, error) {
	var zero O

	// 1. policy
	if err := EvalPolicy(ctx, c.Policy); err != nil {
		return zero, err
	}

	// 2. validators — run in declaration order, abort on first failure.
	for _, ref := range c.Validators {
		fn := LookupValidator(ref.Canonical())
		if fn == nil {
			return zero, &Error{Status: 500, Code: CodeInternal,
				Message: "validator not registered: " + ref.Canonical()}
		}
		if err := fn(ctx, input); err != nil {
			// Wrap non-Lazuli errors so the response carries a typed
			// validation_failed envelope.
			if _, ok := err.(*Error); ok {
				return zero, err
			}
			return zero, &Error{Status: 400, Code: CodeValidationFailed,
				Message: err.Error()}
		}
	}

	var output O
	if eff, ok := c.Effect.(ReturnsEffect); ok {
		out, err := applyReturns[I, O](ctx, eff, input)
		if err != nil {
			return zero, err
		}
		output = out
	} else {
		// 3-4. SQL effects run inside a transaction.
		err := withTx(ctx, func(tx pgx.Tx) error {
			out, err := applyEffect[I, O](ctx, tx, c.Effect, input)
			if err != nil {
				return err
			}
			output = out
			return nil
		})
		if err != nil {
			return zero, err
		}
	}

	// 5. emits (post-commit, best-effort).
	publishEmits(ctx, c.Emits, c.EmitsTrace, c.Effect, input, output)

	// 6. audit (TODO: record according to c.Audit).

	// 7. invalidate caches whose results are now stale.
	if len(c.Invalidates) > 0 {
		queryCache.invalidateQueries(c.Invalidates)
	}

	return output, nil
}

// publishEmits derives event payloads from the producing command's effect
// (or from explicit Bind maps) and publishes them through the in-process
// event bus. Errors are logged but never propagated — emits are post-commit.
func publishEmits[I, O any](ctx *Ctx, emits []EventEmit, traces []EventTraceEmit, effect Effect, input I, output O) {
	for _, emit := range emits {
		payload, err := buildEmitPayload(ctx, emit, effect, input, output)
		if err != nil {
			continue
		}
		Publish(ctx, eventFromCtx(ctx, emit.Name, false, payload))
	}
	for _, t := range traces {
		payload, err := resolveBindings(ctx, t.Bind, input)
		if err != nil {
			continue
		}
		Publish(ctx, eventFromCtx(ctx, t.Name, true, payload))
	}
}

// eventFromCtx builds the common Event envelope from the active request ctx.
func eventFromCtx(ctx *Ctx, name string, trace bool, payload map[string]any) Event {
	e := Event{
		Name:       name,
		Trace:      trace,
		Tenant:     ctx.Tenant,
		Actor:      ctx.Actor,
		Payload:    payload,
		OccurredAt: ctx.Now,
	}
	if ctx.User != nil {
		uid := ctx.User.ID
		e.UserID = &uid
	}
	return e
}

// buildEmitPayload constructs the Payload map for one emit. Derived emits
// (from creates/updates/deletes) carry the producing row in O — the runtime
// reflects O into a map so subscribers see the exact persisted state.
// Explicit emits resolve their own Bind map against input/ctx.
func buildEmitPayload[I, O any](ctx *Ctx, emit EventEmit, effect Effect, input I, output O) (map[string]any, error) {
	if emit.From == FromExplicit {
		return resolveBindings(ctx, emit.Bind, input)
	}
	// Derived: payload is the producing row. Subscribers can read any
	// persisted field via the resulting map.
	return rowToMap(output), nil
}

// resolveBindings resolves a Bindings map of `column -> Source` to a
// `column -> value` map for explicit event payloads.
func resolveBindings[I any](ctx *Ctx, bind Bindings, input I) (map[string]any, error) {
	out := make(map[string]any, len(bind))
	for col, src := range bind {
		val, err := resolveSource(ctx, src, input)
		if err != nil {
			return nil, err
		}
		out[col] = val
	}
	return out, nil
}

// rowToMap reflects an exported struct into a {column: value} map keyed by
// each field's `db` tag (preferred) or `json` tag (fallback) or lowercased
// name (last resort). The output is what subscribers consume.
func rowToMap(v any) map[string]any {
	out := map[string]any{}
	rv := reflect.ValueOf(v)
	if rv.Kind() == reflect.Pointer {
		rv = rv.Elem()
	}
	if rv.Kind() != reflect.Struct {
		return out
	}
	rt := rv.Type()
	for i := 0; i < rv.NumField(); i++ {
		ft := rt.Field(i)
		if !ft.IsExported() {
			continue
		}
		name := ft.Tag.Get("db")
		if name == "" {
			name = strings.SplitN(ft.Tag.Get("json"), ",", 2)[0]
		}
		if name == "" {
			name = strings.ToLower(ft.Name)
		}
		if name == "-" {
			continue
		}
		out[name] = rv.Field(i).Interface()
	}
	return out
}

// atomMatches reports whether the active context satisfies one leaf
// policy atom. The full policy walker (`EvalPolicy` in policy.go)
// composes these results across `and` / `or` / `not` combinators when
// the atom slice carries a structured expression.
//
// Atom semantics:
//
//	@actor.user        → ctx.Actor == ActorUser
//	@actor.system      → ctx.Actor == ActorSystem
//	@actor.anonymous   → ctx.Actor == ActorAnonymous
//	@role.<name>       → ctx.User != nil AND <name> in ctx.User.Roles
//	@scope.public      → always
//	@scope.authenticated → ctx.User != nil
//	@scope.same_org    → ctx.Tenant != nil
//	@scope.self/owner  → returns false until target loading lands
//	rbac.role.<n>      → any of ctx.User.Roles satisfies HasRole(role, n)
//	rbac.permission.X  → any of ctx.User.Roles satisfies HasPermission(role, X)
//	predicate.authenticated → ctx.User != nil (combinator leaf)
func atomMatches(ctx *Ctx, atom PolicyAtom) bool {
	switch atom.Namespace {
	case "actor":
		switch atom.Name {
		case "user":
			return ctx.Actor == ActorUser
		case "system":
			return ctx.Actor == ActorSystem
		case "anonymous":
			return ctx.Actor == ActorAnonymous
		}
	case "role":
		if ctx.User == nil {
			return false
		}
		for _, r := range ctx.User.Roles {
			if r == atom.Name {
				return true
			}
		}
		return false
	case "scope":
		switch atom.Name {
		case "public":
			return true
		case "authenticated":
			return ctx.User != nil
		case "same_org":
			return ctx.Tenant != nil
		case "self", "owner":
			// These require a loaded target row to compare ownership.
			// Target binding is deferred; these atoms always fail closed
			// for now.
			return false
		}
	case "rbac.role":
		if ctx.User == nil {
			return false
		}
		for _, r := range ctx.User.Roles {
			if rbacHasRole(r, atom.Name) {
				return true
			}
		}
		return false
	case "rbac.permission":
		if ctx.User == nil {
			return false
		}
		for _, r := range ctx.User.Roles {
			if rbacHasPermission(r, atom.Name) {
				return true
			}
		}
		return false
	case "predicate":
		if atom.Name == "authenticated" {
			return ctx.User != nil
		}
	}
	return false
}

// applyEffect runs the registered effect against the given transaction. The
// runtime translates the typed effect (Creates/Updates/Deletes) into pgx
// queries.
func applyEffect[I, O any](ctx *Ctx, tx pgx.Tx, effect Effect, input I) (O, error) {
	var zero O
	if effect == nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "command has no effect"}
	}
	switch eff := effect.(type) {
	case CreatesEffect:
		return applyCreates[I, O](ctx, tx, eff, input)
	case UpdatesEffect:
		return applyUpdates[I, O](ctx, tx, eff, input)
	case DeletesEffect:
		return applyDeletes[I, O](ctx, tx, eff, input)
	case ReturnsEffect:
		return applyReturns[I, O](ctx, eff, input)
	default:
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: fmt.Sprintf("unknown effect kind: %T", effect)}
	}
}

// applyReturns calls the user-authored pure handler without entering the SQL
// effect pipeline. Policy, validators, rate limiting, and audit hooks live
// above this dispatch point in Command.Handle.
func applyReturns[I, O any](ctx *Ctx, eff ReturnsEffect, input I) (O, error) {
	var zero O
	if eff.Handler == nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "returns effect requires Handler"}
	}
	raw, err := eff.Handler(ctx, input)
	if err != nil {
		return zero, err
	}
	out, ok := raw.(O)
	if !ok {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "returns handler produced output of wrong type"}
	}
	return out, nil
}

// applyCreates resolves the bindings, builds an `INSERT ... RETURNING *`,
// and scans the inserted row into O. Tenancy auto-injection adds `org_id`
// when the resource declares `TenancyOrg` and the request has a tenant.
func applyCreates[I, O any](ctx *Ctx, tx pgx.Tx, eff CreatesEffect, input I) (O, error) {
	var zero O

	cols := make([]string, 0, len(eff.Bind))
	values := make([]any, 0, len(eff.Bind))
	placeholders := make([]string, 0, len(eff.Bind))
	for col, src := range eff.Bind {
		val, err := resolveSource(ctx, src, input)
		if err != nil {
			return zero, err
		}
		cols = append(cols, quoteIdent(col))
		values = append(values, val)
		placeholders = append(placeholders, fmt.Sprintf("$%d", len(values)))
	}

	// WAR-RUNTIME-MULTITENANT-01 closure: under `@policy.public`, the
	// session-bound tenant is nil, but the resource still requires
	// `org_id`. If the host app has registered a default-tenant
	// resolver via `lazuli.WithDefaultTenant`, invoke it now and pin
	// the resolved tenant for the remainder of the request. Calls
	// outside this codepath (authed commands with session-bound
	// tenant) are unaffected — the resolver runs only when ctx.Tenant
	// is currently nil.
	if eff.Resource.Tenancy == TenancyOrg && ctx.Tenant == nil {
		resolved, err := resolveDefaultTenant(ctx)
		if err != nil {
			return zero, &Error{
				Status: 500, Code: CodeInternal,
				Message: "default tenant resolver failed: " + err.Error(),
			}
		}
		if resolved != nil {
			ctx.Tenant = resolved
		}
	}

	if eff.Resource.Tenancy == TenancyOrg && ctx.Tenant != nil {
		cols = append(cols, quoteIdent("org_id"))
		values = append(values, ctx.Tenant.OrgID)
		placeholders = append(placeholders, fmt.Sprintf("$%d", len(values)))
	}

	// Encrypt @cap.Encrypted / @cap.E2ee bound columns before they
	// reach the driver. The runtime walks `Resource.EncryptedColumns`
	// (populated by codegen) and replaces each plaintext value with
	// AES-256-GCM ciphertext via `encryption.ForCtx`. No-op when the
	// resource has no encrypted fields.
	if err := encryptColumnValues(ctx, eff.Resource, cols, values); err != nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "insert encrypt failed: " + err.Error()}
	}

	sql := fmt.Sprintf(
		`INSERT INTO %s (%s) VALUES (%s) RETURNING *`,
		quoteResourceTable(eff.Resource.Name),
		strings.Join(cols, ", "),
		strings.Join(placeholders, ", "),
	)

	rows, err := tx.Query(ctx, sql, values...)
	if err != nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "insert failed: " + err.Error()}
	}
	defer rows.Close()

	out, err := pgx.CollectOneRow(rows, pgx.RowToStructByName[O])
	if err != nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "insert scan failed: " + err.Error()}
	}
	// Decrypt server-readable encrypted fields on the returned row so
	// downstream code (events, response bodies, audit) sees plaintext.
	if err := decryptScannedRow(ctx, eff.Resource, &out); err != nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "insert decrypt failed: " + err.Error()}
	}
	return out, nil
}

// whereConditionFragment renders a single WHERE-clause condition for
// a (column, source) pair, honoring the `sourceCtxOwnedVia` shape
// that expands to `<col> IN (SELECT id FROM <related> WHERE
// <owner_col> = $N)`. All other Source kinds collapse to the
// scalar `<col> = $N` form. The caller has already appended the
// resolved value to `values`, so `placeholderIdx == len(values)`
// is the 1-based position for the `$N` reference.
//
// Closes the relation-traversal arm of `@scope.owner` per the
// hostpoint Phase 4 capability audit (2026-05-17).
func whereConditionFragment(col string, src Source, placeholderIdx int) string {
	if src.kind == sourceCtxOwnedVia && src.subquery != nil {
		return fmt.Sprintf(
			"%s IN (SELECT id FROM %s WHERE %s = $%d)",
			quoteIdent(col),
			quoteIdent(src.subquery.relatedTable),
			quoteIdent(src.subquery.ownerColumn),
			placeholderIdx,
		)
	}
	return fmt.Sprintf("%s = $%d", quoteIdent(col), placeholderIdx)
}

// applyUpdates resolves the where + bind sources, builds an `UPDATE ... SET
// ... WHERE ... RETURNING *`, and scans the updated row into O. Adds
// tenancy + soft-delete scoping to the WHERE clause.
func applyUpdates[I, O any](ctx *Ctx, tx pgx.Tx, eff UpdatesEffect, input I) (O, error) {
	var zero O
	if len(eff.Where) == 0 {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "updates effect requires Where bindings"}
	}
	if len(eff.Bind) == 0 {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "updates effect requires Bind bindings"}
	}

	values := make([]any, 0, len(eff.Bind)+len(eff.Where))
	sets := make([]string, 0, len(eff.Bind))
	// Track the bind-side column names parallel to `values[0..n-1]` so
	// `encryptColumnValues` can match each value to its `EncryptedColumns`
	// scope. The WHERE-side bindings appended later are intentionally
	// excluded — encrypted columns are never WHERE-keys (the cipher
	// nonce makes equality lookups impossible).
	bindCols := make([]string, 0, len(eff.Bind))
	for col, src := range eff.Bind {
		val, err := resolveSource(ctx, src, input)
		if err != nil {
			return zero, err
		}
		values = append(values, val)
		bindCols = append(bindCols, col)
		sets = append(sets, fmt.Sprintf("%s = $%d", quoteIdent(col), len(values)))
	}
	// Encrypt @cap.Encrypted / @cap.E2ee bound columns before the
	// WHERE values are appended. Only the SET-side bindings are
	// candidates.
	if err := encryptColumnValues(ctx, eff.Resource, bindCols, values[:len(bindCols)]); err != nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "update encrypt failed: " + err.Error()}
	}
	// Always bump updated_at if the table has it.
	sets = append(sets, `"updated_at" = now()`)

	conds, condValues := baseScopeConditions(ctx, eff.Resource)
	values = append(values, condValues...)
	for col, src := range eff.Where {
		val, err := resolveSource(ctx, src, input)
		if err != nil {
			return zero, err
		}
		values = append(values, val)
		conds = append(conds, whereConditionFragment(col, src, len(values)))
	}

	sql := fmt.Sprintf(
		`UPDATE %s SET %s WHERE %s RETURNING *`,
		quoteResourceTable(eff.Resource.Name),
		strings.Join(sets, ", "),
		strings.Join(conds, " AND "),
	)

	rows, err := tx.Query(ctx, sql, values...)
	if err != nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "update failed: " + err.Error()}
	}
	defer rows.Close()

	out, err := pgx.CollectOneRow(rows, pgx.RowToStructByName[O])
	if errors.Is(err, pgx.ErrNoRows) {
		return zero, &Error{Status: 404, Code: CodeNotFound,
			Message: "no row matches update where clause"}
	}
	if err != nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "update scan failed: " + err.Error()}
	}
	if err := decryptScannedRow(ctx, eff.Resource, &out); err != nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "update decrypt failed: " + err.Error()}
	}
	return out, nil
}

// applyDeletes resolves the where bindings and either soft-deletes or
// hard-deletes the matching row, returning the affected row in O. Soft
// delete is the canonical path when `Resource.SoftDelete` is true.
func applyDeletes[I, O any](ctx *Ctx, tx pgx.Tx, eff DeletesEffect, input I) (O, error) {
	var zero O
	if len(eff.Where) == 0 {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "deletes effect requires Where bindings"}
	}

	conds, values := baseScopeConditions(ctx, eff.Resource)
	for col, src := range eff.Where {
		val, err := resolveSource(ctx, src, input)
		if err != nil {
			return zero, err
		}
		values = append(values, val)
		conds = append(conds, whereConditionFragment(col, src, len(values)))
	}

	var sql string
	if eff.Resource.SoftDelete {
		sql = fmt.Sprintf(
			`UPDATE %s SET "deleted_at" = now(), "updated_at" = now() WHERE %s RETURNING *`,
			quoteResourceTable(eff.Resource.Name),
			strings.Join(conds, " AND "),
		)
	} else {
		sql = fmt.Sprintf(
			`DELETE FROM %s WHERE %s RETURNING *`,
			quoteResourceTable(eff.Resource.Name),
			strings.Join(conds, " AND "),
		)
	}

	rows, err := tx.Query(ctx, sql, values...)
	if err != nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "delete failed: " + err.Error()}
	}
	defer rows.Close()

	out, err := pgx.CollectOneRow(rows, pgx.RowToStructByName[O])
	if errors.Is(err, pgx.ErrNoRows) {
		return zero, &Error{Status: 404, Code: CodeNotFound,
			Message: "no row matches delete where clause"}
	}
	if err != nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "delete scan failed: " + err.Error()}
	}
	if err := decryptScannedRow(ctx, eff.Resource, &out); err != nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "delete decrypt failed: " + err.Error()}
	}
	return out, nil
}

// resolveSource turns a binding Source into a concrete value at execution
// time. Reflection-based for the v0 spike; later cuts will let codegen emit
// typed accessors.
func resolveSource[I any](ctx *Ctx, src Source, input I) (any, error) {
	switch src.kind {
	case sourceConst:
		return src.value, nil
	case sourceInput:
		return readPath(reflect.ValueOf(input), src.path)
	case sourceCtx, sourceCtxOwnedVia:
		return readCtx(ctx, src.path)
	case sourceTarget:
		return nil, &Error{Status: 501, Code: CodeInternal,
			Message: "target binding not yet implemented in runtime spike"}
	case sourceFn:
		// WAR-VOCAB-CREATES-FN-CALL-01 closure: `@fn.<name>(<arg>...)`
		// resolves each arg source first, then invokes the registered
		// BindingFn with the resolved args. Fail-closed: unknown fn
		// names abort the command rather than emit a confusing
		// downstream type error.
		fn, ok := lookupBindingFn(src.path)
		if !ok {
			return nil, &Error{Status: 500, Code: CodeInternal,
				Message: "binding fn not registered: @fn." + src.path}
		}
		argSources, _ := src.value.([]Source)
		args := make([]any, 0, len(argSources))
		for _, arg := range argSources {
			v, err := resolveSource(ctx, arg, input)
			if err != nil {
				return nil, err
			}
			args = append(args, v)
		}
		return fn(ctx, args...)
	default:
		return nil, &Error{Status: 500, Code: CodeInternal,
			Message: fmt.Sprintf("unknown source kind: %d", src.kind)}
	}
}

// readPath looks up `path` (e.g. "name", "owner.id") on the given reflect
// Value. Tolerates struct field names that differ in case from the DSL by
// trying PascalCase variants.
//
// Single-level pointer fields are auto-dereferenced so downstream logic
// (filter equality, search pattern building, JSON encoding) sees the
// underlying value. A nil pointer field returns nil so `isNilOrZero`
// correctly skips optional filters.
func readPath(v reflect.Value, path string) (any, error) {
	if v.Kind() == reflect.Pointer {
		v = v.Elem()
	}
	for _, part := range strings.Split(path, ".") {
		if v.Kind() != reflect.Struct {
			return nil, &Error{Status: 500, Code: CodeInternal,
				Message: "input path " + path + " hit non-struct value"}
		}
		field := v.FieldByName(pascalCase(part))
		if !field.IsValid() {
			field = v.FieldByName(part)
		}
		if !field.IsValid() {
			return nil, &Error{Status: 400, Code: CodeBadRequest,
				Message: "input field not found: " + path}
		}
		v = field
	}
	if v.Kind() == reflect.Pointer {
		if v.IsNil() {
			return nil, nil
		}
		v = v.Elem()
	}
	return v.Interface(), nil
}

// readCtx pulls a value from the request context by canonical DSL path.
func readCtx(ctx *Ctx, path string) (any, error) {
	switch path {
	case "user", "user.id":
		if ctx.User == nil {
			return nil, &Error{Status: 401, Code: CodePolicyDenied,
				Message: "no authenticated user in context"}
		}
		return ctx.User.ID, nil
	case "user.org_id", "user.org":
		if ctx.User == nil {
			return nil, &Error{Status: 401, Code: CodePolicyDenied,
				Message: "no authenticated user in context"}
		}
		return ctx.User.OrgID, nil
	case "actor.user_id":
		if ctx.User == nil {
			return nil, &Error{Status: 401, Code: CodePolicyDenied,
				Message: "no authenticated user in context"}
		}
		return ctx.User.ID, nil
	case "actor.org_id":
		if ctx.User != nil {
			return ctx.User.OrgID, nil
		}
		if ctx.Tenant != nil {
			return ctx.Tenant.OrgID, nil
		}
		return nil, &Error{Status: 400, Code: CodeTenantMismatch,
			Message: "no actor org in context"}
	case "tenant.org_id":
		if ctx.Tenant == nil {
			return nil, &Error{Status: 400, Code: CodeTenantMismatch,
				Message: "no tenant in context"}
		}
		return ctx.Tenant.OrgID, nil
	case "now":
		if !ctx.Now.IsZero() {
			return ctx.Now, nil
		}
		return time.Now(), nil
	default:
		return nil, &Error{Status: 500, Code: CodeInternal,
			Message: "unknown ctx path: " + path}
	}
}

// pascalCase converts snake_case to PascalCase ("first_name" -> "FirstName").
func pascalCase(s string) string {
	parts := strings.Split(s, "_")
	for i, p := range parts {
		if len(p) == 0 {
			continue
		}
		parts[i] = strings.ToUpper(p[:1]) + p[1:]
	}
	return strings.Join(parts, "")
}

// Boot wires the runtime: opens the DB pool. Call once at process startup
// before the HTTP server begins serving.
func Boot(ctx context.Context, dbURL string) error {
	pool, err := connectDB(ctx, dbURL)
	if err != nil {
		return err
	}
	SetDB(pool)
	return nil
}
