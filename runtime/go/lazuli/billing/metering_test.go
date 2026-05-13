package billing_test

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/billing"
)

func TestAggregateUsageDeduplicatesAndSortsKeys(t *testing.T) {
	t.Parallel()

	window := billing.MeterWindow{
		Start: time.Date(2026, 5, 1, 0, 0, 0, 0, time.UTC),
		End:   time.Date(2026, 6, 1, 0, 0, 0, 0, time.UTC),
	}
	events := []billing.UsageEvent{
		usageEvent("evt_b", "tenant-a", "customer-a", "api_calls", 7, window.Start.Add(2*time.Hour), window),
		usageEvent("evt_a", "tenant-a", "customer-a", " api_calls ", 3, window.Start.Add(time.Hour), window),
		usageEvent("evt_a", "tenant-a", "customer-a", "api_calls", 3, window.Start.Add(time.Hour), window),
	}
	events[0].Metadata = map[string]string{"region": "sa-east-1"}
	events[1].Metadata = map[string]string{"region": "sa-east-1", " ignored_empty ": "drop", "": "drop"}

	aggregate, err := billing.AggregateUsage(events, window)
	if err != nil {
		t.Fatalf("AggregateUsage() error = %v", err)
	}
	if aggregate.Tenant != "tenant-a" || aggregate.Customer != "customer-a" || aggregate.Feature != "api_calls" {
		t.Fatalf("aggregate dimensions = %#v, want normalized dimensions", aggregate)
	}
	if aggregate.Quantity != 10 || aggregate.EventCount != 2 || aggregate.DuplicateCount != 1 {
		t.Fatalf("aggregate counts = quantity %d events %d duplicates %d, want 10/2/1", aggregate.Quantity, aggregate.EventCount, aggregate.DuplicateCount)
	}
	if len(aggregate.DedupKeys) != 2 || aggregate.DedupKeys[0] >= aggregate.DedupKeys[1] {
		t.Fatalf("DedupKeys = %#v, want two sorted keys", aggregate.DedupKeys)
	}
	if aggregate.FirstOccurredAt != events[1].OccurredAt || aggregate.LastOccurredAt != events[0].OccurredAt {
		t.Fatalf("occurred range = %s..%s, want sorted min/max", aggregate.FirstOccurredAt, aggregate.LastOccurredAt)
	}
	if got := aggregate.Metadata["region"]; got != "sa-east-1" {
		t.Fatalf("Metadata[region] = %q, want sa-east-1", got)
	}
}

func TestUsageEventKeysAreStable(t *testing.T) {
	t.Parallel()

	window := billing.MeterWindow{
		Start: time.Date(2026, 5, 1, 0, 0, 0, 0, time.UTC),
		End:   time.Date(2026, 5, 2, 0, 0, 0, 0, time.UTC),
	}
	event := usageEvent(" evt_123 ", " tenant-a ", "customer-a", "seats", 1, window.Start.Add(time.Minute), window)

	if got, want := event.DedupKey(), "usage_dedup:8:tenant-a:7:evt_123"; got != want {
		t.Fatalf("DedupKey() = %q, want %q", got, want)
	}
	if got, want := event.IdempotencyKey(), "usage_apply:32:usage_dedup:8:tenant-a:7:evt_123"; got != want {
		t.Fatalf("IdempotencyKey() = %q, want %q", got, want)
	}

	withoutID := event
	withoutID.ID = ""
	again := withoutID
	if withoutID.DedupKey() != again.DedupKey() {
		t.Fatalf("derived DedupKey() changed for identical events: %q vs %q", withoutID.DedupKey(), again.DedupKey())
	}
}

func TestAggregateUsageRejectsInvalidEvents(t *testing.T) {
	t.Parallel()

	window := billing.MeterWindow{
		Start: time.Date(2026, 5, 1, 0, 0, 0, 0, time.UTC),
		End:   time.Date(2026, 5, 2, 0, 0, 0, 0, time.UTC),
	}

	_, err := billing.AggregateUsage([]billing.UsageEvent{
		usageEvent("evt_1", "tenant-a", "customer-a", "api_calls", 1, window.End, window),
	}, window)
	if !errors.Is(err, billing.ErrUsageEventInvalid) {
		t.Fatalf("AggregateUsage() boundary error = %v, want ErrUsageEventInvalid", err)
	}

	_, err = billing.AggregateUsage([]billing.UsageEvent{
		usageEvent("evt_1", "tenant-a", "customer-a", "api_calls", 1, window.Start, window),
		usageEvent("evt_2", "tenant-a", "customer-a", "seats", 1, window.Start.Add(time.Minute), window),
	}, window)
	if !errors.Is(err, billing.ErrUsageEventInvalid) {
		t.Fatalf("AggregateUsage() dimension error = %v, want ErrUsageEventInvalid", err)
	}
}

