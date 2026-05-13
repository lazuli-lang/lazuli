// Package billing provides provider-neutral billing primitives for plan
// catalogs, entitlement checks, invoice lifecycles, and revenue ledger events.
package billing

import (
	"errors"
	"fmt"
	"strings"
	"time"
)

var (
	// ErrPlanInvalid is returned when a billing plan cannot be evaluated.
	ErrPlanInvalid = errors.New("lazuli/billing: plan_invalid")

	// ErrEntitlementInvalid is returned when an entitlement definition is invalid.
	ErrEntitlementInvalid = errors.New("lazuli/billing: entitlement_invalid")

	// ErrEntitlementUsageInvalid is returned when usage inputs cannot produce a coherent total.
	ErrEntitlementUsageInvalid = errors.New("lazuli/billing: entitlement_usage_invalid")

	// ErrEntitlementDenied is returned when an entitlement check is blocked before limit evaluation.
	ErrEntitlementDenied = errors.New("lazuli/billing: entitlement_denied")

	// ErrEntitlementLimitExceeded is returned when an entitlement check exceeds a hard usage limit.
	ErrEntitlementLimitExceeded = errors.New("lazuli/billing: entitlement_limit_exceeded")

	// ErrRevenueEventInvalid is returned when a revenue event cannot be recorded consistently.
	ErrRevenueEventInvalid = errors.New("lazuli/billing: revenue_event_invalid")
)

const (
	maxInt64 = int64(1<<63 - 1)
	minInt64 = -1 << 63
)

// PlanStatus is the lifecycle state for a billing plan in a catalog.
type PlanStatus string

const (
	PlanStatusUnknown  PlanStatus = "unknown"
	PlanStatusDraft    PlanStatus = "draft"
	PlanStatusActive   PlanStatus = "active"
	PlanStatusArchived PlanStatus = "archived"
)

// String renders the status as a stable lowercase token.
func (s PlanStatus) String() string {
	switch s {
	case PlanStatusDraft, PlanStatusActive, PlanStatusArchived:
		return string(s)
	default:
		return string(PlanStatusUnknown)
	}
}

// Active reports whether this status may grant entitlements.
func (s PlanStatus) Active() bool {
	return s == PlanStatusActive
}

// Plan is a provider-neutral billing plan definition. It does not declare how
// a customer subscribes or pays; provider adapters can map their own concepts to
// this shape before running entitlement checks.
type Plan struct {
	Key          string
	Name         string
	Status       PlanStatus
	Entitlements []Entitlement
	Metadata     map[string]string
}

// Validate checks that the plan and its entitlements are structurally usable.
func (p Plan) Validate() error {
	return ValidatePlan(p)
}

// Active reports whether this plan may grant entitlements.
func (p Plan) Active() bool {
	return p.Status.Active()
}

// Entitlement returns the entitlement definition for feature.
func (p Plan) Entitlement(feature string) (Entitlement, bool) {
	feature = normalizeBillingToken(feature)
	if feature == "" {
		return Entitlement{}, false
	}
	for _, entitlement := range p.Entitlements {
		if normalizeBillingToken(entitlement.Feature) == feature {
			entitlement.Feature = normalizeBillingToken(entitlement.Feature)
			return entitlement, true
		}
	}
	return Entitlement{}, false
}

// Allows reports whether an active plan grants feature at the supplied time.
// A zero time skips temporal checks on the entitlement.
func (p Plan) Allows(feature string, at time.Time) bool {
	if !p.Active() {
		return false
	}
	entitlement, ok := p.Entitlement(feature)
	return ok && entitlement.ActiveAt(at)
}

// ValidatePlan checks that a plan has a stable key, a known status, and unique
// entitlement feature keys after normalization.
func ValidatePlan(plan Plan) error {
	if normalizeBillingToken(plan.Key) == "" {
		return fmt.Errorf("%w: key must be non-empty", ErrPlanInvalid)
	}
	switch plan.Status {
	case PlanStatusDraft, PlanStatusActive, PlanStatusArchived:
	default:
		return fmt.Errorf("%w: status %q is unknown", ErrPlanInvalid, plan.Status)
	}

	seen := make(map[string]int, len(plan.Entitlements))
	for i, entitlement := range plan.Entitlements {
		if err := entitlement.Validate(); err != nil {
			return fmt.Errorf("%w: entitlement %d: %v", ErrPlanInvalid, i, err)
		}
		feature := normalizeBillingToken(entitlement.Feature)
		if previous, ok := seen[feature]; ok {
			return fmt.Errorf("%w: entitlement %d duplicates entitlement %d feature %q", ErrPlanInvalid, i, previous, feature)
		}
		seen[feature] = i
	}
	return nil
}

