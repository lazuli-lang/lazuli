package billing

import (
	"errors"
	"fmt"
	"math/big"
	"strings"
	"time"
)

var (
	// ErrSubscriptionInvalid is returned when a subscription cannot be evaluated consistently.
	ErrSubscriptionInvalid = errors.New("lazuli/billing: subscription_invalid")
)

// SubscriptionStatus is the provider-neutral lifecycle state for a subscription.
type SubscriptionStatus string

const (
	SubscriptionStatusUnknown    SubscriptionStatus = "unknown"
	SubscriptionStatusTrialing   SubscriptionStatus = "trialing"
	SubscriptionStatusActive     SubscriptionStatus = "active"
	SubscriptionStatusPastDue    SubscriptionStatus = "past_due"
	SubscriptionStatusPaused     SubscriptionStatus = "paused"
	SubscriptionStatusCanceled   SubscriptionStatus = "canceled"
	SubscriptionStatusIncomplete SubscriptionStatus = "incomplete"
	SubscriptionStatusExpired    SubscriptionStatus = "expired"
)

// String renders the status as a stable lowercase token.
func (s SubscriptionStatus) String() string {
	if s.Valid() {
		return string(s)
	}
	return string(SubscriptionStatusUnknown)
}

// Valid reports whether the status is known.
func (s SubscriptionStatus) Valid() bool {
	switch s {
	case SubscriptionStatusTrialing,
		SubscriptionStatusActive,
		SubscriptionStatusPastDue,
		SubscriptionStatusPaused,
		SubscriptionStatusCanceled,
		SubscriptionStatusIncomplete,
		SubscriptionStatusExpired:
		return true
	default:
		return false
	}
}

// Active reports whether the status can grant access before temporal checks.
func (s SubscriptionStatus) Active() bool {
	switch s {
	case SubscriptionStatusTrialing, SubscriptionStatusActive, SubscriptionStatusPastDue:
		return true
	default:
		return false
	}
}

// Terminal reports whether no more lifecycle progress is expected.
func (s SubscriptionStatus) Terminal() bool {
	switch s {
	case SubscriptionStatusCanceled, SubscriptionStatusExpired:
		return true
	default:
		return false
	}
}

// Subscription is a provider-neutral subscription record. Provider adapters can
// map their own identifiers and states to this shape before lifecycle checks.
type Subscription struct {
	ID                 string
	Tenant             string
	Customer           string
	PlanKey            string
	Status             SubscriptionStatus
	CurrentPeriodStart time.Time
	CurrentPeriodEnd   time.Time
	TrialStart         time.Time
	TrialEnd           time.Time
	CancelAt           time.Time
	CanceledAt         time.Time
	GracePeriodEnd     time.Time
	EndsAt             time.Time
	Metadata           map[string]string
}

// Validate checks that the subscription can be evaluated consistently.
func (s Subscription) Validate() error {
	return ValidateSubscription(s)
}

// TrialActive reports whether at falls inside the subscription trial window.
func (s Subscription) TrialActive(at time.Time) bool {
	return timeInWindow(at, s.TrialStart, s.TrialEnd)
}

// CancelScheduled reports whether cancellation is scheduled but not yet effective.
func (s Subscription) CancelScheduled(at time.Time) bool {
	if s.CancelAt.IsZero() {
		return false
	}
	if !s.CanceledAt.IsZero() {
		return false
	}
	if at.IsZero() {
		return true
	}
	return at.Before(s.CancelAt)
}

// CanceledAtTime reports whether cancellation has taken effect by at.
func (s Subscription) CanceledAtTime(at time.Time) bool {
	if s.Status == SubscriptionStatusCanceled {
		return true
	}
	if s.CanceledAt.IsZero() {
		return false
	}
	if at.IsZero() {
		return true
	}
	return !at.Before(s.CanceledAt)
}

// GracePeriodActive reports whether at falls inside the grace period after the
// current period has ended.
func (s Subscription) GracePeriodActive(at time.Time) bool {
	if s.GracePeriodEnd.IsZero() || s.CurrentPeriodEnd.IsZero() || at.IsZero() {
		return false
	}
	return !at.Before(s.CurrentPeriodEnd) && at.Before(s.GracePeriodEnd)
}

// EntitledAt reports whether the subscription should grant access at at.
func (s Subscription) EntitledAt(at time.Time) bool {
	if !s.Status.Active() {
		return false
	}
	if s.CanceledAtTime(at) {
		return false
	}
	if !s.EndsAt.IsZero() && !at.IsZero() && !at.Before(s.EndsAt) {
		return false
	}
	if s.TrialActive(at) {
		return true
	}
	if !s.CurrentPeriodEnd.IsZero() && !at.IsZero() && !at.Before(s.CurrentPeriodEnd) {
		return s.GracePeriodActive(at)
	}
	return true
}

