package lazuli

import (
	"errors"
	"testing"
	"time"
)

func TestEvaluateTenantQuotaReturnsSoftDecisionWithStableExplanation(t *testing.T) {
	t.Parallel()

	at := time.Date(2026, 5, 12, 15, 30, 0, 0, time.FixedZone("BRT", -3*60*60))
	evaluation, err := EvaluateTenantQuota(
		TenantQuotaLimit{
			Tenant:      " tenant-a ",
			Unit:        " Requests ",
			SoftLimit:   90,
			HardLimit:   100,
			ResetWindow: "daily",
		},
		TenantQuotaUsageSnapshot{
			Tenant: "tenant-a",
			Unit:   TenantQuotaUnitRequests,
			Used:   80,
		},
		15,
		at,
	)
	if err != nil {
		t.Fatalf("EvaluateTenantQuota() error = %v", err)
	}

	if !evaluation.Allowed {
		t.Fatal("evaluation Allowed = false, want true for soft limit")
	}
	if evaluation.Decision != TenantQuotaSoftLimitExceeded {
		t.Fatalf("Decision = %s, want %s", evaluation.Decision, TenantQuotaSoftLimitExceeded)
	}
	if evaluation.Before != 80 || evaluation.Delta != 15 || evaluation.After != 95 {
		t.Fatalf("usage totals = before %d delta %d after %d, want 80/15/95", evaluation.Before, evaluation.Delta, evaluation.After)
	}
	if got := evaluation.Usage.Window.String(); got != "2026-05-12T00:00:00Z..2026-05-13T00:00:00Z" {
		t.Fatalf("window = %q", got)
	}

	wantExplanation := `tenant="tenant-a" unit=requests reset=day window=2026-05-12T00:00:00Z..2026-05-13T00:00:00Z reset_at=2026-05-13T00:00:00Z used=80 delta=15 projected=95 soft=90 hard=100 decision=soft_limit_exceeded allowed=true reason=soft_limit_exceeded`
	if got := evaluation.Explanation(); got != wantExplanation {
		t.Fatalf("Explanation() = %q, want %q", got, wantExplanation)
	}
	if err := evaluation.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
}

func TestEvaluateTenantQuotaBlocksPositiveHardLimitDelta(t *testing.T) {
	t.Parallel()

	evaluation, err := EvaluateTenantQuota(
		TenantQuotaLimit{
			Tenant:    "tenant-a",
			Unit:      TenantQuotaUnitJobs,
			HardLimit: 10,
		},
		TenantQuotaUsageSnapshot{
			Tenant: "tenant-a",
			Unit:   TenantQuotaUnitJobs,
			Used:   9,
		},
		2,
		time.Time{},
	)
	if err != nil {
		t.Fatalf("EvaluateTenantQuota() error = %v", err)
	}
	if evaluation.Allowed {
		t.Fatal("evaluation Allowed = true, want false")
	}
	if evaluation.Decision != TenantQuotaHardLimitExceeded {
		t.Fatalf("Decision = %s, want %s", evaluation.Decision, TenantQuotaHardLimitExceeded)
	}
	if err := evaluation.Validate(); !errors.Is(err, ErrTenantQuotaHardLimitExceeded) {
		t.Fatalf("Validate() error = %v, want ErrTenantQuotaHardLimitExceeded", err)
	}

	wantExplanation := `tenant="tenant-a" unit=jobs reset=none window=none reset_at=never used=9 delta=2 projected=11 soft=0 hard=10 decision=hard_limit_exceeded allowed=false reason=hard_limit_exceeded`
	if got := evaluation.Explanation(); got != wantExplanation {
		t.Fatalf("Explanation() = %q, want %q", got, wantExplanation)
	}
}

func TestEvaluateTenantQuotaAllowsUsageReductionOverHardLimit(t *testing.T) {
	t.Parallel()

	evaluation, err := EvaluateTenantQuota(
		TenantQuotaLimit{
			Tenant:    "tenant-a",
			Unit:      TenantQuotaUnitBytes,
			HardLimit: 100,
		},
		TenantQuotaUsageSnapshot{
			Tenant: "tenant-a",
			Unit:   TenantQuotaUnitBytes,
			Used:   130,
		},
		-20,
		time.Time{},
	)
	if err != nil {
		t.Fatalf("EvaluateTenantQuota() error = %v", err)
	}
	if !evaluation.Allowed {
		t.Fatal("evaluation Allowed = false, want true for reduction")
	}
	if evaluation.Decision != TenantQuotaHardLimitExceeded {
		t.Fatalf("Decision = %s, want %s", evaluation.Decision, TenantQuotaHardLimitExceeded)
	}
	if err := evaluation.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
	if evaluation.After != 110 {
		t.Fatalf("After = %d, want 110", evaluation.After)
	}
}