// Entitlement grants a feature, optionally with a hard usage limit. Unlimited
// entitlements are appropriate for boolean features or unmetered usage.
type Entitlement struct {
	Feature   string
	Enabled   bool
	Limit     int64
	Unlimited bool
	StartsAt  time.Time
	EndsAt    time.Time
	Metadata  map[string]string
}

// FeatureEntitlement returns an enabled, unlimited entitlement for feature.
func FeatureEntitlement(feature string) Entitlement {
	return Entitlement{
		Feature:   feature,
		Enabled:   true,
		Unlimited: true,
	}
}

// LimitedEntitlement returns an enabled entitlement with a hard usage limit.
func LimitedEntitlement(feature string, limit int64) Entitlement {
	return Entitlement{
		Feature: feature,
		Enabled: true,
		Limit:   limit,
	}
}

// Validate checks whether the entitlement definition can be evaluated.
func (e Entitlement) Validate() error {
	if normalizeBillingToken(e.Feature) == "" {
		return fmt.Errorf("%w: feature must be non-empty", ErrEntitlementInvalid)
	}
	if e.Limit < 0 {
		return fmt.Errorf("%w: limit must be non-negative", ErrEntitlementInvalid)
	}
	if e.Unlimited && e.Limit > 0 {
		return fmt.Errorf("%w: unlimited entitlement must not set a limit", ErrEntitlementInvalid)
	}
	if !e.StartsAt.IsZero() && !e.EndsAt.IsZero() && !e.StartsAt.Before(e.EndsAt) {
		return fmt.Errorf("%w: starts_at must be before ends_at", ErrEntitlementInvalid)
	}
	return nil
}

// ActiveAt reports whether the entitlement is enabled and currently inside its
// configured time window. A zero time skips temporal checks.
func (e Entitlement) ActiveAt(at time.Time) bool {
	if !e.Enabled {
		return false
	}
	if at.IsZero() {
		return true
	}
	if !e.StartsAt.IsZero() && at.Before(e.StartsAt) {
		return false
	}
	if !e.EndsAt.IsZero() && !at.Before(e.EndsAt) {
		return false
	}
	return true
}

// CheckUsage evaluates a usage delta under this entitlement without mutating
// any usage ledger. Positive deltas model newly consumed units; negative deltas
// model releases or corrections.
func (e Entitlement) CheckUsage(current, delta int64, at time.Time) (EntitlementCheck, error) {
	if err := e.Validate(); err != nil {
		return EntitlementCheck{}, err
	}
	return buildEntitlementCheck(e, current, delta, at)
}

// EntitlementCheck is the dry-run result for applying one usage delta to one
// entitlement.
type EntitlementCheck struct {
	Feature      string
	Allowed      bool
	CurrentUsage int64
	UsageDelta   int64
	AfterUsage   int64
	Limit        int64
	Unlimited    bool
	Reason       string
}

// Validate returns an entitlement sentinel error when the check blocks use.
func (c EntitlementCheck) Validate() error {
	return ValidateEntitlementCheck(c)
}

// ValidateEntitlementCheck returns nil when the dry-run check may be applied.
func ValidateEntitlementCheck(check EntitlementCheck) error {
	if check.Allowed {
		return nil
	}
	if check.Reason == "limit exceeded" {
		return fmt.Errorf("%w: feature %q would use %d over limit %d", ErrEntitlementLimitExceeded, check.Feature, check.AfterUsage, check.Limit)
	}
	return fmt.Errorf("%w: feature %q: %s", ErrEntitlementDenied, check.Feature, check.Reason)
}

