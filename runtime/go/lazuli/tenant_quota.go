package lazuli

import (
	"errors"
	"fmt"
	"strconv"
	"strings"
	"time"
)

var (
	// ErrTenantQuotaLimitInvalid is returned when a tenant quota limit has
	// invalid bounds, unit, tenant key, or reset policy.
	ErrTenantQuotaLimitInvalid = errors.New("lazuli: tenant quota limit invalid")

	// ErrTenantQuotaUsageInvalid is returned when a usage snapshot or planned
	// delta cannot produce a coherent non-negative usage total.
	ErrTenantQuotaUsageInvalid = errors.New("lazuli: tenant quota usage invalid")

	// ErrTenantQuotaHardLimitExceeded is returned by TenantQuotaEvaluation.Validate
	// when a positive delta would exceed the hard limit.
	ErrTenantQuotaHardLimitExceeded = errors.New("lazuli: tenant quota hard limit exceeded")
)

const (
	tenantQuotaMaxInt64 = int64(1<<63 - 1)
	tenantQuotaMinInt64 = -1 << 63
)

// TenantQuotaUnit identifies the measured quantity for a tenant quota. Known
// units are provided as constants, and custom generated units are valid when
// they use lowercase letters, digits, dots, underscores, or dashes.
type TenantQuotaUnit string

const (
	TenantQuotaUnitRequests TenantQuotaUnit = "requests"
	TenantQuotaUnitBytes    TenantQuotaUnit = "bytes"
	TenantQuotaUnitRecords  TenantQuotaUnit = "records"
	TenantQuotaUnitSeats    TenantQuotaUnit = "seats"
	TenantQuotaUnitJobs     TenantQuotaUnit = "jobs"
)

// Normalize returns u in the canonical form used for comparisons and
// diagnostic output.
func (u TenantQuotaUnit) Normalize() TenantQuotaUnit {
	return TenantQuotaUnit(strings.ToLower(strings.TrimSpace(string(u))))
}

// String renders the unit as a stable lowercase token.
func (u TenantQuotaUnit) String() string {
	unit := u.Normalize()
	if unit == "" {
		return "unknown"
	}
	return string(unit)
}

// Validate checks whether u can be used as a quota unit token.
func (u TenantQuotaUnit) Validate() error {
	unit := string(u.Normalize())
	if unit == "" {
		return fmt.Errorf("%w: unit is required", ErrTenantQuotaLimitInvalid)
	}
	for i := 0; i < len(unit); i++ {
		c := unit[i]
		if isTenantQuotaUnitLetter(c) || isTenantQuotaUnitDigit(c) || c == '.' || c == '_' || c == '-' {
			continue
		}
		return fmt.Errorf("%w: unit %q contains invalid character %q", ErrTenantQuotaLimitInvalid, unit, c)
	}
	return nil
}

// TenantQuotaResetWindow declares when a quota ledger resets. Calendar windows
// are evaluated in UTC for deterministic behavior across hosts.
type TenantQuotaResetWindow string

const (
	TenantQuotaResetNone    TenantQuotaResetWindow = "none"
	TenantQuotaResetHourly  TenantQuotaResetWindow = "hour"
	TenantQuotaResetDaily   TenantQuotaResetWindow = "day"
	TenantQuotaResetWeekly  TenantQuotaResetWindow = "week"
	TenantQuotaResetMonthly TenantQuotaResetWindow = "month"
)

// Normalize returns the canonical reset window token.
func (w TenantQuotaResetWindow) Normalize() TenantQuotaResetWindow {
	switch strings.ToLower(strings.TrimSpace(string(w))) {
	case "", "none", "never":
		return TenantQuotaResetNone
	case "hour", "hours", "hourly":
		return TenantQuotaResetHourly
	case "day", "days", "daily":
		return TenantQuotaResetDaily
	case "week", "weeks", "weekly":
		return TenantQuotaResetWeekly
	case "month", "months", "monthly":
		return TenantQuotaResetMonthly
	default:
		return TenantQuotaResetWindow(strings.ToLower(strings.TrimSpace(string(w))))
	}
}

