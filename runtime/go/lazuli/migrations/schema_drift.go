package migrations

import (
	"errors"
	"fmt"
	"sort"
	"strconv"
	"strings"
)

var (
	// ErrSchemaDriftNameRequired is returned when a schema drift table or
	// table-scoped object has no stable name.
	ErrSchemaDriftNameRequired = errors.New("migrations: schema drift name required")
	// ErrDuplicateSchemaDriftObject is returned when a schema drift snapshot
	// contains duplicate tables, columns, indexes, or constraints.
	ErrDuplicateSchemaDriftObject = errors.New("migrations: duplicate schema drift object")
)

// SchemaSnapshot is an in-memory schema description used for drift detection.
//
// Callers are responsible for building snapshots from generated contracts and
// provider metadata. DiffSchemaDrift only compares values; it never opens a
// database connection.
type SchemaSnapshot struct {
	Tables []SchemaTable
}

// SchemaTable describes one expected or observed database table.
type SchemaTable struct {
	// Name is the schema-qualified or unqualified table name.
	Name TableName
	// Columns are matched by Name within this table.
	Columns []SchemaColumn
	// Indexes are matched by Name within this table.
	Indexes []SchemaIndex
	// Constraints are matched by Name within this table.
	Constraints []SchemaConstraint
}

// SchemaColumn describes a table column for drift detection.
type SchemaColumn struct {
	Name      string
	Type      string
	Nullable  bool
	Default   string
	Generated string
}

// SchemaIndex describes a table index for drift detection.
type SchemaIndex struct {
	Name      string
	Columns   []string
	Unique    bool
	Method    string
	Predicate string
}

// SchemaConstraintType names the common provider-neutral constraint kinds.
type SchemaConstraintType string

const (
	// SchemaConstraintPrimaryKey describes a primary-key constraint.
	SchemaConstraintPrimaryKey SchemaConstraintType = "primary_key"
	// SchemaConstraintForeignKey describes a foreign-key constraint.
	SchemaConstraintForeignKey SchemaConstraintType = "foreign_key"
	// SchemaConstraintUnique describes a unique constraint.
	SchemaConstraintUnique SchemaConstraintType = "unique"
	// SchemaConstraintCheck describes a check constraint.
	SchemaConstraintCheck SchemaConstraintType = "check"
)

// SchemaConstraint describes a table constraint for drift detection.
type SchemaConstraint struct {
	Name              string
	Type              SchemaConstraintType
	Columns           []string
	ReferencedTable   TableName
	ReferencedColumns []string
	Expression        string
}

// SchemaDriftObject identifies the schema object type involved in an issue.
type SchemaDriftObject string

const (
	// SchemaDriftObjectTable identifies a table-level drift issue.
	SchemaDriftObjectTable SchemaDriftObject = "table"
	// SchemaDriftObjectColumn identifies a column-level drift issue.
	SchemaDriftObjectColumn SchemaDriftObject = "column"
	// SchemaDriftObjectIndex identifies an index-level drift issue.
	SchemaDriftObjectIndex SchemaDriftObject = "index"
	// SchemaDriftObjectConstraint identifies a constraint-level drift issue.
	SchemaDriftObjectConstraint SchemaDriftObject = "constraint"
)

// SchemaDriftKind classifies a schema drift issue.
type SchemaDriftKind string

const (
	// SchemaDriftMissingTable means an expected table is absent from observed.
	SchemaDriftMissingTable SchemaDriftKind = "missing_table"
	// SchemaDriftUnexpectedTable means observed contains a table not expected.
	SchemaDriftUnexpectedTable SchemaDriftKind = "unexpected_table"
	// SchemaDriftMissingColumn means an expected column is absent from observed.
	SchemaDriftMissingColumn SchemaDriftKind = "missing_column"
	// SchemaDriftUnexpectedColumn means observed contains a column not expected.
	SchemaDriftUnexpectedColumn SchemaDriftKind = "unexpected_column"
	// SchemaDriftChangedColumn means an expected column exists but differs.
	SchemaDriftChangedColumn SchemaDriftKind = "changed_column"
	// SchemaDriftMissingIndex means an expected index is absent from observed.
	SchemaDriftMissingIndex SchemaDriftKind = "missing_index"
	// SchemaDriftUnexpectedIndex means observed contains an index not expected.
	SchemaDriftUnexpectedIndex SchemaDriftKind = "unexpected_index"
	// SchemaDriftChangedIndex means an expected index exists but differs.
	SchemaDriftChangedIndex SchemaDriftKind = "changed_index"
	// SchemaDriftMissingConstraint means an expected constraint is absent from
	// observed.
	SchemaDriftMissingConstraint SchemaDriftKind = "missing_constraint"
	// SchemaDriftUnexpectedConstraint means observed contains a constraint not
	// expected.
	SchemaDriftUnexpectedConstraint SchemaDriftKind = "unexpected_constraint"
	// SchemaDriftChangedConstraint means an expected constraint exists but
	// differs.
	SchemaDriftChangedConstraint SchemaDriftKind = "changed_constraint"
)