// NextRenewal computes the first renewal instant after at using the supplied interval.
func (s Subscription) NextRenewal(at time.Time, interval RenewalInterval) (time.Time, error) {
	start := s.CurrentPeriodEnd
	if start.IsZero() {
		start = s.CurrentPeriodStart
	}
	return NextRenewal(start, at, interval)
}

// ValidateSubscription checks status, temporal windows, and basic identifiers.
func ValidateSubscription(subscription Subscription) error {
	if normalizeBillingToken(subscription.PlanKey) == "" {
		return fmt.Errorf("%w: plan_key must be non-empty", ErrSubscriptionInvalid)
	}
	if !subscription.Status.Valid() {
		return fmt.Errorf("%w: status %q is unknown", ErrSubscriptionInvalid, subscription.Status)
	}
	if !validTimeWindow(subscription.CurrentPeriodStart, subscription.CurrentPeriodEnd) {
		return fmt.Errorf("%w: current_period_start must be before current_period_end", ErrSubscriptionInvalid)
	}
	if !validTimeWindow(subscription.TrialStart, subscription.TrialEnd) {
		return fmt.Errorf("%w: trial_start must be before trial_end", ErrSubscriptionInvalid)
	}
	if !subscription.CancelAt.IsZero() && !subscription.CanceledAt.IsZero() && subscription.CanceledAt.Before(subscription.CancelAt) {
		return fmt.Errorf("%w: canceled_at must not be before cancel_at", ErrSubscriptionInvalid)
	}
	if !subscription.GracePeriodEnd.IsZero() && subscription.CurrentPeriodEnd.IsZero() {
		return fmt.Errorf("%w: grace_period_end requires current_period_end", ErrSubscriptionInvalid)
	}
	if !subscription.GracePeriodEnd.IsZero() && !subscription.CurrentPeriodEnd.Before(subscription.GracePeriodEnd) {
		return fmt.Errorf("%w: grace_period_end must be after current_period_end", ErrSubscriptionInvalid)
	}
	return nil
}

// RenewalInterval describes a recurring subscription period.
type RenewalInterval struct {
	Unit  string
	Count int
}

// Validate checks that the interval can advance time.
func (i RenewalInterval) Validate() error {
	if i.Count <= 0 {
		return fmt.Errorf("%w: renewal interval count must be positive", ErrSubscriptionInvalid)
	}
	switch normalizeRenewalUnit(i.Unit) {
	case "day", "week", "month", "year":
		return nil
	default:
		return fmt.Errorf("%w: renewal interval unit %q is unknown", ErrSubscriptionInvalid, i.Unit)
	}
}

// Add advances t by one interval.
func (i RenewalInterval) Add(t time.Time) (time.Time, error) {
	if err := i.Validate(); err != nil {
		return time.Time{}, err
	}
	switch normalizeRenewalUnit(i.Unit) {
	case "day":
		return t.AddDate(0, 0, i.Count), nil
	case "week":
		return t.AddDate(0, 0, 7*i.Count), nil
	case "month":
		return t.AddDate(0, i.Count, 0), nil
	case "year":
		return t.AddDate(i.Count, 0, 0), nil
	default:
		return time.Time{}, fmt.Errorf("%w: renewal interval unit %q is unknown", ErrSubscriptionInvalid, i.Unit)
	}
}

// NextRenewal computes the first renewal instant after at.
func NextRenewal(start, at time.Time, interval RenewalInterval) (time.Time, error) {
	if start.IsZero() {
		return time.Time{}, fmt.Errorf("%w: renewal start must be set", ErrSubscriptionInvalid)
	}
	next := start
	for i := 0; i < 10000 && !next.After(at); i++ {
		var err error
		next, err = interval.Add(next)
		if err != nil {
			return time.Time{}, err
		}
	}
	if !next.After(at) {
		return time.Time{}, fmt.Errorf("%w: next renewal could not be computed", ErrSubscriptionInvalid)
	}
	return next, nil
}

// PlanRate is the recurring price for a plan in minor currency units.
type PlanRate struct {
	PlanKey  string
	Amount   int64
	Currency string
	Interval RenewalInterval
}

