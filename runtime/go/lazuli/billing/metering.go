package billing

import (
	"errors"
	"fmt"
	"sort"
	"strconv"
	"strings"
	"time"
)

var (
	// ErrUsageEventInvalid is returned when a metered usage event is not usable.
	ErrUsageEventInvalid = errors.New("lazuli/billing: usage_event_invalid")

	// ErrMeterWindowInvalid is returned when a metering aggregation window is invalid.
	ErrMeterWindowInvalid = errors.New("lazuli/billing: meter_window_invalid")
)

// MeterWindow is the half-open interval [Start, End) used to bucket metered
// usage. End is exclusive so adjacent windows can be aggregated without overlap.
type MeterWindow struct {
	Start time.Time
	End   time.Time
}

// Validate checks that the window can safely bucket usage.
func (w MeterWindow) Validate() error {
	if w.Start.IsZero() {
		return fmt.Errorf("%w: start must be set", ErrMeterWindowInvalid)
	}
	if w.End.IsZero() {
		return fmt.Errorf("%w: end must be set", ErrMeterWindowInvalid)
	}
	if !w.Start.Before(w.End) {
		return fmt.Errorf("%w: start must be before end", ErrMeterWindowInvalid)
	}
	return nil
}

// Contains reports whether at falls inside the half-open window.
func (w MeterWindow) Contains(at time.Time) bool {
	if w.Validate() != nil || at.IsZero() {
		return false
	}
	return !at.Before(w.Start) && at.Before(w.End)
}

// UsageEvent is a provider-neutral metered usage event emitted by application
// code before any payment or invoicing adapter sees it.
type UsageEvent struct {
	ID         string
	Tenant     string
	Customer   string
	Feature    string
	Quantity   int64
	OccurredAt time.Time
	Window     MeterWindow
	Source     string
	Metadata   map[string]string
}

// Validate checks whether the event can be aggregated and entitlement-checked.
func (e UsageEvent) Validate() error {
	if normalizeBillingToken(e.Tenant) == "" {
		return fmt.Errorf("%w: tenant must be non-empty", ErrUsageEventInvalid)
	}
	if normalizeBillingToken(e.Feature) == "" {
		return fmt.Errorf("%w: feature must be non-empty", ErrUsageEventInvalid)
	}
	if e.Quantity == 0 {
		return fmt.Errorf("%w: quantity must be non-zero", ErrUsageEventInvalid)
	}
	if e.OccurredAt.IsZero() {
		return fmt.Errorf("%w: occurred_at must be set", ErrUsageEventInvalid)
	}
	if err := e.Window.Validate(); err != nil {
		return fmt.Errorf("%w: %v", ErrUsageEventInvalid, err)
	}
	if !e.Window.Contains(e.OccurredAt) {
		return fmt.Errorf("%w: occurred_at must fall inside window", ErrUsageEventInvalid)
	}
	return nil
}

// DedupKey returns a stable key for dropping duplicate usage events before
// aggregation. If ID is present, it anchors the key; otherwise the key is derived
// from the event's normalized business dimensions.
func (e UsageEvent) DedupKey() string {
	id := normalizeBillingToken(e.ID)
	if id != "" {
		return meteringKey("usage_dedup", normalizeBillingToken(e.Tenant), id)
	}
	return meteringKey(
		"usage_dedup",
		normalizeBillingToken(e.Tenant),
		normalizeBillingToken(e.Customer),
		normalizeBillingToken(e.Feature),
		e.Window.Start.UTC().Format(time.RFC3339Nano),
		e.Window.End.UTC().Format(time.RFC3339Nano),
		e.OccurredAt.UTC().Format(time.RFC3339Nano),
		strconv.FormatInt(e.Quantity, 10),
		normalizeBillingToken(e.Source),
	)
}

// IdempotencyKey returns a stable key for applying the usage event to a ledger.
func (e UsageEvent) IdempotencyKey() string {
	return meteringKey("usage_apply", e.DedupKey())
}