// SchemaDriftFieldChange describes one field mismatch on a matching schema
// object.
type SchemaDriftFieldChange struct {
	Field    string
	Expected string
	Observed string
}

// SchemaDriftIssue describes one deterministic drift issue between expected
// and observed schema snapshots.
type SchemaDriftIssue struct {
	Kind   SchemaDriftKind
	Object SchemaDriftObject
	Path   string
	Table  TableName
	Name   string
	Fields []SchemaDriftFieldChange
}

// SchemaDriftReport is the deterministic result of DiffSchemaDrift.
type SchemaDriftReport struct {
	Issues []SchemaDriftIssue
}

// HasDrift reports whether the comparison found any schema drift.
func (r SchemaDriftReport) HasDrift() bool {
	return len(r.Issues) > 0
}

// DiffSchemaDrift compares expected and observed schema snapshots without
// contacting a database. The returned issues are sorted by path, object, name,
// and kind so equivalent inputs produce identical output.
func DiffSchemaDrift(expected, observed SchemaSnapshot) (SchemaDriftReport, error) {
	expectedSnapshot, err := normalizeSchemaDriftSnapshot("expected", expected)
	if err != nil {
		return SchemaDriftReport{}, err
	}
	observedSnapshot, err := normalizeSchemaDriftSnapshot("observed", observed)
	if err != nil {
		return SchemaDriftReport{}, err
	}

	issues := make([]SchemaDriftIssue, 0)
	for _, expectedTable := range expectedSnapshot.tables {
		tableKey := schemaDriftTableMapKey(expectedTable.Name)
		observedTable, ok := observedSnapshot.byKey[tableKey]
		if !ok {
			issues = append(issues, schemaDriftIssue(SchemaDriftMissingTable, SchemaDriftObjectTable, expectedTable.Name, schemaDriftTableDisplayName(expectedTable.Name), nil))
			continue
		}

		issues = append(issues, diffSchemaDriftColumns(expectedTable.Name, expectedTable.Columns, observedTable.Columns)...)
		issues = append(issues, diffSchemaDriftIndexes(expectedTable.Name, expectedTable.Indexes, observedTable.Indexes)...)
		issues = append(issues, diffSchemaDriftConstraints(expectedTable.Name, expectedTable.Constraints, observedTable.Constraints)...)
	}

	for _, observedTable := range observedSnapshot.tables {
		if _, ok := expectedSnapshot.byKey[schemaDriftTableMapKey(observedTable.Name)]; ok {
			continue
		}
		issues = append(issues, schemaDriftIssue(SchemaDriftUnexpectedTable, SchemaDriftObjectTable, observedTable.Name, schemaDriftTableDisplayName(observedTable.Name), nil))
	}

	sort.SliceStable(issues, func(i, j int) bool {
		return schemaDriftIssueLess(issues[i], issues[j])
	})
	return SchemaDriftReport{Issues: issues}, nil
}

type normalizedSchemaDriftSnapshot struct {
	tables []SchemaTable
	byKey  map[string]SchemaTable
}

