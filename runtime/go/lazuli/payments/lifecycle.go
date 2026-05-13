package payments

import (
	"errors"
	"fmt"
	"strings"
	"time"
)

var (
	// ErrPaymentLifecycleInvalid is returned when lifecycle inputs cannot be
	// evaluated coherently.
	ErrPaymentLifecycleInvalid = errors.New("payments: lifecycle invalid")

	// ErrPaymentLifecycleBlocked is returned when a valid dry-run lifecycle
	// plan would be blocked by current state or policy.
	ErrPaymentLifecycleBlocked = errors.New("payments: lifecycle blocked")
)

// SubscriptionStatus is the provider-neutral lifecycle state for recurring
// access tied to payments.
type SubscriptionStatus string

const (
	SubscriptionStatusUnknown     SubscriptionStatus = "unknown"
	SubscriptionStatusActive      SubscriptionStatus = "active"
	SubscriptionStatusPastDue     SubscriptionStatus = "past_due"
	SubscriptionStatusGracePeriod SubscriptionStatus = "grace_period"
	SubscriptionStatusSuspended   SubscriptionStatus = "suspended"
	SubscriptionStatusCanceled    SubscriptionStatus = "canceled"
	SubscriptionStatusUnpaid      SubscriptionStatus = "unpaid"
)

// String renders the status as a stable lowercase token.
func (s SubscriptionStatus) String() string {
	if s.Valid() {
		return string(s)
	}
	return string(SubscriptionStatusUnknown)
}

// Valid reports whether the status is known and actionable.
func (s SubscriptionStatus) Valid() bool {
	switch s {
	case SubscriptionStatusActive,
		SubscriptionStatusPastDue,
		SubscriptionStatusGracePeriod,
		SubscriptionStatusSuspended,
		SubscriptionStatusCanceled,
		SubscriptionStatusUnpaid:
		return true
	default:
		return false
	}
}

// Terminal reports whether no further subscription access changes are expected
// without creating a new subscription record.
func (s SubscriptionStatus) Terminal() bool {
	return s == SubscriptionStatusCanceled
}

// PlanChangeMode selects when a requested subscription plan change applies.
type PlanChangeMode string

const (
	PlanChangeModeImmediate PlanChangeMode = "immediate"
	PlanChangeModePeriodEnd PlanChangeMode = "period_end"
)

// Valid reports whether the plan change mode is known.
func (m PlanChangeMode) Valid() bool {
	switch m {
	case PlanChangeModeImmediate, PlanChangeModePeriodEnd:
		return true
	default:
		return false
	}
}

// PlanChangeRequest describes a provider-neutral request to move a
// subscription from one plan key to another.
type PlanChangeRequest struct {
	CurrentPlan      string
	TargetPlan       string
	Status           SubscriptionStatus
	Mode             PlanChangeMode
	RequestedAt      time.Time
	CurrentPeriodEnd time.Time
	Reason           string
}

// Validate checks that the plan change request can be evaluated.
func (r PlanChangeRequest) Validate() error {
	if normalizeLifecycleToken(r.CurrentPlan) == "" {
		return fmt.Errorf("%w: current_plan must be non-empty", ErrPaymentLifecycleInvalid)
	}
	if normalizeLifecycleToken(r.TargetPlan) == "" {
		return fmt.Errorf("%w: target_plan must be non-empty", ErrPaymentLifecycleInvalid)
	}
	if normalizeLifecycleToken(r.CurrentPlan) == normalizeLifecycleToken(r.TargetPlan) {
		return fmt.Errorf("%w: target_plan must differ from current_plan", ErrPaymentLifecycleInvalid)
	}
	if !r.Mode.Valid() {
		return fmt.Errorf("%w: plan change mode %q is unknown", ErrPaymentLifecycleInvalid, r.Mode)
	}
	if !r.Status.Valid() {
		return fmt.Errorf("%w: subscription status %q is unknown", ErrPaymentLifecycleInvalid, r.Status)
	}
	if r.RequestedAt.IsZero() {
		return fmt.Errorf("%w: requested_at must be set", ErrPaymentLifecycleInvalid)
	}
	if r.Mode == PlanChangeModePeriodEnd {
		if r.CurrentPeriodEnd.IsZero() {
			return fmt.Errorf("%w: current_period_end must be set", ErrPaymentLifecycleInvalid)
		}
		if r.CurrentPeriodEnd.Before(r.RequestedAt) {
			return fmt.Errorf("%w: current_period_end must not be before requested_at", ErrPaymentLifecycleInvalid)
		}
	}
	return nil
}