func TestCheckUsageEntitlementHelpersUsePlanLimits(t *testing.T) {
	t.Parallel()

	window := billing.MeterWindow{
		Start: time.Date(2026, 5, 1, 0, 0, 0, 0, time.UTC),
		End:   time.Date(2026, 6, 1, 0, 0, 0, 0, time.UTC),
	}
	plan := billing.Plan{
		Key:    "pro",
		Status: billing.PlanStatusActive,
		Entitlements: []billing.Entitlement{
			billing.LimitedEntitlement("api_calls", 100),
		},
	}
	event := usageEvent("evt_1", "tenant-a", "customer-a", "api_calls", 25, window.Start.Add(time.Hour), window)

	check, err := billing.CheckUsageEventEntitlement(plan, 75, event)
	if err != nil {
		t.Fatalf("CheckUsageEventEntitlement() error = %v", err)
	}
	if !check.Allowed || check.AfterUsage != 100 {
		t.Fatalf("event check = %#v, want allowed up to limit", check)
	}

	aggregate, err := billing.AggregateUsage([]billing.UsageEvent{event}, window)
	if err != nil {
		t.Fatalf("AggregateUsage() error = %v", err)
	}
	check, err = billing.CheckUsageAggregateEntitlement(plan, 80, aggregate)
	if err != nil {
		t.Fatalf("CheckUsageAggregateEntitlement() error = %v", err)
	}
	if check.Allowed || check.Reason != "limit exceeded" || check.AfterUsage != 105 {
		t.Fatalf("aggregate check = %#v, want blocked over limit", check)
	}
	if err := check.Validate(); !errors.Is(err, billing.ErrEntitlementLimitExceeded) {
		t.Fatalf("Validate() error = %v, want ErrEntitlementLimitExceeded", err)
	}
}

func TestPreviewUsageInvoiceLineBuildsMetadata(t *testing.T) {
	t.Parallel()

	window := billing.MeterWindow{
		Start: time.Date(2026, 5, 1, 0, 0, 0, 0, time.UTC),
		End:   time.Date(2026, 6, 1, 0, 0, 0, 0, time.UTC),
	}
	aggregate, err := billing.AggregateUsage([]billing.UsageEvent{
		usageEvent("evt_1", "tenant-a", "customer-a", "api_calls", 12, window.Start.Add(time.Hour), window),
	}, window)
	if err != nil {
		t.Fatalf("AggregateUsage() error = %v", err)
	}

	preview, err := billing.PreviewUsageInvoiceLine(aggregate, billing.RevenueAmount{Amount: 15, Currency: " brl "})
	if err != nil {
		t.Fatalf("PreviewUsageInvoiceLine() error = %v", err)
	}
	if preview.Quantity != 12 || preview.UnitAmount.Currency != "BRL" || preview.TotalAmount.Amount != 180 {
		t.Fatalf("preview = %#v, want 12 units at BRL 0.15", preview)
	}
	if preview.Metadata["usage.idempotency_key"] != aggregate.IdempotencyKey() {
		t.Fatalf("metadata idempotency key = %q, want aggregate key", preview.Metadata["usage.idempotency_key"])
	}
	if preview.Metadata["usage.quantity"] != "12" || preview.Metadata["usage.event_count"] != "1" {
		t.Fatalf("metadata = %#v, want quantity and event count", preview.Metadata)
	}
}

func TestPreviewUsageInvoiceLineRejectsNegativeCorrections(t *testing.T) {
	t.Parallel()

	window := billing.MeterWindow{
		Start: time.Date(2026, 5, 1, 0, 0, 0, 0, time.UTC),
		End:   time.Date(2026, 6, 1, 0, 0, 0, 0, time.UTC),
	}
	aggregate, err := billing.AggregateUsage([]billing.UsageEvent{
		usageEvent("evt_1", "tenant-a", "customer-a", "api_calls", -1, window.Start.Add(time.Hour), window),
	}, window)
	if err != nil {
		t.Fatalf("AggregateUsage() error = %v", err)
	}

	_, err = billing.PreviewUsageInvoiceLine(aggregate, billing.RevenueAmount{Amount: 15, Currency: "BRL"})
	if !errors.Is(err, billing.ErrEntitlementUsageInvalid) {
		t.Fatalf("PreviewUsageInvoiceLine() error = %v, want ErrEntitlementUsageInvalid", err)
	}
}

func usageEvent(id, tenant, customer, feature string, quantity int64, occurredAt time.Time, window billing.MeterWindow) billing.UsageEvent {
	return billing.UsageEvent{
		ID:         id,
		Tenant:     tenant,
		Customer:   customer,
		Feature:    feature,
		Quantity:   quantity,
		OccurredAt: occurredAt,
		Window:     window,
		Source:     "unit-test",
	}
}
