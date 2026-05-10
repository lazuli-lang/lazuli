package lazuli

// Effect is the side-effect of a command on a resource. The runtime applies
// the effect transactionally inside `Command.Handle()` after policy and
// validators pass.
//
// Concrete effects are produced by the `Creates`, `Updates`, and `Deletes`
// builders. Generated code never implements `Effect` directly.
type Effect interface {
	effectKind() effectKind
}

type effectKind int

const (
	effectCreates effectKind = iota
	effectUpdates
	effectDeletes
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
	kind  sourceKind
	path  string
	value any
}

type sourceKind int

const (
	sourceInput  sourceKind = iota // value comes from `input.<path>`
	sourceCtx                      // value comes from `ctx.<path>`
	sourceTarget                   // value comes from the loaded `target.<path>`
	sourceConst                    // value is a literal
)

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