// PlanChange is the dry-run result for a provider-neutral subscription plan
// change. Callers apply allowed plans in their own transaction or adapter flow.
type PlanChange struct {
	Allowed     bool
	CurrentPlan string
	TargetPlan  string
	Mode        PlanChangeMode
	RequestedAt time.Time
	EffectiveAt time.Time
	Reason      string
}

// Validate returns nil when the plan change may be applied.
func (c PlanChange) Validate() error {
	if !c.Allowed {
		return fmt.Errorf("%w: %s", ErrPaymentLifecycleBlocked, lifecycleReason(c.Reason))
	}
	if normalizeLifecycleToken(c.CurrentPlan) == "" {
		return fmt.Errorf("%w: current_plan must be non-empty", ErrPaymentLifecycleInvalid)
	}
	if normalizeLifecycleToken(c.TargetPlan) == "" {
		return fmt.Errorf("%w: target_plan must be non-empty", ErrPaymentLifecycleInvalid)
	}
	if normalizeLifecycleToken(c.CurrentPlan) == normalizeLifecycleToken(c.TargetPlan) {
		return fmt.Errorf("%w: target_plan must differ from current_plan", ErrPaymentLifecycleInvalid)
	}
	if !c.Mode.Valid() {
		return fmt.Errorf("%w: plan change mode %q is unknown", ErrPaymentLifecycleInvalid, c.Mode)
	}
	if c.RequestedAt.IsZero() {
		return fmt.Errorf("%w: requested_at must be set", ErrPaymentLifecycleInvalid)
	}
	if c.EffectiveAt.IsZero() {
		return fmt.Errorf("%w: effective_at must be set", ErrPaymentLifecycleInvalid)
	}
	return nil
}

// BuildPlanChange evaluates when a plan change should take effect without
// mutating provider state.
func BuildPlanChange(req PlanChangeRequest) (PlanChange, error) {
	if err := req.Validate(); err != nil {
		return PlanChange{}, err
	}

	effectiveAt := req.RequestedAt
	if req.Mode == PlanChangeModePeriodEnd {
		effectiveAt = req.CurrentPeriodEnd
	}

	change := PlanChange{
		Allowed:     true,
		CurrentPlan: normalizeLifecycleToken(req.CurrentPlan),
		TargetPlan:  normalizeLifecycleToken(req.TargetPlan),
		Mode:        req.Mode,
		RequestedAt: req.RequestedAt,
		EffectiveAt: effectiveAt,
		Reason:      lifecycleReason(req.Reason),
	}
	if subscriptionStatusBlocksPlanChange(req.Status) {
		change.Allowed = false
		change.Reason = "subscription status does not allow plan changes"
	}
	return change, nil
}

// CancellationMode selects when subscription cancellation takes effect.
type CancellationMode string

const (
	CancellationModeImmediate CancellationMode = "immediate"
	CancellationModePeriodEnd CancellationMode = "period_end"
)

// Valid reports whether the cancellation mode is known.
func (m CancellationMode) Valid() bool {
	switch m {
	case CancellationModeImmediate, CancellationModePeriodEnd:
		return true
	default:
		return false
	}
}

// CancellationRequest describes a provider-neutral subscription cancellation
// request.
type CancellationRequest struct {
	Status           SubscriptionStatus
	Mode             CancellationMode
	RequestedAt      time.Time
	CurrentPeriodEnd time.Time
	Reason           string
}

