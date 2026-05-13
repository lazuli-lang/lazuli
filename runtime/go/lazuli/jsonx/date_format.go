package jsonx

import (
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"time"
)

var (
	// ErrInvalidDateFormatPolicy reports an invalid JSON date/time formatting
	// policy before a timestamp is serialized.
	ErrInvalidDateFormatPolicy = errors.New("lazuli/jsonx: invalid date format policy")
)

// DateFormatKind selects the layout used by DateFormatPolicy.
type DateFormatKind string

const (
	// DateFormatRFC3339 renders full timestamps with time.RFC3339.
	DateFormatRFC3339 DateFormatKind = "rfc3339"
	// DateFormatDate renders date-only values with time.DateOnly.
	DateFormatDate DateFormatKind = "date"
	// DateFormatTime renders time-only values with time.TimeOnly.
	DateFormatTime DateFormatKind = "time"
	// DateFormatCustom renders values with DateFormatPolicy.Layout.
	DateFormatCustom DateFormatKind = "custom"
)

// TimezoneNormalization controls how a timestamp location is handled before it
// is formatted.
type TimezoneNormalization string

const (
	// TimezonePreserve leaves the timestamp location unchanged.
	TimezonePreserve TimezoneNormalization = "preserve"
	// TimezoneUTC converts timestamps to UTC before formatting.
	TimezoneUTC TimezoneNormalization = "utc"
	// TimezoneLocation converts timestamps to DateFormatPolicy.Location before
	// formatting.
	TimezoneLocation TimezoneNormalization = "location"
)

// DateFormatPolicy describes how a time.Time should be rendered into a JSON
// string. The zero value is valid and renders RFC3339 timestamps normalized to
// UTC.
type DateFormatPolicy struct {
	// Kind selects one of the built-in layouts or enables Layout for custom
	// formatting. Empty defaults to DateFormatRFC3339.
	Kind DateFormatKind

	// Layout is used only when Kind is DateFormatCustom. For built-in kinds,
	// Layout may be empty or match the canonical built-in layout.
	Layout string

	// Timezone controls whether timestamps are preserved or converted before
	// formatting. Empty defaults to TimezoneUTC, unless Location is set, in
	// which case it defaults to TimezoneLocation.
	Timezone TimezoneNormalization

	// Location is required when Timezone is TimezoneLocation.
	Location *time.Location
}

// FormattedTime wraps a time.Time with a DateFormatPolicy for direct use with
// encoding/json.
type FormattedTime struct {
	Time   time.Time
	Policy DateFormatPolicy
}

// RFC3339DateFormat returns the default full timestamp policy.
func RFC3339DateFormat() DateFormatPolicy {
	return DateFormatPolicy{Kind: DateFormatRFC3339, Timezone: TimezoneUTC}
}

// DateOnlyFormat returns a policy that renders dates as YYYY-MM-DD.
func DateOnlyFormat() DateFormatPolicy {
	return DateFormatPolicy{Kind: DateFormatDate, Timezone: TimezoneUTC}
}

// TimeOnlyFormat returns a policy that renders times as HH:MM:SS.
func TimeOnlyFormat() DateFormatPolicy {
	return DateFormatPolicy{Kind: DateFormatTime, Timezone: TimezoneUTC}
}

// CustomDateFormat returns a policy that renders timestamps using layout.
func CustomDateFormat(layout string) DateFormatPolicy {
	return DateFormatPolicy{Kind: DateFormatCustom, Layout: layout, Timezone: TimezoneUTC}
}

// NewFormattedTime returns a value that marshals t with policy.
func NewFormattedTime(t time.Time, policy DateFormatPolicy) FormattedTime {
	return FormattedTime{Time: t, Policy: policy}
}

// Normalize returns a copy with default kind, layout, and timezone decisions
// filled in.
func (p DateFormatPolicy) Normalize() DateFormatPolicy {
	p.Kind = DateFormatKind(strings.ToLower(strings.TrimSpace(string(p.Kind))))
	if p.Kind == "" {
		p.Kind = DateFormatRFC3339
	}

	switch p.Kind {
	case DateFormatRFC3339:
		p.Layout = time.RFC3339
	case DateFormatDate:
		p.Layout = time.DateOnly
	case DateFormatTime:
		p.Layout = time.TimeOnly
	case DateFormatCustom:
	}

	p.Timezone = TimezoneNormalization(strings.ToLower(strings.TrimSpace(string(p.Timezone))))
	if p.Timezone == "" {
		if p.Location != nil {
			p.Timezone = TimezoneLocation
		} else {
			p.Timezone = TimezoneUTC
		}
	}
	return p
}

