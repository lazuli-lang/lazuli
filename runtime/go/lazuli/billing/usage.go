package billing

import (
	"context"
	"time"

	"lazuli.dev/runtime/lazuli"
)

// UsageStore is the storage seam for the period-bucket usage counter.
// Default implementation lives in `postgres_usage.go` (a future
// PG.D follow-up) and reads/writes the `subscription_usage` table
// declared in the Lazuli migrations bucket.
//
// Tests substitute in-memory implementations through Register.
type UsageStore interface {
	Read(ctx context.Context, subscriptionID lazuli.ID, limit string, period time.Time) (uint64, error)
	Incr(ctx context.Context, subscriptionID lazuli.ID, limit string, period time.Time) error
}

// usageStore is the process-global usage store registered via
// RegisterUsage. When nil, readUsage / incrUsage return 0 / nil — the
// runtime treats missing storage as a temporary no-op so test fixtures
// without a usage adapter still flow.
var usageStore UsageStore

// RegisterUsage installs the active UsageStore adapter.
func RegisterUsage(s UsageStore) { usageStore = s }

// PeriodStart returns the calendar-month bucket aligned to `started`.
// v0.1 hardcodes monthly periods per the proposal's §1194 deferral
// note. Daily / weekly / yearly buckets become available when the
// `limits <name> <int> per <period>` form lands in a future cell.
func PeriodStart(started time.Time, now time.Time) time.Time {
	// Anchor to the month containing `now`. If `started` is in the
	// future (shouldn't happen for active subs), fall back to it.
	if now.Before(started) {
		return started
	}
	return time.Date(now.Year(), now.Month(), 1, 0, 0, 0, 0, now.Location())
}

// readUsage is the internal reader hit by CheckQuota.
func readUsage(ctx *lazuli.Ctx, subscriptionID lazuli.ID, limit string) (uint64, error) {
	if usageStore == nil {
		return 0, nil
	}
	period := PeriodStart(time.Time{}, ctx.Now)
	return usageStore.Read(ctx, subscriptionID, limit, period)
}

// incrUsage is the internal writer hit by IncrQuota.
func incrUsage(ctx *lazuli.Ctx, subscriptionID lazuli.ID, limit string) error {
	if usageStore == nil {
		return nil
	}
	period := PeriodStart(time.Time{}, ctx.Now)
	return usageStore.Incr(ctx, subscriptionID, limit, period)
}
