package admin

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"unicode"
)

const (
	// DefaultDashboardGroup is used by dashboard grouping helpers when
	// Display.Group is empty.
	DefaultDashboardGroup = "Dashboard"
)

// MetricFormat describes how a dashboard metric value should be rendered by a
// generated admin surface.
type MetricFormat string

const (
	// MetricFormatNumber renders a plain numeric value.
	MetricFormatNumber MetricFormat = "number"

	// MetricFormatCurrency renders a money value.
	MetricFormatCurrency MetricFormat = "currency"

	// MetricFormatPercent renders a percentage value.
	MetricFormatPercent MetricFormat = "percent"

	// MetricFormatDuration renders a duration value.
	MetricFormatDuration MetricFormat = "duration"

	// MetricFormatBytes renders a byte count.
	MetricFormatBytes MetricFormat = "bytes"

	// MetricFormatText renders a textual value.
	MetricFormatText MetricFormat = "text"
)

var (
	// ErrInvalidDashboardMetric reports structurally invalid dashboard metric
	// metadata.
	ErrInvalidDashboardMetric = errors.New("lazuli/admin: invalid dashboard metric")

	// ErrDuplicateDashboardMetric reports duplicate dashboard metric names.
	ErrDuplicateDashboardMetric = errors.New("lazuli/admin: duplicate dashboard metric")

	// ErrInvalidDashboardCard reports structurally invalid dashboard card
	// metadata.
	ErrInvalidDashboardCard = errors.New("lazuli/admin: invalid dashboard card")

	// ErrDuplicateDashboardCard reports duplicate dashboard card names.
	ErrDuplicateDashboardCard = errors.New("lazuli/admin: duplicate dashboard card")

	// ErrInvalidDashboardTable reports structurally invalid dashboard table
	// metadata.
	ErrInvalidDashboardTable = errors.New("lazuli/admin: invalid dashboard table")

	// ErrDuplicateDashboardTable reports duplicate dashboard table names.
	ErrDuplicateDashboardTable = errors.New("lazuli/admin: duplicate dashboard table")

	// ErrInvalidDashboardTableColumn reports structurally invalid dashboard
	// table column metadata.
	ErrInvalidDashboardTableColumn = errors.New("lazuli/admin: invalid dashboard table column")

	// ErrDuplicateDashboardTableColumn reports duplicate dashboard table column
	// names within one table.
	ErrDuplicateDashboardTableColumn = errors.New("lazuli/admin: duplicate dashboard table column")
)

// DashboardDataRef names an existing generated data provider or value binding
// used by a generated dashboard surface.
type DashboardDataRef string

// String returns the raw data reference.
func (r DashboardDataRef) String() string {
	return string(r)
}

// DashboardMetricRef names a DashboardMetric by its stable Name.
type DashboardMetricRef string

// String returns the raw metric reference.
func (r DashboardMetricRef) String() string {
	return string(r)
}

// DashboardVisibility carries visibility and role metadata for generated admin
// dashboard descriptors. Empty Roles means all roles may see the descriptor
// when Hidden is false.
type DashboardVisibility struct {
	// Hidden marks the descriptor as hidden from generated default surfaces.
	Hidden bool

	// Roles optionally restricts the descriptor to users with at least one of
	// these roles.
	Roles []string
}

// VisibleForRoles reports whether this visibility metadata allows at least one
// active role. Hidden descriptors are never visible.
func (visibility DashboardVisibility) VisibleForRoles(activeRoles ...string) bool {
	if visibility.Hidden {
		return false
	}

	requiredRoles := cleanDashboardRoleNames(visibility.Roles)
	if len(requiredRoles) == 0 {
		return true
	}

	active := dashboardRoleSet(activeRoles)
	for _, role := range requiredRoles {
		if _, ok := active[metadataKey(role)]; ok {
			return true
		}
	}
	return false
}

// DashboardMetric describes one generator-neutral admin dashboard metric.
type DashboardMetric struct {
	// Name is the stable generator identifier, for example "monthly_revenue".
	Name string

	// Label is optional display text. Empty defaults to Name in normalized copies.
	Label string

	// Description is optional one-line display text for the metric.
	Description string

	// Format describes how the metric value should be rendered. Empty defaults
	// to MetricFormatNumber in normalized copies.
	Format MetricFormat

	// Unit is an optional short unit label, for example "ms" or "orders".
	Unit string

	// ValueRef optionally points at a generated data binding for the metric
	// value.
	ValueRef DashboardDataRef

	// Display carries grouping and placement hints for generated dashboard
	// surfaces.
	Display DisplayHint

	// Visibility carries hidden and role metadata for generated dashboard
	// surfaces.
	Visibility DashboardVisibility
}

