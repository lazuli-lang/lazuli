package lazuli

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"strings"
)

const (
	maxDBIndexPlanIdentifierLength = 63
	dbIndexPlanNameHashLength      = 12
)

var (
	errDBIndexPlanTableRequired        = errors.New("lazuli: db index plan table is required")
	errDBIndexPlanReferenceRequired    = errors.New("lazuli: db foreign key reference table is required")
	errDBIndexPlanColumnsRequired      = errors.New("lazuli: db index plan requires at least one column")
	errDBIndexPlanCompositeKeyColumns  = errors.New("lazuli: db composite key plan requires at least two columns")
	errDBIndexPlanForeignKeyColumnPair = errors.New("lazuli: db foreign key columns must match reference columns")
	errDBIndexPlanInvalidAction        = errors.New("lazuli: invalid db foreign key action")
	errDBIndexPlanInvalidDeferrable    = errors.New("lazuli: invalid db foreign key deferrable options")
)

// DBConstraintKind names the constraint shape produced by a DBConstraintPlan.
type DBConstraintKind int

const (
	// DBUniqueConstraint is an ALTER TABLE ... UNIQUE plan.
	DBUniqueConstraint DBConstraintKind = iota
	// DBForeignKeyConstraint is an ALTER TABLE ... FOREIGN KEY plan.
	DBForeignKeyConstraint
	// DBCompositeKeyConstraint is an ALTER TABLE ... PRIMARY KEY plan over
	// more than one column.
	DBCompositeKeyConstraint
)

func (k DBConstraintKind) String() string {
	switch k {
	case DBUniqueConstraint:
		return "unique"
	case DBForeignKeyConstraint:
		return "foreign_key"
	case DBCompositeKeyConstraint:
		return "composite_key"
	default:
		return fmt.Sprintf("unknown(%d)", k)
	}
}

// DBForeignKeyAction names an optional ON DELETE or ON UPDATE action.
type DBForeignKeyAction string

const (
	// DBForeignKeyDefaultAction omits the action clause and lets PostgreSQL use
	// its default NO ACTION behavior.
	DBForeignKeyDefaultAction DBForeignKeyAction = ""
	// DBForeignKeyNoAction builds NO ACTION.
	DBForeignKeyNoAction DBForeignKeyAction = "NO ACTION"
	// DBForeignKeyRestrict builds RESTRICT.
	DBForeignKeyRestrict DBForeignKeyAction = "RESTRICT"
	// DBForeignKeyCascade builds CASCADE.
	DBForeignKeyCascade DBForeignKeyAction = "CASCADE"
	// DBForeignKeySetNull builds SET NULL.
	DBForeignKeySetNull DBForeignKeyAction = "SET NULL"
	// DBForeignKeySetDefault builds SET DEFAULT.
	DBForeignKeySetDefault DBForeignKeyAction = "SET DEFAULT"
)

// DBIndexPlanOptions configures BuildDBIndexPlan.
type DBIndexPlanOptions struct {
	// Name overrides the generated deterministic name when set.
	Name string
	// Table is an unqualified or schema-qualified table name.
	Table string
	// Columns is the ordered set of indexed columns.
	Columns []string
	// Method optionally adds a PostgreSQL USING method such as "btree" or
	// "gin". Empty leaves PostgreSQL's default method.
	Method string
	// IfNotExists adds IF NOT EXISTS to the CREATE INDEX snippet.
	IfNotExists bool
}

// DBUniqueConstraintPlanOptions configures BuildDBUniqueConstraintPlan.
type DBUniqueConstraintPlanOptions struct {
	// Name overrides the generated deterministic name when set.
	Name string
	// Table is an unqualified or schema-qualified table name.
	Table string
	// Columns is the ordered set of constrained columns.
	Columns []string
}