// UsageAggregate is the deterministic rollup of usage events for one tenant,
// customer, feature, and metering window.
type UsageAggregate struct {
	Tenant          string
	Customer        string
	Feature         string
	Window          MeterWindow
	Quantity        int64
	EventCount      int
	DuplicateCount  int
	FirstOccurredAt time.Time
	LastOccurredAt  time.Time
	DedupKeys       []string
	Metadata        map[string]string
}

// IdempotencyKey returns a stable key for applying the aggregate exactly once.
func (a UsageAggregate) IdempotencyKey() string {
	return meteringKey(
		"usage_aggregate",
		normalizeBillingToken(a.Tenant),
		normalizeBillingToken(a.Customer),
		normalizeBillingToken(a.Feature),
		a.Window.Start.UTC().Format(time.RFC3339Nano),
		a.Window.End.UTC().Format(time.RFC3339Nano),
	)
}

// AggregateUsage deduplicates and sums events inside window. All accepted events
// must target the same tenant, customer, and feature.
func AggregateUsage(events []UsageEvent, window MeterWindow) (UsageAggregate, error) {
	if err := window.Validate(); err != nil {
		return UsageAggregate{}, err
	}

	aggregate := UsageAggregate{
		Window:    window,
		DedupKeys: make([]string, 0, len(events)),
		Metadata:  map[string]string{},
	}
	seen := make(map[string]struct{}, len(events))
	for i, event := range events {
		if err := event.Validate(); err != nil {
			return UsageAggregate{}, fmt.Errorf("%w: event %d: %v", ErrUsageEventInvalid, i, err)
		}
		if !event.Window.Start.Equal(window.Start) || !event.Window.End.Equal(window.End) {
			return UsageAggregate{}, fmt.Errorf("%w: event %d window does not match aggregate window", ErrUsageEventInvalid, i)
		}
		if !window.Contains(event.OccurredAt) {
			return UsageAggregate{}, fmt.Errorf("%w: event %d occurred outside aggregate window", ErrUsageEventInvalid, i)
		}

		tenant := normalizeBillingToken(event.Tenant)
		customer := normalizeBillingToken(event.Customer)
		feature := normalizeBillingToken(event.Feature)
		if aggregate.EventCount == 0 {
			aggregate.Tenant = tenant
			aggregate.Customer = customer
			aggregate.Feature = feature
		} else if aggregate.Tenant != tenant || aggregate.Customer != customer || aggregate.Feature != feature {
			return UsageAggregate{}, fmt.Errorf("%w: event %d dimensions differ from aggregate", ErrUsageEventInvalid, i)
		}

		key := event.DedupKey()
		if _, ok := seen[key]; ok {
			aggregate.DuplicateCount++
			continue
		}
		seen[key] = struct{}{}

		quantity, err := checkedBillingAdd(aggregate.Quantity, event.Quantity)
		if err != nil {
			return UsageAggregate{}, fmt.Errorf("%w: aggregate quantity overflows", ErrEntitlementUsageInvalid)
		}
		aggregate.Quantity = quantity
		aggregate.EventCount++
		aggregate.DedupKeys = append(aggregate.DedupKeys, key)
		if aggregate.FirstOccurredAt.IsZero() || event.OccurredAt.Before(aggregate.FirstOccurredAt) {
			aggregate.FirstOccurredAt = event.OccurredAt
		}
		if aggregate.LastOccurredAt.IsZero() || event.OccurredAt.After(aggregate.LastOccurredAt) {
			aggregate.LastOccurredAt = event.OccurredAt
		}
		mergeUsageMetadata(aggregate.Metadata, event.Metadata)
	}
	sort.Strings(aggregate.DedupKeys)
	return aggregate, nil
}

// CheckUsageEventEntitlement evaluates one event against the plan entitlement
// before a caller records it.
func CheckUsageEventEntitlement(plan Plan, current int64, event UsageEvent) (EntitlementCheck, error) {
	if err := event.Validate(); err != nil {
		return EntitlementCheck{}, err
	}
	return CheckPlanUsage(plan, event.Feature, current, event.Quantity, event.OccurredAt)
}

