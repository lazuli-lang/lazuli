package lazuli

import (
	"context"
	"fmt"
	"time"
)

// ScheduleRuleDate resolves the base Date for a `schedule_rule` computed
// field (W4 GAP-08). The generated `Compute<Field>(rule)` method calls
// this to obtain the base date selected by the rule enum, then applies
// the same `AddDate(0, 0, offset)` arithmetic the `computed_date` field
// kind uses.
//
// Wire-thin: it looks up the binding fn registered under `fnRef` (the
// `@fn.<name>` from the `.lzi` `schedule_rule from @fn.<name>(<rule>)`
// surface), invokes it with the rule argument, and parses the returned
// value into a time.Time. The base-date selection logic lives entirely
// in the user-authored binding fn — no calendar policy is reimplemented
// here.
//
// The fn may return a `time.Time`, a `Date` / `string` in RFC 3339
// `YYYY-MM-DD` form, or anything that stringifies to that form.
func ScheduleRuleDate(fnRef string, rule string) (time.Time, error) {
	fn, ok := lookupBindingFn(fnRef)
	if !ok {
		return time.Time{}, fmt.Errorf("schedule_rule: no binding fn registered for @fn.%s", fnRef)
	}
	out, err := fn(context.Background(), rule)
	if err != nil {
		return time.Time{}, fmt.Errorf("schedule_rule @fn.%s: %w", fnRef, err)
	}
	switch v := out.(type) {
	case time.Time:
		return v, nil
	case string:
		// `Date` is a string alias (RFC 3339 `YYYY-MM-DD`) — covered here.
		return time.Parse("2006-01-02", v)
	default:
		return time.Parse("2006-01-02", fmt.Sprintf("%v", out))
	}
}
