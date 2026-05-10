package lazuli

import (
	"context"
	"fmt"
	"reflect"
	"strings"

	"github.com/jackc/pgx/v5"
)

// Handle executes the command transactionally. The runtime calls this from
// the HTTP dispatcher (and later from job triggers and webhook callbacks).
//
// Pipeline:
//  1. enforce policy (placeholder for v0)
//  2. run validators (placeholder for v0)
//  3. open transaction
//  4. apply effect (insert/update/delete)
//  5. publish events (placeholder for v0)
//  6. write audit (placeholder for v0)
//  7. invalidate caches (placeholder for v0)
//
// The placeholders exist so the type signatures stay stable; later cuts
// fill them in without changing the generated dist code.
func (c *Command[I, O]) Handle(ctx *Ctx, input I) (O, error) {
	var zero O

	// 1. policy
	if err := enforcePolicy(ctx, c.Policy); err != nil {
		return zero, err
	}

	// 2. validators (TODO: invoke c.Validators)
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

	// 5. emits (TODO: publish c.Emits)
	// 6. audit (TODO: record according to c.Audit)
	// 7. invalidate (TODO: signal c.Invalidates)

	return output, nil
}

// enforcePolicy is the v0 placeholder. Real implementation arrives with the
// auth/RBAC cut.
func enforcePolicy(ctx *Ctx, p Policy) error {
	if len(p.Atoms) == 0 {
		// Empty policy — the DSL invariant rejects this at compile time, so
		// reaching here means a registration bug. Fail closed.
		return &Error{Status: 500, Code: CodeInternal,
			Message: "command registered with empty policy: " + p.Name}
	}
	// Placeholder: accept everything. The real check inspects ctx.Actor /
	// ctx.User against p.Atoms.
	return nil
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
		return zero, &Error{Status: 501, Code: CodeInternal,
			Message: "updates effect not yet implemented in runtime spike"}
	case DeletesEffect:
		return zero, &Error{Status: 501, Code: CodeInternal,
			Message: "deletes effect not yet implemented in runtime spike"}
	default:
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: fmt.Sprintf("unknown effect kind: %T", effect)}
	}
}

// applyCreates resolves the bindings, builds an INSERT, and returns the
// inserted row's ID populated into a freshly-zeroed O.
//
// v0 spike: minimal SQL building. Real implementation will live in a query
// builder under runtime/lazuli/query.go.
func applyCreates[I, O any](ctx *Ctx, tx pgx.Tx, eff CreatesEffect, input I) (O, error) {
	var zero O

	// Resolve every binding to a value the database accepts.
	cols := make([]string, 0, len(eff.Bind))
	values := make([]any, 0, len(eff.Bind))
	placeholders := make([]string, 0, len(eff.Bind))
	for col, src := range eff.Bind {
		val, err := resolveSource(ctx, src, input)
		if err != nil {
			return zero, err
		}
		cols = append(cols, col)
		values = append(values, val)
		placeholders = append(placeholders, fmt.Sprintf("$%d", len(values)))
	}

	// Tenancy auto-injection.
	if eff.Resource.Tenancy == TenancyOrg && ctx.Tenant != nil {
		cols = append(cols, "org_id")
		values = append(values, ctx.Tenant.OrgID)
		placeholders = append(placeholders, fmt.Sprintf("$%d", len(values)))
	}

	sql := fmt.Sprintf(
		`INSERT INTO %s (%s) VALUES (%s) RETURNING id`,
		eff.Resource.Name,
		strings.Join(cols, ", "),
		strings.Join(placeholders, ", "),
	)

	var id ID
	if err := tx.QueryRow(ctx, sql, values...).Scan(&id); err != nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "insert failed: " + err.Error()}
	}

	// Populate ID on the output if it has an `ID` field.
	out := reflect.New(reflect.TypeOf(zero)).Elem()
	if idField := out.FieldByName("ID"); idField.IsValid() && idField.CanSet() {
		idField.SetInt(id)
	}
	return out.Interface().(O), nil
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
		if path == "user" {
			return ctx.User.ID, nil
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
