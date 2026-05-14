package billing_test

import (
	"context"
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/billing"
)

func TestFeatureSetHas(t *testing.T) {
	set := billing.NewFeatureSet("search", "view_history")
	if !set.Has("search") {
		t.Fatal("expected `search` in feature set")
	}
	if set.Has("export_csv") {
		t.Fatal("`export_csv` should not be in free-tier feature set")
	}
}

func TestPeriodStartAlignsToCalendarMonth(t *testing.T) {
	loc := time.UTC
	now := time.Date(2026, time.May, 14, 13, 45, 0, 0, loc)
	got := billing.PeriodStart(time.Time{}, now)
	want := time.Date(2026, time.May, 1, 0, 0, 0, 0, loc)
	if !got.Equal(want) {
		t.Fatalf("period start: got %v, want %v", got, want)
	}
}

func TestCheckFeaturePassesWhenPlanIncludesFeature(t *testing.T) {
	catalog := makeCatalog()
	billing.Register(&stubStore{
		PlanName: "pro",
	})
	t.Cleanup(func() { billing.Register(nil) })
	ctx := newCtx()
	if err := billing.CheckFeature(ctx, catalog, "bulk_consult"); err != nil {
		t.Fatalf("expected pro tier to include bulk_consult, got %v", err)
	}
}

func TestCheckFeatureRefusesWhenPlanLacksFeature(t *testing.T) {
	catalog := makeCatalog()
	billing.Register(&stubStore{
		PlanName: "free",
	})
	t.Cleanup(func() { billing.Register(nil) })
	ctx := newCtx()
	err := billing.CheckFeature(ctx, catalog, "bulk_consult")
	var forbidden billing.ErrPlanFeatureForbidden
	if !errors.As(err, &forbidden) {
		t.Fatalf("expected ErrPlanFeatureForbidden, got %v", err)
	}
	if forbidden.Plan != "free" || forbidden.Feature != "bulk_consult" {
		t.Fatalf("forbidden carries wrong plan/feature: %+v", forbidden)
	}
}

func TestCheckQuotaUnlimitedIsNoop(t *testing.T) {
	catalog := makeCatalog()
	billing.Register(&stubStore{PlanName: "enterprise"})
	t.Cleanup(func() { billing.Register(nil) })
	ctx := newCtx()
	if err := billing.CheckQuota(ctx, catalog, "queries_per_month"); err != nil {
		t.Fatalf("enterprise unlimited should be a no-op, got %v", err)
	}
}

func TestCheckQuotaRefusesWhenUsageReachesLimit(t *testing.T) {
	catalog := makeCatalog()
	billing.Register(&stubStore{PlanName: "free"})
	billing.RegisterUsage(&stubUsage{used: 100}) // free.queries_per_month = 100
	t.Cleanup(func() {
		billing.Register(nil)
		billing.RegisterUsage(nil)
	})
	ctx := newCtx()
	err := billing.CheckQuota(ctx, catalog, "queries_per_month")
	var exceeded billing.ErrPlanQuotaExceeded
	if !errors.As(err, &exceeded) {
		t.Fatalf("expected ErrPlanQuotaExceeded, got %v", err)
	}
	if exceeded.Used != 100 || exceeded.Max != 100 {
		t.Fatalf("exceeded counters wrong: %+v", exceeded)
	}
}

func TestIncrQuotaSkipsUnlimitedPlan(t *testing.T) {
	catalog := makeCatalog()
	billing.Register(&stubStore{PlanName: "enterprise"})
	usage := &stubUsage{}
	billing.RegisterUsage(usage)
	t.Cleanup(func() {
		billing.Register(nil)
		billing.RegisterUsage(nil)
	})
	ctx := newCtx()
	if err := billing.IncrQuota(ctx, catalog, "queries_per_month"); err != nil {
		t.Fatalf("incr quota on unlimited tier should be no-op, got %v", err)
	}
	if usage.incrs != 0 {
		t.Fatalf("expected zero increments on unlimited tier, got %d", usage.incrs)
	}
}

func TestIncrQuotaIncrementsFinitePlan(t *testing.T) {
	catalog := makeCatalog()
	billing.Register(&stubStore{PlanName: "free"})
	usage := &stubUsage{}
	billing.RegisterUsage(usage)
	t.Cleanup(func() {
		billing.Register(nil)
		billing.RegisterUsage(nil)
	})
	ctx := newCtx()
	if err := billing.IncrQuota(ctx, catalog, "queries_per_month"); err != nil {
		t.Fatalf("incr quota: %v", err)
	}
	if usage.incrs != 1 {
		t.Fatalf("expected 1 increment, got %d", usage.incrs)
	}
}

func TestLookupPlanFailsWithoutStore(t *testing.T) {
	billing.Register(nil)
	catalog := makeCatalog()
	ctx := newCtx()
	_, err := billing.LookupPlan(ctx, catalog)
	var lookupErr billing.ErrPlanLookupFailed
	if !errors.As(err, &lookupErr) {
		t.Fatalf("expected ErrPlanLookupFailed, got %v", err)
	}
}

// --- helpers ---------------------------------------------------------------

func makeCatalog() billing.PlanCatalog {
	return billing.PlanCatalog{
		Plans: map[string]billing.Plan{
			"free": {
				Name:     "free",
				Features: billing.NewFeatureSet("search", "view_history"),
				Limits: map[string]billing.LimitValue{
					"queries_per_month": billing.LimitInt(100),
				},
			},
			"pro": {
				Name: "pro",
				Features: billing.NewFeatureSet(
					"search", "view_history", "bulk_consult",
				),
				Limits: map[string]billing.LimitValue{
					"queries_per_month": billing.LimitInt(5000),
				},
			},
			"enterprise": {
				Name: "enterprise",
				Features: billing.NewFeatureSet(
					"search", "view_history", "bulk_consult", "sso",
				),
				Limits: map[string]billing.LimitValue{
					"queries_per_month": billing.LimitUnlimited,
				},
			},
		},
	}
}

type stubStore struct {
	PlanName string
}

func (s *stubStore) LookupActive(ctx context.Context, _ lazuli.ID) (*billing.ActiveSubscription, error) {
	return &billing.ActiveSubscription{
		SubscriptionID: 42,
		PlanName:       s.PlanName,
		Status:         "active",
		StartedAt:      time.Now().Add(-30 * 24 * time.Hour),
		ExpiresAt:      time.Now().Add(30 * 24 * time.Hour),
	}, nil
}

type stubUsage struct {
	used  uint64
	incrs int
}

func (u *stubUsage) Read(ctx context.Context, _ lazuli.ID, _ string, _ time.Time) (uint64, error) {
	return u.used, nil
}

func (u *stubUsage) Incr(ctx context.Context, _ lazuli.ID, _ string, _ time.Time) error {
	u.incrs++
	u.used++
	return nil
}

func newCtx() *lazuli.Ctx {
	return &lazuli.Ctx{
		Context: context.Background(),
		User:    &lazuli.User{ID: 1, OrgID: 1, Email: "test@example.com"},
		Now:     time.Now(),
	}
}
