package payments_test

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/payments"
)

func TestBuildPlanChange(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 10, 0, 0, 0, time.UTC)
	periodEnd := now.Add(30 * 24 * time.Hour)

	immediate, err := payments.BuildPlanChange(payments.PlanChangeRequest{
		CurrentPlan: "starter",
		TargetPlan:  "pro",
		Status:      payments.SubscriptionStatusActive,
		Mode:        payments.PlanChangeModeImmediate,
		RequestedAt: now,
	})
	if err != nil {
		t.Fatalf("BuildPlanChange(immediate) error = %v", err)
	}
	if !immediate.Allowed || !immediate.EffectiveAt.Equal(now) {
		t.Fatalf("immediate change = %+v", immediate)
	}
	if err := immediate.Validate(); err != nil {
		t.Fatalf("immediate Validate() error = %v", err)
	}

	periodEndChange, err := payments.BuildPlanChange(payments.PlanChangeRequest{
		CurrentPlan:      "starter",
		TargetPlan:       "enterprise",
		Status:           payments.SubscriptionStatusPastDue,
		Mode:             payments.PlanChangeModePeriodEnd,
		RequestedAt:      now,
		CurrentPeriodEnd: periodEnd,
	})
	if err != nil {
		t.Fatalf("BuildPlanChange(period end) error = %v", err)
	}
	if !periodEndChange.Allowed || !periodEndChange.EffectiveAt.Equal(periodEnd) {
		t.Fatalf("period end change = %+v", periodEndChange)
	}

	blocked, err := payments.BuildPlanChange(payments.PlanChangeRequest{
		CurrentPlan: "starter",
		TargetPlan:  "pro",
		Status:      payments.SubscriptionStatusSuspended,
		Mode:        payments.PlanChangeModeImmediate,
		RequestedAt: now,
	})
	if err != nil {
		t.Fatalf("BuildPlanChange(blocked) error = %v", err)
	}
	if blocked.Allowed {
		t.Fatalf("blocked change was allowed: %+v", blocked)
	}
	if err := blocked.Validate(); !errors.Is(err, payments.ErrPaymentLifecycleBlocked) {
		t.Fatalf("blocked Validate() error = %v, want ErrPaymentLifecycleBlocked", err)
	}
}

func TestBuildPlanChangeRejectsInvalidRequests(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 10, 0, 0, 0, time.UTC)
	cases := []payments.PlanChangeRequest{
		{
			CurrentPlan: "starter",
			TargetPlan:  "starter",
			Status:      payments.SubscriptionStatusActive,
			Mode:        payments.PlanChangeModeImmediate,
			RequestedAt: now,
		},
		{
			CurrentPlan: "starter",
			TargetPlan:  "pro",
			Status:      payments.SubscriptionStatusUnknown,
			Mode:        payments.PlanChangeModeImmediate,
			RequestedAt: now,
		},
		{
			CurrentPlan: "starter",
			TargetPlan:  "pro",
			Status:      payments.SubscriptionStatusActive,
			Mode:        payments.PlanChangeModePeriodEnd,
			RequestedAt: now,
		},
	}

	for _, tc := range cases {
		if _, err := payments.BuildPlanChange(tc); !errors.Is(err, payments.ErrPaymentLifecycleInvalid) {
			t.Fatalf("BuildPlanChange(%+v) error = %v, want ErrPaymentLifecycleInvalid", tc, err)
		}
	}
}

func TestBuildCancellationPlan(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 10, 0, 0, 0, time.UTC)
	periodEnd := now.Add(14 * 24 * time.Hour)

	immediate, err := payments.BuildCancellationPlan(payments.CancellationRequest{
		Status:      payments.SubscriptionStatusActive,
		Mode:        payments.CancellationModeImmediate,
		RequestedAt: now,
	})
	if err != nil {
		t.Fatalf("BuildCancellationPlan(immediate) error = %v", err)
	}
	if !immediate.Allowed || !immediate.CancelAt.Equal(now) || !immediate.AccessEndsAt.Equal(now) {
		t.Fatalf("immediate cancellation = %+v", immediate)
	}

	periodEndPlan, err := payments.BuildCancellationPlan(payments.CancellationRequest{
		Status:           payments.SubscriptionStatusGracePeriod,
		Mode:             payments.CancellationModePeriodEnd,
		RequestedAt:      now,
		CurrentPeriodEnd: periodEnd,
	})
	if err != nil {
		t.Fatalf("BuildCancellationPlan(period end) error = %v", err)
	}
	if !periodEndPlan.Allowed || !periodEndPlan.CancelAt.Equal(periodEnd) {
		t.Fatalf("period end cancellation = %+v", periodEndPlan)
	}

	blocked, err := payments.BuildCancellationPlan(payments.CancellationRequest{
		Status:      payments.SubscriptionStatusCanceled,
		Mode:        payments.CancellationModeImmediate,
		RequestedAt: now,
	})
	if err != nil {
		t.Fatalf("BuildCancellationPlan(blocked) error = %v", err)
	}
	if blocked.Allowed {
		t.Fatalf("blocked cancellation was allowed: %+v", blocked)
	}
	if err := blocked.Validate(); !errors.Is(err, payments.ErrPaymentLifecycleBlocked) {
		t.Fatalf("blocked Validate() error = %v, want ErrPaymentLifecycleBlocked", err)
	}
}

