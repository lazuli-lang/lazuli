package billing_test

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/billing"
)

func TestPlanEntitlementHelpers(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	plan := billing.Plan{
		Key:    "pro",
		Name:   "Pro",
		Status: billing.PlanStatusActive,
		Entitlements: []billing.Entitlement{
			billing.FeatureEntitlement("projects"),
			billing.LimitedEntitlement("seats", 10),
			{
				Feature:  "scheduled_reports",
				Enabled:  true,
				Limit:    5,
				StartsAt: now.Add(-time.Hour),
				EndsAt:   now.Add(time.Hour),
			},
		},
	}

	if err := plan.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
	if !plan.Active() {
		t.Fatal("Active() = false, want true")
	}
	if !plan.Allows(" projects ", time.Time{}) {
		t.Fatal("Allows() = false, want true for normalized unlimited feature")
	}
	if !plan.Allows("scheduled_reports", now) {
		t.Fatal("Allows() = false, want true inside entitlement window")
	}
	if plan.Allows("scheduled_reports", now.Add(2*time.Hour)) {
		t.Fatal("Allows() = true, want false outside entitlement window")
	}

	entitlement, ok := plan.Entitlement(" seats ")
	if !ok {
		t.Fatal("Entitlement() did not find normalized feature")
	}
	if entitlement.Feature != "seats" || entitlement.Limit != 10 || entitlement.Unlimited {
		t.Fatalf("entitlement = %#v, want limited seats entitlement", entitlement)
	}
}

func TestCheckPlanUsageEvaluatesLimits(t *testing.T) {
	t.Parallel()

	plan := billing.Plan{
		Key:    "pro",
		Status: billing.PlanStatusActive,
		Entitlements: []billing.Entitlement{
			billing.LimitedEntitlement("seats", 10),
			billing.FeatureEntitlement("projects"),
		},
	}

	check, err := billing.CheckPlanUsage(plan, "seats", 8, 2, time.Time{})
	if err != nil {
		t.Fatalf("CheckPlanUsage() error = %v", err)
	}
	if !check.Allowed || check.AfterUsage != 10 || check.Reason != "within limit" {
		t.Fatalf("check = %#v, want allowed within limit", check)
	}
	if err := check.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	check, err = billing.CheckPlanUsage(plan, "seats", 8, 3, time.Time{})
	if err != nil {
		t.Fatalf("CheckPlanUsage() over limit error = %v", err)
	}
	if check.Allowed || check.AfterUsage != 11 || check.Reason != "limit exceeded" {
		t.Fatalf("check = %#v, want blocked limit exceeded", check)
	}
	if err := check.Validate(); !errors.Is(err, billing.ErrEntitlementLimitExceeded) {
		t.Fatalf("Validate() error = %v, want ErrEntitlementLimitExceeded", err)
	}

	check, err = billing.CheckPlanUsage(plan, "seats", 12, -1, time.Time{})
	if err != nil {
		t.Fatalf("CheckPlanUsage() reduction error = %v", err)
	}
	if !check.Allowed || check.AfterUsage != 11 || check.Reason != "limit exceeded" {
		t.Fatalf("check = %#v, want allowed usage reduction over limit", check)
	}

	check, err = billing.CheckPlanUsage(plan, "projects", 500, 50, time.Time{})
	if err != nil {
		t.Fatalf("CheckPlanUsage() unlimited error = %v", err)
	}
	if !check.Allowed || !check.Unlimited || check.Reason != "unlimited" {
		t.Fatalf("check = %#v, want unlimited allowed", check)
	}
}

func TestCheckPlanUsageDeniesInactiveOrMissingEntitlement(t *testing.T) {
	t.Parallel()

	plan := billing.Plan{
		Key:    "draft",
		Status: billing.PlanStatusDraft,
		Entitlements: []billing.Entitlement{
			billing.FeatureEntitlement("projects"),
		},
	}

	check, err := billing.CheckPlanUsage(plan, "projects", 0, 1, time.Time{})
	if err != nil {
		t.Fatalf("CheckPlanUsage() error = %v", err)
	}
	if check.Allowed || check.Reason != "plan inactive" {
		t.Fatalf("check = %#v, want inactive plan denial", check)
	}
	if err := check.Validate(); !errors.Is(err, billing.ErrEntitlementDenied) {
		t.Fatalf("Validate() error = %v, want ErrEntitlementDenied", err)
	}

	plan.Status = billing.PlanStatusActive
	check, err = billing.CheckPlanUsage(plan, "seats", 0, 1, time.Time{})
	if err != nil {
		t.Fatalf("CheckPlanUsage() missing entitlement error = %v", err)
	}
	if check.Allowed || check.Reason != "entitlement missing" {
		t.Fatalf("check = %#v, want missing entitlement denial", check)
	}
}