// Validate checks that the cancellation request can be evaluated.
func (r CancellationRequest) Validate() error {
	if !r.Status.Valid() {
		return fmt.Errorf("%w: subscription status %q is unknown", ErrPaymentLifecycleInvalid, r.Status)
	}
	if !r.Mode.Valid() {
		return fmt.Errorf("%w: cancellation mode %q is unknown", ErrPaymentLifecycleInvalid, r.Mode)
	}
	if r.RequestedAt.IsZero() {
		return fmt.Errorf("%w: requested_at must be set", ErrPaymentLifecycleInvalid)
	}
	if r.Mode == CancellationModePeriodEnd {
		if r.CurrentPeriodEnd.IsZero() {
			return fmt.Errorf("%w: current_period_end must be set", ErrPaymentLifecycleInvalid)
		}
		if r.CurrentPeriodEnd.Before(r.RequestedAt) {
			return fmt.Errorf("%w: current_period_end must not be before requested_at", ErrPaymentLifecycleInvalid)
		}
	}
	return nil
}

// CancellationPlan is the dry-run result for subscription cancellation.
type CancellationPlan struct {
	Allowed      bool
	Mode         CancellationMode
	RequestedAt  time.Time
	CancelAt     time.Time
	AccessEndsAt time.Time
	Reason       string
}

// Validate returns nil when the cancellation plan may be applied.
func (p CancellationPlan) Validate() error {
	if !p.Allowed {
		return fmt.Errorf("%w: %s", ErrPaymentLifecycleBlocked, lifecycleReason(p.Reason))
	}
	if !p.Mode.Valid() {
		return fmt.Errorf("%w: cancellation mode %q is unknown", ErrPaymentLifecycleInvalid, p.Mode)
	}
	if p.RequestedAt.IsZero() {
		return fmt.Errorf("%w: requested_at must be set", ErrPaymentLifecycleInvalid)
	}
	if p.CancelAt.IsZero() {
		return fmt.Errorf("%w: cancel_at must be set", ErrPaymentLifecycleInvalid)
	}
	if p.AccessEndsAt.IsZero() {
		return fmt.Errorf("%w: access_ends_at must be set", ErrPaymentLifecycleInvalid)
	}
	return nil
}

// BuildCancellationPlan evaluates when cancellation should take effect without
// mutating provider state.
func BuildCancellationPlan(req CancellationRequest) (CancellationPlan, error) {
	if err := req.Validate(); err != nil {
		return CancellationPlan{}, err
	}

	cancelAt := req.RequestedAt
	if req.Mode == CancellationModePeriodEnd {
		cancelAt = req.CurrentPeriodEnd
	}
	plan := CancellationPlan{
		Allowed:      true,
		Mode:         req.Mode,
		RequestedAt:  req.RequestedAt,
		CancelAt:     cancelAt,
		AccessEndsAt: cancelAt,
		Reason:       lifecycleReason(req.Reason),
	}
	if req.Status.Terminal() {
		plan.Allowed = false
		plan.Reason = "subscription is already terminal"
	}
	return plan, nil
}

// GracePeriod describes an access window after a failed or late payment.
type GracePeriod struct {
	StartsAt time.Time
	EndsAt   time.Time
}

// NewGracePeriod builds a grace period from a start time and duration.
func NewGracePeriod(startsAt time.Time, duration time.Duration) (GracePeriod, error) {
	if duration < 0 {
		return GracePeriod{}, fmt.Errorf("%w: grace duration must be non-negative", ErrPaymentLifecycleInvalid)
	}
	period := GracePeriod{
		StartsAt: startsAt,
		EndsAt:   startsAt.Add(duration),
	}
	if err := period.Validate(); err != nil {
		return GracePeriod{}, err
	}
	return period, nil
}

// Validate checks that the grace period has a usable time range.
func (g GracePeriod) Validate() error {
	if g.StartsAt.IsZero() {
		return fmt.Errorf("%w: grace starts_at must be set", ErrPaymentLifecycleInvalid)
	}
	if g.EndsAt.IsZero() {
		return fmt.Errorf("%w: grace ends_at must be set", ErrPaymentLifecycleInvalid)
	}
	if !g.StartsAt.Before(g.EndsAt) {
		return fmt.Errorf("%w: grace starts_at must be before ends_at", ErrPaymentLifecycleInvalid)
	}
	return nil
}