// CheckPlanUsage evaluates feature usage against an active plan.
func CheckPlanUsage(plan Plan, feature string, current, delta int64, at time.Time) (EntitlementCheck, error) {
	if err := plan.Validate(); err != nil {
		return EntitlementCheck{}, err
	}

	feature = normalizeBillingToken(feature)
	if !plan.Active() {
		return deniedEntitlementCheck(feature, current, delta, "plan inactive")
	}

	entitlement, ok := plan.Entitlement(feature)
	if !ok {
		return deniedEntitlementCheck(feature, current, delta, "entitlement missing")
	}
	return buildEntitlementCheck(entitlement, current, delta, at)
}

// InvoiceStatus is the provider-neutral lifecycle of an invoice.
type InvoiceStatus string

const (
	InvoiceStatusUnknown       InvoiceStatus = "unknown"
	InvoiceStatusDraft         InvoiceStatus = "draft"
	InvoiceStatusOpen          InvoiceStatus = "open"
	InvoiceStatusPastDue       InvoiceStatus = "past_due"
	InvoiceStatusPaid          InvoiceStatus = "paid"
	InvoiceStatusVoid          InvoiceStatus = "void"
	InvoiceStatusUncollectible InvoiceStatus = "uncollectible"
)

// String renders the status as a stable lowercase token.
func (s InvoiceStatus) String() string {
	switch s {
	case InvoiceStatusDraft,
		InvoiceStatusOpen,
		InvoiceStatusPastDue,
		InvoiceStatusPaid,
		InvoiceStatusVoid,
		InvoiceStatusUncollectible:
		return string(s)
	default:
		return string(InvoiceStatusUnknown)
	}
}

// Payable reports whether the invoice can still accept collection attempts.
func (s InvoiceStatus) Payable() bool {
	switch s {
	case InvoiceStatusOpen, InvoiceStatusPastDue:
		return true
	default:
		return false
	}
}

// Terminal reports whether no further invoice collection progress is expected.
func (s InvoiceStatus) Terminal() bool {
	switch s {
	case InvoiceStatusPaid, InvoiceStatusVoid, InvoiceStatusUncollectible:
		return true
	default:
		return false
	}
}

// Paid reports whether the invoice has completed successfully.
func (s InvoiceStatus) Paid() bool {
	return s == InvoiceStatusPaid
}

// RevenueEventType names provider-neutral revenue ledger events.
type RevenueEventType string

const (
	RevenueEventUnknown       RevenueEventType = "unknown"
	RevenueEventInvoiceIssued RevenueEventType = "invoice.issued"
	RevenueEventInvoicePaid   RevenueEventType = "invoice.paid"
	RevenueEventInvoiceVoided RevenueEventType = "invoice.voided"
	RevenueEventCreditApplied RevenueEventType = "credit.applied"
	RevenueEventRefundIssued  RevenueEventType = "refund.issued"
)

// String renders the event type as a stable lowercase token.
func (t RevenueEventType) String() string {
	if t.Valid() {
		return string(t)
	}
	return string(RevenueEventUnknown)
}

// Valid reports whether the event type is known.
func (t RevenueEventType) Valid() bool {
	switch t {
	case RevenueEventInvoiceIssued,
		RevenueEventInvoicePaid,
		RevenueEventInvoiceVoided,
		RevenueEventCreditApplied,
		RevenueEventRefundIssued:
		return true
	default:
		return false
	}
}

// ReducesRevenue reports whether the event should be signed as a reduction.
func (t RevenueEventType) ReducesRevenue() bool {
	switch t {
	case RevenueEventInvoiceVoided, RevenueEventCreditApplied, RevenueEventRefundIssued:
		return true
	default:
		return false
	}
}

// RevenueAmount stores an amount in the minor unit for Currency. For example,
// BRL 10.50 is Amount 1050 with Currency "BRL".
type RevenueAmount struct {
	Amount   int64
	Currency string
}

// Normalize returns the amount with trimmed, uppercase currency.
func (a RevenueAmount) Normalize() RevenueAmount {
	a.Currency = strings.ToUpper(normalizeBillingToken(a.Currency))
	return a
}

