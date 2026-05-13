package admin

import (
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestDashboardMetricCardTableHelpersAndVisibility(t *testing.T) {
	metric := CurrencyMetric("revenue").
		WithLabel("Revenue").
		WithDescription("Monthly recurring revenue").
		WithGroup("Finance").
		WithOrder(2).
		WithUnit("USD").
		WithValueRef("metrics.revenue").
		WithRoles("finance", "admin")

	if metric.Format != MetricFormatCurrency || metric.ValueRef.String() != "metrics.revenue" {
		t.Fatalf("CurrencyMetric helper = %#v", metric)
	}
	if !metric.VisibleForRoles("admin") {
		t.Fatal("metric should be visible for admin")
	}
	if metric.VisibleForRoles("support") {
		t.Fatal("metric should not be visible for support")
	}
	if metric.AsHidden().VisibleForRoles("admin") {
		t.Fatal("hidden metric should not be visible")
	}

	card := MetricCard("revenue_card", "revenue").
		WithLabel("Revenue").
		WithMetricRef("monthly_revenue").
		WithRoles("finance")
	if card.MetricRef.String() != "monthly_revenue" || !card.VisibleForRoles("finance") {
		t.Fatalf("MetricCard helper = %#v", card)
	}

	column := TableColumn("total", "decimal").
		WithLabel("Total").
		WithDescription("Order total").
		WithFormat(MetricFormatCurrency).
		WithWidth("8rem").
		WithRoles("finance")
	table := DataTable("recent_orders", column).
		WithSourceRef("tables.recent_orders").
		WithGroup("Orders").
		WithOrder(1)

	column.Name = "changed"
	if table.SourceRef.String() != "tables.recent_orders" || table.Columns[0].Name != "total" {
		t.Fatalf("DataTable helper did not copy columns or source ref: %#v", table)
	}
	if !table.Columns[0].VisibleForRoles("finance") || table.Columns[0].VisibleForRoles("support") {
		t.Fatalf("column visibility mismatch: %#v", table.Columns[0].Visibility)
	}
}

func TestSortedDashboardDescriptorsNormalizeSortAndDoNotMutate(t *testing.T) {
	metrics := []DashboardMetric{
		CurrencyMetric(" revenue ").
			WithLabel(" Revenue ").
			WithGroup(" Business ").
			WithOrder(2).
			WithUnit(" USD ").
			WithValueRef(" metrics.revenue ").
			WithRoles(" ops ", " admin "),
		NumberMetric("tickets").WithOrder(1),
		{Name: "conversion", Label: "Conversion", Display: DisplayHint{Group: "Business", Order: 1}},
	}

	gotMetrics, err := SortedDashboardMetrics(metrics)
	if err != nil {
		t.Fatalf("SortedDashboardMetrics() error = %v", err)
	}
	if gotNames := dashboardMetricNames(gotMetrics); !reflect.DeepEqual(gotNames, []string{"conversion", "revenue", "tickets"}) {
		t.Fatalf("SortedDashboardMetrics() names = %#v", gotNames)
	}
	if gotMetrics[1].Label != "Revenue" || gotMetrics[1].Unit != "USD" || gotMetrics[1].ValueRef != "metrics.revenue" {
		t.Fatalf("metric was not trimmed and normalized: %#v", gotMetrics[1])
	}
	if gotMetrics[0].Format != MetricFormatNumber {
		t.Fatalf("default metric format = %q, want %q", gotMetrics[0].Format, MetricFormatNumber)
	}
	if !reflect.DeepEqual(gotMetrics[1].Visibility.Roles, []string{"admin", "ops"}) {
		t.Fatalf("metric roles = %#v", gotMetrics[1].Visibility.Roles)
	}
	if metrics[0].Name != " revenue " || metrics[0].Visibility.Roles[0] != " ops " {
		t.Fatal("SortedDashboardMetrics() mutated input metric")
	}

	cards := []DashboardCard{
		MetricCard(" revenue_card ", " revenue ").WithLabel(" Revenue ").WithOrder(2),
		MetricCard("tickets_card", "tickets").WithOrder(1),
	}
	gotCards, err := SortedDashboardCards(cards)
	if err != nil {
		t.Fatalf("SortedDashboardCards() error = %v", err)
	}
	if gotNames := dashboardCardNames(gotCards); !reflect.DeepEqual(gotNames, []string{"tickets_card", "revenue_card"}) {
		t.Fatalf("SortedDashboardCards() names = %#v", gotNames)
	}
	if gotCards[1].Name != "revenue_card" || gotCards[1].MetricRef != "revenue" {
		t.Fatalf("card was not trimmed and normalized: %#v", gotCards[1])
	}
	if cards[0].MetricRef != " revenue " {
		t.Fatal("SortedDashboardCards() mutated input card")
	}

	tables := []DashboardTable{
		DataTable(" recent_orders ",
			TableColumn("total", "decimal").WithFormat(MetricFormatCurrency).WithWidth(" 8rem "),
			TableColumn("created_at", "datetime").WithLabel("Created"),
		).WithLabel(" Recent Orders ").WithSourceRef(" tables.recent_orders "),
	}
	gotTables, err := SortedDashboardTables(tables)
	if err != nil {
		t.Fatalf("SortedDashboardTables() error = %v", err)
	}
	if gotTables[0].Name != "recent_orders" || gotTables[0].SourceRef != "tables.recent_orders" {
		t.Fatalf("table was not trimmed and normalized: %#v", gotTables[0])
	}
	if gotNames := dashboardTableColumnNames(gotTables[0].Columns); !reflect.DeepEqual(gotNames, []string{"created_at", "total"}) {
		t.Fatalf("SortedDashboardTables() columns = %#v", gotNames)
	}
	if gotTables[0].Columns[1].Display.Width != "8rem" {
		t.Fatalf("column width was not trimmed: %#v", gotTables[0].Columns[1])
	}
	if tables[0].Name != " recent_orders " || tables[0].Columns[0].Display.Width != " 8rem " {
		t.Fatal("SortedDashboardTables() mutated input table")
	}
}

func TestGroupDashboardDescriptorsAreDeterministic(t *testing.T) {
	metricGroups, err := GroupDashboardMetrics([]DashboardMetric{
		NumberMetric("signups").WithGroup("Growth").WithOrder(2),
		NumberMetric("revenue"),
		NumberMetric("conversion").WithGroup("Growth").WithOrder(1),
	})
	if err != nil {
		t.Fatalf("GroupDashboardMetrics() error = %v", err)
	}
	if got := dashboardMetricGroupNames(metricGroups); !reflect.DeepEqual(got, [][]string{{DefaultDashboardGroup, "revenue"}, {"Growth", "conversion", "signups"}}) {
		t.Fatalf("GroupDashboardMetrics() groups = %#v", got)
	}

	cardGroups, err := GroupDashboardCards([]DashboardCard{
		MetricCard("latency", "latency").WithGroup("Operations").WithOrder(2),
		MetricCard("revenue", "revenue"),
		MetricCard("errors", "errors").WithGroup("Operations").WithOrder(1),
	})
	if err != nil {
		t.Fatalf("GroupDashboardCards() error = %v", err)
	}
	if got := dashboardCardGroupNames(cardGroups); !reflect.DeepEqual(got, [][]string{{DefaultDashboardGroup, "revenue"}, {"Operations", "errors", "latency"}}) {
		t.Fatalf("GroupDashboardCards() groups = %#v", got)
	}

	tableGroups, err := GroupDashboardTables([]DashboardTable{
		DataTable("failed_jobs", TableColumn("id", "id")).WithGroup("Operations").WithOrder(2),
		DataTable("recent_orders", TableColumn("id", "id")),
		DataTable("slow_jobs", TableColumn("id", "id")).WithGroup("Operations").WithOrder(1),
	})
	if err != nil {
		t.Fatalf("GroupDashboardTables() error = %v", err)
	}
	if got := dashboardTableGroupNames(tableGroups); !reflect.DeepEqual(got, [][]string{{DefaultDashboardGroup, "recent_orders"}, {"Operations", "slow_jobs", "failed_jobs"}}) {
		t.Fatalf("GroupDashboardTables() groups = %#v", got)
	}
}

func TestValidateDashboardDescriptorsRejectInvalidAndDuplicateMetadata(t *testing.T) {
	metricErr := ValidateDashboardMetrics([]DashboardMetric{
		NumberMetric("revenue"),
		PercentMetric("Revenue"),
		{
			Name:       "bad_format",
			Format:     "ratio",
			ValueRef:   "metrics bad",
			Display:    DisplayHint{Order: -1},
			Visibility: DashboardVisibility{Roles: []string{"ops", "OPS", " "}},
		},
		{Name: "bad\nname"},
	})
	for _, wantErr := range []error{ErrDuplicateDashboardMetric, ErrInvalidDashboardMetric} {
		if !errors.Is(metricErr, wantErr) {
			t.Fatalf("ValidateDashboardMetrics() error = %v, want %v", metricErr, wantErr)
		}
	}
	for _, want := range []string{
		"metric[1] \"Revenue\" also appears at metric[0]",
		"metric[2].format",
		"metric[2].value_ref contains whitespace",
		"metric[2].display",
		"metric[2].visibility.roles[1]",
		"metric[2].visibility.roles[2]",
		"metric[3].name",
	} {
		if !strings.Contains(metricErr.Error(), want) {
			t.Fatalf("ValidateDashboardMetrics() error = %q, want substring %q", metricErr.Error(), want)
		}
	}

	cardErr := ValidateDashboardCards([]DashboardCard{
		MetricCard("revenue_card", "revenue"),
		MetricCard("Revenue_Card", "revenue"),
		{Name: "blank_metric", MetricRef: " "},
		MetricCard("bad_ref", "revenue total").WithLabel("Bad\nRef"),
	})
	for _, wantErr := range []error{ErrDuplicateDashboardCard, ErrInvalidDashboardCard} {
		if !errors.Is(cardErr, wantErr) {
			t.Fatalf("ValidateDashboardCards() error = %v, want %v", cardErr, wantErr)
		}
	}
	for _, want := range []string{
		"card[1] \"Revenue_Card\" also appears at card[0]",
		"card[2].metric_ref",
		"card[3].label",
		"card[3].metric_ref contains whitespace",
	} {
		if !strings.Contains(cardErr.Error(), want) {
			t.Fatalf("ValidateDashboardCards() error = %q, want substring %q", cardErr.Error(), want)
		}
	}

	tableErr := ValidateDashboardTables([]DashboardTable{
		DataTable("recent", TableColumn("id", "id")),
		DataTable("Recent", TableColumn("id", "id")),
		DataTable("empty"),
		DataTable("bad_columns",
			TableColumn("id", "id"),
			TableColumn("ID", "id"),
			TableColumn("empty_type", "").WithFormat("custom").WithWidth("bad\nwidth"),
		).WithSourceRef("tables recent"),
	})
	for _, wantErr := range []error{
		ErrDuplicateDashboardTable,
		ErrInvalidDashboardTable,
		ErrDuplicateDashboardTableColumn,
		ErrInvalidDashboardTableColumn,
	} {
		if !errors.Is(tableErr, wantErr) {
			t.Fatalf("ValidateDashboardTables() error = %v, want %v", tableErr, wantErr)
		}
	}
	for _, want := range []string{
		"table[1] \"Recent\" also appears at table[0]",
		"table[2].columns",
		"table[3].source_ref contains whitespace",
		"table[3].columns[1] \"ID\" also appears at table[3].columns[0]",
		"table[3].columns[2].type",
		"table[3].columns[2].format",
		"table[3].columns[2].display",
	} {
		if !strings.Contains(tableErr.Error(), want) {
			t.Fatalf("ValidateDashboardTables() error = %q, want substring %q", tableErr.Error(), want)
		}
	}
}

func dashboardMetricNames(metrics []DashboardMetric) []string {
	names := make([]string, 0, len(metrics))
	for _, metric := range metrics {
		names = append(names, metric.Name)
	}
	return names
}

func dashboardCardNames(cards []DashboardCard) []string {
	names := make([]string, 0, len(cards))
	for _, card := range cards {
		names = append(names, card.Name)
	}
	return names
}

func dashboardTableNames(tables []DashboardTable) []string {
	names := make([]string, 0, len(tables))
	for _, table := range tables {
		names = append(names, table.Name)
	}
	return names
}

func dashboardTableColumnNames(columns []DashboardTableColumn) []string {
	names := make([]string, 0, len(columns))
	for _, column := range columns {
		names = append(names, column.Name)
	}
	return names
}

func dashboardMetricGroupNames(groups []DashboardMetricGroup) [][]string {
	out := make([][]string, 0, len(groups))
	for _, group := range groups {
		row := []string{group.Label}
		row = append(row, dashboardMetricNames(group.Metrics)...)
		out = append(out, row)
	}
	return out
}

func dashboardCardGroupNames(groups []DashboardCardGroup) [][]string {
	out := make([][]string, 0, len(groups))
	for _, group := range groups {
		row := []string{group.Label}
		row = append(row, dashboardCardNames(group.Cards)...)
		out = append(out, row)
	}
	return out
}

func dashboardTableGroupNames(groups []DashboardTableGroup) [][]string {
	out := make([][]string, 0, len(groups))
	for _, group := range groups {
		row := []string{group.Label}
		row = append(row, dashboardTableNames(group.Tables)...)
		out = append(out, row)
	}
	return out
}
