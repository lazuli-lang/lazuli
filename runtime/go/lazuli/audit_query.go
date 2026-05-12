package lazuli

import (
	"errors"
	"fmt"
	"strings"
)

var (
	errInvalidAuditLogPlaceholder = errors.New("lazuli: audit log placeholder index must be positive")
	errInvalidAuditLogTimeRange   = errors.New("lazuli: invalid audit log created_at range")
	errInvalidAuditLogOrderColumn = errors.New("lazuli: invalid audit log order column")
	errDuplicateAuditLogOrder     = errors.New("lazuli: duplicate audit log order column")
	errInvalidAuditLogPagination  = errors.New("lazuli: invalid audit log pagination")
)

// AuditLogFilter is the provider-neutral filter shape for generated
// audit_log readers. Empty strings, nil IDs, and zero timestamps are omitted.
//
// CreatedAtFrom is inclusive. CreatedAtTo is exclusive.
type AuditLogFilter struct {
	OrgID          *ID
	ActorID        *ID
	ActorKind      string
	CommandName    string
	TargetResource string
	TargetID       *ID
	ResultStatus   string
	ErrorCode      string
	CorrelationID  string
	CreatedAtFrom  Time
	CreatedAtTo    Time
}

// AuditLogOrderColumn is a generated audit_log column that can be used for
// deterministic reader ordering.
type AuditLogOrderColumn string

const (
	// AuditLogOrderCreatedAt sorts audit rows by creation time.
	AuditLogOrderCreatedAt AuditLogOrderColumn = "created_at"
	// AuditLogOrderID sorts audit rows by the unique audit_log id.
	AuditLogOrderID AuditLogOrderColumn = "id"
)

// AuditLogOrder is one audit_log ORDER BY term.
type AuditLogOrder struct {
	Column AuditLogOrderColumn
	Desc   bool
}

// AuditLogSQLFragment is a SQL fragment plus the bind arguments it references.
type AuditLogSQLFragment struct {
	SQL  string
	Args []any
}

// AuditLogPaginationFragment is a LIMIT/OFFSET fragment plus its normalized
// page metadata.
type AuditLogPaginationFragment struct {
	SQL  string
	Args []any
	Page Page
}

// BuildAuditLogWhere builds a WHERE predicate fragment for audit_log readers.
//
// The returned SQL does not include the leading "WHERE". firstPlaceholder is
// the 1-based PostgreSQL placeholder assigned to the first emitted filter arg.
// Empty filters return an empty fragment and no args.
func BuildAuditLogWhere(filter AuditLogFilter, firstPlaceholder int) (AuditLogSQLFragment, error) {
	if !filter.CreatedAtFrom.IsZero() && !filter.CreatedAtTo.IsZero() && filter.CreatedAtFrom.After(filter.CreatedAtTo) {
		return AuditLogSQLFragment{}, errInvalidAuditLogTimeRange
	}

	var predicates []string
	var args []any

	add := func(column, op string, value any) {
		predicates = append(predicates, fmt.Sprintf("%s %s %s", quoteIdent(column), op, auditLogPlaceholder(firstPlaceholder+len(args))))
		args = append(args, value)
	}
	addString := func(column, value string) {
		value = strings.TrimSpace(value)
		if value != "" {
			add(column, "=", value)
		}
	}
	addID := func(column string, value *ID) {
		if value != nil {
			add(column, "=", *value)
		}
	}

	addID("org_id", filter.OrgID)
	addID("actor_id", filter.ActorID)
	addString("actor_kind", filter.ActorKind)
	addString("command_name", filter.CommandName)
	addString("target_resource", filter.TargetResource)
	addID("target_id", filter.TargetID)
	addString("result_status", filter.ResultStatus)
	addString("error_code", filter.ErrorCode)
	addString("correlation_id", filter.CorrelationID)
	if !filter.CreatedAtFrom.IsZero() {
		add("created_at", ">=", filter.CreatedAtFrom)
	}
	if !filter.CreatedAtTo.IsZero() {
		add("created_at", "<", filter.CreatedAtTo)
	}

	if len(args) == 0 {
		return AuditLogSQLFragment{}, nil
	}
	if firstPlaceholder <= 0 {
		return AuditLogSQLFragment{}, errInvalidAuditLogPlaceholder
	}
	return AuditLogSQLFragment{
		SQL:  strings.Join(predicates, " AND "),
		Args: args,
	}, nil
}

// BuildAuditLogOrderBy builds a deterministic ORDER BY clause.
//
// The default order is created_at DESC, id DESC. Custom ordering is limited to
// audit_log columns with stable semantics and automatically gains an id
// tie-breaker when one is not supplied.
func BuildAuditLogOrderBy(order []AuditLogOrder) (string, error) {
	if len(order) == 0 {
		order = []AuditLogOrder{
			{Column: AuditLogOrderCreatedAt, Desc: true},
			{Column: AuditLogOrderID, Desc: true},
		}
	}

	parts := make([]string, 0, len(order)+1)
	seen := make(map[AuditLogOrderColumn]struct{}, len(order)+1)
	hasID := false
	lastDesc := order[len(order)-1].Desc

	for _, term := range order {
		if _, ok := seen[term.Column]; ok {
			return "", fmt.Errorf("%w: %s", errDuplicateAuditLogOrder, term.Column)
		}
		seen[term.Column] = struct{}{}

		column, err := auditLogOrderColumnSQL(term.Column)
		if err != nil {
			return "", err
		}
		if term.Column == AuditLogOrderID {
			hasID = true
		}
		parts = append(parts, column+" "+auditLogOrderDirection(term.Desc))
	}

	if !hasID {
		parts = append(parts, quoteIdent(string(AuditLogOrderID))+" "+auditLogOrderDirection(lastDesc))
	}
	return "ORDER BY " + strings.Join(parts, ", "), nil
}

// BuildAuditLogPagination builds a parameterized LIMIT/OFFSET fragment for a
// generated audit_log reader.
//
// It normalizes PaginationInput using the shared runtime pagination rules and
// returns the normalized Page so callers can produce response metadata with
// the same values used in SQL.
func BuildAuditLogPagination(input PaginationInput, options PaginationOptions, firstPlaceholder int) (AuditLogPaginationFragment, error) {
	if firstPlaceholder <= 0 {
		return AuditLogPaginationFragment{}, errInvalidAuditLogPlaceholder
	}

	page, err := NormalizePagination(input, options)
	if err != nil {
		return AuditLogPaginationFragment{}, err
	}
	if page.Limit <= 0 || page.Offset < 0 {
		return AuditLogPaginationFragment{}, errInvalidAuditLogPagination
	}

	return AuditLogPaginationFragment{
		SQL:  fmt.Sprintf("LIMIT %s OFFSET %s", auditLogPlaceholder(firstPlaceholder), auditLogPlaceholder(firstPlaceholder+1)),
		Args: []any{page.Limit, page.Offset},
		Page: page,
	}, nil
}

func auditLogOrderColumnSQL(column AuditLogOrderColumn) (string, error) {
	switch column {
	case AuditLogOrderCreatedAt, AuditLogOrderID:
		return quoteIdent(string(column)), nil
	default:
		return "", fmt.Errorf("%w: %s", errInvalidAuditLogOrderColumn, column)
	}
}

func auditLogOrderDirection(desc bool) string {
	if desc {
		return "DESC"
	}
	return "ASC"
}

func auditLogPlaceholder(n int) string {
	return fmt.Sprintf("$%d", n)
}
