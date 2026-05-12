package webhooks

import (
	"errors"
	"testing"
	"time"
)

func TestCheckReplayAllowsMissingSpec(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)

	if err := CheckReplay(now, nil, now.Add(-time.Hour)); err != nil {
		t.Fatalf("CheckReplay returned error for nil spec: %v", err)
	}
}

func TestCheckReplayDeniesReplay(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	err := CheckReplay(now, &ReplaySpec{Mode: ReplayDeny}, now.Add(-time.Minute))

	if !errors.Is(err, ErrWebhookReplayDenied) {
		t.Fatalf("CheckReplay error = %v, want ErrWebhookReplayDenied", err)
	}
}

func TestCheckReplayAllowWithinWindow(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	spec := &ReplaySpec{Mode: ReplayAllow, Window: "24h"}

	if err := CheckReplay(now, spec, now.Add(-23*time.Hour)); err != nil {
		t.Fatalf("CheckReplay returned error within window: %v", err)
	}
}

func TestCheckReplayRejectsStaleTimestamp(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	spec := &ReplaySpec{Mode: ReplayAllow, Window: "24h"}
	err := CheckReplay(now, spec, now.Add(-24*time.Hour-time.Second))

	if !errors.Is(err, ErrWebhookReplayWindowExpired) {
		t.Fatalf("CheckReplay error = %v, want ErrWebhookReplayWindowExpired", err)
	}
}

func TestCheckReplayRejectsFutureTimestamp(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	spec := &ReplaySpec{Mode: ReplayAllow, Window: "24h"}
	err := CheckReplay(now, spec, now.Add(time.Second))

	if !errors.Is(err, ErrWebhookReplayTimestampInvalid) {
		t.Fatalf("CheckReplay error = %v, want ErrWebhookReplayTimestampInvalid", err)
	}
}

func TestCheckReplayRejectsInvalidWindow(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	spec := &ReplaySpec{Mode: ReplayAllow, Window: "tomorrow"}
	err := CheckReplay(now, spec, now.Add(-time.Minute))

	if !errors.Is(err, ErrWebhookReplayWindowInvalid) {
		t.Fatalf("CheckReplay error = %v, want ErrWebhookReplayWindowInvalid", err)
	}
}

func TestParseWebhookTimestamp(t *testing.T) {
	want := time.Date(2026, 5, 12, 12, 0, 0, 123456789, time.UTC)

	tests := []struct {
		name  string
		value string
		want  time.Time
	}{
		{
			name:  "rfc3339 nano",
			value: "2026-05-12T12:00:00.123456789Z",
			want:  want,
		},
		{
			name:  "unix seconds",
			value: "1778587200",
			want:  time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC),
		},
		{
			name:  "unix milliseconds",
			value: "1778587200123",
			want:  time.Date(2026, 5, 12, 12, 0, 0, 123000000, time.UTC),
		},
		{
			name:  "http date",
			value: "Tue, 12 May 2026 12:00:00 GMT",
			want:  time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC),
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := ParseWebhookTimestamp(tt.value)
			if err != nil {
				t.Fatalf("ParseWebhookTimestamp returned error: %v", err)
			}
			if !got.Equal(tt.want) {
				t.Fatalf("ParseWebhookTimestamp = %s, want %s", got, tt.want)
			}
		})
	}
}

func TestParseWebhookTimestampRejectsInvalidValue(t *testing.T) {
	_, err := ParseWebhookTimestamp("not-a-timestamp")

	if !errors.Is(err, ErrWebhookReplayTimestampInvalid) {
		t.Fatalf("ParseWebhookTimestamp error = %v, want ErrWebhookReplayTimestampInvalid", err)
	}
}
