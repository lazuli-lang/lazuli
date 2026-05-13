package jsonx

import (
	"encoding/json"
	"errors"
	"testing"
	"time"
)

func TestDateFormatPolicyZeroValueFormatsRFC3339UTC(t *testing.T) {
	local := time.FixedZone("BRT", -3*60*60)
	timestamp := time.Date(2026, time.May, 12, 23, 30, 15, 0, local)

	got, err := DateFormatPolicy{}.Format(timestamp)
	if err != nil {
		t.Fatalf("Format() error = %v", err)
	}
	const want = "2026-05-13T02:30:15Z"
	if got != want {
		t.Fatalf("Format() = %q, want %q", got, want)
	}
}

func TestDateFormatPolicyFormatsPresetLayouts(t *testing.T) {
	local := time.FixedZone("BRT", -3*60*60)
	timestamp := time.Date(2026, time.May, 12, 23, 30, 15, 0, local)

	date, err := DateOnlyFormat().WithLocation(local).Format(timestamp)
	if err != nil {
		t.Fatalf("date Format() error = %v", err)
	}
	if date != "2026-05-12" {
		t.Fatalf("date Format() = %q, want 2026-05-12", date)
	}

	clock, err := TimeOnlyFormat().WithTimezone(TimezonePreserve).Format(timestamp)
	if err != nil {
		t.Fatalf("time Format() error = %v", err)
	}
	if clock != "23:30:15" {
		t.Fatalf("time Format() = %q, want 23:30:15", clock)
	}
}

func TestDateFormatPolicyFormatsCustomLayout(t *testing.T) {
	local := time.FixedZone("BRT", -3*60*60)
	timestamp := time.Date(2026, time.May, 12, 23, 30, 15, 0, local)
	policy := CustomDateFormat("02/01/2006 15:04").WithTimezone(TimezonePreserve)

	got, err := policy.Format(timestamp)
	if err != nil {
		t.Fatalf("Format() error = %v", err)
	}
	const want = "12/05/2026 23:30"
	if got != want {
		t.Fatalf("Format() = %q, want %q", got, want)
	}
}

func TestDateFormatPolicyNormalizesToLocation(t *testing.T) {
	local := time.FixedZone("BRT", -3*60*60)
	timestamp := time.Date(2026, time.May, 13, 2, 30, 15, 0, time.UTC)

	got, err := RFC3339DateFormat().WithLocation(local).NormalizeTime(timestamp)
	if err != nil {
		t.Fatalf("NormalizeTime() error = %v", err)
	}
	if got.Location() != local {
		t.Fatalf("NormalizeTime() location = %v, want %v", got.Location(), local)
	}
	if got.Format(time.RFC3339) != "2026-05-12T23:30:15-03:00" {
		t.Fatalf("NormalizeTime() = %s", got.Format(time.RFC3339))
	}
}

func TestDateFormatPolicyMarshalTime(t *testing.T) {
	local := time.FixedZone("BRT", -3*60*60)
	timestamp := time.Date(2026, time.May, 12, 23, 30, 15, 0, local)
	payload := struct {
		Date FormattedTime `json:"date"`
	}{
		Date: NewFormattedTime(timestamp, DateOnlyFormat().WithLocation(local)),
	}

	got, err := json.Marshal(payload)
	if err != nil {
		t.Fatalf("json.Marshal() error = %v", err)
	}
	const want = `{"date":"2026-05-12"}`
	if string(got) != want {
		t.Fatalf("json.Marshal() = %s, want %s", got, want)
	}
}

func TestValidateDateFormatPolicyRejectsInvalidPolicies(t *testing.T) {
	tests := []struct {
		name   string
		policy DateFormatPolicy
	}{
		{
			name:   "unknown kind",
			policy: DateFormatPolicy{Kind: DateFormatKind("unix")},
		},
		{
			name:   "custom layout required",
			policy: CustomDateFormat(" "),
		},
		{
			name:   "custom layout needs tokens",
			policy: CustomDateFormat("created"),
		},
		{
			name:   "location required",
			policy: DateOnlyFormat().WithTimezone(TimezoneLocation),
		},
		{
			name: "location conflicts with utc",
			policy: DateFormatPolicy{
				Kind:     DateFormatDate,
				Timezone: TimezoneUTC,
				Location: time.UTC,
			},
		},
		{
			name: "builtin layout mismatch",
			policy: DateFormatPolicy{
				Kind:   DateFormatDate,
				Layout: time.RFC3339,
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.policy.Validate()
			if !errors.Is(err, ErrInvalidDateFormatPolicy) {
				t.Fatalf("Validate() error = %v, want ErrInvalidDateFormatPolicy", err)
			}
		})
	}
}