// String renders the reset window as a stable lowercase token.
func (w TenantQuotaResetWindow) String() string {
	switch w.Normalize() {
	case TenantQuotaResetNone:
		return "none"
	case TenantQuotaResetHourly:
		return "hour"
	case TenantQuotaResetDaily:
		return "day"
	case TenantQuotaResetWeekly:
		return "week"
	case TenantQuotaResetMonthly:
		return "month"
	default:
		return "unknown"
	}
}

// Validate checks whether w is a supported reset window.
func (w TenantQuotaResetWindow) Validate() error {
	switch w.Normalize() {
	case TenantQuotaResetNone, TenantQuotaResetHourly, TenantQuotaResetDaily, TenantQuotaResetWeekly, TenantQuotaResetMonthly:
		return nil
	default:
		return fmt.Errorf("%w: unsupported reset window %q", ErrTenantQuotaLimitInvalid, strings.TrimSpace(string(w)))
	}
}

// Bounds returns the UTC half-open window containing at. TenantQuotaResetNone
// returns the zero window.
func (w TenantQuotaResetWindow) Bounds(at time.Time) (TenantQuotaWindow, error) {
	if err := w.Validate(); err != nil {
		return TenantQuotaWindow{}, err
	}
	if w.Normalize() == TenantQuotaResetNone {
		return TenantQuotaWindow{}, nil
	}
	if at.IsZero() {
		return TenantQuotaWindow{}, fmt.Errorf("%w: reset window requires a non-zero time", ErrTenantQuotaUsageInvalid)
	}

	at = normalizeTenantQuotaTime(at)
	year, month, day := at.Date()
	hour := at.Hour()

	switch w.Normalize() {
	case TenantQuotaResetHourly:
		start := time.Date(year, month, day, hour, 0, 0, 0, time.UTC)
		return TenantQuotaWindow{Start: start, End: start.Add(time.Hour)}, nil
	case TenantQuotaResetDaily:
		start := time.Date(year, month, day, 0, 0, 0, 0, time.UTC)
		return TenantQuotaWindow{Start: start, End: start.AddDate(0, 0, 1)}, nil
	case TenantQuotaResetWeekly:
		start := time.Date(year, month, day, 0, 0, 0, 0, time.UTC)
		daysSinceMonday := (int(start.Weekday()) + 6) % 7
		start = start.AddDate(0, 0, -daysSinceMonday)
		return TenantQuotaWindow{Start: start, End: start.AddDate(0, 0, 7)}, nil
	case TenantQuotaResetMonthly:
		start := time.Date(year, month, 1, 0, 0, 0, 0, time.UTC)
		return TenantQuotaWindow{Start: start, End: start.AddDate(0, 1, 0)}, nil
	default:
		return TenantQuotaWindow{}, fmt.Errorf("%w: unsupported reset window %q", ErrTenantQuotaLimitInvalid, w)
	}
}

// TenantQuotaWindow is the half-open interval [Start, End) for a resettable
// quota ledger. The zero value means no reset window.
type TenantQuotaWindow struct {
	Start time.Time
	End   time.Time
}

// IsZero reports whether w represents an unbounded quota window.
func (w TenantQuotaWindow) IsZero() bool {
	return w.Start.IsZero() && w.End.IsZero()
}

// Normalize strips monotonic clock readings and stores non-zero boundaries in
// UTC.
func (w TenantQuotaWindow) Normalize() TenantQuotaWindow {
	return TenantQuotaWindow{
		Start: normalizeTenantQuotaTime(w.Start),
		End:   normalizeTenantQuotaTime(w.End),
	}
}

