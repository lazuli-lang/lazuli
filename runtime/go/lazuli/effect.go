package lazuli

// Effect is the side-effect of a command on a resource. The runtime applies
// the effect transactionally inside `Command.Handle()` after policy and
// validators pass.
//
// Concrete effects are produced by the `Creates`, `Updates`, `Deletes`, and
// `Returns` builders. Generated code never implements `Effect` directly.
type Effect interface {
	effectKind() effectKind
}

type effectKind int

const (
	effectCreates effectKind = iota
	effectUpdates
	effectDeletes
	effectReturns
)

// Bindings maps a target field (resource column or input field) to the
// expression that produces its value at command time.
//
// The runtime resolves Source values against the inbound input, the request
// context, the loaded target row, or constants — depending on the source.
type Bindings map[string]Source

// Source is a single binding's source. See `FromInput`, `FromCtx`,
// `FromTarget`, `FromConst` for canonical constructors.
type Source struct {
	kind     sourceKind
	path     string
	value    any
	subquery *ownedViaSubquery
}

type sourceKind int

const (
	sourceInput        sourceKind = iota // value comes from `input.<path>`
	sourceCtx                            // value comes from `ctx.<path>`
	sourceTarget                         // value comes from the loaded `target.<path>`
	sourceConst                          // value is a literal
	sourceFn                             // value comes from invoking a registered binding fn
	sourceCtxOwnedVia                    // WHERE binding expands to `<col> IN (SELECT id FROM <related> WHERE <owner_col> = $N)`
)

// ownedViaSubquery captures the related-resource traversal for
// `@scope.owner` policies on resources that don't carry a direct
// owner column. Composed by [FromCtxOwnedVia]; consumed by the SQL
// builder in `applyUpdates` / `applyDeletes`.
//
// Closes the relation-traversal gap surfaced by the hostpoint Phase 4
// capability audit 2026-05-17 (the 8 BLOCKED handlers that needed
// `<fk> IN (SELECT id FROM <related> WHERE <related>.<owner> = $N)`).
type ownedViaSubquery struct {
	// RelatedTable is the SQL table name of the related resource
	// (e.g. "host" when the policy is on `property.host → host.user_id`).
	relatedTable string
	// OwnerColumn is the column in <relatedTable> that carries the
	// owner reference (e.g. "user_id").
	ownerColumn string
}

// FromInput binds a target field to an input field by path. The runtime
// resolves at execution time.
//
//	"name": lazuli.FromInput("name")
func FromInput(path string) Source { return Source{kind: sourceInput, path: path} }

// FromCtx binds a target field to a context value (e.g. "user", "tenant.org_id").
func FromCtx(path string) Source { return Source{kind: sourceCtx, path: path} }

// FromTarget binds a target field to the loaded `target.*` row.
func FromTarget(path string) Source { return Source{kind: sourceTarget, path: path} }

// FromConst binds a target field to a literal value (enum, number, string).
func FromConst(value any) Source { return Source{kind: sourceConst, value: value} }

// FromCtxOwnedVia builds a WHERE-only Source that expands to
// `<col> IN (SELECT id FROM <related_table> WHERE <owner_column> = $N)`
// in the generated UPDATE / DELETE SQL. The runtime resolves
// `<ctx_path>` against `ctx` (same as [FromCtx]) for the `$N` bind.
//
// Codegen emits this for `@scope.owner` policies on resources that
// reference an owner via a relation (e.g. `property.host → host.user_id`)
// rather than directly. Closes the relation-traversal half of the
// hostpoint 2026-05-17 capability matrix.
//
//	// property has `host: Host required` — Host owns the row's
//	// user via `Host.user_id`.
//	"host": lazuli.FromCtxOwnedVia("host", "user_id", "user.id")
//
// SQL emitted: `host IN (SELECT id FROM "host" WHERE user_id = $N)`.
//
// WHERE-only: must NOT appear in `Bind` (the SET side of UPDATE);
// the runtime resolves the value scalarly there. Codegen guarantees
// the constraint; this comment is informational.
func FromCtxOwnedVia(relatedTable, ownerColumn, ctxPath string) Source {
	return Source{
		kind: sourceCtxOwnedVia,
		path: ctxPath,
		subquery: &ownedViaSubquery{
			relatedTable: relatedTable,
			ownerColumn:  ownerColumn,
		},
	}
}