// Validate checks that the amount is non-negative and has a currency.
func (a RevenueAmount) Validate() error {
	a = a.Normalize()
	if a.Amount < 0 {
		return fmt.Errorf("%w: amount must be non-negative", ErrRevenueEventInvalid)
	}
	if a.Currency == "" {
		return fmt.Errorf("%w: currency must be non-empty", ErrRevenueEventInvalid)
	}
	return nil
}

// RevenueEvent is an adapter-neutral ledger event emitted by billing workflows.
type RevenueEvent struct {
	ID         string
	Type       RevenueEventType
	Tenant     string
	Customer   string
	InvoiceID  string
	PlanKey    string
	Amount     RevenueAmount
	OccurredAt time.Time
	Metadata   map[string]string
}

// Validate checks whether the revenue event can be recorded consistently.
func (e RevenueEvent) Validate() error {
	return ValidateRevenueEvent(e)
}

// SignedAmount returns the ledger amount with reductions represented as a
// negative value. Event amounts themselves remain non-negative.
func (e RevenueEvent) SignedAmount() int64 {
	if e.Type.ReducesRevenue() {
		return -e.Amount.Amount
	}
	return e.Amount.Amount
}

// ValidateRevenueEvent checks event type, amount, and occurrence time.
func ValidateRevenueEvent(event RevenueEvent) error {
	if !event.Type.Valid() {
		return fmt.Errorf("%w: type %q is unknown", ErrRevenueEventInvalid, event.Type)
	}
	if err := event.Amount.Validate(); err != nil {
		return err
	}
	if event.OccurredAt.IsZero() {
		return fmt.Errorf("%w: occurred_at must be set", ErrRevenueEventInvalid)
	}
	return nil
}

func buildEntitlementCheck(e Entitlement, current, delta int64, at time.Time) (EntitlementCheck, error) {
	if current < 0 {
		return EntitlementCheck{}, fmt.Errorf("%w: current usage must be non-negative", ErrEntitlementUsageInvalid)
	}
	after, err := checkedBillingAdd(current, delta)
	if err != nil {
		return EntitlementCheck{}, fmt.Errorf("%w: usage total overflows", ErrEntitlementUsageInvalid)
	}
	if after < 0 {
		return EntitlementCheck{}, fmt.Errorf("%w: usage total would be negative", ErrEntitlementUsageInvalid)
	}

	check := EntitlementCheck{
		Feature:      normalizeBillingToken(e.Feature),
		Allowed:      true,
		CurrentUsage: current,
		UsageDelta:   delta,
		AfterUsage:   after,
		Limit:        e.Limit,
		Unlimited:    e.Unlimited,
		Reason:       "within limit",
	}
	if !e.ActiveAt(at) {
		check.Allowed = false
		check.Reason = "entitlement inactive"
		return check, nil
	}
	if e.Unlimited {
		check.Reason = "unlimited"
		return check, nil
	}
	if after > e.Limit {
		check.Allowed = delta <= 0
		check.Reason = "limit exceeded"
	}
	return check, nil
}

func deniedEntitlementCheck(feature string, current, delta int64, reason string) (EntitlementCheck, error) {
	if current < 0 {
		return EntitlementCheck{}, fmt.Errorf("%w: current usage must be non-negative", ErrEntitlementUsageInvalid)
	}
	after, err := checkedBillingAdd(current, delta)
	if err != nil {
		return EntitlementCheck{}, fmt.Errorf("%w: usage total overflows", ErrEntitlementUsageInvalid)
	}
	if after < 0 {
		return EntitlementCheck{}, fmt.Errorf("%w: usage total would be negative", ErrEntitlementUsageInvalid)
	}
	return EntitlementCheck{
		Feature:      normalizeBillingToken(feature),
		Allowed:      false,
		CurrentUsage: current,
		UsageDelta:   delta,
		AfterUsage:   after,
		Reason:       reason,
	}, nil
}

func checkedBillingAdd(a, b int64) (int64, error) {
	if b > 0 && a > maxInt64-b {
		return 0, ErrEntitlementUsageInvalid
	}
	if b < 0 && a < minInt64-b {
		return 0, ErrEntitlementUsageInvalid
	}
	return a + b, nil
}

func normalizeBillingToken(value string) string {
	return strings.TrimSpace(value)
}
