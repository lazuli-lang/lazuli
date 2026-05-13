package billing_test

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/billing"
)

func TestSubscriptionStatusHelpers(t *testing.T) {
	t.Parallel()

	if !billing.SubscriptionStatusTrialing.Active() {
		t.Fatal("trialing subscription should be active")
	}
	if !billing.SubscriptionStatusActive.Active() {
		t.Fatal("active subscription should be active")
	}
	if !billing.SubscriptionStatusPastDue.Active() {
		t.Fatal("past_due subscription should be active during grace evaluation")
	}
	if billing.SubscriptionStatusPaused.Active() {
		t.Fatal("paused subscription should not be active")
	}
	if !billing.SubscriptionStatusCanceled.Terminal() {
		t.Fatal("canceled subscription should be terminal")
	}
	if got := billing.SubscriptionStatus("provider_only").String(); got != "unknown" {
		t.Fatalf("String() = %q, want unknown", got)
	}
}

func TestSubscriptionLifecycleHelpers(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	subscription := billing.Subscription{
		ID:                 "sub_test",
		Customer:           "customer-a",
		PlanKey:            "pro",
		Status:             billing.SubscriptionStatusTrialing,
		CurrentPeriodStart: now.Add(-24 * time.Hour),
		CurrentPeriodEnd:   now.Add(24 * time.Hour),
		TrialStart:         now.Add(-time.Hour),
		TrialEnd:           now.Add(time.Hour),
		CancelAt:           now.Add(48 * time.Hour),
		GracePeriodEnd:     now.Add(72 * time.Hour),
	}

	if err := subscription.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
	if !subscription.TrialActive(now) {
		t.Fatal("TrialActive() = false, want true")
	}
	if !subscription.CancelScheduled(now) {
		t.Fatal("CancelScheduled() = false, want true")
	}
	if !subscription.EntitledAt(now) {
		t.Fatal("EntitledAt() = false, want true inside current period")
	}
	if !subscription.GracePeriodActive(now.Add(48 * time.Hour)) {
		t.Fatal("GracePeriodActive() = false, want true after current period before grace end")
	}
	if !subscription.EntitledAt(now.Add(48 * time.Hour)) {
		t.Fatal("EntitledAt() = false, want true during grace period")
	}
	if subscription.EntitledAt(now.Add(96 * time.Hour)) {
		t.Fatal("EntitledAt() = true, want false after grace period")
	}

	subscription.CanceledAt = now.Add(time.Hour)
	if subscription.CanceledAtTime(now.Add(2*time.Hour)) != true {
		t.Fatal("CanceledAtTime() = false, want true after canceled_at")
	}
	if subscription.EntitledAt(now.Add(2 * time.Hour)) {
		t.Fatal("EntitledAt() = true, want false after cancellation")
	}
}

func TestSubscriptionValidationRejectsInvalidDefinitions(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	cases := []struct {
		name         string
		subscription billing.Subscription
	}{
		{
			name:         "missing plan",
			subscription: billing.Subscription{Status: billing.SubscriptionStatusActive},
		},
		{
			name: "unknown status",
			subscription: billing.Subscription{
				PlanKey: "pro",
				Status:  billing.SubscriptionStatusUnknown,
			},
		},
		{
			name: "invalid current period",
			subscription: billing.Subscription{
				PlanKey:            "pro",
				Status:             billing.SubscriptionStatusActive,
				CurrentPeriodStart: now,
				CurrentPeriodEnd:   now,
			},
		},
		{
			name: "invalid grace period",
			subscription: billing.Subscription{
				PlanKey:            "pro",
				Status:             billing.SubscriptionStatusPastDue,
				CurrentPeriodStart: now.Add(-time.Hour),
				CurrentPeriodEnd:   now,
				GracePeriodEnd:     now,
			},
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := tc.subscription.Validate()
			if !errors.Is(err, billing.ErrSubscriptionInvalid) {
				t.Fatalf("Validate() error = %v, want ErrSubscriptionInvalid", err)
			}
		})
	}
}

