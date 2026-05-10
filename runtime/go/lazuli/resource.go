package lazuli

// Resource declares a domain resource. The type parameter T is the row shape
// that the runtime materialises (insert/update/select).
//
// The struct mirrors what the DSL `resource <Name>` block declares. Generated
// code populates this once per resource at package init.
type Resource[T any] struct {
	// Name is the canonical resource name as written in the DSL
	// (e.g. "customer", "customer_note").
	Name string

	// Feature is the owning feature name (DSL `feature <name>`). Used by the
	// registry to namespace queries/commands and by audit/event qualification.
	Feature string

	// Tenancy scopes every read/write to the active tenant by default.
	// The runtime adds `WHERE tenant.org_id = ctx.tenant.org_id` automatically
	// for `TenancyOrg`.
	Tenancy TenancyMode

	// SoftDelete switches `delete` semantics from `DELETE FROM ...` to
	// `UPDATE ... SET deleted_at = now()`, and adds `WHERE deleted_at IS NULL`
	// to every default query.
	SoftDelete bool

	// Retention names the terminal lifecycle policy applied after soft-delete.
	// Nil means "rows soft-deleted stay soft-deleted forever".
	Retention *RetentionSpec

	// Validators are extension validators that run on every create/update.
	// Order matches the order in `extensions.validator <name>`.
	Validators []ValidatorRef

	// Indexes are explicit indexes declared in the DSL `constraints` block.
	// Lazuli derives implicit indexes from `query.list` filters; this list
	// only carries the explicit ones (unique constraints, custom indexes).
	Indexes []Index

	// HasMany declares typed collection edges. The runtime materialises the
	// inverse query and FK contract.
	HasMany []HasMany

	// untouched: erased generic parameter so `*Resource[T]` is comparable when
	// stored in a heterogeneous registry. Generated code does not touch this.
	_ struct{}
}

// erased returns the type-erased view used by the registry and dispatcher.
// Generated code does not call this directly.
func (r *Resource[T]) erased() *resourceErased {
	return &resourceErased{
		Name:       r.Name,
		Feature:    r.Feature,
		Tenancy:    r.Tenancy,
		SoftDelete: r.SoftDelete,
		Retention:  r.Retention,
		Validators: r.Validators,
		Indexes:    r.Indexes,
		HasMany:    r.HasMany,
	}
}

// resourceErased is the runtime's view of any Resource[T]. It drops the
// type parameter so the registry can hold all resources in one slice.
type resourceErased struct {
	Name       string
	Feature    string
	Tenancy    TenancyMode
	SoftDelete bool
	Retention  *RetentionSpec
	Validators []ValidatorRef
	Indexes    []Index
	HasMany    []HasMany
}

// Index declares an explicit DB index from the DSL `constraints` block.
type Index struct {
	Fields []string
	Unique bool
	Per    string // "org" for `unique email per org`, empty otherwise
}

// HasMany declares a typed collection edge from this resource to another.
type HasMany struct {
	Name    string // edge name on this resource ("notes")
	Type    string // target resource name ("CustomerNote")
	Inverse string // FK field on the target ("customer")
}