// ActiveAt reports whether at is inside the grace period. The end boundary is
// exclusive.
func (g GracePeriod) ActiveAt(at time.Time) bool {
	if at.IsZero() || g.Validate() != nil {
		return false
	}
	return !at.Before(g.StartsAt) && at.Before(g.EndsAt)
}

// ExpiredAt reports whether at is at or after the grace period end.
func (g GracePeriod) ExpiredAt(at time.Time) bool {
	if at.IsZero() || g.Validate() != nil {
		return false
	}
	return !at.Before(g.EndsAt)
}

// Remaining returns the remaining grace duration at the supplied time.
func (g GracePeriod) Remaining(at time.Time) time.Duration {
	if at.IsZero() || g.Validate() != nil || !at.Before(g.EndsAt) {
		return 0
	}
	if at.Before(g.StartsAt) {
		return g.EndsAt.Sub(g.StartsAt)
	}
	return g.EndsAt.Sub(at)
}

// RefundPlanRequest describes a provider-neutral refund dry run.
type RefundPlanRequest struct {
	PaymentID       string
	PaidAmount      Money
	RefundedAmount  Money
	RequestedAmount Money
	PaidAt          time.Time
	RequestedAt     time.Time
	RefundWindow    time.Duration
	Reason          string
}

// Validate checks that the refund request can be evaluated.
func (r RefundPlanRequest) Validate() error {
	paymentID := strings.TrimSpace(r.PaymentID)
	if paymentID == "" {
		return fmt.Errorf("%w: payment_id must be non-empty", ErrPaymentLifecycleInvalid)
	}

	paid := normalizeLifecycleMoney(r.PaidAmount, "")
	if err := validateLifecycleMoney("paid_amount", paid, true); err != nil {
		return err
	}
	refunded := normalizeLifecycleMoney(r.RefundedAmount, paid.Currency)
	if err := validateLifecycleMoney("refunded_amount", refunded, false); err != nil {
		return err
	}
	requested := normalizeLifecycleMoney(r.RequestedAmount, paid.Currency)
	if err := validateLifecycleMoney("requested_amount", requested, false); err != nil {
		return err
	}
	if refunded.Currency != paid.Currency {
		return fmt.Errorf("%w: refunded_amount currency must match paid_amount currency", ErrPaymentLifecycleInvalid)
	}
	if requested.Currency != paid.Currency {
		return fmt.Errorf("%w: requested_amount currency must match paid_amount currency", ErrPaymentLifecycleInvalid)
	}
	if refunded.Amount > paid.Amount {
		return fmt.Errorf("%w: refunded_amount must not exceed paid_amount", ErrPaymentLifecycleInvalid)
	}
	if r.RequestedAt.IsZero() {
		return fmt.Errorf("%w: requested_at must be set", ErrPaymentLifecycleInvalid)
	}
	if r.RefundWindow < 0 {
		return fmt.Errorf("%w: refund_window must be non-negative", ErrPaymentLifecycleInvalid)
	}
	if r.RefundWindow > 0 && r.PaidAt.IsZero() {
		return fmt.Errorf("%w: paid_at must be set when refund_window is set", ErrPaymentLifecycleInvalid)
	}
	return nil
}

// RefundPlan is the dry-run result for a provider-neutral refund decision.
type RefundPlan struct {
	Allowed     bool
	PaymentID   string
	Amount      Money
	Remaining   Money
	Full        bool
	RequestedAt time.Time
	Reason      string
}

// Validate returns nil when the refund plan may be applied.
func (p RefundPlan) Validate() error {
	if !p.Allowed {
		return fmt.Errorf("%w: %s", ErrPaymentLifecycleBlocked, lifecycleReason(p.Reason))
	}
	if strings.TrimSpace(p.PaymentID) == "" {
		return fmt.Errorf("%w: payment_id must be non-empty", ErrPaymentLifecycleInvalid)
	}
	amount := normalizeLifecycleMoney(p.Amount, "")
	if err := validateLifecycleMoney("amount", amount, true); err != nil {
		return err
	}
	remaining := normalizeLifecycleMoney(p.Remaining, amount.Currency)
	if err := validateLifecycleMoney("remaining", remaining, false); err != nil {
		return err
	}
	if amount.Currency != remaining.Currency {
		return fmt.Errorf("%w: amount currency must match remaining currency", ErrPaymentLifecycleInvalid)
	}
	if amount.Amount > remaining.Amount {
		return fmt.Errorf("%w: amount must not exceed remaining", ErrPaymentLifecycleInvalid)
	}
	if p.RequestedAt.IsZero() {
		return fmt.Errorf("%w: requested_at must be set", ErrPaymentLifecycleInvalid)
	}
	return nil
}

