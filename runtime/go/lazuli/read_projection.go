package lazuli

import (
	"reflect"
	"strings"
)

// readProjection builds the SELECT column list for a resource-backed read
// (`query.list` / `query.lookup`), honoring per-field read policies.
//
// W1-2 SEC-FIELDPOLICY-READ-NULL. The legacy read path emitted `SELECT *`,
// which pulled EVERY column from the database — including columns gated by a
// `policies fields <R>` `read:` policy (`password_hash read: @actor.system`,
// role-gated `@pii` fields, ...). Those restricted columns were only NOT
// leaked by luck: when the destination struct happened to omit them, or when
// codegen tagged them `json:"-"`. Where the struct carried such a field the
// value leaked to the wire.
//
// The projection closes that hole at the SQL boundary: it SELECTs an explicit
// list derived from the destination row type's `db` tags (never `*`), and for
// any column whose declared read-policy the ACTIVE actor fails it projects a
// MASKING CONSTANT (`maskedProjection`) instead of the real column. The
// restricted value never leaves the database for an unauthorized caller, while
// the column NAME (via `... AS "<col>"`) stays in the result set so
// `pgx.RowToStructByName` still maps every struct field — it errors on a
// struct field with no matching result column, so dropping the column outright
// would break the scan; masking its value keeps the scan whole.
//
// `rowType` is the destination row struct type (the `R` of `Query[A, R]`,
// pointer-unwrapped). When it is not a struct, or carries no usable `db`
// tags (scalar returns, dynamic shapes), the function returns `"*"` so the
// caller keeps the previous behaviour — there is no declared-column list to
// project from. Field policies cannot apply to a shape the runtime cannot
// enumerate; such shapes are the `query.sql` escape hatch the caller owns.
//
// Tenant/org scoping, WHERE filters, ORDER BY and LIMIT are unaffected — this
// only replaces the column list that follows `SELECT`.
func readProjection(ctx *Ctx, res *resourceErased, rowType reflect.Type) string {
	cols := dbColumnsOf(rowType)
	if len(cols) == 0 {
		// No enumerable column list (scalar / non-struct / dynamic shape).
		// Preserve the prior `SELECT *` behaviour; field policies have no
		// column surface to gate here.
		return "*"
	}

	var policies map[string]Policy
	if res != nil {
		policies = res.FieldReadPolicies
	}

	parts := make([]string, 0, len(cols))
	for _, col := range cols {
		if pol, gated := policies[col.name]; gated {
			// A field-level read policy gates this column. Project the real
			// column only when the active actor satisfies the policy;
			// otherwise project a constant that masks the restricted value so
			// it never leaves the database for an unauthorized caller.
			if EvalPolicy(ctx, pol) != nil {
				parts = append(parts, maskedProjection(col))
				continue
			}
		}
		parts = append(parts, quoteIdent(col.name))
	}
	return strings.Join(parts, ", ")
}

// maskedProjection renders the SELECT expression that masks a gated
// column's value. The destination struct field type decides the masking
// literal so `pgx.RowToStructByName` can still scan it:
//
//   - A POINTER field (`*string`, `*int64`, `*Time`, ...) scans a SQL
//     NULL fine, so the column is projected as `NULL AS "<col>"` — the
//     caller sees a nil pointer for the restricted field.
//   - A non-pointer (value) field cannot receive a SQL NULL (pgx errors
//     `cannot scan NULL into *string`). Project a typed zero LITERAL whose
//     postgres type maps cleanly onto the Go field kind: empty-string for
//     string kinds, `0` for integer/float kinds, `FALSE` for bool. The literal —
//     not the real column — is what reaches the wire, so the secret value
//     is never read from the table.
//
// In every case the actual column value stays in the database; the SELECT
// emits a constant in its place. This is the W1-2 SEC-FIELDPOLICY-READ-NULL
// guarantee at the SQL boundary.
func maskedProjection(col dbColumn) string {
	aliased := func(expr string) string { return expr + " AS " + quoteIdent(col.name) }

	t := col.typ
	if t != nil && t.Kind() == reflect.Pointer {
		// Nil pointer is the natural masked value for an optional field.
		return aliased("NULL")
	}
	if t == nil {
		return aliased("NULL")
	}
	switch t.Kind() {
	case reflect.String:
		return aliased("''")
	case reflect.Bool:
		return aliased("FALSE")
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64,
		reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64:
		return aliased("0")
	case reflect.Float32, reflect.Float64:
		return aliased("0")
	default:
		// Unknown value kind (e.g. a struct carrier like time.Time). NULL is
		// the safest mask; if pgx cannot scan it the failure is loud (a 500),
		// never a silent leak. Such fields are not the documented field-policy
		// targets (those are scalar secrets / PII), so this path is rare.
		return aliased("NULL")
	}
}

// dbColumn pairs a resolved `db` tag column name with the Go struct field
// type that scans it. The type drives the masked-projection literal choice
// in `maskedProjection`.
type dbColumn struct {
	name string
	typ  reflect.Type
}

// dbColumnsOf returns the `db`-tagged columns declared by the struct type t
// (pointer-unwrapped), in struct-field order, each paired with its field
// type. Anonymous-embedded structs are not walked (the generated row shapes
// are flat). A field tagged `db:"-"`, untagged, or whose name resolves empty
// is skipped. Tag options after the first comma (e.g.
// `db:"loc,type:geography(point,4326)"`) are stripped so the returned name is
// the bare column. Returns nil for non-struct types and for `time.Time` (a
// struct that maps to a single scalar SQL value, never a row).
func dbColumnsOf(t reflect.Type) []dbColumn {
	for t != nil && t.Kind() == reflect.Pointer {
		t = t.Elem()
	}
	if t == nil || t.Kind() != reflect.Struct {
		return nil
	}
	if t.PkgPath() == "time" && t.Name() == "Time" {
		return nil
	}
	cols := make([]dbColumn, 0, t.NumField())
	for i := 0; i < t.NumField(); i++ {
		f := t.Field(i)
		if f.PkgPath != "" {
			continue // unexported
		}
		tag, ok := f.Tag.Lookup("db")
		if !ok {
			continue
		}
		name := tag
		if idx := strings.IndexByte(name, ','); idx >= 0 {
			name = name[:idx]
		}
		name = strings.TrimSpace(name)
		if name == "" || name == "-" {
			continue
		}
		cols = append(cols, dbColumn{name: name, typ: f.Type})
	}
	return cols
}