func normalizeSchemaDriftSnapshot(side string, snapshot SchemaSnapshot) (normalizedSchemaDriftSnapshot, error) {
	tables := make([]SchemaTable, len(snapshot.Tables))
	byKey := make(map[string]SchemaTable, len(snapshot.Tables))
	for i, table := range snapshot.Tables {
		normalized, err := normalizeSchemaDriftTable(table)
		if err != nil {
			return normalizedSchemaDriftSnapshot{}, fmt.Errorf("migrations: %s table %d: %w", side, i, err)
		}

		key := schemaDriftTableMapKey(normalized.Name)
		if _, exists := byKey[key]; exists {
			return normalizedSchemaDriftSnapshot{}, fmt.Errorf("%w %q", ErrDuplicateSchemaDriftObject, schemaDriftTableDisplayName(normalized.Name))
		}
		tables[i] = normalized
		byKey[key] = normalized
	}

	sort.SliceStable(tables, func(i, j int) bool {
		return schemaDriftTableLess(tables[i].Name, tables[j].Name)
	})
	return normalizedSchemaDriftSnapshot{tables: tables, byKey: byKey}, nil
}

func normalizeSchemaDriftTable(table SchemaTable) (SchemaTable, error) {
	table.Name = normalizeSchemaDriftRequiredTableName(table.Name)
	if table.Name.Name == "" {
		return SchemaTable{}, ErrSchemaDriftNameRequired
	}

	columns, err := normalizeSchemaDriftColumns(table.Name, table.Columns)
	if err != nil {
		return SchemaTable{}, err
	}
	indexes, err := normalizeSchemaDriftIndexes(table.Name, table.Indexes)
	if err != nil {
		return SchemaTable{}, err
	}
	constraints, err := normalizeSchemaDriftConstraints(table.Name, table.Constraints)
	if err != nil {
		return SchemaTable{}, err
	}

	table.Columns = columns
	table.Indexes = indexes
	table.Constraints = constraints
	return table, nil
}

func normalizeSchemaDriftColumns(table TableName, columns []SchemaColumn) ([]SchemaColumn, error) {
	normalized := make([]SchemaColumn, len(columns))
	seen := make(map[string]struct{}, len(columns))
	for i, column := range columns {
		column = normalizeSchemaDriftColumn(column)
		if column.Name == "" {
			return nil, fmt.Errorf("column %d: %w", i, ErrSchemaDriftNameRequired)
		}
		if _, exists := seen[column.Name]; exists {
			return nil, fmt.Errorf("%w %q", ErrDuplicateSchemaDriftObject, schemaDriftObjectPath(table, SchemaDriftObjectColumn, column.Name))
		}
		seen[column.Name] = struct{}{}
		normalized[i] = column
	}
	sort.SliceStable(normalized, func(i, j int) bool {
		return normalized[i].Name < normalized[j].Name
	})
	return normalized, nil
}

func normalizeSchemaDriftColumn(column SchemaColumn) SchemaColumn {
	column.Name = strings.TrimSpace(column.Name)
	column.Type = strings.TrimSpace(column.Type)
	column.Default = strings.TrimSpace(column.Default)
	column.Generated = strings.TrimSpace(column.Generated)
	return column
}

func normalizeSchemaDriftIndexes(table TableName, indexes []SchemaIndex) ([]SchemaIndex, error) {
	normalized := make([]SchemaIndex, len(indexes))
	seen := make(map[string]struct{}, len(indexes))
	for i, index := range indexes {
		index = normalizeSchemaDriftIndex(index)
		if index.Name == "" {
			return nil, fmt.Errorf("index %d: %w", i, ErrSchemaDriftNameRequired)
		}
		if _, exists := seen[index.Name]; exists {
			return nil, fmt.Errorf("%w %q", ErrDuplicateSchemaDriftObject, schemaDriftObjectPath(table, SchemaDriftObjectIndex, index.Name))
		}
		seen[index.Name] = struct{}{}
		normalized[i] = index
	}
	sort.SliceStable(normalized, func(i, j int) bool {
		return normalized[i].Name < normalized[j].Name
	})
	return normalized, nil
}

func normalizeSchemaDriftIndex(index SchemaIndex) SchemaIndex {
	index.Name = strings.TrimSpace(index.Name)
	index.Columns = normalizeSchemaDriftStrings(index.Columns)
	index.Method = strings.TrimSpace(index.Method)
	index.Predicate = strings.TrimSpace(index.Predicate)
	return index
}