// DBForeignKeyPlanOptions configures BuildDBForeignKeyPlan.
type DBForeignKeyPlanOptions struct {
	// Name overrides the generated deterministic name when set.
	Name string
	// Table is the table that owns the foreign key.
	Table string
	// Columns is the ordered set of local foreign-key columns.
	Columns []string
	// ReferenceTable is the table referenced by the foreign key.
	ReferenceTable string
	// ReferenceColumns is the ordered set of referenced columns.
	ReferenceColumns []string
	// OnDelete optionally adds an ON DELETE action.
	OnDelete DBForeignKeyAction
	// OnUpdate optionally adds an ON UPDATE action.
	OnUpdate DBForeignKeyAction
	// Deferrable adds DEFERRABLE when true, otherwise NOT DEFERRABLE.
	Deferrable bool
	// InitiallyDeferred adds INITIALLY DEFERRED. It requires Deferrable.
	InitiallyDeferred bool
}

// DBCompositeKeyPlanOptions configures BuildDBCompositeKeyPlan.
type DBCompositeKeyPlanOptions struct {
	// Name overrides the generated deterministic name when set.
	Name string
	// Table is an unqualified or schema-qualified table name.
	Table string
	// Columns is the ordered set of primary-key columns. At least two columns
	// are required because single-column primary keys do not need this helper.
	Columns []string
}

// DBIndexPlan is an adapter-neutral index creation plan.
type DBIndexPlan struct {
	// Name is the unqualified index name.
	Name string
	// Table is copied from the options.
	Table string
	// Columns is a copy of the ordered input columns.
	Columns []string
	// SQL is the CREATE INDEX snippet.
	SQL string
	// DropSQL is the matching DROP INDEX snippet.
	DropSQL string
}

// DBConstraintPlan is an adapter-neutral constraint creation plan.
type DBConstraintPlan struct {
	// Kind names the planned constraint shape.
	Kind DBConstraintKind
	// Name is the unqualified constraint name.
	Name string
	// Table is copied from the options.
	Table string
	// Columns is a copy of the ordered local columns.
	Columns []string
	// ReferenceTable is set for foreign-key plans.
	ReferenceTable string
	// ReferenceColumns is set for foreign-key plans.
	ReferenceColumns []string
	// SQL is the ALTER TABLE ... ADD CONSTRAINT snippet.
	SQL string
	// DropSQL is the matching ALTER TABLE ... DROP CONSTRAINT snippet.
	DropSQL string
}

// BuildDBIndexPlan builds a PostgreSQL CREATE INDEX plan.
//
// Identifiers must be generated SQL identifiers made from ASCII letters,
// digits, and underscores, starting with a letter or underscore. Table names
// may contain one schema separator, such as "app.users". Generated names are
// deterministic and capped to PostgreSQL's 63-byte identifier limit with a
// stable hash suffix when needed.
func BuildDBIndexPlan(opts DBIndexPlanOptions) (DBIndexPlan, error) {
	quotedTable, err := quoteDBIndexPlanTable(opts.Table)
	if err != nil {
		return DBIndexPlan{}, err
	}
	quotedColumns, err := quoteDBIndexPlanIdentifiers(opts.Columns, "column")
	if err != nil {
		return DBIndexPlan{}, err
	}
	if len(quotedColumns) == 0 {
		return DBIndexPlan{}, errDBIndexPlanColumnsRequired
	}

	name, err := dbIndexPlanName(opts.Name, "idx", opts.Table, opts.Columns)
	if err != nil {
		return DBIndexPlan{}, err
	}
	quotedName, err := quoteDBIndexPlanIdentifier(name, "index name")
	if err != nil {
		return DBIndexPlan{}, err
	}

	var method string
	if opts.Method != "" {
		quotedMethod, err := quoteDBIndexPlanIdentifier(opts.Method, "index method")
		if err != nil {
			return DBIndexPlan{}, err
		}
		method = " USING " + quotedMethod
	}

	var ifNotExists string
	if opts.IfNotExists {
		ifNotExists = " IF NOT EXISTS"
	}

	sql := "CREATE INDEX" + ifNotExists + " " + quotedName + " ON " + quotedTable + method + " (" + strings.Join(quotedColumns, ", ") + ")"
	return DBIndexPlan{
		Name:    name,
		Table:   opts.Table,
		Columns: cloneDBIndexPlanStrings(opts.Columns),
		SQL:     sql,
		DropSQL: "DROP INDEX " + quoteDBIndexPlanSchemaObjectName(name, opts.Table),
	}, nil
}