// CheckUsageAggregateEntitlement evaluates aggregate usage against the plan
// entitlement before a caller records or invoices it.
func CheckUsageAggregateEntitlement(plan Plan, current int64, aggregate UsageAggregate) (EntitlementCheck, error) {
	if err := aggregate.Window.Validate(); err != nil {
		return EntitlementCheck{}, err
	}
	return CheckPlanUsage(plan, aggregate.Feature, current, aggregate.Quantity, aggregate.Window.End.Add(-time.Nanosecond))
}

// InvoiceLinePreview is adapter-neutral metadata for showing a usage line before
// it is handed to an invoicing provider.
type InvoiceLinePreview struct {
	Tenant      string
	Customer    string
	Feature     string
	Quantity    int64
	UnitAmount  RevenueAmount
	TotalAmount RevenueAmount
	Window      MeterWindow
	Metadata    map[string]string
}

// PreviewUsageInvoiceLine builds deterministic invoice-line preview metadata
// for an aggregate and a non-negative per-unit amount.
func PreviewUsageInvoiceLine(aggregate UsageAggregate, unitAmount RevenueAmount) (InvoiceLinePreview, error) {
	if err := aggregate.Window.Validate(); err != nil {
		return InvoiceLinePreview{}, err
	}
	unitAmount = unitAmount.Normalize()
	if err := unitAmount.Validate(); err != nil {
		return InvoiceLinePreview{}, err
	}
	if aggregate.Quantity < 0 {
		return InvoiceLinePreview{}, fmt.Errorf("%w: aggregate quantity must be non-negative for invoice preview", ErrEntitlementUsageInvalid)
	}
	if unitAmount.Amount != 0 && aggregate.Quantity > maxInt64/unitAmount.Amount {
		return InvoiceLinePreview{}, fmt.Errorf("%w: invoice line amount overflows", ErrRevenueEventInvalid)
	}

	total := RevenueAmount{
		Amount:   aggregate.Quantity * unitAmount.Amount,
		Currency: unitAmount.Currency,
	}
	return InvoiceLinePreview{
		Tenant:      normalizeBillingToken(aggregate.Tenant),
		Customer:    normalizeBillingToken(aggregate.Customer),
		Feature:     normalizeBillingToken(aggregate.Feature),
		Quantity:    aggregate.Quantity,
		UnitAmount:  unitAmount,
		TotalAmount: total,
		Window:      aggregate.Window,
		Metadata:    UsageInvoiceLineMetadata(aggregate),
	}, nil
}

// UsageInvoiceLineMetadata returns stable string metadata suitable for attaching
// to provider-specific invoice line previews.
func UsageInvoiceLineMetadata(aggregate UsageAggregate) map[string]string {
	metadata := map[string]string{
		"usage.idempotency_key": aggregate.IdempotencyKey(),
		"usage.tenant":          normalizeBillingToken(aggregate.Tenant),
		"usage.customer":        normalizeBillingToken(aggregate.Customer),
		"usage.feature":         normalizeBillingToken(aggregate.Feature),
		"usage.window_start":    aggregate.Window.Start.UTC().Format(time.RFC3339Nano),
		"usage.window_end":      aggregate.Window.End.UTC().Format(time.RFC3339Nano),
		"usage.quantity":        strconv.FormatInt(aggregate.Quantity, 10),
		"usage.event_count":     strconv.Itoa(aggregate.EventCount),
		"usage.duplicate_count": strconv.Itoa(aggregate.DuplicateCount),
	}
	for key, value := range aggregate.Metadata {
		key = normalizeBillingToken(key)
		if key == "" {
			continue
		}
		metadata["usage.meta."+key] = value
	}
	return metadata
}

func mergeUsageMetadata(dst, src map[string]string) {
	for key, value := range src {
		key = normalizeBillingToken(key)
		if key == "" {
			continue
		}
		if existing, ok := dst[key]; !ok || existing == value {
			dst[key] = value
		}
	}
}

func meteringKey(prefix string, parts ...string) string {
	var b strings.Builder
	b.WriteString(prefix)
	for _, part := range parts {
		b.WriteByte(':')
		b.WriteString(strconv.Itoa(len(part)))
		b.WriteByte(':')
		b.WriteString(part)
	}
	return b.String()
}
