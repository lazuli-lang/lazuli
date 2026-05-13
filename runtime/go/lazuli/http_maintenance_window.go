package lazuli

import (
	"errors"
	"fmt"
	"time"
)

const maintenanceWindowDay = 24 * time.Hour

// ErrMaintenanceWindowInvalid is returned when a maintenance window schedule
// contains a malformed fixed window or recurring rule.
var ErrMaintenanceWindowInvalid = errors.New("lazuli: maintenance window invalid")

// MaintenanceWindow is a concrete maintenance interval.
//
// StartsAt is inclusive and EndsAt is exclusive.
type MaintenanceWindow struct {
	StartsAt time.Time
	EndsAt   time.Time
}

// NewMaintenanceWindow returns a concrete maintenance window that starts at
// startsAt and lasts for duration.
func NewMaintenanceWindow(startsAt time.Time, duration time.Duration) MaintenanceWindow {
	return MaintenanceWindow{StartsAt: startsAt, EndsAt: startsAt.Add(duration)}
}

// Contains reports whether at falls inside w.
func (w MaintenanceWindow) Contains(at time.Time) bool {
	return !w.StartsAt.IsZero() &&
		w.EndsAt.After(w.StartsAt) &&
		!at.Before(w.StartsAt) &&
		at.Before(w.EndsAt)
}

// RetryAfter returns the remaining window duration when at is inside w.
func (w MaintenanceWindow) RetryAfter(at time.Time) (time.Duration, bool) {
	if !w.Contains(at) {
		return 0, false
	}
	return w.EndsAt.Sub(at), true
}

// MaintenanceWindowRule describes a weekly recurring maintenance interval in
// a schedule's local time zone.
//
// StartsAt is the offset from local midnight on Weekday. Duration is elapsed
// time from the resolved local start instant.
type MaintenanceWindowRule struct {
	Weekday  time.Weekday
	StartsAt time.Duration
	Duration time.Duration
}

// MaintenanceWindowSchedule groups concrete and weekly maintenance windows.
//
// Location is used to resolve Weekly rules. Nil uses UTC.
type MaintenanceWindowSchedule struct {
	Location *time.Location
	Windows  []MaintenanceWindow
	Weekly   []MaintenanceWindowRule
}

// MaintenanceWindowEvaluation is the active/inactive state of a schedule at a
// point in time.
type MaintenanceWindowEvaluation struct {
	Active     bool
	Window     MaintenanceWindow
	Next       MaintenanceWindow
	RetryAfter time.Duration
}

// MaintenanceWindowStatus is an alias for MaintenanceWindowEvaluation.
type MaintenanceWindowStatus = MaintenanceWindowEvaluation

// Validate checks that the schedule can be evaluated deterministically.
func (s MaintenanceWindowSchedule) Validate() error {
	for i, window := range s.Windows {
		if window.StartsAt.IsZero() {
			return fmt.Errorf("%w: window %d starts_at required", ErrMaintenanceWindowInvalid, i)
		}
		if !window.EndsAt.After(window.StartsAt) {
			return fmt.Errorf("%w: window %d ends_at must be after starts_at", ErrMaintenanceWindowInvalid, i)
		}
	}

	for i, rule := range s.Weekly {
		if rule.Weekday < time.Sunday || rule.Weekday > time.Saturday {
			return fmt.Errorf("%w: weekly rule %d weekday invalid", ErrMaintenanceWindowInvalid, i)
		}
		if rule.StartsAt < 0 || rule.StartsAt >= maintenanceWindowDay {
			return fmt.Errorf("%w: weekly rule %d starts_at invalid", ErrMaintenanceWindowInvalid, i)
		}
		if rule.Duration <= 0 {
			return fmt.Errorf("%w: weekly rule %d duration must be positive", ErrMaintenanceWindowInvalid, i)
		}
	}

	return nil
}

// Active reports whether at falls inside any scheduled maintenance window.
func (s MaintenanceWindowSchedule) Active(at time.Time) bool {
	_, ok := s.ActiveWindow(at)
	return ok
}

// ActiveWindow returns the active window containing at. When multiple windows
// overlap, the one ending latest is returned so Retry-After estimates cover
// the full active maintenance span.
func (s MaintenanceWindowSchedule) ActiveWindow(at time.Time) (MaintenanceWindow, bool) {
	var (
		active MaintenanceWindow
		ok     bool
	)

	for _, window := range s.Windows {
		if !window.Contains(at) {
			continue
		}
		if !ok || active.EndsAt.Before(window.EndsAt) {
			active = window
			ok = true
		}
	}

	for _, rule := range s.Weekly {
		window, ruleOK := s.activeWeeklyWindow(rule, at)
		if !ruleOK {
			continue
		}
		if !ok || active.EndsAt.Before(window.EndsAt) {
			active = window
			ok = true
		}
	}

	return active, ok
}

