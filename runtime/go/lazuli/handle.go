package lazuli

import (
	"context"
	"errors"
	"fmt"
	"reflect"
	"strings"

	"github.com/jackc/pgx/v5"
)

// Handle executes the command transactionally. The runtime calls this from
// the HTTP dispatcher (and later from job triggers and webhook callbacks).
//
// Pipeline:
//  1. enforce policy
//  2. run validators (placeholder; Phase F)
//  3. open transaction
//  4. apply effect (insert / update / delete)
//  5. publish events (post-commit, best-effort)
//  6. write audit (placeholder; later phase)
//  7. invalidate caches (placeholder; Phase H)
//
// The placeholders exist so the type signatures stay stable; later cuts
// fill them in without changing the generated dist code.
func (c *Command[I, O]) Handle(ctx *Ctx, input I) (O, error) {
	var zero O

	// 1. policy
	if err := enforcePolicy(ctx, c.Policy); err != nil {
		return zero, err
	}

	// 2. validators (TODO Phase F: invoke c.Validators in declaration order)
	_ = c.Validators

	// 3-4. effect inside a transaction
	var output O
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

	// 5. emits (post-commit, best-effort).
	publishEmits(ctx, c.Emits, c.EmitsTrace, c.Effect, input, output)

	// 6. audit (TODO: record according to c.Audit).
	// 7. invalidate (TODO Phase H: signal c.Invalidates to the cache layer).

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

// enforcePolicy walks the resolved atom list and grants access on the first
// match (OR semantics). Empty atom lists are a registration bug per the DSL
// invariant — fail closed with 500.
//
// Atom semantics for the spike:
//
//	@actor.user        → ctx.Actor == ActorUser
//	@actor.system      → ctx.Actor == ActorSystem
//	@actor.anonymous   → ctx.Actor == ActorAnonymous
//	@role.<name>       → ctx.User != nil AND <name> in ctx.User.Roles
//	@scope.public      → always
//	@scope.authenticated → ctx.User != nil
//	@scope.same_org    → ctx.Tenant != nil
//	@scope.self/owner  → returns false until target loading lands
func enforcePolicy(ctx *Ctx, p Policy) error {
	if len(p.Atoms) == 0 {
		return &Error{Status: 500, Code: CodeInternal,
			Message: "command/query registered with empty policy: " + p.Name}
	}
	for _, atom := range p.Atoms {
		if atomMatches(ctx, atom) {
			return nil
		}
	}
	return &Error{Status: 403, Code: CodePolicyDenied,
		Message: "no policy atom matches the active actor for " + p.Name}
}

// atomMatches reports whether the active context satisfies one policy atom.
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
	default:
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: fmt.Sprintf("unknown effect kind: %T", effect)}
	}
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

	if eff.Resource.Tenancy == TenancyOrg && ctx.Tenant != nil {
		cols = append(cols, quoteIdent("org_id"))
		values = append(values, ctx.Tenant.OrgID)
		placeholders = append(placeholders, fmt.Sprintf("$%d", len(values)))
	}

	sql := fmt.Sprintf(
		`INSERT INTO %s (%s) VALUES (%s) RETURNING *`,
		quoteIdent(eff.Resource.Name),
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
	return out, nil
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
	for col, src := range eff.Bind {
		val, err := resolveSource(ctx, src, input)
		if err != nil {
			return zero, err
		}
		values = append(values, val)
		sets = append(sets, fmt.Sprintf("%s = $%d", quoteIdent(col), len(values)))
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
		conds = append(conds, fmt.Sprintf("%s = $%d", quoteIdent(col), len(values)))
	}

	sql := fmt.Sprintf(
		`UPDATE %s SET %s WHERE %s RETURNING *`,
		quoteIdent(eff.Resource.Name),
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
		conds = append(conds, fmt.Sprintf("%s = $%d", quoteIdent(col), len(values)))
	}

	var sql string
	if eff.Resource.SoftDelete {
		sql = fmt.Sprintf(
			`UPDATE %s SET "deleted_at" = now(), "updated_at" = now() WHERE %s RETURNING *`,
			quoteIdent(eff.Resource.Name),
			strings.Join(conds, " AND "),
		)
	} else {
		sql = fmt.Sprintf(
			`DELETE FROM %s WHERE %s RETURNING *`,
			quoteIdent(eff.Resource.Name),
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
	case sourceCtx:
		return readCtx(ctx, src.path)
	case sourceTarget:
		return nil, &Error{Status: 501, Code: CodeInternal,
			Message: "target binding not yet implemented in runtime spike"}
	default:
		return nil, &Error{Status: 500, Code: CodeInternal,
			Message: fmt.Sprintf("unknown source kind: %d", src.kind)}
	}
}

// readPath looks up `path` (e.g. "name", "owner.id") on the given reflect
// Value. Tolerates struct field names that differ in case from the DSL by
// trying PascalCase variants.
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
	case "tenant.org_id":
		if ctx.Tenant == nil {
			return nil, &Error{Status: 400, Code: CodeTenantMismatch,
				Message: "no tenant in context"}
		}
		return ctx.Tenant.OrgID, nil
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