// BuildDBUniqueConstraintPlan builds a PostgreSQL UNIQUE constraint plan.
func BuildDBUniqueConstraintPlan(opts DBUniqueConstraintPlanOptions) (DBConstraintPlan, error) {
	quotedTable, err := quoteDBIndexPlanTable(opts.Table)
	if err != nil {
		return DBConstraintPlan{}, err
	}
	quotedColumns, err := quoteDBIndexPlanIdentifiers(opts.Columns, "column")
	if err != nil {
		return DBConstraintPlan{}, err
	}
	if len(quotedColumns) == 0 {
		return DBConstraintPlan{}, errDBIndexPlanColumnsRequired
	}

	name, err := dbIndexPlanName(opts.Name, "uq", opts.Table, opts.Columns)
	if err != nil {
		return DBConstraintPlan{}, err
	}
	quotedName, err := quoteDBIndexPlanIdentifier(name, "constraint name")
	if err != nil {
		return DBConstraintPlan{}, err
	}

	sql := "ALTER TABLE " + quotedTable + " ADD CONSTRAINT " + quotedName + " UNIQUE (" + strings.Join(quotedColumns, ", ") + ")"
	return DBConstraintPlan{
		Kind:    DBUniqueConstraint,
		Name:    name,
		Table:   opts.Table,
		Columns: cloneDBIndexPlanStrings(opts.Columns),
		SQL:     sql,
		DropSQL: dbIndexPlanDropConstraintSQL(quotedTable, quotedName),
	}, nil
}

// BuildDBForeignKeyPlan builds a PostgreSQL FOREIGN KEY constraint plan.
func BuildDBForeignKeyPlan(opts DBForeignKeyPlanOptions) (DBConstraintPlan, error) {
	if opts.InitiallyDeferred && !opts.Deferrable {
		return DBConstraintPlan{}, errDBIndexPlanInvalidDeferrable
	}

	quotedTable, err := quoteDBIndexPlanTable(opts.Table)
	if err != nil {
		return DBConstraintPlan{}, err
	}
	quotedReferenceTable, err := quoteDBIndexPlanReferenceTable(opts.ReferenceTable)
	if err != nil {
		return DBConstraintPlan{}, err
	}
	quotedColumns, err := quoteDBIndexPlanIdentifiers(opts.Columns, "column")
	if err != nil {
		return DBConstraintPlan{}, err
	}
	if len(quotedColumns) == 0 {
		return DBConstraintPlan{}, errDBIndexPlanColumnsRequired
	}
	quotedReferenceColumns, err := quoteDBIndexPlanIdentifiers(opts.ReferenceColumns, "reference column")
	if err != nil {
		return DBConstraintPlan{}, err
	}
	if len(quotedColumns) != len(quotedReferenceColumns) {
		return DBConstraintPlan{}, errDBIndexPlanForeignKeyColumnPair
	}

	name, err := dbIndexPlanName(opts.Name, "fk", opts.Table, dbIndexPlanForeignKeyNameParts(opts.Columns, opts.ReferenceTable, opts.ReferenceColumns))
	if err != nil {
		return DBConstraintPlan{}, err
	}
	quotedName, err := quoteDBIndexPlanIdentifier(name, "constraint name")
	if err != nil {
		return DBConstraintPlan{}, err
	}

	deleteSQL, err := dbForeignKeyActionSQL(" ON DELETE ", opts.OnDelete)
	if err != nil {
		return DBConstraintPlan{}, err
	}
	updateSQL, err := dbForeignKeyActionSQL(" ON UPDATE ", opts.OnUpdate)
	if err != nil {
		return DBConstraintPlan{}, err
	}

	deferrableSQL := " NOT DEFERRABLE"
	if opts.Deferrable {
		deferrableSQL = " DEFERRABLE"
		if opts.InitiallyDeferred {
			deferrableSQL += " INITIALLY DEFERRED"
		}
	}

	sql := "ALTER TABLE " + quotedTable + " ADD CONSTRAINT " + quotedName +
		" FOREIGN KEY (" + strings.Join(quotedColumns, ", ") + ") REFERENCES " +
		quotedReferenceTable + " (" + strings.Join(quotedReferenceColumns, ", ") + ")" +
		deleteSQL + updateSQL + deferrableSQL
	return DBConstraintPlan{
		Kind:             DBForeignKeyConstraint,
		Name:             name,
		Table:            opts.Table,
		Columns:          cloneDBIndexPlanStrings(opts.Columns),
		ReferenceTable:   opts.ReferenceTable,
		ReferenceColumns: cloneDBIndexPlanStrings(opts.ReferenceColumns),
		SQL:              sql,
		DropSQL:          dbIndexPlanDropConstraintSQL(quotedTable, quotedName),
	}, nil
}