func normalizeSchemaDriftConstraints(table TableName, constraints []SchemaConstraint) ([]SchemaConstraint, error) {
	normalized := make([]SchemaConstraint, len(constraints))
	seen := make(map[string]struct{}, len(constraints))
	for i, constraint := range constraints {
		constraint = normalizeSchemaDriftConstraint(constraint)
		if constraint.Name == "" {
			return nil, fmt.Errorf("constraint %d: %w", i, ErrSchemaDriftNameRequired)
		}
		if _, exists := seen[constraint.Name]; exists {
			return nil, fmt.Errorf("%w %q", ErrDuplicateSchemaDriftObject, schemaDriftObjectPath(table, SchemaDriftObjectConstraint, constraint.Name))
		}
		seen[constraint.Name] = struct{}{}
		normalized[i] = constraint
	}
	sort.SliceStable(normalized, func(i, j int) bool {
		return normalized[i].Name < normalized[j].Name
	})
	return normalized, nil
}

func normalizeSchemaDriftConstraint(constraint SchemaConstraint) SchemaConstraint {
	constraint.Name = strings.TrimSpace(constraint.Name)
	constraint.Type = SchemaConstraintType(strings.TrimSpace(string(constraint.Type)))
	constraint.Columns = normalizeSchemaDriftStrings(constraint.Columns)
	constraint.ReferencedTable = normalizeSchemaDriftOptionalTableName(constraint.ReferencedTable)
	constraint.ReferencedColumns = normalizeSchemaDriftStrings(constraint.ReferencedColumns)
	constraint.Expression = strings.TrimSpace(constraint.Expression)
	return constraint
}

func normalizeSchemaDriftStrings(values []string) []string {
	normalized := make([]string, len(values))
	for i, value := range values {
		normalized[i] = strings.TrimSpace(value)
	}
	return normalized
}

func normalizeSchemaDriftRequiredTableName(table TableName) TableName {
	return TableName{
		Schema: strings.TrimSpace(table.Schema),
		Name:   strings.TrimSpace(table.Name),
	}
}

func normalizeSchemaDriftOptionalTableName(table TableName) TableName {
	if table.Schema == "" && table.Name == "" {
		return TableName{}
	}
	return normalizeSchemaDriftRequiredTableName(table)
}

func diffSchemaDriftColumns(table TableName, expected, observed []SchemaColumn) []SchemaDriftIssue {
	observedByName := schemaDriftColumnMap(observed)
	expectedByName := schemaDriftColumnMap(expected)
	issues := make([]SchemaDriftIssue, 0)

	for _, expectedColumn := range expected {
		observedColumn, ok := observedByName[expectedColumn.Name]
		if !ok {
			issues = append(issues, schemaDriftIssue(SchemaDriftMissingColumn, SchemaDriftObjectColumn, table, expectedColumn.Name, nil))
			continue
		}

		fields := schemaDriftColumnChanges(expectedColumn, observedColumn)
		if len(fields) > 0 {
			issues = append(issues, schemaDriftIssue(SchemaDriftChangedColumn, SchemaDriftObjectColumn, table, expectedColumn.Name, fields))
		}
	}

	for _, observedColumn := range observed {
		if _, ok := expectedByName[observedColumn.Name]; ok {
			continue
		}
		issues = append(issues, schemaDriftIssue(SchemaDriftUnexpectedColumn, SchemaDriftObjectColumn, table, observedColumn.Name, nil))
	}
	return issues
}

func diffSchemaDriftIndexes(table TableName, expected, observed []SchemaIndex) []SchemaDriftIssue {
	observedByName := schemaDriftIndexMap(observed)
	expectedByName := schemaDriftIndexMap(expected)
	issues := make([]SchemaDriftIssue, 0)

	for _, expectedIndex := range expected {
		observedIndex, ok := observedByName[expectedIndex.Name]
		if !ok {
			issues = append(issues, schemaDriftIssue(SchemaDriftMissingIndex, SchemaDriftObjectIndex, table, expectedIndex.Name, nil))
			continue
		}

		fields := schemaDriftIndexChanges(expectedIndex, observedIndex)
		if len(fields) > 0 {
			issues = append(issues, schemaDriftIssue(SchemaDriftChangedIndex, SchemaDriftObjectIndex, table, expectedIndex.Name, fields))
		}
	}

	for _, observedIndex := range observed {
		if _, ok := expectedByName[observedIndex.Name]; ok {
			continue
		}
		issues = append(issues, schemaDriftIssue(SchemaDriftUnexpectedIndex, SchemaDriftObjectIndex, table, observedIndex.Name, nil))
	}
	return issues
}