// BuildRefundPlan evaluates whether a refund amount can be issued without
// mutating provider state. A zero RequestedAmount means refund the remaining
// paid amount.
func BuildRefundPlan(req RefundPlanRequest) (RefundPlan, error) {
	if err := req.Validate(); err != nil {
		return RefundPlan{}, err
	}

	paid := normalizeLifecycleMoney(req.PaidAmount, "")
	refunded := normalizeLifecycleMoney(req.RefundedAmount, paid.Currency)
	requested := normalizeLifecycleMoney(req.RequestedAmount, paid.Currency)
	remaining := Money{
		Amount:   paid.Amount - refunded.Amount,
		Currency: paid.Currency,
	}
	amount := requested
	if amount.Amount == 0 {
		amount = remaining
	}

	plan := RefundPlan{
		Allowed:     true,
		PaymentID:   strings.TrimSpace(req.PaymentID),
		Amount:      amount,
		Remaining:   remaining,
		Full:        amount.Amount == remaining.Amount,
		RequestedAt: req.RequestedAt,
		Reason:      lifecycleReason(req.Reason),
	}
	if remaining.Amount == 0 {
		plan.Allowed = false
		plan.Reason = "payment is already fully refunded"
		return plan, nil
	}
	if amount.Amount > remaining.Amount {
		plan.Allowed = false
		plan.Reason = "refund amount exceeds remaining paid amount"
		return plan, nil
	}
	if req.RefundWindow > 0 && req.RequestedAt.After(req.PaidAt.Add(req.RefundWindow)) {
		plan.Allowed = false
		plan.Reason = "refund window expired"
	}
	return plan, nil
}

// DunningAction names a provider-neutral action for overdue payment recovery.
type DunningAction string

const (
	DunningActionNone             DunningAction = "none"
	DunningActionNotify           DunningAction = "notify"
	DunningActionRetryPayment     DunningAction = "retry_payment"
	DunningActionEnterGracePeriod DunningAction = "enter_grace_period"
	DunningActionSuspend          DunningAction = "suspend"
	DunningActionCancel           DunningAction = "cancel"
)

// Valid reports whether the dunning action is known.
func (a DunningAction) Valid() bool {
	switch a {
	case DunningActionNone,
		DunningActionNotify,
		DunningActionRetryPayment,
		DunningActionEnterGracePeriod,
		DunningActionSuspend,
		DunningActionCancel:
		return true
	default:
		return false
	}
}

// DunningStep schedules one action after a payment failure.
type DunningStep struct {
	After  time.Duration
	Action DunningAction
	Reason string
}

// DunningPolicy is an ordered provider-neutral recovery schedule.
type DunningPolicy struct {
	Steps []DunningStep
}

// Validate checks that the dunning schedule has deterministic ordering.
func (p DunningPolicy) Validate() error {
	return ValidateDunningPolicy(p)
}

// ValidateDunningPolicy checks that steps are strictly ordered and actionable.
func ValidateDunningPolicy(policy DunningPolicy) error {
	var previous time.Duration
	for i, step := range policy.Steps {
		if step.After < 0 {
			return fmt.Errorf("%w: dunning step %d after must be non-negative", ErrPaymentLifecycleInvalid, i)
		}
		if i > 0 && step.After <= previous {
			return fmt.Errorf("%w: dunning step %d must be after previous step", ErrPaymentLifecycleInvalid, i)
		}
		if !step.Action.Valid() || step.Action == DunningActionNone {
			return fmt.Errorf("%w: dunning step %d action %q is not actionable", ErrPaymentLifecycleInvalid, i, step.Action)
		}
		previous = step.After
	}
	return nil
}