// DashboardCard describes one metric card on a generated admin dashboard.
type DashboardCard struct {
	// Name is the stable generator identifier, for example "revenue_card".
	Name string

	// Label is optional display text. Empty defaults to Name in normalized copies.
	Label string

	// Description is optional one-line display text for the card.
	Description string

	// MetricRef points at the metric rendered by this card.
	MetricRef DashboardMetricRef

	// Display carries grouping and placement hints for generated dashboard
	// surfaces.
	Display DisplayHint

	// Visibility carries hidden and role metadata for generated dashboard
	// surfaces.
	Visibility DashboardVisibility
}

// DashboardTable describes one tabular block on a generated admin dashboard.
type DashboardTable struct {
	// Name is the stable generator identifier, for example "recent_orders".
	Name string

	// Label is optional display text. Empty defaults to Name in normalized copies.
	Label string

	// Description is optional one-line display text for the table.
	Description string

	// SourceRef optionally points at a generated data binding for the table rows.
	SourceRef DashboardDataRef

	// Columns describes the table columns exposed to the generated dashboard.
	Columns []DashboardTableColumn

	// Display carries grouping and placement hints for generated dashboard
	// surfaces.
	Display DisplayHint

	// Visibility carries hidden and role metadata for generated dashboard
	// surfaces.
	Visibility DashboardVisibility
}

// DashboardTableColumn describes one generated dashboard table column.
type DashboardTableColumn struct {
	// Name is the stable generator identifier, for example "created_at".
	Name string

	// Label is optional display text. Empty defaults to Name in normalized copies.
	Label string

	// Type is the generator-neutral column type, for example "string" or
	// "datetime".
	Type string

	// Description is optional one-line display text for the column.
	Description string

	// Format optionally describes how the column value should be rendered.
	Format MetricFormat

	// Display carries grouping and placement hints for generated dashboard
	// surfaces.
	Display DisplayHint

	// Visibility carries hidden and role metadata for generated dashboard
	// surfaces.
	Visibility DashboardVisibility
}

// DashboardMetricGroup is a deterministic group of dashboard metrics.
type DashboardMetricGroup struct {
	Label   string
	Metrics []DashboardMetric
}

// DashboardCardGroup is a deterministic group of dashboard cards.
type DashboardCardGroup struct {
	Label string
	Cards []DashboardCard
}

// DashboardTableGroup is a deterministic group of dashboard tables.
type DashboardTableGroup struct {
	Label  string
	Tables []DashboardTable
}

// NumberMetric returns a dashboard metric with MetricFormatNumber.
func NumberMetric(name string) DashboardMetric {
	return DashboardMetric{Name: name, Format: MetricFormatNumber}
}

// CurrencyMetric returns a dashboard metric with MetricFormatCurrency.
func CurrencyMetric(name string) DashboardMetric {
	return DashboardMetric{Name: name, Format: MetricFormatCurrency}
}

// PercentMetric returns a dashboard metric with MetricFormatPercent.
func PercentMetric(name string) DashboardMetric {
	return DashboardMetric{Name: name, Format: MetricFormatPercent}
}

// MetricCard returns a dashboard card for one metric.
func MetricCard(name, metric string) DashboardCard {
	return DashboardCard{Name: name, MetricRef: DashboardMetricRef(metric)}
}

// DataTable returns a dashboard table with copied column descriptors.
func DataTable(name string, columns ...DashboardTableColumn) DashboardTable {
	return DashboardTable{Name: name, Columns: append([]DashboardTableColumn(nil), columns...)}
}

// TableColumn returns a dashboard table column descriptor.
func TableColumn(name, typ string) DashboardTableColumn {
	return DashboardTableColumn{Name: name, Type: typ}
}

// WithLabel returns a copy with display label metadata.
func (metric DashboardMetric) WithLabel(label string) DashboardMetric {
	metric.Label = label
	return metric
}

// WithDescription returns a copy with display description metadata.
func (metric DashboardMetric) WithDescription(description string) DashboardMetric {
	metric.Description = description
	return metric
}

// WithGroup returns a copy assigned to a dashboard display group.
func (metric DashboardMetric) WithGroup(group string) DashboardMetric {
	metric.Display.Group = group
	return metric
}

// WithOrder returns a copy with dashboard ordering metadata inside its group.
func (metric DashboardMetric) WithOrder(order int) DashboardMetric {
	metric.Display.Order = order
	return metric
}