// NextWindow returns the active window containing at, or the next scheduled
// window after at when the schedule is currently inactive.
func (s MaintenanceWindowSchedule) NextWindow(at time.Time) (MaintenanceWindow, bool) {
	if window, ok := s.ActiveWindow(at); ok {
		return window, true
	}

	var (
		next MaintenanceWindow
		ok   bool
	)

	for _, window := range s.Windows {
		if !window.EndsAt.After(window.StartsAt) || !window.StartsAt.After(at) {
			continue
		}
		if !ok || maintenanceWindowBefore(window, next) {
			next = window
			ok = true
		}
	}

	for _, rule := range s.Weekly {
		window, ruleOK := s.nextWeeklyWindow(rule, at)
		if !ruleOK {
			continue
		}
		if !ok || maintenanceWindowBefore(window, next) {
			next = window
			ok = true
		}
	}

	return next, ok
}

// RetryAfter estimates how long callers should wait when at is inside an
// active maintenance window.
func (s MaintenanceWindowSchedule) RetryAfter(at time.Time) (time.Duration, bool) {
	window, ok := s.ActiveWindow(at)
	if !ok {
		return 0, false
	}
	return window.RetryAfter(at)
}

// RetryAfterHeader returns a Retry-After header value for the active window at
// at, or an empty string when the schedule is inactive.
func (s MaintenanceWindowSchedule) RetryAfterHeader(at time.Time) string {
	retryAfter, ok := s.RetryAfter(at)
	if !ok {
		return ""
	}
	return maintenanceRetryAfter(retryAfter)
}

// Evaluate returns the schedule state at a point in time.
func (s MaintenanceWindowSchedule) Evaluate(at time.Time) MaintenanceWindowEvaluation {
	evaluation := MaintenanceWindowEvaluation{}
	if window, ok := s.ActiveWindow(at); ok {
		evaluation.Active = true
		evaluation.Window = window
		evaluation.Next = window
		evaluation.RetryAfter = window.EndsAt.Sub(at)
		return evaluation
	}
	if next, ok := s.NextWindow(at); ok {
		evaluation.Next = next
	}
	return evaluation
}

// RetryAfterHeader returns a Retry-After header value for the evaluation, or
// an empty string when the schedule is inactive.
func (e MaintenanceWindowEvaluation) RetryAfterHeader() string {
	if !e.Active {
		return ""
	}
	return maintenanceRetryAfter(e.RetryAfter)
}

func (s MaintenanceWindowSchedule) activeWeeklyWindow(rule MaintenanceWindowRule, at time.Time) (MaintenanceWindow, bool) {
	if !validMaintenanceWindowRule(rule) {
		return MaintenanceWindow{}, false
	}

	loc := maintenanceWindowLocation(s.Location)
	local := at.In(loc)
	midnight := maintenanceWindowLocalMidnight(local, loc)
	daysSince := (int(local.Weekday()) - int(rule.Weekday) + 7) % 7
	start := midnight.AddDate(0, 0, -daysSince).Add(rule.StartsAt)
	window := NewMaintenanceWindow(start, rule.Duration)
	if window.Contains(at) {
		return window, true
	}
	return MaintenanceWindow{}, false
}

func (s MaintenanceWindowSchedule) nextWeeklyWindow(rule MaintenanceWindowRule, at time.Time) (MaintenanceWindow, bool) {
	if !validMaintenanceWindowRule(rule) {
		return MaintenanceWindow{}, false
	}

	loc := maintenanceWindowLocation(s.Location)
	local := at.In(loc)
	midnight := maintenanceWindowLocalMidnight(local, loc)
	daysUntil := (int(rule.Weekday) - int(local.Weekday()) + 7) % 7
	start := midnight.AddDate(0, 0, daysUntil).Add(rule.StartsAt)
	if !start.After(at) {
		start = start.AddDate(0, 0, 7)
	}
	return NewMaintenanceWindow(start, rule.Duration), true
}

func maintenanceWindowLocation(loc *time.Location) *time.Location {
	if loc == nil {
		return time.UTC
	}
	return loc
}

func maintenanceWindowLocalMidnight(t time.Time, loc *time.Location) time.Time {
	year, month, day := t.In(loc).Date()
	return time.Date(year, month, day, 0, 0, 0, 0, loc)
}

func validMaintenanceWindowRule(rule MaintenanceWindowRule) bool {
	return rule.Weekday >= time.Sunday &&
		rule.Weekday <= time.Saturday &&
		rule.StartsAt >= 0 &&
		rule.StartsAt < maintenanceWindowDay &&
		rule.Duration > 0
}

func maintenanceWindowBefore(a, b MaintenanceWindow) bool {
	if !a.StartsAt.Equal(b.StartsAt) {
		return a.StartsAt.Before(b.StartsAt)
	}
	return a.EndsAt.Before(b.EndsAt)
}