// Validate checks that w is either empty or a positive half-open interval.
func (w TenantQuotaWindow) Validate() error {
	w = w.Normalize()
	if w.IsZero() {
		return nil
	}
	if w.Start.IsZero() || w.End.IsZero() {
		return fmt.Errorf("%w: quota window requires both start and end", ErrTenantQuotaUsageInvalid)
	}
	if !w.End.After(w.Start) {
		return fmt.Errorf("%w: quota window end must be after start", ErrTenantQuotaUsageInvalid)
	}
	return nil
}

// Contains reports whether at falls within the half-open quota window. The
// zero window contains all instants.
func (w TenantQuotaWindow) Contains(at time.Time) bool {
	w = w.Normalize()
	if w.IsZero() {
		return true
	}
	at = normalizeTenantQuotaTime(at)
	return !at.Before(w.Start) && at.Before(w.End)
}

// String renders the window as deterministic UTC text.
func (w TenantQuotaWindow) String() string {
	w = w.Normalize()
	if w.IsZero() {
		return "none"
	}
	return tenantQuotaFormatTime(w.Start) + ".." + tenantQuotaFormatTime(w.End)
}

// TenantQuotaLimit is the generic per-tenant limit for one quota unit. Zero
// soft or hard limits disable that bound. Soft limits warn; hard limits block
// positive deltas that would exceed the bound.
type TenantQuotaLimit struct {
	Tenant      string
	Unit        TenantQuotaUnit
	SoftLimit   int64
	HardLimit   int64
	ResetWindow TenantQuotaResetWindow
}

// Normalize returns l in the canonical form used for comparisons.
func (l TenantQuotaLimit) Normalize() TenantQuotaLimit {
	l.Tenant = strings.TrimSpace(l.Tenant)
	l.Unit = l.Unit.Normalize()
	l.ResetWindow = l.ResetWindow.Normalize()
	return l
}

// Validate checks whether l has coherent bounds and scope metadata.
func (l TenantQuotaLimit) Validate() error {
	l = l.Normalize()
	if l.Tenant == "" {
		return fmt.Errorf("%w: tenant is required", ErrTenantQuotaLimitInvalid)
	}
	if err := l.Unit.Validate(); err != nil {
		return err
	}
	if l.SoftLimit < 0 {
		return fmt.Errorf("%w: soft limit must be non-negative", ErrTenantQuotaLimitInvalid)
	}
	if l.HardLimit < 0 {
		return fmt.Errorf("%w: hard limit must be non-negative", ErrTenantQuotaLimitInvalid)
	}
	if l.SoftLimit > 0 && l.HardLimit > 0 && l.SoftLimit > l.HardLimit {
		return fmt.Errorf("%w: soft limit must not exceed hard limit", ErrTenantQuotaLimitInvalid)
	}
	if err := l.ResetWindow.Validate(); err != nil {
		return err
	}
	return nil
}

// TenantQuotaUsageSnapshot is a point-in-time usage total for one tenant and
// quota unit. Used must be non-negative; planned changes are supplied as a
// delta to EvaluateTenantQuota.
type TenantQuotaUsageSnapshot struct {
	Tenant     string
	Unit       TenantQuotaUnit
	Used       int64
	Window     TenantQuotaWindow
	ObservedAt time.Time
}

// Normalize returns s in the canonical form used for comparisons.
func (s TenantQuotaUsageSnapshot) Normalize() TenantQuotaUsageSnapshot {
	s.Tenant = strings.TrimSpace(s.Tenant)
	s.Unit = s.Unit.Normalize()
	s.Window = s.Window.Normalize()
	s.ObservedAt = normalizeTenantQuotaTime(s.ObservedAt)
	return s
}

// Validate checks whether s can be evaluated against a tenant quota limit.
func (s TenantQuotaUsageSnapshot) Validate() error {
	s = s.Normalize()
	if s.Tenant == "" {
		return fmt.Errorf("%w: tenant is required", ErrTenantQuotaUsageInvalid)
	}
	if err := s.Unit.Validate(); err != nil {
		return fmt.Errorf("%w: %v", ErrTenantQuotaUsageInvalid, err)
	}
	if s.Used < 0 {
		return fmt.Errorf("%w: used must be non-negative", ErrTenantQuotaUsageInvalid)
	}
	if err := s.Window.Validate(); err != nil {
		return err
	}
	return nil
}