// DunningPlan is the dry-run result for an overdue payment recovery schedule.
type DunningPlan struct {
	Action     DunningAction
	StepIndex  int
	FailedAt   time.Time
	DueAt      time.Time
	NextAction DunningAction
	NextDueAt  time.Time
	Reason     string
}

// Validate checks that the planned dunning action is structurally usable.
func (p DunningPlan) Validate() error {
	if !p.Action.Valid() {
		return fmt.Errorf("%w: dunning action %q is unknown", ErrPaymentLifecycleInvalid, p.Action)
	}
	if p.Action != DunningActionNone && p.DueAt.IsZero() {
		return fmt.Errorf("%w: due_at must be set for actionable dunning plans", ErrPaymentLifecycleInvalid)
	}
	if p.NextAction != "" && !p.NextAction.Valid() {
		return fmt.Errorf("%w: next dunning action %q is unknown", ErrPaymentLifecycleInvalid, p.NextAction)
	}
	if p.FailedAt.IsZero() {
		return fmt.Errorf("%w: failed_at must be set", ErrPaymentLifecycleInvalid)
	}
	return nil
}

// BuildDunningPlan selects the latest due action for a failed payment and
// reports the next scheduled action, if any.
func BuildDunningPlan(policy DunningPolicy, failedAt, now time.Time) (DunningPlan, error) {
	if err := policy.Validate(); err != nil {
		return DunningPlan{}, err
	}
	if failedAt.IsZero() {
		return DunningPlan{}, fmt.Errorf("%w: failed_at must be set", ErrPaymentLifecycleInvalid)
	}
	if now.IsZero() {
		return DunningPlan{}, fmt.Errorf("%w: now must be set", ErrPaymentLifecycleInvalid)
	}
	if now.Before(failedAt) {
		return DunningPlan{}, fmt.Errorf("%w: now must not be before failed_at", ErrPaymentLifecycleInvalid)
	}

	plan := DunningPlan{
		Action:     DunningActionNone,
		StepIndex:  -1,
		FailedAt:   failedAt,
		NextAction: DunningActionNone,
		Reason:     "no action due",
	}
	for i, step := range policy.Steps {
		dueAt := failedAt.Add(step.After)
		if now.Before(dueAt) {
			plan.NextAction = step.Action
			plan.NextDueAt = dueAt
			return plan, nil
		}
		plan.Action = step.Action
		plan.StepIndex = i
		plan.DueAt = dueAt
		plan.NextAction = DunningActionNone
		plan.NextDueAt = time.Time{}
		plan.Reason = lifecycleReason(step.Reason)
	}
	return plan, nil
}

func subscriptionStatusBlocksPlanChange(status SubscriptionStatus) bool {
	switch status {
	case SubscriptionStatusCanceled, SubscriptionStatusSuspended, SubscriptionStatusUnpaid:
		return true
	default:
		return false
	}
}

func normalizeLifecycleToken(value string) string {
	return strings.TrimSpace(value)
}

func normalizeLifecycleMoney(money Money, fallbackCurrency string) Money {
	money.Currency = strings.ToUpper(strings.TrimSpace(money.Currency))
	if money.Currency == "" {
		money.Currency = strings.ToUpper(strings.TrimSpace(fallbackCurrency))
	}
	return money
}

func validateLifecycleMoney(label string, money Money, positive bool) error {
	if positive {
		if money.Amount <= 0 {
			return fmt.Errorf("%w: %s amount must be positive", ErrPaymentLifecycleInvalid, label)
		}
	} else if money.Amount < 0 {
		return fmt.Errorf("%w: %s amount must be non-negative", ErrPaymentLifecycleInvalid, label)
	}
	if money.Currency == "" {
		return fmt.Errorf("%w: %s currency must be non-empty", ErrPaymentLifecycleInvalid, label)
	}
	return nil
}

func lifecycleReason(reason string) string {
	reason = strings.TrimSpace(reason)
	if reason == "" {
		return "lifecycle policy"
	}
	return reason
}