// WithUnit returns a copy with unit display metadata.
func (metric DashboardMetric) WithUnit(unit string) DashboardMetric {
	metric.Unit = unit
	return metric
}

// WithValueRef returns a copy with a generated value binding reference.
func (metric DashboardMetric) WithValueRef(ref string) DashboardMetric {
	metric.ValueRef = DashboardDataRef(ref)
	return metric
}

// WithRoles returns a copy restricted to the supplied roles.
func (metric DashboardMetric) WithRoles(roles ...string) DashboardMetric {
	metric.Visibility.Roles = append([]string(nil), roles...)
	return metric
}

// AsHidden returns a copy hidden from generated default dashboard surfaces.
func (metric DashboardMetric) AsHidden() DashboardMetric {
	metric.Visibility.Hidden = true
	return metric
}

// VisibleForRoles reports whether this metric should be visible for at least
// one active role.
func (metric DashboardMetric) VisibleForRoles(activeRoles ...string) bool {
	return !metric.Display.Hidden && metric.Visibility.VisibleForRoles(activeRoles...)
}

// WithLabel returns a copy with display label metadata.
func (card DashboardCard) WithLabel(label string) DashboardCard {
	card.Label = label
	return card
}

// WithDescription returns a copy with display description metadata.
func (card DashboardCard) WithDescription(description string) DashboardCard {
	card.Description = description
	return card
}

// WithGroup returns a copy assigned to a dashboard display group.
func (card DashboardCard) WithGroup(group string) DashboardCard {
	card.Display.Group = group
	return card
}

// WithOrder returns a copy with dashboard ordering metadata inside its group.
func (card DashboardCard) WithOrder(order int) DashboardCard {
	card.Display.Order = order
	return card
}

// WithMetricRef returns a copy pointing at a dashboard metric.
func (card DashboardCard) WithMetricRef(ref string) DashboardCard {
	card.MetricRef = DashboardMetricRef(ref)
	return card
}

// WithRoles returns a copy restricted to the supplied roles.
func (card DashboardCard) WithRoles(roles ...string) DashboardCard {
	card.Visibility.Roles = append([]string(nil), roles...)
	return card
}

// AsHidden returns a copy hidden from generated default dashboard surfaces.
func (card DashboardCard) AsHidden() DashboardCard {
	card.Visibility.Hidden = true
	return card
}

// VisibleForRoles reports whether this card should be visible for at least one
// active role.
func (card DashboardCard) VisibleForRoles(activeRoles ...string) bool {
	return !card.Display.Hidden && card.Visibility.VisibleForRoles(activeRoles...)
}

// WithLabel returns a copy with display label metadata.
func (table DashboardTable) WithLabel(label string) DashboardTable {
	table.Label = label
	return table
}

// WithDescription returns a copy with display description metadata.
func (table DashboardTable) WithDescription(description string) DashboardTable {
	table.Description = description
	return table
}

// WithGroup returns a copy assigned to a dashboard display group.
func (table DashboardTable) WithGroup(group string) DashboardTable {
	table.Display.Group = group
	return table
}

// WithOrder returns a copy with dashboard ordering metadata inside its group.
func (table DashboardTable) WithOrder(order int) DashboardTable {
	table.Display.Order = order
	return table
}

// WithSourceRef returns a copy with a generated table row binding reference.
func (table DashboardTable) WithSourceRef(ref string) DashboardTable {
	table.SourceRef = DashboardDataRef(ref)
	return table
}

// WithRoles returns a copy restricted to the supplied roles.
func (table DashboardTable) WithRoles(roles ...string) DashboardTable {
	table.Visibility.Roles = append([]string(nil), roles...)
	return table
}

// AsHidden returns a copy hidden from generated default dashboard surfaces.
func (table DashboardTable) AsHidden() DashboardTable {
	table.Visibility.Hidden = true
	return table
}

// VisibleForRoles reports whether this table should be visible for at least one
// active role.
func (table DashboardTable) VisibleForRoles(activeRoles ...string) bool {
	return !table.Display.Hidden && table.Visibility.VisibleForRoles(activeRoles...)
}

// WithLabel returns a copy with display label metadata.
func (column DashboardTableColumn) WithLabel(label string) DashboardTableColumn {
	column.Label = label
	return column
}

// WithDescription returns a copy with display description metadata.
func (column DashboardTableColumn) WithDescription(description string) DashboardTableColumn {
	column.Description = description
	return column
}

// WithFormat returns a copy with value format metadata.
func (column DashboardTableColumn) WithFormat(format MetricFormat) DashboardTableColumn {
	column.Format = format
	return column
}