func TestNextRenewal(t *testing.T) {
	t.Parallel()

	start := time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC)
	interval := billing.RenewalInterval{Unit: "month", Count: 1}

	next, err := billing.NextRenewal(start, time.Date(2026, 3, 15, 0, 0, 0, 0, time.UTC), interval)
	if err != nil {
		t.Fatalf("NextRenewal() error = %v", err)
	}
	want := time.Date(2026, 4, 1, 0, 0, 0, 0, time.UTC)
	if !next.Equal(want) {
		t.Fatalf("NextRenewal() = %s, want %s", next, want)
	}

	subscription := billing.Subscription{
		PlanKey:            "pro",
		Status:             billing.SubscriptionStatusActive,
		CurrentPeriodStart: start,
		CurrentPeriodEnd:   time.Date(2026, 2, 1, 0, 0, 0, 0, time.UTC),
	}
	next, err = subscription.NextRenewal(time.Date(2026, 2, 1, 0, 0, 0, 0, time.UTC), interval)
	if err != nil {
		t.Fatalf("Subscription.NextRenewal() error = %v", err)
	}
	want = time.Date(2026, 3, 1, 0, 0, 0, 0, time.UTC)
	if !next.Equal(want) {
		t.Fatalf("Subscription.NextRenewal() = %s, want %s", next, want)
	}

	_, err = billing.NextRenewal(time.Time{}, start, interval)
	if !errors.Is(err, billing.ErrSubscriptionInvalid) {
		t.Fatalf("NextRenewal() error = %v, want ErrSubscriptionInvalid", err)
	}
}

func TestPreviewPlanChange(t *testing.T) {
	t.Parallel()

	start := time.Date(2026, 5, 1, 0, 0, 0, 0, time.UTC)
	end := time.Date(2026, 5, 31, 0, 0, 0, 0, time.UTC)
	effective := time.Date(2026, 5, 16, 0, 0, 0, 0, time.UTC)
	interval := billing.RenewalInterval{Unit: "months", Count: 1}

	preview, err := billing.PreviewPlanChange(billing.PlanChangeRequest{
		CurrentPlan: billing.PlanRate{PlanKey: "basic", Amount: 3000, Currency: " brl ", Interval: interval},
		NewPlan:     billing.PlanRate{PlanKey: "pro", Amount: 9000, Currency: "BRL", Interval: interval},
		PeriodStart: start,
		PeriodEnd:   end,
		EffectiveAt: effective,
	})
	if err != nil {
		t.Fatalf("PreviewPlanChange() error = %v", err)
	}
	if preview.FromPlanKey != "basic" || preview.ToPlanKey != "pro" {
		t.Fatalf("preview plan keys = %q -> %q, want basic -> pro", preview.FromPlanKey, preview.ToPlanKey)
	}
	if preview.Currency != "BRL" {
		t.Fatalf("Currency = %q, want BRL", preview.Currency)
	}
	if preview.ProrationCredit != 1500 || preview.ProrationCharge != 4500 || preview.AmountDue != 3000 {
		t.Fatalf("preview = %#v, want 1500 credit, 4500 charge, 3000 due", preview)
	}
}

func TestPreviewPlanChangeRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	start := time.Date(2026, 5, 1, 0, 0, 0, 0, time.UTC)
	end := time.Date(2026, 5, 31, 0, 0, 0, 0, time.UTC)
	monthly := billing.RenewalInterval{Unit: "month", Count: 1}

	_, err := billing.PreviewPlanChange(billing.PlanChangeRequest{
		CurrentPlan: billing.PlanRate{PlanKey: "basic", Amount: 3000, Currency: "BRL", Interval: monthly},
		NewPlan:     billing.PlanRate{PlanKey: "pro", Amount: 9000, Currency: "USD", Interval: monthly},
		PeriodStart: start,
		PeriodEnd:   end,
		EffectiveAt: start.Add(24 * time.Hour),
	})
	if !errors.Is(err, billing.ErrSubscriptionInvalid) {
		t.Fatalf("PreviewPlanChange() currency error = %v, want ErrSubscriptionInvalid", err)
	}

	_, err = billing.PreviewPlanChange(billing.PlanChangeRequest{
		CurrentPlan: billing.PlanRate{PlanKey: "basic", Amount: 3000, Currency: "BRL", Interval: monthly},
		NewPlan:     billing.PlanRate{PlanKey: "pro", Amount: 9000, Currency: "BRL", Interval: monthly},
		PeriodStart: start,
		PeriodEnd:   end,
		EffectiveAt: start.Add(-time.Hour),
	})
	if !errors.Is(err, billing.ErrSubscriptionInvalid) {
		t.Fatalf("PreviewPlanChange() effective_at error = %v, want ErrSubscriptionInvalid", err)
	}
}