func TestGracePeriod(t *testing.T) {
	t.Parallel()

	start := time.Date(2026, 5, 12, 10, 0, 0, 0, time.UTC)
	period, err := payments.NewGracePeriod(start, 72*time.Hour)
	if err != nil {
		t.Fatalf("NewGracePeriod() error = %v", err)
	}

	if !period.ActiveAt(start) {
		t.Fatal("grace period should be active at start")
	}
	if !period.ActiveAt(start.Add(71 * time.Hour)) {
		t.Fatal("grace period should be active before end")
	}
	if period.ActiveAt(period.EndsAt) {
		t.Fatal("grace period end should be exclusive")
	}
	if !period.ExpiredAt(period.EndsAt) {
		t.Fatal("grace period should expire at end")
	}
	if got := period.Remaining(start.Add(24 * time.Hour)); got != 48*time.Hour {
		t.Fatalf("Remaining() = %v, want 48h", got)
	}
	if _, err := payments.NewGracePeriod(start, 0); !errors.Is(err, payments.ErrPaymentLifecycleInvalid) {
		t.Fatalf("NewGracePeriod(zero) error = %v, want ErrPaymentLifecycleInvalid", err)
	}
}

func TestBuildRefundPlan(t *testing.T) {
	t.Parallel()

	paidAt := time.Date(2026, 5, 1, 10, 0, 0, 0, time.UTC)
	requestedAt := paidAt.Add(24 * time.Hour)
	base := payments.RefundPlanRequest{
		PaymentID:      "pay_123",
		PaidAmount:     payments.Money{Amount: 10000, Currency: "brl"},
		RefundedAmount: payments.Money{Amount: 2500},
		PaidAt:         paidAt,
		RequestedAt:    requestedAt,
		RefundWindow:   30 * 24 * time.Hour,
	}

	full, err := payments.BuildRefundPlan(base)
	if err != nil {
		t.Fatalf("BuildRefundPlan(full remaining) error = %v", err)
	}
	if !full.Allowed || !full.Full || full.Amount != (payments.Money{Amount: 7500, Currency: "BRL"}) {
		t.Fatalf("full refund plan = %+v", full)
	}
	if err := full.Validate(); err != nil {
		t.Fatalf("full Validate() error = %v", err)
	}

	partialReq := base
	partialReq.RequestedAmount = payments.Money{Amount: 3000}
	partial, err := payments.BuildRefundPlan(partialReq)
	if err != nil {
		t.Fatalf("BuildRefundPlan(partial) error = %v", err)
	}
	if !partial.Allowed || partial.Full || partial.Amount.Amount != 3000 {
		t.Fatalf("partial refund plan = %+v", partial)
	}

	tooMuchReq := base
	tooMuchReq.RequestedAmount = payments.Money{Amount: 9000}
	tooMuch, err := payments.BuildRefundPlan(tooMuchReq)
	if err != nil {
		t.Fatalf("BuildRefundPlan(too much) error = %v", err)
	}
	if tooMuch.Allowed {
		t.Fatalf("oversized refund was allowed: %+v", tooMuch)
	}
	if err := tooMuch.Validate(); !errors.Is(err, payments.ErrPaymentLifecycleBlocked) {
		t.Fatalf("oversized Validate() error = %v, want ErrPaymentLifecycleBlocked", err)
	}

	expiredReq := base
	expiredReq.RequestedAt = paidAt.Add(31 * 24 * time.Hour)
	expired, err := payments.BuildRefundPlan(expiredReq)
	if err != nil {
		t.Fatalf("BuildRefundPlan(expired) error = %v", err)
	}
	if expired.Allowed {
		t.Fatalf("expired refund was allowed: %+v", expired)
	}
}

