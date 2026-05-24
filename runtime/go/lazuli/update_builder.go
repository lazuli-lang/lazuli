package lazuli

import (
	"context"
	"errors"
	"fmt"
	"reflect"
	"strings"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
)

// §4 backend helper — nil-aware partial-update SQL constructor.
// Replaces the ~30-line `if input.X != nil { sets = append(...) }`
// dance at the top of every UPDATE handler. Wire-thin: just
// formats SET fragments with stable parameter indices and executes
// against any pgx-compatible connection.
//
// Usage:
//
//	user, err := lazuli.RequireActor(ctx)
//	if err != nil { return Property{}, err }
//
//	b := lazuli.NewUpdate("property").
//	  Where("id = $1 AND org_id = $2", input.PropertyID, user.OrgID).
//	  SetIfNotNil("name", input.Name).
//	  SetIfNotNil("description", input.Description).
//	  SetIfNotNil("city", input.City)
//	if b.IsNoop() { return loadProperty(ctx, input.PropertyID) }
//	if _, err := b.Exec(ctx, lazuli.DB()); err != nil { return Property{}, err }
//
// Where-clause arguments are bound to $1..$N first; SET-clause
// arguments append starting at $N+1. The query is composed lazily
// at Exec time so chained Set calls remain order-independent for
// readability (final SQL puts the SET fragments in the order they
// were added, which most tools' query logs sort alphabetically
// anyway).

// UpdateBuilder is a nil-aware partial-update SQL builder. Not safe
// for concurrent use — each handler should construct + execute one.
type UpdateBuilder struct {
	table       string
	sets        []string
	setArgs     []any
	whereClause string
	whereArgs   []any
	returning   string
}

// ErrUpdateNoChanges is returned by [UpdateBuilder.Exec] when no
// fields were set and the caller didn't opt in to a no-op via
// [UpdateBuilder.AllowNoChanges]. Most update handlers should
// short-circuit BEFORE Exec with [UpdateBuilder.IsNoop] instead;
// this error is the safety net for callers that forgot.
var ErrUpdateNoChanges = errors.New("lazuli: UpdateBuilder.Exec called with zero SET fragments")

// NewUpdate starts a new partial-update against `table`. Table name
// is interpolated as-is into the SQL — pass a literal, NOT user
// input. The runtime expects authored DSL table names (e.g.
// `"property"`, `"host"`) which by construction are SQL-safe.
func NewUpdate(table string) *UpdateBuilder {
	return &UpdateBuilder{table: table}
}

// Where sets the WHERE clause. Pass a literal SQL fragment with
// $1..$N placeholders and the matching arguments. Replaces any
// previous Where call.
//
//	b.Where("id = $1 AND org_id = $2", id, orgID)
func (b *UpdateBuilder) Where(clause string, args ...any) *UpdateBuilder {
	b.whereClause = clause
	b.whereArgs = args
	return b
}

// SetIfNotNil appends `col = $N` when val is a non-nil pointer.
// The dereferenced value is bound. Accepts `*string`, `*int`,
// `*int64`, `*bool`, `*float64`, `*time.Time`, etc. — any pgx-
// encodable pointer type.
//
// Wrapper around the typed methods below; if you know the field
// type, prefer the explicit `SetIfNotNilString` etc. for compile-
// time safety.
func (b *UpdateBuilder) SetIfNotNil(col string, val any) *UpdateBuilder {
	if isNilPointer(val) {
		return b
	}
	deref := dereferencePointer(val)
	b.sets = append(b.sets, col)
	b.setArgs = append(b.setArgs, deref)
	return b
}

// SetIfNotNilString is the type-checked variant for *string fields
// (the common shape for `optional` text inputs in Lazuli commands).
func (b *UpdateBuilder) SetIfNotNilString(col string, val *string) *UpdateBuilder {
	if val == nil {
		return b
	}
	b.sets = append(b.sets, col)
	b.setArgs = append(b.setArgs, *val)
	return b
}