func TestPlanValidationRejectsInvalidDefinitions(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name string
		plan billing.Plan
	}{
		{
			name: "missing key",
			plan: billing.Plan{Status: billing.PlanStatusActive},
		},
		{
			name: "unknown status",
			plan: billing.Plan{Key: "pro", Status: billing.PlanStatusUnknown},
		},
		{
			name: "duplicate normalized feature",
			plan: billing.Plan{
				Key:    "pro",
				Status: billing.PlanStatusActive,
				Entitlements: []billing.Entitlement{
					billing.FeatureEntitlement(" seats "),
					billing.FeatureEntitlement("seats"),
				},
			},
		},
		{
			name: "unlimited with limit",
			plan: billing.Plan{
				Key:    "pro",
				Status: billing.PlanStatusActive,
				Entitlements: []billing.Entitlement{
					{Feature: "projects", Enabled: true, Unlimited: true, Limit: 1},
				},
			},
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := tc.plan.Validate()
			if !errors.Is(err, billing.ErrPlanInvalid) {
				t.Fatalf("Validate() error = %v, want ErrPlanInvalid", err)
			}
		})
	}
}

func TestEntitlementUsageRejectsInvalidTotals(t *testing.T) {
	t.Parallel()

	entitlement := billing.LimitedEntitlement("seats", 10)

	_, err := entitlement.CheckUsage(-1, 1, time.Time{})
	if !errors.Is(err, billing.ErrEntitlementUsageInvalid) {
		t.Fatalf("CheckUsage() negative current error = %v, want ErrEntitlementUsageInvalid", err)
	}

	_, err = entitlement.CheckUsage(0, -1, time.Time{})
	if !errors.Is(err, billing.ErrEntitlementUsageInvalid) {
		t.Fatalf("CheckUsage() negative total error = %v, want ErrEntitlementUsageInvalid", err)
	}
}

func TestInvoiceStatusHelpers(t *testing.T) {
	t.Parallel()

	if !billing.InvoiceStatusOpen.Payable() {
		t.Fatal("open invoice should be payable")
	}
	if !billing.InvoiceStatusPastDue.Payable() {
		t.Fatal("past_due invoice should be payable")
	}
	if billing.InvoiceStatusPaid.Payable() {
		t.Fatal("paid invoice should not be payable")
	}
	if !billing.InvoiceStatusPaid.Terminal() || !billing.InvoiceStatusPaid.Paid() {
		t.Fatal("paid invoice should be terminal and paid")
	}
	if !billing.InvoiceStatusVoid.Terminal() {
		t.Fatal("void invoice should be terminal")
	}
	if got := billing.InvoiceStatus("provider_only").String(); got != "unknown" {
		t.Fatalf("String() = %q, want unknown", got)
	}
}

func TestRevenueEventHelpers(t *testing.T) {
	t.Parallel()

	occurredAt := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	issued := billing.RevenueEvent{
		ID:         "evt_1",
		Type:       billing.RevenueEventInvoiceIssued,
		Tenant:     "tenant-a",
		InvoiceID:  "inv_1",
		PlanKey:    "pro",
		Amount:     billing.RevenueAmount{Amount: 2590, Currency: " brl "},
		OccurredAt: occurredAt,
	}

	if err := issued.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
	if issued.SignedAmount() != 2590 {
		t.Fatalf("SignedAmount() = %d, want 2590", issued.SignedAmount())
	}
	if got := issued.Amount.Normalize().Currency; got != "BRL" {
		t.Fatalf("Normalize().Currency = %q, want BRL", got)
	}

	refund := issued
	refund.Type = billing.RevenueEventRefundIssued
	if !refund.Type.ReducesRevenue() {
		t.Fatal("refund event should reduce revenue")
	}
	if refund.SignedAmount() != -2590 {
		t.Fatalf("SignedAmount() = %d, want -2590", refund.SignedAmount())
	}

	invalid := issued
	invalid.Type = billing.RevenueEventUnknown
	if err := invalid.Validate(); !errors.Is(err, billing.ErrRevenueEventInvalid) {
		t.Fatalf("Validate() error = %v, want ErrRevenueEventInvalid", err)
	}

	invalid = issued
	invalid.OccurredAt = time.Time{}
	if err := invalid.Validate(); !errors.Is(err, billing.ErrRevenueEventInvalid) {
		t.Fatalf("Validate() missing time error = %v, want ErrRevenueEventInvalid", err)
	}
}