// WithTimezone returns a copy of p with timezone normalization set.
func (p DateFormatPolicy) WithTimezone(normalization TimezoneNormalization) DateFormatPolicy {
	p.Timezone = normalization
	normalized := TimezoneNormalization(strings.ToLower(strings.TrimSpace(string(normalization))))
	if normalized != TimezoneLocation {
		p.Location = nil
	}
	return p
}

// WithLocation returns a copy of p that converts timestamps to location before
// formatting.
func (p DateFormatPolicy) WithLocation(location *time.Location) DateFormatPolicy {
	p.Timezone = TimezoneLocation
	p.Location = location
	return p
}

// Validate reports invalid layout, timezone, or location combinations.
func (p DateFormatPolicy) Validate() error {
	return ValidateDateFormatPolicy(p)
}

// NormalizeTime applies the policy timezone decision to t.
func (p DateFormatPolicy) NormalizeTime(t time.Time) (time.Time, error) {
	if err := ValidateDateFormatPolicy(p); err != nil {
		return time.Time{}, err
	}
	p = p.Normalize()
	return p.normalizeTime(t), nil
}

// Format renders t according to p.
func (p DateFormatPolicy) Format(t time.Time) (string, error) {
	if err := ValidateDateFormatPolicy(p); err != nil {
		return "", err
	}
	p = p.Normalize()
	return p.normalizeTime(t).Format(p.Layout), nil
}

// MarshalTime renders t according to p and returns a JSON string literal.
func (p DateFormatPolicy) MarshalTime(t time.Time) ([]byte, error) {
	formatted, err := p.Format(t)
	if err != nil {
		return nil, err
	}
	return json.Marshal(formatted)
}

// MarshalJSON renders t as a JSON string using its policy.
func (t FormattedTime) MarshalJSON() ([]byte, error) {
	return t.Policy.MarshalTime(t.Time)
}

// ValidateDateFormatPolicy checks that policy is structurally valid.
func ValidateDateFormatPolicy(policy DateFormatPolicy) error {
	return validateNormalizedDateFormatPolicy(policy.Normalize(), policy.Layout)
}

// FormatTime renders t according to policy.
func FormatTime(t time.Time, policy DateFormatPolicy) (string, error) {
	return policy.Format(t)
}

// MarshalTime renders t according to policy and returns a JSON string literal.
func MarshalTime(t time.Time, policy DateFormatPolicy) ([]byte, error) {
	return policy.MarshalTime(t)
}

func (p DateFormatPolicy) normalizeTime(t time.Time) time.Time {
	switch p.Timezone {
	case TimezonePreserve:
		return t
	case TimezoneLocation:
		return t.In(p.Location)
	default:
		return t.UTC()
	}
}

func validateNormalizedDateFormatPolicy(policy DateFormatPolicy, rawLayout string) error {
	var errs []error

	switch policy.Kind {
	case DateFormatRFC3339, DateFormatDate, DateFormatTime:
		if rawLayout != "" && rawLayout != policy.Layout {
			errs = append(errs, fmt.Errorf("%w: Layout does not match %s format", ErrInvalidDateFormatPolicy, policy.Kind))
		}
	case DateFormatCustom:
		if strings.TrimSpace(policy.Layout) == "" {
			errs = append(errs, fmt.Errorf("%w: custom Layout is required", ErrInvalidDateFormatPolicy))
		}
		if hasDateFormatControlRune(policy.Layout) {
			errs = append(errs, fmt.Errorf("%w: custom Layout contains control characters", ErrInvalidDateFormatPolicy))
		}
		if policy.Layout != "" && !dateFormatLayoutUsesReferenceTime(policy.Layout) {
			errs = append(errs, fmt.Errorf("%w: custom Layout must include Go time layout tokens", ErrInvalidDateFormatPolicy))
		}
	default:
		errs = append(errs, fmt.Errorf("%w: unknown Kind %q", ErrInvalidDateFormatPolicy, policy.Kind))
	}

	switch policy.Timezone {
	case TimezonePreserve, TimezoneUTC:
		if policy.Location != nil {
			errs = append(errs, fmt.Errorf("%w: Location requires TimezoneLocation", ErrInvalidDateFormatPolicy))
		}
	case TimezoneLocation:
		if policy.Location == nil {
			errs = append(errs, fmt.Errorf("%w: Location is required", ErrInvalidDateFormatPolicy))
		}
	default:
		errs = append(errs, fmt.Errorf("%w: unknown Timezone %q", ErrInvalidDateFormatPolicy, policy.Timezone))
	}

	return errors.Join(errs...)
}

func hasDateFormatControlRune(value string) bool {
	for _, r := range value {
		if r < 0x20 || r == 0x7f {
			return true
		}
	}
	return false
}

func dateFormatLayoutUsesReferenceTime(layout string) bool {
	probe := time.Date(2024, time.November, 23, 10, 30, 45, 987654321, time.FixedZone("LZT", 2*60*60+30*60))
	return probe.Format(layout) != layout
}