// SetIfNotNilInt is the type-checked variant for *int64 fields
// (Lazuli's canonical integer representation).
func (b *UpdateBuilder) SetIfNotNilInt(col string, val *int64) *UpdateBuilder {
	if val == nil {
		return b
	}
	b.sets = append(b.sets, col)
	b.setArgs = append(b.setArgs, *val)
	return b
}

// SetIfNotNilBool is the type-checked variant for *bool fields.
func (b *UpdateBuilder) SetIfNotNilBool(col string, val *bool) *UpdateBuilder {
	if val == nil {
		return b
	}
	b.sets = append(b.sets, col)
	b.setArgs = append(b.setArgs, *val)
	return b
}

// SetIfNotNilEnum is the generic helper for `*T` where T is a
// string-newtype (e.g. `*PropertyCategory`). Dereferences and
// stores as the underlying string so pgx encodes consistently
// regardless of the named type.
//
// Free function (not a method) because Go does not allow generic
// methods on non-generic types. Usage:
//
//	lazuli.SetIfNotNilEnum(b, "category", input.Category)
//
// where `input.Category` is `*PropertyCategory` declared as
// `type PropertyCategory string`.
func SetIfNotNilEnum[T ~string](b *UpdateBuilder, col string, val *T) *UpdateBuilder {
	if val == nil {
		return b
	}
	b.sets = append(b.sets, col)
	b.setArgs = append(b.setArgs, string(*val))
	return b
}

// SetIfNotNilSlice is the generic helper for `*[]T` array fields
// (the common shape for `optional Amenity[]`-class inputs).
// Dereferences and stores the slice as-is — pgx encodes Go slices
// to Postgres arrays via its built-in array codec.
func SetIfNotNilSlice[T any](b *UpdateBuilder, col string, val *[]T) *UpdateBuilder {
	if val == nil {
		return b
	}
	b.sets = append(b.sets, col)
	b.setArgs = append(b.setArgs, *val)
	return b
}

// Set unconditionally appends `col = $N` with the given value.
// Use when the caller has already filtered the nil case (e.g.
// for computed/derived columns like `updated_at = now()`).
func (b *UpdateBuilder) Set(col string, val any) *UpdateBuilder {
	b.sets = append(b.sets, col)
	b.setArgs = append(b.setArgs, val)
	return b
}

// SetRaw appends a literal SET fragment without binding an argument.
// Useful for clauses like `updated_at = now()` or
// `version = version + 1` that don't take a parameter.
//
//	b.SetRaw("updated_at = now()")
func (b *UpdateBuilder) SetRaw(fragment string) *UpdateBuilder {
	b.sets = append(b.sets, "__raw__"+fragment)
	return b
}

// Returning attaches a `RETURNING <cols>` clause. Used together
// with [UpdateBuilder.QueryRow] to read back generated/computed
// columns in the same round-trip.
func (b *UpdateBuilder) Returning(cols string) *UpdateBuilder {
	b.returning = cols
	return b
}

// IsNoop reports whether any SET fragments have been registered.
// Handlers should call this before [UpdateBuilder.Exec] to skip
// the round-trip when the input carries no updatable fields:
//
//	if b.IsNoop() { return loadProperty(ctx, input.PropertyID) }
func (b *UpdateBuilder) IsNoop() bool {
	return len(b.sets) == 0
}