func TestTenantQuotaResetWindowBounds(t *testing.T) {
	t.Parallel()

	at := time.Date(2026, 5, 13, 14, 45, 30, 0, time.FixedZone("BRT", -3*60*60))
	tests := []struct {
		name   string
		window TenantQuotaResetWindow
		want   string
	}{
		{
			name:   "hourly",
			window: TenantQuotaResetHourly,
			want:   "2026-05-13T17:00:00Z..2026-05-13T18:00:00Z",
		},
		{
			name:   "daily",
			window: TenantQuotaResetDaily,
			want:   "2026-05-13T00:00:00Z..2026-05-14T00:00:00Z",
		},
		{
			name:   "weekly",
			window: TenantQuotaResetWeekly,
			want:   "2026-05-11T00:00:00Z..2026-05-18T00:00:00Z",
		},
		{
			name:   "monthly",
			window: TenantQuotaResetMonthly,
			want:   "2026-05-01T00:00:00Z..2026-06-01T00:00:00Z",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got, err := tt.window.Bounds(at)
			if err != nil {
				t.Fatalf("Bounds() error = %v", err)
			}
			if got.String() != tt.want {
				t.Fatalf("Bounds() = %q, want %q", got, tt.want)
			}
			if !got.Contains(at) {
				t.Fatalf("Bounds() %q does not contain %s", got, at)
			}
			if got.Contains(got.End) {
				t.Fatalf("Bounds() %q contains its end instant", got)
			}
		})
	}
}

func TestTenantQuotaValidationRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name  string
		limit TenantQuotaLimit
		usage TenantQuotaUsageSnapshot
		delta int64
		at    time.Time
		want  error
	}{
		{
			name:  "blank tenant",
			limit: TenantQuotaLimit{Tenant: " ", Unit: TenantQuotaUnitRequests, HardLimit: 1},
			usage: TenantQuotaUsageSnapshot{Tenant: "tenant-a", Unit: TenantQuotaUnitRequests},
			want:  ErrTenantQuotaLimitInvalid,
		},
		{
			name:  "invalid unit",
			limit: TenantQuotaLimit{Tenant: "tenant-a", Unit: "bad unit", HardLimit: 1},
			usage: TenantQuotaUsageSnapshot{Tenant: "tenant-a", Unit: "bad unit"},
			want:  ErrTenantQuotaLimitInvalid,
		},
		{
			name:  "soft above hard",
			limit: TenantQuotaLimit{Tenant: "tenant-a", Unit: TenantQuotaUnitRequests, SoftLimit: 11, HardLimit: 10},
			usage: TenantQuotaUsageSnapshot{Tenant: "tenant-a", Unit: TenantQuotaUnitRequests},
			want:  ErrTenantQuotaLimitInvalid,
		},
		{
			name:  "mismatched tenant",
			limit: TenantQuotaLimit{Tenant: "tenant-a", Unit: TenantQuotaUnitRequests, HardLimit: 10},
			usage: TenantQuotaUsageSnapshot{Tenant: "tenant-b", Unit: TenantQuotaUnitRequests},
			want:  ErrTenantQuotaUsageInvalid,
		},
		{
			name:  "negative usage",
			limit: TenantQuotaLimit{Tenant: "tenant-a", Unit: TenantQuotaUnitRecords, HardLimit: 10},
			usage: TenantQuotaUsageSnapshot{Tenant: "tenant-a", Unit: TenantQuotaUnitRecords, Used: -1},
			want:  ErrTenantQuotaUsageInvalid,
		},
		{
			name:  "negative projected usage",
			limit: TenantQuotaLimit{Tenant: "tenant-a", Unit: TenantQuotaUnitRecords, HardLimit: 10},
			usage: TenantQuotaUsageSnapshot{Tenant: "tenant-a", Unit: TenantQuotaUnitRecords, Used: 1},
			delta: -2,
			want:  ErrTenantQuotaUsageInvalid,
		},
		{
			name:  "reset window needs time",
			limit: TenantQuotaLimit{Tenant: "tenant-a", Unit: TenantQuotaUnitRecords, HardLimit: 10, ResetWindow: TenantQuotaResetMonthly},
			usage: TenantQuotaUsageSnapshot{Tenant: "tenant-a", Unit: TenantQuotaUnitRecords},
			want:  ErrTenantQuotaUsageInvalid,
		},
		{
			name:  "stale usage window",
			limit: TenantQuotaLimit{Tenant: "tenant-a", Unit: TenantQuotaUnitRequests, HardLimit: 10, ResetWindow: TenantQuotaResetDaily},
			usage: TenantQuotaUsageSnapshot{
				Tenant: "tenant-a",
				Unit:   TenantQuotaUnitRequests,
				Window: TenantQuotaWindow{
					Start: time.Date(2026, 5, 12, 0, 0, 0, 0, time.UTC),
					End:   time.Date(2026, 5, 13, 0, 0, 0, 0, time.UTC),
				},
			},
			at:   time.Date(2026, 5, 13, 1, 0, 0, 0, time.UTC),
			want: ErrTenantQuotaUsageInvalid,
		},
	}

	for _, tt := range cases {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			_, err := EvaluateTenantQuota(tt.limit, tt.usage, tt.delta, tt.at)
			if !errors.Is(err, tt.want) {
				t.Fatalf("EvaluateTenantQuota() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestTenantQuotaHelperStrings(t *testing.T) {
	t.Parallel()

	if got := TenantQuotaUnit(" API.Calls ").String(); got != "api.calls" {
		t.Fatalf("unit String() = %q, want api.calls", got)
	}
	if got := TenantQuotaResetWindow("Monthly").String(); got != "month" {
		t.Fatalf("reset String() = %q, want month", got)
	}
	if got := TenantQuotaDecision(99).String(); got != "unknown" {
		t.Fatalf("decision String() = %q, want unknown", got)
	}
	if got := (TenantQuotaWindow{}).String(); got != "none" {
		t.Fatalf("zero window String() = %q, want none", got)
	}
}