// TenantQuotaDecision is the selected limit class for an evaluated usage delta.
type TenantQuotaDecision int

const (
	TenantQuotaWithinLimit TenantQuotaDecision = iota
	TenantQuotaSoftLimitExceeded
	TenantQuotaHardLimitExceeded
)

// String renders the decision as a stable lowercase token.
func (d TenantQuotaDecision) String() string {
	switch d {
	case TenantQuotaWithinLimit:
		return "within_limit"
	case TenantQuotaSoftLimitExceeded:
		return "soft_limit_exceeded"
	case TenantQuotaHardLimitExceeded:
		return "hard_limit_exceeded"
	default:
		return "unknown"
	}
}

// TenantQuotaEvaluation is the deterministic result of applying a planned
// usage delta to a usage snapshot under one limit.
type TenantQuotaEvaluation struct {
	Limit    TenantQuotaLimit
	Usage    TenantQuotaUsageSnapshot
	Before   int64
	Delta    int64
	After    int64
	Decision TenantQuotaDecision
	Allowed  bool
	Reason   string
	ResetAt  time.Time
}

// Validate returns ErrTenantQuotaHardLimitExceeded when this evaluation blocks
// the planned delta.
func (e TenantQuotaEvaluation) Validate() error {
	if e.Allowed {
		return nil
	}
	limit := e.Limit.Normalize()
	return fmt.Errorf("%w: tenant %q unit %s would use %d over hard limit %d", ErrTenantQuotaHardLimitExceeded, limit.Tenant, limit.Unit, e.After, limit.HardLimit)
}

// Explanation renders the evaluation in a stable field order for logs, tests,
// and generated problem details.
func (e TenantQuotaEvaluation) Explanation() string {
	limit := e.Limit.Normalize()
	usage := e.Usage.Normalize()
	reason := e.Reason
	if reason == "" {
		reason = e.Decision.String()
	}
	resetAt := normalizeTenantQuotaTime(e.ResetAt)

	parts := []string{
		"tenant=" + strconv.Quote(limit.Tenant),
		"unit=" + limit.Unit.String(),
		"reset=" + limit.ResetWindow.String(),
		"window=" + usage.Window.String(),
		"reset_at=" + tenantQuotaFormatTime(resetAt),
		"used=" + strconv.FormatInt(e.Before, 10),
		"delta=" + strconv.FormatInt(e.Delta, 10),
		"projected=" + strconv.FormatInt(e.After, 10),
		"soft=" + strconv.FormatInt(limit.SoftLimit, 10),
		"hard=" + strconv.FormatInt(limit.HardLimit, 10),
		"decision=" + e.Decision.String(),
		"allowed=" + strconv.FormatBool(e.Allowed),
		"reason=" + reason,
	}
	return strings.Join(parts, " ")
}