// SQL renders the composed UPDATE statement and the bound argument
// slice (where-args first, then set-args). Exposed for tests +
// observability — handlers normally call [UpdateBuilder.Exec] or
// [UpdateBuilder.QueryRow] instead.
//
// Returns "" + nil when [UpdateBuilder.IsNoop] is true.
func (b *UpdateBuilder) SQL() (string, []any) {
	if b.IsNoop() {
		return "", nil
	}
	// Where args occupy $1..$len(whereArgs); set args start after.
	offset := len(b.whereArgs)
	fragments := make([]string, 0, len(b.sets))
	for i, col := range b.sets {
		if strings.HasPrefix(col, "__raw__") {
			fragments = append(fragments, col[len("__raw__"):])
			continue
		}
		fragments = append(fragments, fmt.Sprintf("%s = $%d", col, offset+i+1))
	}
	var sb strings.Builder
	sb.WriteString("UPDATE ")
	sb.WriteString(strconvQuoteIdent(b.table))
	sb.WriteString(" SET ")
	sb.WriteString(strings.Join(fragments, ", "))
	if b.whereClause != "" {
		sb.WriteString(" WHERE ")
		sb.WriteString(b.whereClause)
	}
	if b.returning != "" {
		sb.WriteString(" RETURNING ")
		sb.WriteString(b.returning)
	}
	args := make([]any, 0, len(b.whereArgs)+len(b.setArgs))
	args = append(args, b.whereArgs...)
	args = append(args, b.setArgs...)
	return sb.String(), args
}

// Exec runs the composed UPDATE through `db`. Returns the row
// count + any error. Returns [ErrUpdateNoChanges] when called with
// no SET fragments — callers should either branch on
// [UpdateBuilder.IsNoop] first or treat the error as a signal to
// short-circuit.
//
// `db` accepts anything implementing pgx's Exec contract
// (pgxpool.Pool, pgx.Conn, pgx.Tx).
func (b *UpdateBuilder) Exec(ctx context.Context, db updateExecer) (pgconn.CommandTag, error) {
	if b.IsNoop() {
		return pgconn.CommandTag{}, ErrUpdateNoChanges
	}
	sql, args := b.SQL()
	return db.Exec(ctx, sql, args...)
}

// QueryRow runs the composed UPDATE ... RETURNING and returns the
// scanned row. Use when [UpdateBuilder.Returning] was set.
//
//	row := b.Returning("id, name, updated_at").QueryRow(ctx, db)
//	var id int64
//	var name string
//	var updatedAt time.Time
//	if err := row.Scan(&id, &name, &updatedAt); err != nil { ... }
func (b *UpdateBuilder) QueryRow(ctx context.Context, db updateQuerier) pgx.Row {
	sql, args := b.SQL()
	return db.QueryRow(ctx, sql, args...)
}

// updateExecer is the minimal pgx interface UpdateBuilder.Exec needs.
type updateExecer interface {
	Exec(ctx context.Context, sql string, args ...any) (pgconn.CommandTag, error)
}

// updateQuerier is the minimal pgx interface UpdateBuilder.QueryRow needs.
type updateQuerier interface {
	QueryRow(ctx context.Context, sql string, args ...any) pgx.Row
}

// strconvQuoteIdent wraps a SQL identifier in double quotes. Used
// for table + column names to allow reserved words and to make the
// emitted SQL self-documenting.
func strconvQuoteIdent(ident string) string {
	if strings.ContainsAny(ident, `"\`) {
		// Defensive — the table name should never contain a quote.
		// Reject by returning the un-quoted form, which will fail
		// the syntax check at the DB and surface a loud error.
		return ident
	}
	return `"` + ident + `"`
}

// isNilPointer / dereferencePointer use reflect to detect typed
// nil pointers (the `(*string)(nil)` shape). The reflect cost is
// trivial against a DB round-trip and only fires on SetIfNotNil
// (the typed Set*IfNotNil variants avoid reflect entirely).
func isNilPointer(v any) bool {
	if v == nil {
		return true
	}
	rv := reflect.ValueOf(v)
	if rv.Kind() == reflect.Pointer {
		return rv.IsNil()
	}
	return false
}

func dereferencePointer(v any) any {
	rv := reflect.ValueOf(v)
	if rv.Kind() == reflect.Pointer && !rv.IsNil() {
		return rv.Elem().Interface()
	}
	return v
}