// BuildDBCompositeKeyPlan builds a composite PRIMARY KEY constraint plan.
func BuildDBCompositeKeyPlan(opts DBCompositeKeyPlanOptions) (DBConstraintPlan, error) {
	quotedTable, err := quoteDBIndexPlanTable(opts.Table)
	if err != nil {
		return DBConstraintPlan{}, err
	}
	quotedColumns, err := quoteDBIndexPlanIdentifiers(opts.Columns, "column")
	if err != nil {
		return DBConstraintPlan{}, err
	}
	if len(quotedColumns) < 2 {
		return DBConstraintPlan{}, errDBIndexPlanCompositeKeyColumns
	}

	name, err := dbIndexPlanName(opts.Name, "pk", opts.Table, opts.Columns)
	if err != nil {
		return DBConstraintPlan{}, err
	}
	quotedName, err := quoteDBIndexPlanIdentifier(name, "constraint name")
	if err != nil {
		return DBConstraintPlan{}, err
	}

	sql := "ALTER TABLE " + quotedTable + " ADD CONSTRAINT " + quotedName + " PRIMARY KEY (" + strings.Join(quotedColumns, ", ") + ")"
	return DBConstraintPlan{
		Kind:    DBCompositeKeyConstraint,
		Name:    name,
		Table:   opts.Table,
		Columns: cloneDBIndexPlanStrings(opts.Columns),
		SQL:     sql,
		DropSQL: dbIndexPlanDropConstraintSQL(quotedTable, quotedName),
	}, nil
}

func dbIndexPlanName(explicit, prefix, table string, parts []string) (string, error) {
	if explicit != "" {
		if _, err := quoteDBIndexPlanIdentifier(explicit, "name"); err != nil {
			return "", err
		}
		if len(explicit) > maxDBIndexPlanIdentifierLength {
			return "", fmt.Errorf("lazuli: db index plan name %q exceeds %d bytes", explicit, maxDBIndexPlanIdentifierLength)
		}
		return explicit, nil
	}

	nameParts := []string{prefix, dbIndexPlanTableNamePart(table)}
	nameParts = append(nameParts, parts...)
	return dbIndexPlanDeterministicName(nameParts), nil
}

func dbIndexPlanForeignKeyNameParts(columns []string, referenceTable string, referenceColumns []string) []string {
	parts := make([]string, 0, len(columns)+1+len(referenceColumns))
	parts = append(parts, columns...)
	parts = append(parts, dbIndexPlanTableNamePart(referenceTable))
	parts = append(parts, referenceColumns...)
	return parts
}