// FromFn binds a target field to the return value of a user-registered
// binding fn (closes WAR-VOCAB-CREATES-FN-CALL-01). Codegen emits this
// shape for `@fn.<name>(<arg>...)` expressions on the RHS of
// `creates`/`updates` field bindings (and `query.list` filter values).
// The runtime resolves each arg source first, then invokes the fn
// registered under `name` via [RegisterBindingFn] with the resolved
// args. Returns an error if no fn is registered, or if the fn returns
// an error.
//
//	"password_hash": lazuli.FromFn("hash_password", []lazuli.Source{
//	    lazuli.FromInput("password"),
//	}),
func FromFn(name string, args []Source) Source {
	return Source{kind: sourceFn, path: name, value: args}
}

// CreatesEffect is the effect for a `creates <Resource>` block. Generated
// code constructs one via `lazuli.Creates(&customerResource, lazuli.Bindings{...})`.
type CreatesEffect struct {
	Resource *resourceErased
	Bind     Bindings
}

func (CreatesEffect) effectKind() effectKind { return effectCreates }

// Creates builds a CreatesEffect for the given resource and field bindings.
// The runtime, at command execution, resolves each binding and inserts the
// row.
func Creates[T any](r *Resource[T], bind Bindings) CreatesEffect {
	return CreatesEffect{Resource: r.erased(), Bind: bind}
}

// UpdatesEffect is the effect for an `updates <Resource>` block.
//
// Where names the row to mutate (typically `id` keyed by `FromInput("ID")`);
// Bind names the columns whose values change. The runtime composes
// `UPDATE <resource> SET <bind> WHERE <where> AND <tenancy> AND deleted_at
// IS NULL RETURNING *`.
type UpdatesEffect struct {
	Resource *resourceErased
	Where    Bindings
	Bind     Bindings
}

func (UpdatesEffect) effectKind() effectKind { return effectUpdates }

// Updates builds an UpdatesEffect. `where` selects the row(s); `bind`
// supplies the new column values.
func Updates[T any](r *Resource[T], where Bindings, bind Bindings) UpdatesEffect {
	return UpdatesEffect{Resource: r.erased(), Where: where, Bind: bind}
}

// DeletesEffect is the effect for a `deletes <Resource>` block. The runtime
// chooses soft-delete (`UPDATE ... SET deleted_at = now()`) when
// `Resource.SoftDelete` is true; otherwise hard-delete (`DELETE FROM ...`).
// Both variants honour `Where` (to scope the row(s)), tenant scoping, and
// the existing `deleted_at IS NULL` filter so a delete is idempotent.
type DeletesEffect struct {
	Resource *resourceErased
	Where    Bindings
}

func (DeletesEffect) effectKind() effectKind { return effectDeletes }

// Deletes builds a DeletesEffect. `where` selects the row(s) to remove.
func Deletes[T any](r *Resource[T], where Bindings) DeletesEffect {
	return DeletesEffect{Resource: r.erased(), Where: where}
}

// ReturnsEffect carries a pure handler that produces an output without
// writing through the standard Creates/Updates/Deletes SQL pipeline.
// Used for command shapes like `command summary returns CustomerSummary`
// where the body computes data rather than mutating it. The runtime
// still runs policy, validators, rate_limit, and audit; it skips the
// transactional INSERT/UPDATE/DELETE step.
type ReturnsEffect struct {
	Handler func(ctx *Ctx, input any) (any, error)
}

func (ReturnsEffect) effectKind() effectKind { return effectReturns }

func Returns[I, O any](handler func(ctx *Ctx, input I) (O, error)) ReturnsEffect {
	return ReturnsEffect{
		Handler: func(ctx *Ctx, input any) (any, error) {
			typed, ok := input.(I)
			if !ok {
				var zero I
				return zero, &Error{Status: 500, Code: CodeInternal,
					Message: "returns handler received input of wrong type"}
			}
			return handler(ctx, typed)
		},
	}
}