// WithWidth returns a copy with column sizing metadata.
func (column DashboardTableColumn) WithWidth(width string) DashboardTableColumn {
	column.Display.Width = width
	return column
}

// WithRoles returns a copy restricted to the supplied roles.
func (column DashboardTableColumn) WithRoles(roles ...string) DashboardTableColumn {
	column.Visibility.Roles = append([]string(nil), roles...)
	return column
}

// AsHidden returns a copy hidden from generated default dashboard surfaces.
func (column DashboardTableColumn) AsHidden() DashboardTableColumn {
	column.Visibility.Hidden = true
	return column
}

// VisibleForRoles reports whether this column should be visible for at least
// one active role.
func (column DashboardTableColumn) VisibleForRoles(activeRoles ...string) bool {
	return !column.Display.Hidden && column.Visibility.VisibleForRoles(activeRoles...)
}

// ValidateDashboardMetrics checks dashboard metric metadata without mutating the
// input slice.
func ValidateDashboardMetrics(metrics []DashboardMetric) error {
	_, err := normalizeDashboardMetrics(metrics)
	return err
}

// ValidateDashboardCards checks dashboard card metadata without mutating the
// input slice.
func ValidateDashboardCards(cards []DashboardCard) error {
	_, err := normalizeDashboardCards(cards)
	return err
}

// ValidateDashboardTables checks dashboard table metadata without mutating the
// input slice.
func ValidateDashboardTables(tables []DashboardTable) error {
	_, err := normalizeDashboardTables(tables)
	return err
}

// SortedDashboardMetrics returns a validated, normalized, deterministically
// sorted copy.
func SortedDashboardMetrics(metrics []DashboardMetric) ([]DashboardMetric, error) {
	normalized, err := normalizeDashboardMetrics(metrics)
	if err != nil {
		return nil, err
	}
	sortDashboardMetrics(normalized)
	return normalized, nil
}

// SortedDashboardCards returns a validated, normalized, deterministically
// sorted copy.
func SortedDashboardCards(cards []DashboardCard) ([]DashboardCard, error) {
	normalized, err := normalizeDashboardCards(cards)
	if err != nil {
		return nil, err
	}
	sortDashboardCards(normalized)
	return normalized, nil
}

// SortedDashboardTables returns a validated, normalized, deterministically
// sorted copy. Columns on each table are also sorted deterministically.
func SortedDashboardTables(tables []DashboardTable) ([]DashboardTable, error) {
	normalized, err := normalizeDashboardTables(tables)
	if err != nil {
		return nil, err
	}
	sortDashboardTables(normalized)
	return normalized, nil
}

// GroupDashboardMetrics returns validated dashboard metrics grouped by
// DashboardMetric.Display.Group.
//
// Empty groups are labeled DefaultDashboardGroup. Groups and metrics inside
// each group are sorted deterministically.
func GroupDashboardMetrics(metrics []DashboardMetric) ([]DashboardMetricGroup, error) {
	normalized, err := SortedDashboardMetrics(metrics)
	if err != nil {
		return nil, err
	}
	return groupDashboardMetrics(normalized), nil
}

// GroupDashboardCards returns validated dashboard cards grouped by
// DashboardCard.Display.Group.
//
// Empty groups are labeled DefaultDashboardGroup. Groups and cards inside each
// group are sorted deterministically.
func GroupDashboardCards(cards []DashboardCard) ([]DashboardCardGroup, error) {
	normalized, err := SortedDashboardCards(cards)
	if err != nil {
		return nil, err
	}
	return groupDashboardCards(normalized), nil
}

// GroupDashboardTables returns validated dashboard tables grouped by
// DashboardTable.Display.Group.
//
// Empty groups are labeled DefaultDashboardGroup. Groups and tables inside each
// group are sorted deterministically.
func GroupDashboardTables(tables []DashboardTable) ([]DashboardTableGroup, error) {
	normalized, err := SortedDashboardTables(tables)
	if err != nil {
		return nil, err
	}
	return groupDashboardTables(normalized), nil
}