// Validate checks that the rate can be used in a plan change preview.
func (r PlanRate) Validate() error {
	if normalizeBillingToken(r.PlanKey) == "" {
		return fmt.Errorf("%w: plan_key must be non-empty", ErrSubscriptionInvalid)
	}
	if r.Amount < 0 {
		return fmt.Errorf("%w: amount must be non-negative", ErrSubscriptionInvalid)
	}
	if strings.ToUpper(normalizeBillingToken(r.Currency)) == "" {
		return fmt.Errorf("%w: currency must be non-empty", ErrSubscriptionInvalid)
	}
	return r.Interval.Validate()
}

// PlanChangeRequest describes a provider-neutral plan change dry run.
type PlanChangeRequest struct {
	CurrentPlan PlanRate
	NewPlan     PlanRate
	PeriodStart time.Time
	PeriodEnd   time.Time
	EffectiveAt time.Time
}

// PlanChangePreview is the deterministic result of a plan change dry run.
type PlanChangePreview struct {
	FromPlanKey     string
	ToPlanKey       string
	EffectiveAt     time.Time
	PeriodStart     time.Time
	PeriodEnd       time.Time
	Currency        string
	ProrationCredit int64
	ProrationCharge int64
	AmountDue       int64
}

// PreviewPlanChange prorates the remaining period from the current plan to the new plan.
func PreviewPlanChange(req PlanChangeRequest) (PlanChangePreview, error) {
	if err := req.CurrentPlan.Validate(); err != nil {
		return PlanChangePreview{}, err
	}
	if err := req.NewPlan.Validate(); err != nil {
		return PlanChangePreview{}, err
	}
	currentCurrency := strings.ToUpper(normalizeBillingToken(req.CurrentPlan.Currency))
	newCurrency := strings.ToUpper(normalizeBillingToken(req.NewPlan.Currency))
	if currentCurrency != newCurrency {
		return PlanChangePreview{}, fmt.Errorf("%w: plan currencies must match", ErrSubscriptionInvalid)
	}
	if !req.CurrentPlan.Interval.equal(req.NewPlan.Interval) {
		return PlanChangePreview{}, fmt.Errorf("%w: plan intervals must match", ErrSubscriptionInvalid)
	}
	if req.PeriodStart.IsZero() || req.PeriodEnd.IsZero() || !req.PeriodStart.Before(req.PeriodEnd) {
		return PlanChangePreview{}, fmt.Errorf("%w: period_start must be before period_end", ErrSubscriptionInvalid)
	}
	if req.EffectiveAt.Before(req.PeriodStart) || req.EffectiveAt.After(req.PeriodEnd) {
		return PlanChangePreview{}, fmt.Errorf("%w: effective_at must be inside the current period", ErrSubscriptionInvalid)
	}

	total := req.PeriodEnd.Sub(req.PeriodStart)
	remaining := req.PeriodEnd.Sub(req.EffectiveAt)
	credit := prorateMinorUnits(req.CurrentPlan.Amount, remaining, total)
	charge := prorateMinorUnits(req.NewPlan.Amount, remaining, total)
	return PlanChangePreview{
		FromPlanKey:     normalizeBillingToken(req.CurrentPlan.PlanKey),
		ToPlanKey:       normalizeBillingToken(req.NewPlan.PlanKey),
		EffectiveAt:     req.EffectiveAt,
		PeriodStart:     req.PeriodStart,
		PeriodEnd:       req.PeriodEnd,
		Currency:        currentCurrency,
		ProrationCredit: credit,
		ProrationCharge: charge,
		AmountDue:       charge - credit,
	}, nil
}

func (i RenewalInterval) equal(other RenewalInterval) bool {
	return normalizeRenewalUnit(i.Unit) == normalizeRenewalUnit(other.Unit) && i.Count == other.Count
}

func normalizeRenewalUnit(unit string) string {
	unit = strings.ToLower(normalizeBillingToken(unit))
	return strings.TrimSuffix(unit, "s")
}

func prorateMinorUnits(amount int64, remaining, total time.Duration) int64 {
	if amount == 0 || remaining <= 0 || total <= 0 {
		return 0
	}
	value := big.NewInt(amount)
	value.Mul(value, big.NewInt(remaining.Nanoseconds()))
	value.Quo(value, big.NewInt(total.Nanoseconds()))
	return value.Int64()
}

func validTimeWindow(start, end time.Time) bool {
	return start.IsZero() || end.IsZero() || start.Before(end)
}

func timeInWindow(at, start, end time.Time) bool {
	if at.IsZero() || start.IsZero() || end.IsZero() {
		return false
	}
	return !at.Before(start) && at.Before(end)
}
