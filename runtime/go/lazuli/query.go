package lazuli

// QueryKind names which canonical query shape a `Query[A, R]` represents.
// Mirrors the DSL `query.list`, `query.lookup`, `query.sql` distinction.
type QueryKind int

const (
	QueryList   QueryKind = iota // collection query with filters/order/paginate
	QueryLookup                  // single-record query keyed by columns
	QuerySQL                     // opaque SQL backed by a `.sql` file
)

// Query declares a read operation. Type parameter A is the args shape (the
// `params` block plus the lookup keys); R is the row shape (typically the
// Resource[T]'s T).
//
// Generated code populates this once per query at package init.
type Query[A, R any] struct {
	// Name is the canonical name as written in the DSL (qualified with the
	// owning feature). Examples: "customer.query.list", "customer.query.by_id".
	Name string

	// Resource is the resource the query reads from. Stored as `any` so the
	// registry can hold queries with different row types side-by-side.
	Resource any

	// Kind names the canonical shape: list, lookup, or sql.
	Kind QueryKind

	// Policy resolves which actors may invoke the query. Empty means
	// unreachable.
	Policy Policy

	// Audit declares whether and what to record when the query runs.
	// Most queries omit this; commands and writes do the bulk of auditing.
	Audit *AuditSpec

	// Filters are optional WHERE-clauses for `query.list`. Each rule names
	// a column and a `When` source; if the source resolves to a non-zero
	// value, the runtime appends `<column> = <value>` to the WHERE clause.
	// Empty/nil sources skip the rule (canonical "optional filter" behaviour).
	Filters []FilterRule

	// Order is the canonical sort. Empty defaults to `created_at DESC` for
	// `query.list`. Ignored for `query.lookup` and `query.sql`.
	Order []OrderClause

	// Search applies an optional text-match filter across one or more
	// columns, mirroring the DSL `search params.<field> over <columns>`.
	// Nil disables search.
	Search *SearchSpec

	// Paginate is the default page size for `query.list`. Zero defaults to
	// 100. Ignored for `query.lookup` and `query.sql`.
	Paginate int

	// LookupBy lists the columns + arg sources that key a `query.lookup`.
	// Single-key lookups (`by id: ID`) carry one entry; multi-key lookups
	// carry several. Order matches the DSL.
	LookupBy []LookupKey

	// SQL is the relative path to the `.sql` file that backs a `query.sql`.
	// Ignored for list/lookup.
	SQL string

	// Returns is the canonical name of the resource or record the SQL query
	// returns. Used for documentation and codegen of the row type. Ignored
	// for list/lookup (the row type is the Resource[T]'s T).
	Returns string

	// untouched generic erasure marker for registry storage.
	_ struct{}
}

// FilterRule is one optional WHERE-clause for a list query. `When` is the
// source whose presence/absence decides whether to apply the filter. The
// runtime resolves it against the args at execution time.
type FilterRule struct {
	Column string // SQL column name on the resource table
	When   Source // FromInput / FromCtx / FromConst; nil-or-zero result -> skip
}

// OrderClause is one ORDER BY column.
type OrderClause struct {
	Column string
	Desc   bool
}

// LookupKey ties a SQL column to the args field that supplies its value.
type LookupKey struct {
	Column string // SQL column on the resource table
	Source Source // typically FromInput("FieldName")
}

// SearchSpec declares a text search filter:
// `search params.<field> over <col>, <col> [mode contains|starts_with|exact]`.
type SearchSpec struct {
	// Source is the args source for the search term (typically
	// FromInput("Search")). When the resolved value is empty/zero, the
	// filter is skipped.
	Source Source

	// Over lists the columns the runtime ORs across with the chosen mode.
	Over []string

	// Mode controls match shape. Defaults to SearchContains when empty.
	Mode SearchMode
}

// SearchMode names the SQL pattern shape for a search term.
type SearchMode int

const (
	SearchContains   SearchMode = iota // %term%
	SearchStartsWith                   // term%
	SearchExact                        // term
)

// erased returns the type-erased view used by the dispatcher and registry.
func (q *Query[A, R]) erased() *queryErased {
	return &queryErased{
		Name:     q.Name,
		Resource: q.Resource,
		Kind:     q.Kind,
		Policy:   q.Policy,
		Audit:    q.Audit,
		Filters:  q.Filters,
		Order:    q.Order,
		Search:   q.Search,
		Paginate: q.Paginate,
		LookupBy: q.LookupBy,
		SQL:      q.SQL,
		Returns:  q.Returns,
	}
}

// queryErased is the runtime's view of any Query[A, R].
type queryErased struct {
	Name     string
	Resource any
	Kind     QueryKind
	Policy   Policy
	Audit    *AuditSpec
	Filters  []FilterRule
	Order    []OrderClause
	Search   *SearchSpec
	Paginate int
	LookupBy []LookupKey
	SQL      string
	Returns  string
}