func normalizeDashboardMetrics(metrics []DashboardMetric) ([]DashboardMetric, error) {
	normalized := make([]DashboardMetric, 0, len(metrics))
	seen := make(map[string]int, len(metrics))

	var errs []error
	for i, metric := range metrics {
		clean, err := normalizeDashboardMetric(metric, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		key := metadataKey(clean.Name)
		if first, ok := seen[key]; ok {
			errs = append(errs, fmt.Errorf("%w: metric[%d] %q also appears at metric[%d]", ErrDuplicateDashboardMetric, i, clean.Name, first))
			continue
		}
		seen[key] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	return normalized, nil
}

func normalizeDashboardMetric(metric DashboardMetric, index int) (DashboardMetric, error) {
	clean := DashboardMetric{
		Name:        strings.TrimSpace(metric.Name),
		Label:       strings.TrimSpace(metric.Label),
		Description: strings.TrimSpace(metric.Description),
		Format:      metric.Format,
		Unit:        strings.TrimSpace(metric.Unit),
		ValueRef:    DashboardDataRef(strings.TrimSpace(metric.ValueRef.String())),
		Display:     normalizeDisplayHint(metric.Display),
	}
	if clean.Label == "" {
		clean.Label = clean.Name
	}
	if clean.Format == "" {
		clean.Format = MetricFormatNumber
	}

	var errs []error
	if clean.Name == "" {
		errs = append(errs, invalidDashboardMetricField(index, "name", "is required"))
	} else if hasControl(clean.Name) {
		errs = append(errs, invalidDashboardMetricField(index, "name", "contains control characters"))
	}
	if hasControl(clean.Label) {
		errs = append(errs, invalidDashboardMetricField(index, "label", "contains control characters"))
	}
	if hasControl(clean.Description) {
		errs = append(errs, invalidDashboardMetricField(index, "description", "contains control characters"))
	}
	if hasControl(clean.Unit) {
		errs = append(errs, invalidDashboardMetricField(index, "unit", "contains control characters"))
	}
	if err := validateMetricFormat(clean.Format); err != nil {
		errs = append(errs, invalidDashboardMetricField(index, "format", err.Error()))
	}
	if err := validateDashboardRef(clean.ValueRef.String()); err != nil {
		errs = append(errs, invalidDashboardMetricField(index, "value_ref", err.Error()))
	}
	if err := validateDisplayHint(clean.Display); err != nil {
		errs = append(errs, invalidDashboardMetricField(index, "display", err.Error()))
	}

	visibility, err := normalizeDashboardVisibility(metric.Visibility, func(field, reason string) error {
		return invalidDashboardMetricField(index, "visibility."+field, reason)
	})
	if err != nil {
		errs = append(errs, err)
	}
	clean.Visibility = visibility

	if err := errors.Join(errs...); err != nil {
		return DashboardMetric{}, err
	}
	return clean, nil
}

func normalizeDashboardCards(cards []DashboardCard) ([]DashboardCard, error) {
	normalized := make([]DashboardCard, 0, len(cards))
	seen := make(map[string]int, len(cards))

	var errs []error
	for i, card := range cards {
		clean, err := normalizeDashboardCard(card, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		key := metadataKey(clean.Name)
		if first, ok := seen[key]; ok {
			errs = append(errs, fmt.Errorf("%w: card[%d] %q also appears at card[%d]", ErrDuplicateDashboardCard, i, clean.Name, first))
			continue
		}
		seen[key] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	return normalized, nil
}

func normalizeDashboardCard(card DashboardCard, index int) (DashboardCard, error) {
	clean := DashboardCard{
		Name:        strings.TrimSpace(card.Name),
		Label:       strings.TrimSpace(card.Label),
		Description: strings.TrimSpace(card.Description),
		MetricRef:   DashboardMetricRef(strings.TrimSpace(card.MetricRef.String())),
		Display:     normalizeDisplayHint(card.Display),
	}
	if clean.Label == "" {
		clean.Label = clean.Name
	}

	var errs []error
	if clean.Name == "" {
		errs = append(errs, invalidDashboardCardField(index, "name", "is required"))
	} else if hasControl(clean.Name) {
		errs = append(errs, invalidDashboardCardField(index, "name", "contains control characters"))
	}
	if hasControl(clean.Label) {
		errs = append(errs, invalidDashboardCardField(index, "label", "contains control characters"))
	}
	if hasControl(clean.Description) {
		errs = append(errs, invalidDashboardCardField(index, "description", "contains control characters"))
	}
	if clean.MetricRef == "" {
		errs = append(errs, invalidDashboardCardField(index, "metric_ref", "is required"))
	} else if err := validateDashboardRef(clean.MetricRef.String()); err != nil {
		errs = append(errs, invalidDashboardCardField(index, "metric_ref", err.Error()))
	}
	if err := validateDisplayHint(clean.Display); err != nil {
		errs = append(errs, invalidDashboardCardField(index, "display", err.Error()))
	}

	visibility, err := normalizeDashboardVisibility(card.Visibility, func(field, reason string) error {
		return invalidDashboardCardField(index, "visibility."+field, reason)
	})
	if err != nil {
		errs = append(errs, err)
	}
	clean.Visibility = visibility

	if err := errors.Join(errs...); err != nil {
		return DashboardCard{}, err
	}
	return clean, nil
}

func normalizeDashboardTables(tables []DashboardTable) ([]DashboardTable, error) {
	normalized := make([]DashboardTable, 0, len(tables))
	seen := make(map[string]int, len(tables))

	var errs []error
	for i, table := range tables {
		clean, err := normalizeDashboardTable(table, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		key := metadataKey(clean.Name)
		if first, ok := seen[key]; ok {
			errs = append(errs, fmt.Errorf("%w: table[%d] %q also appears at table[%d]", ErrDuplicateDashboardTable, i, clean.Name, first))
			continue
		}
		seen[key] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	return normalized, nil
}

func normalizeDashboardTable(table DashboardTable, index int) (DashboardTable, error) {
	clean := DashboardTable{
		Name:        strings.TrimSpace(table.Name),
		Label:       strings.TrimSpace(table.Label),
		Description: strings.TrimSpace(table.Description),
		SourceRef:   DashboardDataRef(strings.TrimSpace(table.SourceRef.String())),
		Display:     normalizeDisplayHint(table.Display),
	}
	if clean.Label == "" {
		clean.Label = clean.Name
	}

	var errs []error
	if clean.Name == "" {
		errs = append(errs, invalidDashboardTableField(index, "name", "is required"))
	} else if hasControl(clean.Name) {
		errs = append(errs, invalidDashboardTableField(index, "name", "contains control characters"))
	}
	if hasControl(clean.Label) {
		errs = append(errs, invalidDashboardTableField(index, "label", "contains control characters"))
	}
	if hasControl(clean.Description) {
		errs = append(errs, invalidDashboardTableField(index, "description", "contains control characters"))
	}
	if err := validateDashboardRef(clean.SourceRef.String()); err != nil {
		errs = append(errs, invalidDashboardTableField(index, "source_ref", err.Error()))
	}
	if err := validateDisplayHint(clean.Display); err != nil {
		errs = append(errs, invalidDashboardTableField(index, "display", err.Error()))
	}

	columns, err := normalizeDashboardTableColumns(table.Columns, index)
	if err != nil {
		errs = append(errs, err)
	}
	clean.Columns = columns
	if len(table.Columns) == 0 {
		errs = append(errs, invalidDashboardTableField(index, "columns", "must contain at least one column"))
	}

	visibility, err := normalizeDashboardVisibility(table.Visibility, func(field, reason string) error {
		return invalidDashboardTableField(index, "visibility."+field, reason)
	})
	if err != nil {
		errs = append(errs, err)
	}
	clean.Visibility = visibility

	if err := errors.Join(errs...); err != nil {
		return DashboardTable{}, err
	}
	return clean, nil
}

func normalizeDashboardTableColumns(columns []DashboardTableColumn, tableIndex int) ([]DashboardTableColumn, error) {
	normalized := make([]DashboardTableColumn, 0, len(columns))
	seen := make(map[string]int, len(columns))

	var errs []error
	for i, column := range columns {
		clean, err := normalizeDashboardTableColumn(column, tableIndex, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		key := metadataKey(clean.Name)
		if first, ok := seen[key]; ok {
			errs = append(errs, fmt.Errorf("%w: %s %q also appears at %s", ErrDuplicateDashboardTableColumn, dashboardTableColumnPath(tableIndex, i), clean.Name, dashboardTableColumnPath(tableIndex, first)))
			continue
		}
		seen[key] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	return normalized, nil
}

func normalizeDashboardTableColumn(column DashboardTableColumn, tableIndex, columnIndex int) (DashboardTableColumn, error) {
	clean := DashboardTableColumn{
		Name:        strings.TrimSpace(column.Name),
		Label:       strings.TrimSpace(column.Label),
		Type:        strings.TrimSpace(column.Type),
		Description: strings.TrimSpace(column.Description),
		Format:      column.Format,
		Display:     normalizeDisplayHint(column.Display),
	}
	if clean.Label == "" {
		clean.Label = clean.Name
	}

	var errs []error
	if clean.Name == "" {
		errs = append(errs, invalidDashboardTableColumnField(tableIndex, columnIndex, "name", "is required"))
	} else if hasControl(clean.Name) {
		errs = append(errs, invalidDashboardTableColumnField(tableIndex, columnIndex, "name", "contains control characters"))
	}
	if clean.Type == "" {
		errs = append(errs, invalidDashboardTableColumnField(tableIndex, columnIndex, "type", "is required"))
	} else if hasControl(clean.Type) {
		errs = append(errs, invalidDashboardTableColumnField(tableIndex, columnIndex, "type", "contains control characters"))
	}
	if hasControl(clean.Label) {
		errs = append(errs, invalidDashboardTableColumnField(tableIndex, columnIndex, "label", "contains control characters"))
	}
	if hasControl(clean.Description) {
		errs = append(errs, invalidDashboardTableColumnField(tableIndex, columnIndex, "description", "contains control characters"))
	}
	if err := validateMetricFormat(clean.Format); err != nil {
		errs = append(errs, invalidDashboardTableColumnField(tableIndex, columnIndex, "format", err.Error()))
	}
	if err := validateDisplayHint(clean.Display); err != nil {
		errs = append(errs, invalidDashboardTableColumnField(tableIndex, columnIndex, "display", err.Error()))
	}

	visibility, err := normalizeDashboardVisibility(column.Visibility, func(field, reason string) error {
		return invalidDashboardTableColumnField(tableIndex, columnIndex, "visibility."+field, reason)
	})
	if err != nil {
		errs = append(errs, err)
	}
	clean.Visibility = visibility

	if err := errors.Join(errs...); err != nil {
		return DashboardTableColumn{}, err
	}
	return clean, nil
}

func normalizeDashboardVisibility(visibility DashboardVisibility, invalid func(field, reason string) error) (DashboardVisibility, error) {
	clean := DashboardVisibility{Hidden: visibility.Hidden}
	seen := make(map[string]int, len(visibility.Roles))

	var errs []error
	for i, role := range visibility.Roles {
		role = strings.TrimSpace(role)
		roleField := fmt.Sprintf("roles[%d]", i)
		if role == "" {
			errs = append(errs, invalid(roleField, "is required"))
			continue
		}
		if hasControl(role) {
			errs = append(errs, invalid(roleField, "contains control characters"))
			continue
		}

		key := metadataKey(role)
		if first, ok := seen[key]; ok {
			errs = append(errs, invalid(roleField, fmt.Sprintf("%q also appears at roles[%d]", role, first)))
			continue
		}
		seen[key] = i
		clean.Roles = append(clean.Roles, role)
	}
	sortStringsStable(clean.Roles)

	if err := errors.Join(errs...); err != nil {
		return DashboardVisibility{}, err
	}
	return clean, nil
}

func validateMetricFormat(format MetricFormat) error {
	switch format {
	case "", MetricFormatNumber, MetricFormatCurrency, MetricFormatPercent, MetricFormatDuration, MetricFormatBytes, MetricFormatText:
		return nil
	default:
		return fmt.Errorf("must be a known metric format, got %q", format)
	}
}

func validateDashboardRef(ref string) error {
	if ref == "" {
		return nil
	}

	var errs []error
	if hasControl(ref) {
		errs = append(errs, errors.New("contains control characters"))
	}
	if strings.ContainsFunc(ref, unicode.IsSpace) {
		errs = append(errs, errors.New("contains whitespace"))
	}
	return errors.Join(errs...)
}

func sortDashboardMetrics(metrics []DashboardMetric) {
	sort.SliceStable(metrics, func(i, j int) bool {
		return compareDashboardMetric(metrics[i], metrics[j]) < 0
	})
}

func sortDashboardCards(cards []DashboardCard) {
	sort.SliceStable(cards, func(i, j int) bool {
		return compareDashboardCard(cards[i], cards[j]) < 0
	})
}

func sortDashboardTables(tables []DashboardTable) {
	for i := range tables {
		sortDashboardTableColumns(tables[i].Columns)
	}
	sort.SliceStable(tables, func(i, j int) bool {
		return compareDashboardTable(tables[i], tables[j]) < 0
	})
}

func sortDashboardTableColumns(columns []DashboardTableColumn) {
	sort.SliceStable(columns, func(i, j int) bool {
		return compareDashboardTableColumn(columns[i], columns[j]) < 0
	})
}

func compareDashboardMetric(left, right DashboardMetric) int {
	return compareDashboardItem(left.Display, left.Label, left.Name, right.Display, right.Label, right.Name)
}

func compareDashboardCard(left, right DashboardCard) int {
	return compareDashboardItem(left.Display, left.Label, left.Name, right.Display, right.Label, right.Name)
}

func compareDashboardTable(left, right DashboardTable) int {
	return compareDashboardItem(left.Display, left.Label, left.Name, right.Display, right.Label, right.Name)
}

func compareDashboardTableColumn(left, right DashboardTableColumn) int {
	return compareDashboardItem(left.Display, left.Label, left.Name, right.Display, right.Label, right.Name)
}

func compareDashboardItem(leftDisplay DisplayHint, leftLabel, leftName string, rightDisplay DisplayHint, rightLabel, rightName string) int {
	for _, cmp := range []int{
		compareFold(dashboardGroupLabel(leftDisplay.Group), dashboardGroupLabel(rightDisplay.Group)),
		compareInt(leftDisplay.Order, rightDisplay.Order),
		compareDisplayName(leftLabel, leftName, rightLabel, rightName),
		compareFold(leftName, rightName),
	} {
		if cmp != 0 {
			return cmp
		}
	}
	return 0
}

func groupDashboardMetrics(metrics []DashboardMetric) []DashboardMetricGroup {
	byLabel := make(map[string][]DashboardMetric)
	for _, metric := range metrics {
		label := dashboardGroupLabel(metric.Display.Group)
		byLabel[label] = append(byLabel[label], metric)
	}

	labels := sortedLabels(byLabel)
	groups := make([]DashboardMetricGroup, 0, len(labels))
	for _, label := range labels {
		metrics := append([]DashboardMetric(nil), byLabel[label]...)
		sortDashboardMetrics(metrics)
		groups = append(groups, DashboardMetricGroup{Label: label, Metrics: metrics})
	}
	return groups
}

func groupDashboardCards(cards []DashboardCard) []DashboardCardGroup {
	byLabel := make(map[string][]DashboardCard)
	for _, card := range cards {
		label := dashboardGroupLabel(card.Display.Group)
		byLabel[label] = append(byLabel[label], card)
	}

	labels := sortedLabels(byLabel)
	groups := make([]DashboardCardGroup, 0, len(labels))
	for _, label := range labels {
		cards := append([]DashboardCard(nil), byLabel[label]...)
		sortDashboardCards(cards)
		groups = append(groups, DashboardCardGroup{Label: label, Cards: cards})
	}
	return groups
}

func groupDashboardTables(tables []DashboardTable) []DashboardTableGroup {
	byLabel := make(map[string][]DashboardTable)
	for _, table := range tables {
		label := dashboardGroupLabel(table.Display.Group)
		byLabel[label] = append(byLabel[label], table)
	}

	labels := sortedLabels(byLabel)
	groups := make([]DashboardTableGroup, 0, len(labels))
	for _, label := range labels {
		tables := append([]DashboardTable(nil), byLabel[label]...)
		sortDashboardTables(tables)
		groups = append(groups, DashboardTableGroup{Label: label, Tables: tables})
	}
	return groups
}

func dashboardGroupLabel(group string) string {
	if group == "" {
		return DefaultDashboardGroup
	}
	return group
}

func cleanDashboardRoleNames(names []string) []string {
	seen := map[string]struct{}{}
	out := make([]string, 0, len(names))
	for _, name := range names {
		name = strings.TrimSpace(name)
		if name == "" {
			continue
		}
		key := metadataKey(name)
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		out = append(out, name)
	}
	sortStringsStable(out)
	return out
}

func dashboardRoleSet(names []string) map[string]struct{} {
	out := make(map[string]struct{}, len(names))
	for _, name := range names {
		name = strings.TrimSpace(name)
		if name == "" {
			continue
		}
		out[metadataKey(name)] = struct{}{}
	}
	return out
}

func invalidDashboardMetricField(index int, field, reason string) error {
	return fmt.Errorf("%w: metric[%d].%s %s", ErrInvalidDashboardMetric, index, field, reason)
}

func invalidDashboardCardField(index int, field, reason string) error {
	return fmt.Errorf("%w: card[%d].%s %s", ErrInvalidDashboardCard, index, field, reason)
}

func invalidDashboardTableField(index int, field, reason string) error {
	return fmt.Errorf("%w: table[%d].%s %s", ErrInvalidDashboardTable, index, field, reason)
}

func invalidDashboardTableColumnField(tableIndex, columnIndex int, field, reason string) error {
	return fmt.Errorf("%w: %s.%s %s", ErrInvalidDashboardTableColumn, dashboardTableColumnPath(tableIndex, columnIndex), field, reason)
}

func dashboardTableColumnPath(tableIndex, columnIndex int) string {
	return fmt.Sprintf("table[%d].columns[%d]", tableIndex, columnIndex)
}