func diffSchemaDriftConstraints(table TableName, expected, observed []SchemaConstraint) []SchemaDriftIssue {
	observedByName := schemaDriftConstraintMap(observed)
	expectedByName := schemaDriftConstraintMap(expected)
	issues := make([]SchemaDriftIssue, 0)

	for _, expectedConstraint := range expected {
		observedConstraint, ok := observedByName[expectedConstraint.Name]
		if !ok {
			issues = append(issues, schemaDriftIssue(SchemaDriftMissingConstraint, SchemaDriftObjectConstraint, table, expectedConstraint.Name, nil))
			continue
		}

		fields := schemaDriftConstraintChanges(expectedConstraint, observedConstraint)
		if len(fields) > 0 {
			issues = append(issues, schemaDriftIssue(SchemaDriftChangedConstraint, SchemaDriftObjectConstraint, table, expectedConstraint.Name, fields))
		}
	}

	for _, observedConstraint := range observed {
		if _, ok := expectedByName[observedConstraint.Name]; ok {
			continue
		}
		issues = append(issues, schemaDriftIssue(SchemaDriftUnexpectedConstraint, SchemaDriftObjectConstraint, table, observedConstraint.Name, nil))
	}
	return issues
}

func schemaDriftColumnChanges(expected, observed SchemaColumn) []SchemaDriftFieldChange {
	fields := make([]SchemaDriftFieldChange, 0)
	if expected.Type != observed.Type {
		fields = append(fields, schemaDriftFieldChange("type", expected.Type, observed.Type))
	}
	if expected.Nullable != observed.Nullable {
		fields = append(fields, schemaDriftFieldChange("nullable", strconv.FormatBool(expected.Nullable), strconv.FormatBool(observed.Nullable)))
	}
	if expected.Default != observed.Default {
		fields = append(fields, schemaDriftFieldChange("default", expected.Default, observed.Default))
	}
	if expected.Generated != observed.Generated {
		fields = append(fields, schemaDriftFieldChange("generated", expected.Generated, observed.Generated))
	}
	return fields
}

func schemaDriftIndexChanges(expected, observed SchemaIndex) []SchemaDriftFieldChange {
	fields := make([]SchemaDriftFieldChange, 0)
	if expected.Unique != observed.Unique {
		fields = append(fields, schemaDriftFieldChange("unique", strconv.FormatBool(expected.Unique), strconv.FormatBool(observed.Unique)))
	}
	if expected.Method != observed.Method {
		fields = append(fields, schemaDriftFieldChange("method", expected.Method, observed.Method))
	}
	if !schemaDriftStringsEqual(expected.Columns, observed.Columns) {
		fields = append(fields, schemaDriftFieldChange("columns", schemaDriftStringList(expected.Columns), schemaDriftStringList(observed.Columns)))
	}
	if expected.Predicate != observed.Predicate {
		fields = append(fields, schemaDriftFieldChange("predicate", expected.Predicate, observed.Predicate))
	}
	return fields
}

func schemaDriftConstraintChanges(expected, observed SchemaConstraint) []SchemaDriftFieldChange {
	fields := make([]SchemaDriftFieldChange, 0)
	if expected.Type != observed.Type {
		fields = append(fields, schemaDriftFieldChange("type", string(expected.Type), string(observed.Type)))
	}
	if !schemaDriftStringsEqual(expected.Columns, observed.Columns) {
		fields = append(fields, schemaDriftFieldChange("columns", schemaDriftStringList(expected.Columns), schemaDriftStringList(observed.Columns)))
	}
	if expected.ReferencedTable != observed.ReferencedTable {
		fields = append(fields, schemaDriftFieldChange("referenced_table", schemaDriftTableDisplayName(expected.ReferencedTable), schemaDriftTableDisplayName(observed.ReferencedTable)))
	}
	if !schemaDriftStringsEqual(expected.ReferencedColumns, observed.ReferencedColumns) {
		fields = append(fields, schemaDriftFieldChange("referenced_columns", schemaDriftStringList(expected.ReferencedColumns), schemaDriftStringList(observed.ReferencedColumns)))
	}
	if expected.Expression != observed.Expression {
		fields = append(fields, schemaDriftFieldChange("expression", expected.Expression, observed.Expression))
	}
	return fields
}

