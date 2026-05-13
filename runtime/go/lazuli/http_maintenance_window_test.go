package lazuli

import (
	"errors"
	"testing"
	"time"
)

func TestMaintenanceWindowScheduleEvaluatesFixedWindow(t *testing.T) {
	start := time.Date(2026, 5, 12, 2, 0, 0, 0, time.UTC)
	schedule := MaintenanceWindowSchedule{
		Windows: []MaintenanceWindow{
			NewMaintenanceWindow(start, 90*time.Minute),
		},
	}

	at := start.Add(30 * time.Minute)
	evaluation := schedule.Evaluate(at)
	if !evaluation.Active {
		t.Fatal("Evaluate().Active = false, want true")
	}
	if !evaluation.Window.StartsAt.Equal(start) || !evaluation.Window.EndsAt.Equal(start.Add(90*time.Minute)) {
		t.Fatalf("active window = %s..%s", evaluation.Window.StartsAt, evaluation.Window.EndsAt)
	}
	if evaluation.RetryAfter != time.Hour {
		t.Fatalf("RetryAfter = %s, want 1h", evaluation.RetryAfter)
	}
	if got := evaluation.RetryAfterHeader(); got != "3600" {
		t.Fatalf("evaluation RetryAfterHeader() = %q, want 3600", got)
	}

	retryAfter, ok := schedule.RetryAfter(at)
	if !ok {
		t.Fatal("RetryAfter() ok = false, want true")
	}
	if retryAfter != time.Hour {
		t.Fatalf("RetryAfter() = %s, want 1h", retryAfter)
	}
	if got := schedule.RetryAfterHeader(at); got != "3600" {
		t.Fatalf("schedule RetryAfterHeader() = %q, want 3600", got)
	}

	if schedule.Active(start.Add(90 * time.Minute)) {
		t.Fatal("Active() accepted exclusive window end")
	}
}

func TestMaintenanceWindowScheduleFindsNextFixedWindow(t *testing.T) {
	first := NewMaintenanceWindow(time.Date(2026, 5, 12, 1, 0, 0, 0, time.UTC), time.Hour)
	second := NewMaintenanceWindow(time.Date(2026, 5, 13, 3, 0, 0, 0, time.UTC), time.Hour)
	schedule := MaintenanceWindowSchedule{
		Windows: []MaintenanceWindow{second, first},
	}

	next, ok := schedule.NextWindow(time.Date(2026, 5, 12, 2, 0, 0, 0, time.UTC))
	if !ok {
		t.Fatal("NextWindow() ok = false, want true")
	}
	if !next.StartsAt.Equal(second.StartsAt) {
		t.Fatalf("NextWindow().StartsAt = %s, want %s", next.StartsAt, second.StartsAt)
	}
}

func TestMaintenanceWindowScheduleUsesLocationForWeeklyRules(t *testing.T) {
	location := time.FixedZone("maintenance", 2*60*60)
	schedule := MaintenanceWindowSchedule{
		Location: location,
		Weekly: []MaintenanceWindowRule{
			{
				Weekday:  time.Monday,
				StartsAt: 2 * time.Hour,
				Duration: 45 * time.Minute,
			},
		},
	}

	at := time.Date(2026, 5, 10, 23, 30, 0, 0, time.UTC) // Monday 01:30 local.
	next, ok := schedule.NextWindow(at)
	if !ok {
		t.Fatal("NextWindow() ok = false, want true")
	}

	wantStart := time.Date(2026, 5, 11, 2, 0, 0, 0, location)
	if !next.StartsAt.Equal(wantStart) {
		t.Fatalf("NextWindow().StartsAt = %s, want %s", next.StartsAt, wantStart)
	}
	if got := next.StartsAt.In(location).Hour(); got != 2 {
		t.Fatalf("local hour = %d, want 2", got)
	}
}

func TestMaintenanceWindowScheduleWeeklyRuleCanCrossMidnight(t *testing.T) {
	location := time.FixedZone("maintenance", -3*60*60)
	schedule := MaintenanceWindowSchedule{
		Location: location,
		Weekly: []MaintenanceWindowRule{
			{
				Weekday:  time.Monday,
				StartsAt: 23 * time.Hour,
				Duration: 2 * time.Hour,
			},
		},
	}

	at := time.Date(2026, 5, 12, 0, 30, 0, 0, location)
	active, ok := schedule.ActiveWindow(at)
	if !ok {
		t.Fatal("ActiveWindow() ok = false, want true")
	}

	wantStart := time.Date(2026, 5, 11, 23, 0, 0, 0, location)
	wantEnd := time.Date(2026, 5, 12, 1, 0, 0, 0, location)
	if !active.StartsAt.Equal(wantStart) || !active.EndsAt.Equal(wantEnd) {
		t.Fatalf("active window = %s..%s, want %s..%s", active.StartsAt, active.EndsAt, wantStart, wantEnd)
	}
}

func TestMaintenanceWindowScheduleRetryAfterUsesLatestOverlappingEnd(t *testing.T) {
	start := time.Date(2026, 5, 12, 2, 0, 0, 0, time.UTC)
	schedule := MaintenanceWindowSchedule{
		Windows: []MaintenanceWindow{
			NewMaintenanceWindow(start, 30*time.Minute),
			NewMaintenanceWindow(start.Add(15*time.Minute), time.Hour),
		},
	}

	retryAfter, ok := schedule.RetryAfter(start.Add(20 * time.Minute))
	if !ok {
		t.Fatal("RetryAfter() ok = false, want true")
	}
	if retryAfter != 55*time.Minute {
		t.Fatalf("RetryAfter() = %s, want 55m", retryAfter)
	}
}

func TestMaintenanceWindowScheduleValidateRejectsInvalidInputs(t *testing.T) {
	tests := []struct {
		name     string
		schedule MaintenanceWindowSchedule
	}{
		{
			name: "fixed zero start",
			schedule: MaintenanceWindowSchedule{
				Windows: []MaintenanceWindow{{EndsAt: time.Date(2026, 5, 12, 1, 0, 0, 0, time.UTC)}},
			},
		},
		{
			name: "fixed end before start",
			schedule: MaintenanceWindowSchedule{
				Windows: []MaintenanceWindow{{
					StartsAt: time.Date(2026, 5, 12, 2, 0, 0, 0, time.UTC),
					EndsAt:   time.Date(2026, 5, 12, 1, 0, 0, 0, time.UTC),
				}},
			},
		},
		{
			name: "weekly invalid weekday",
			schedule: MaintenanceWindowSchedule{
				Weekly: []MaintenanceWindowRule{{Weekday: time.Weekday(99), StartsAt: time.Hour, Duration: time.Hour}},
			},
		},
		{
			name: "weekly invalid local start",
			schedule: MaintenanceWindowSchedule{
				Weekly: []MaintenanceWindowRule{{Weekday: time.Monday, StartsAt: 24 * time.Hour, Duration: time.Hour}},
			},
		},
		{
			name: "weekly invalid duration",
			schedule: MaintenanceWindowSchedule{
				Weekly: []MaintenanceWindowRule{{Weekday: time.Monday, StartsAt: time.Hour}},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := tt.schedule.Validate(); !errors.Is(err, ErrMaintenanceWindowInvalid) {
				t.Fatalf("Validate() error = %v, want ErrMaintenanceWindowInvalid", err)
			}
		})
	}
}