func dbIndexPlanDeterministicName(parts []string) string {
	normalized := make([]string, 0, len(parts))
	for _, part := range parts {
		normalized = append(normalized, strings.ToLower(part))
	}
	base := strings.Join(normalized, "_")
	if len(base) <= maxDBIndexPlanIdentifierLength {
		return base
	}

	sum := sha256.Sum256([]byte(base))
	hash := hex.EncodeToString(sum[:])[:dbIndexPlanNameHashLength]
	headLength := maxDBIndexPlanIdentifierLength - len(hash) - 1
	head := strings.TrimRight(base[:headLength], "_")
	if head == "" {
		head = base[:headLength]
	}
	return head + "_" + hash
}

func dbIndexPlanTableNamePart(table string) string {
	parts := strings.Split(table, ".")
	return parts[len(parts)-1]
}

func quoteDBIndexPlanReferenceTable(table string) (string, error) {
	if table == "" {
		return "", errDBIndexPlanReferenceRequired
	}
	return quoteDBIndexPlanTable(table)
}

func quoteDBIndexPlanTable(table string) (string, error) {
	if table == "" {
		return "", errDBIndexPlanTableRequired
	}
	parts := strings.Split(table, ".")
	if len(parts) > 2 {
		return "", fmt.Errorf("lazuli: invalid db index plan table name %q", table)
	}

	quoted := make([]string, 0, len(parts))
	for _, part := range parts {
		quotedPart, err := quoteDBIndexPlanIdentifier(part, "table name")
		if err != nil {
			return "", fmt.Errorf("lazuli: invalid db index plan table name %q", table)
		}
		quoted = append(quoted, quotedPart)
	}
	return strings.Join(quoted, "."), nil
}

func quoteDBIndexPlanIdentifiers(identifiers []string, label string) ([]string, error) {
	quoted := make([]string, 0, len(identifiers))
	for _, identifier := range identifiers {
		quotedIdentifier, err := quoteDBIndexPlanIdentifier(identifier, label)
		if err != nil {
			return nil, err
		}
		quoted = append(quoted, quotedIdentifier)
	}
	return quoted, nil
}

func quoteDBIndexPlanIdentifier(identifier, label string) (string, error) {
	if !validDBIndexPlanIdentifier(identifier) {
		return "", fmt.Errorf("lazuli: invalid db index plan %s %q", label, identifier)
	}
	return `"` + identifier + `"`, nil
}

func validDBIndexPlanIdentifier(identifier string) bool {
	if identifier == "" {
		return false
	}
	for i := 0; i < len(identifier); i++ {
		c := identifier[i]
		if i == 0 {
			if !isDBIndexPlanIdentifierLetter(c) && c != '_' {
				return false
			}
			continue
		}
		if !isDBIndexPlanIdentifierLetter(c) && !isDBIndexPlanIdentifierDigit(c) && c != '_' {
			return false
		}
	}
	return true
}

func isDBIndexPlanIdentifierLetter(c byte) bool {
	return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')
}

func isDBIndexPlanIdentifierDigit(c byte) bool {
	return c >= '0' && c <= '9'
}

func quoteDBIndexPlanSchemaObjectName(name, table string) string {
	quotedName := `"` + name + `"`
	parts := strings.Split(table, ".")
	if len(parts) != 2 {
		return quotedName
	}
	return `"` + parts[0] + `".` + quotedName
}

func dbIndexPlanDropConstraintSQL(quotedTable, quotedName string) string {
	return "ALTER TABLE " + quotedTable + " DROP CONSTRAINT " + quotedName
}

func dbForeignKeyActionSQL(prefix string, action DBForeignKeyAction) (string, error) {
	switch action {
	case DBForeignKeyDefaultAction:
		return "", nil
	case DBForeignKeyNoAction, DBForeignKeyRestrict, DBForeignKeyCascade, DBForeignKeySetNull, DBForeignKeySetDefault:
		return prefix + string(action), nil
	default:
		return "", fmt.Errorf("%w: %q", errDBIndexPlanInvalidAction, action)
	}
}

func cloneDBIndexPlanStrings(values []string) []string {
	if len(values) == 0 {
		return nil
	}
	out := make([]string, len(values))
	copy(out, values)
	return out
}