func schemaDriftFieldChange(field, expected, observed string) SchemaDriftFieldChange {
	return SchemaDriftFieldChange{
		Field:    field,
		Expected: expected,
		Observed: observed,
	}
}

func schemaDriftColumnMap(columns []SchemaColumn) map[string]SchemaColumn {
	byName := make(map[string]SchemaColumn, len(columns))
	for _, column := range columns {
		byName[column.Name] = column
	}
	return byName
}

func schemaDriftIndexMap(indexes []SchemaIndex) map[string]SchemaIndex {
	byName := make(map[string]SchemaIndex, len(indexes))
	for _, index := range indexes {
		byName[index.Name] = index
	}
	return byName
}

func schemaDriftConstraintMap(constraints []SchemaConstraint) map[string]SchemaConstraint {
	byName := make(map[string]SchemaConstraint, len(constraints))
	for _, constraint := range constraints {
		byName[constraint.Name] = constraint
	}
	return byName
}

func schemaDriftIssue(kind SchemaDriftKind, object SchemaDriftObject, table TableName, name string, fields []SchemaDriftFieldChange) SchemaDriftIssue {
	return SchemaDriftIssue{
		Kind:   kind,
		Object: object,
		Path:   schemaDriftObjectPath(table, object, name),
		Table:  table,
		Name:   name,
		Fields: fields,
	}
}

func schemaDriftObjectPath(table TableName, object SchemaDriftObject, name string) string {
	tableName := schemaDriftTableDisplayName(table)
	if object == SchemaDriftObjectTable {
		return tableName
	}
	return tableName + "." + schemaDriftObjectCollection(object) + "." + name
}

func schemaDriftObjectCollection(object SchemaDriftObject) string {
	switch object {
	case SchemaDriftObjectColumn:
		return "columns"
	case SchemaDriftObjectIndex:
		return "indexes"
	case SchemaDriftObjectConstraint:
		return "constraints"
	default:
		return string(object) + "s"
	}
}

func schemaDriftIssueLess(a, b SchemaDriftIssue) bool {
	if a.Path != b.Path {
		return a.Path < b.Path
	}
	if a.Object != b.Object {
		return schemaDriftObjectRank(a.Object) < schemaDriftObjectRank(b.Object)
	}
	if a.Name != b.Name {
		return a.Name < b.Name
	}
	return schemaDriftKindRank(a.Kind) < schemaDriftKindRank(b.Kind)
}

func schemaDriftObjectRank(object SchemaDriftObject) int {
	switch object {
	case SchemaDriftObjectTable:
		return 0
	case SchemaDriftObjectColumn:
		return 1
	case SchemaDriftObjectIndex:
		return 2
	case SchemaDriftObjectConstraint:
		return 3
	default:
		return 4
	}
}

func schemaDriftKindRank(kind SchemaDriftKind) int {
	switch kind {
	case SchemaDriftMissingTable, SchemaDriftMissingColumn, SchemaDriftMissingIndex, SchemaDriftMissingConstraint:
		return 0
	case SchemaDriftUnexpectedTable, SchemaDriftUnexpectedColumn, SchemaDriftUnexpectedIndex, SchemaDriftUnexpectedConstraint:
		return 1
	case SchemaDriftChangedColumn, SchemaDriftChangedIndex, SchemaDriftChangedConstraint:
		return 2
	default:
		return 3
	}
}

func schemaDriftTableLess(a, b TableName) bool {
	if a.Schema != b.Schema {
		return a.Schema < b.Schema
	}
	return a.Name < b.Name
}

func schemaDriftTableMapKey(table TableName) string {
	return table.Schema + "\x00" + table.Name
}

func schemaDriftTableDisplayName(table TableName) string {
	if table.Schema == "" {
		return table.Name
	}
	if table.Name == "" {
		return table.Schema
	}
	return table.Schema + "." + table.Name
}

func schemaDriftStringList(values []string) string {
	return strings.Join(values, ", ")
}

func schemaDriftStringsEqual(left, right []string) bool {
	if len(left) != len(right) {
		return false
	}
	for i := range left {
		if left[i] != right[i] {
			return false
		}
	}
	return true
}
