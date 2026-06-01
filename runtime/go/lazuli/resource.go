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

	// SoftDeleteActor mirrors the DSL `soft_delete by` form (spec 0015):
	// when true, the resource carries a nullable `deleted_by` actor column
	// alongside `deleted_at`, and the soft-delete write additionally
	// stamps `"deleted_by" = ctx.actor` in the SET clause. False (bare
	// `soft_delete`) leaves the column absent — the runtime MUST NOT emit
	// `"deleted_by" = ...` or it raises PG 42703 (undefined_column).
	SoftDeleteActor bool

	// Timestamps mirrors the DSL `defaults.timestamps` / per-resource
	// `timestamps` knob: when true, the resource carries `created_at` and
	// `updated_at` columns and the runtime bumps `updated_at = now()` on
	// every UPDATE (and on soft-delete). When false, the columns are
	// absent and the runtime MUST NOT emit `"updated_at" = now()` into
	// the SET clause — doing so raises PG 42703 (undefined_column).
	Timestamps bool

	// Retention names the terminal lifecycle policy applied after soft-delete.
	// Nil means "rows soft-deleted stay soft-deleted forever".
	Retention *RetentionSpec

	// PIIFields names columns that hold personally identifiable data. Retention
	// anonymization sets these columns to NULL after the retention window.
	PIIFields []string

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

	// EncryptedColumns names the columns declared with
	// `@cap.Encrypted` / `@cap.E2ee` and maps each to its `@key.<scope>`
	// reference. The runtime walks this at the SQL boundary
	// (`applyCreates`/`applyUpdates`) to wrap each resolved binding value
	// through `encryption.ForCtx(ctx, scope, "")` before INSERT/UPDATE.
	// Nil when the resource has no encrypted fields — the runtime skips
	// the loop in that case. See `runtime/go/lazuli/encryption/registry.go`
	// for the resolver contract and
	// `docs/proposals/encryption-vocab.md` §Codegen for the rule.
	EncryptedColumns map[string]string

	// Decrypt is the per-resource decrypt callback emitted by codegen
	// alongside the `Decrypt<Resource>` helper. The runtime calls it on
	// every scanned row right after `pgx.CollectOneRow` / `pgx.CollectRows`
	// (insert/update/delete RETURNING * and query list/lookup) so that
	// in-memory rows hold plaintext while DB columns hold ciphertext.
	// Nil when the resource has no server-readable encrypted fields. The
	// callback receives a `*T`-shaped pointer via the `any` interface;
	// codegen wraps the typed `DecryptResource(ctx, *T)` helper into
	// this shape at the `Resource[T]` literal site.
	Decrypt func(ctx *Ctx, row any) error

	// untouched: erased generic parameter so `*Resource[T]` is comparable when
	// stored in a heterogeneous registry. Generated code does not touch this.
	_ struct{}
}

// erased returns the type-erased view used by the registry and dispatcher.
// Generated code does not call this directly.
func (r *Resource[T]) erased() *resourceErased {
	return &resourceErased{
		Name:             r.Name,
		Feature:          r.Feature,
		Tenancy:          r.Tenancy,
		SoftDelete:       r.SoftDelete,
		SoftDeleteActor:  r.SoftDeleteActor,
		Timestamps:       r.Timestamps,
		Retention:        r.Retention,
		PIIFields:        r.PIIFields,
		Validators:       r.Validators,
		Indexes:          r.Indexes,
		HasMany:          r.HasMany,
		EncryptedColumns: r.EncryptedColumns,
		Decrypt:          r.Decrypt,
	}
}

// resourceErased is the runtime's view of any Resource[T]. It drops the
// type parameter so the registry can hold all resources in one slice.
type resourceErased struct {
	Name             string
	Feature          string
	Tenancy          TenancyMode
	SoftDelete       bool
	SoftDeleteActor  bool
	Timestamps       bool
	Retention        *RetentionSpec
	PIIFields        []string
	Validators       []ValidatorRef
	Indexes          []Index
	HasMany          []HasMany
	EncryptedColumns map[string]string
	Decrypt          func(ctx *Ctx, row any) error
}

// HasColumn reports whether the resource declares the named column.
// The runtime SET-clause builders (`applyUpdates`, `applyDeletes` soft
// path) use this to gate the lifecycle-timestamp append: only resources
// that declare the column receive the `<col> = now()` SET clause —
// otherwise the emitted UPDATE raises PG 42703.
//
// The set is intentionally small (lifecycle timestamps only) since
// every other column is supplied through the typed `Bind` map. Add
// new entries here when a future lifecycle column needs the same
// unconditional-bump treatment.
func (r *resourceErased) HasColumn(name string) bool {
	switch name {
	case "updated_at", "created_at":
		return r.Timestamps
	case "deleted_at":
		return r.SoftDelete
	case "deleted_by":
		return r.SoftDeleteActor
	default:
		return false
	}
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