// EvaluateTenantQuota applies delta to usage and returns a deterministic soft
// or hard limit decision without mutating any quota ledger.
func EvaluateTenantQuota(limit TenantQuotaLimit, usage TenantQuotaUsageSnapshot, delta int64, at time.Time) (TenantQuotaEvaluation, error) {
	limit = limit.Normalize()
	if err := limit.Validate(); err != nil {
		return TenantQuotaEvaluation{}, err
	}

	usage = usage.Normalize()
	if err := usage.Validate(); err != nil {
		return TenantQuotaEvaluation{}, err
	}
	if usage.Tenant != limit.Tenant {
		return TenantQuotaEvaluation{}, fmt.Errorf("%w: usage tenant %q does not match limit tenant %q", ErrTenantQuotaUsageInvalid, usage.Tenant, limit.Tenant)
	}
	if usage.Unit != limit.Unit {
		return TenantQuotaEvaluation{}, fmt.Errorf("%w: usage unit %s does not match limit unit %s", ErrTenantQuotaUsageInvalid, usage.Unit, limit.Unit)
	}

	at = normalizeTenantQuotaTime(at)
	if at.IsZero() {
		at = usage.ObservedAt
	}
	window, resetAt, err := tenantQuotaEvaluationWindow(limit.ResetWindow, usage.Window, at)
	if err != nil {
		return TenantQuotaEvaluation{}, err
	}
	usage.Window = window

	after, err := checkedTenantQuotaAdd(usage.Used, delta)
	if err != nil {
		return TenantQuotaEvaluation{}, fmt.Errorf("%w: usage overflows total", ErrTenantQuotaUsageInvalid)
	}
	if after < 0 {
		return TenantQuotaEvaluation{}, fmt.Errorf("%w: usage would become negative", ErrTenantQuotaUsageInvalid)
	}

	evaluation := TenantQuotaEvaluation{
		Limit:    limit,
		Usage:    usage,
		Before:   usage.Used,
		Delta:    delta,
		After:    after,
		Decision: TenantQuotaWithinLimit,
		Allowed:  true,
		Reason:   TenantQuotaWithinLimit.String(),
		ResetAt:  resetAt,
	}
	if limit.HardLimit > 0 && after > limit.HardLimit {
		evaluation.Decision = TenantQuotaHardLimitExceeded
		evaluation.Allowed = delta <= 0
		evaluation.Reason = TenantQuotaHardLimitExceeded.String()
		return evaluation, nil
	}
	if limit.SoftLimit > 0 && after > limit.SoftLimit {
		evaluation.Decision = TenantQuotaSoftLimitExceeded
		evaluation.Reason = TenantQuotaSoftLimitExceeded.String()
	}
	return evaluation, nil
}

func tenantQuotaEvaluationWindow(reset TenantQuotaResetWindow, usageWindow TenantQuotaWindow, at time.Time) (TenantQuotaWindow, time.Time, error) {
	reset = reset.Normalize()
	usageWindow = usageWindow.Normalize()
	if err := usageWindow.Validate(); err != nil {
		return TenantQuotaWindow{}, time.Time{}, err
	}
	if reset == TenantQuotaResetNone {
		if usageWindow.IsZero() {
			return TenantQuotaWindow{}, time.Time{}, nil
		}
		return usageWindow, usageWindow.End, nil
	}

	expected, err := reset.Bounds(at)
	if err != nil {
		return TenantQuotaWindow{}, time.Time{}, err
	}
	if usageWindow.IsZero() {
		return expected, expected.End, nil
	}
	if usageWindow != expected {
		return TenantQuotaWindow{}, time.Time{}, fmt.Errorf("%w: usage window %s does not match %s reset window %s", ErrTenantQuotaUsageInvalid, usageWindow, reset, expected)
	}
	return usageWindow, usageWindow.End, nil
}

func checkedTenantQuotaAdd(a, b int64) (int64, error) {
	if b > 0 && a > tenantQuotaMaxInt64-b {
		return 0, ErrTenantQuotaUsageInvalid
	}
	if b < 0 && a < tenantQuotaMinInt64-b {
		return 0, ErrTenantQuotaUsageInvalid
	}
	return a + b, nil
}

func tenantQuotaFormatTime(t time.Time) string {
	t = normalizeTenantQuotaTime(t)
	if t.IsZero() {
		return "never"
	}
	return t.Format(time.RFC3339Nano)
}

func normalizeTenantQuotaTime(t time.Time) time.Time {
	if t.IsZero() {
		return time.Time{}
	}
	return t.Round(0).UTC()
}

func isTenantQuotaUnitLetter(c byte) bool {
	return c >= 'a' && c <= 'z'
}

func isTenantQuotaUnitDigit(c byte) bool {
	return c >= '0' && c <= '9'
}