func TestBuildRefundPlanRejectsInvalidRequests(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 10, 0, 0, 0, time.UTC)
	cases := []payments.RefundPlanRequest{
		{
			PaidAmount:  payments.Money{Amount: 1000, Currency: "BRL"},
			RequestedAt: now,
		},
		{
			PaymentID:   "pay_123",
			PaidAmount:  payments.Money{Amount: 0, Currency: "BRL"},
			RequestedAt: now,
		},
		{
			PaymentID:       "pay_123",
			PaidAmount:      payments.Money{Amount: 1000, Currency: "BRL"},
			RequestedAmount: payments.Money{Amount: 100, Currency: "USD"},
			RequestedAt:     now,
		},
	}

	for _, tc := range cases {
		if _, err := payments.BuildRefundPlan(tc); !errors.Is(err, payments.ErrPaymentLifecycleInvalid) {
			t.Fatalf("BuildRefundPlan(%+v) error = %v, want ErrPaymentLifecycleInvalid", tc, err)
		}
	}
}

func TestBuildDunningPlan(t *testing.T) {
	t.Parallel()

	failedAt := time.Date(2026, 5, 12, 10, 0, 0, 0, time.UTC)
	policy := payments.DunningPolicy{Steps: []payments.DunningStep{
		{After: 0, Action: payments.DunningActionNotify, Reason: "send first notice"},
		{After: 24 * time.Hour, Action: payments.DunningActionRetryPayment, Reason: "retry collection"},
		{After: 72 * time.Hour, Action: payments.DunningActionSuspend, Reason: "suspend access"},
	}}

	notDue, err := payments.BuildDunningPlan(policy, failedAt, failedAt.Add(-time.Nanosecond))
	if !errors.Is(err, payments.ErrPaymentLifecycleInvalid) {
		t.Fatalf("BuildDunningPlan(before failure) = %+v, %v; want ErrPaymentLifecycleInvalid", notDue, err)
	}

	first, err := payments.BuildDunningPlan(policy, failedAt, failedAt)
	if err != nil {
		t.Fatalf("BuildDunningPlan(first) error = %v", err)
	}
	if first.Action != payments.DunningActionNotify || first.NextAction != payments.DunningActionRetryPayment {
		t.Fatalf("first dunning plan = %+v", first)
	}
	if err := first.Validate(); err != nil {
		t.Fatalf("first Validate() error = %v", err)
	}

	next, err := payments.BuildDunningPlan(policy, failedAt, failedAt.Add(12*time.Hour))
	if err != nil {
		t.Fatalf("BuildDunningPlan(next) error = %v", err)
	}
	if next.Action != payments.DunningActionNotify || !next.NextDueAt.Equal(failedAt.Add(24*time.Hour)) {
		t.Fatalf("next dunning plan = %+v", next)
	}

	last, err := payments.BuildDunningPlan(policy, failedAt, failedAt.Add(96*time.Hour))
	if err != nil {
		t.Fatalf("BuildDunningPlan(last) error = %v", err)
	}
	if last.Action != payments.DunningActionSuspend || last.NextAction != payments.DunningActionNone {
		t.Fatalf("last dunning plan = %+v", last)
	}
}

func TestBuildDunningPlanHandlesNoDueStep(t *testing.T) {
	t.Parallel()

	failedAt := time.Date(2026, 5, 12, 10, 0, 0, 0, time.UTC)
	policy := payments.DunningPolicy{Steps: []payments.DunningStep{
		{After: time.Hour, Action: payments.DunningActionNotify},
	}}

	plan, err := payments.BuildDunningPlan(policy, failedAt, failedAt.Add(30*time.Minute))
	if err != nil {
		t.Fatalf("BuildDunningPlan(no due) error = %v", err)
	}
	if plan.Action != payments.DunningActionNone || plan.StepIndex != -1 || plan.NextAction != payments.DunningActionNotify {
		t.Fatalf("no due plan = %+v", plan)
	}
}

func TestValidateDunningPolicyRejectsAmbiguousSteps(t *testing.T) {
	t.Parallel()

	policy := payments.DunningPolicy{Steps: []payments.DunningStep{
		{After: time.Hour, Action: payments.DunningActionNotify},
		{After: time.Hour, Action: payments.DunningActionRetryPayment},
	}}

	if err := policy.Validate(); !errors.Is(err, payments.ErrPaymentLifecycleInvalid) {
		t.Fatalf("Validate() error = %v, want ErrPaymentLifecycleInvalid", err)
	}
}
