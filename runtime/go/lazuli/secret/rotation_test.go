package secret_test

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/secret"
)

func TestRotationScheduleFindsActiveNextPreviousAndOverlap(t *testing.T) {
	base := time.Date(2026, 1, 1, 12, 0, 0, 0, time.UTC)
	schedule := secret.RotationSchedule{
		Purpose: "webhook.hmac",
		Overlap: 2 * time.Hour,
		Versions: []secret.RotationVersion{
			{Ref: secret.Ref("webhook.hmac").WithVersion("2026-03"), ActiveAt: base.Add(48 * time.Hour)},
			{Ref: secret.Ref("webhook.hmac").WithVersion("2026-01"), ActiveAt: base},
			{Ref: secret.Ref("webhook.hmac").WithVersion("2026-02"), ActiveAt: base.Add(24 * time.Hour)},
		},
	}

	at := base.Add(25 * time.Hour)
	active, ok := schedule.ActiveVersion(at)
	if !ok {
		t.Fatal("ActiveVersion() did not find active version")
	}
	assertRotationVersion(t, active, "webhook.hmac", "2026-02")

	next, ok := schedule.NextVersion(at)
	if !ok {
		t.Fatal("NextVersion() did not find next version")
	}
	assertRotationVersion(t, next, "webhook.hmac", "2026-03")

	previous, ok := schedule.PreviousVersion(at)
	if !ok {
		t.Fatal("PreviousVersion() did not find previous version")
	}
	assertRotationVersion(t, previous, "webhook.hmac", "2026-01")

	window, ok := schedule.OverlapWindow(at)
	if !ok {
		t.Fatal("OverlapWindow() did not find active overlap")
	}
	assertRotationVersion(t, window.Previous, "webhook.hmac", "2026-01")
	assertRotationVersion(t, window.Active, "webhook.hmac", "2026-02")
	if !window.StartsAt.Equal(base.Add(24 * time.Hour)) {
		t.Fatalf("window StartsAt = %v, want %v", window.StartsAt, base.Add(24*time.Hour))
	}
	if !window.EndsAt.Equal(base.Add(26 * time.Hour)) {
		t.Fatalf("window EndsAt = %v, want %v", window.EndsAt, base.Add(26*time.Hour))
	}
	if !window.Contains(window.StartsAt) {
		t.Fatal("Contains() rejected inclusive start")
	}
	if window.Contains(window.EndsAt) {
		t.Fatal("Contains() accepted exclusive end")
	}

	beforeFirst := base.Add(-time.Nanosecond)
	if _, ok := schedule.ActiveVersion(beforeFirst); ok {
		t.Fatal("ActiveVersion() before first activation found a version")
	}
	next, ok = schedule.NextVersion(beforeFirst)
	if !ok {
		t.Fatal("NextVersion() before first activation did not find first version")
	}
	assertRotationVersion(t, next, "webhook.hmac", "2026-01")
	if _, ok := schedule.PreviousVersion(beforeFirst); ok {
		t.Fatal("PreviousVersion() before first activation found a version")
	}
}

func TestRotationScheduleValidateAcceptsNormalizedUnorderedSchedule(t *testing.T) {
	base := time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC)
	schedule := secret.RotationSchedule{
		Purpose: " signing ",
		Overlap: time.Hour,
		Versions: []secret.RotationVersion{
			{Ref: secret.Env(" env.SIGNING_KEY ").WithVersion(" 2026-02 "), ActiveAt: base.Add(24 * time.Hour)},
			{Ref: secret.Env("SIGNING_KEY").WithVersion("2026-01"), ActiveAt: base},
		},
	}

	if err := schedule.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
}

func TestRotationScheduleValidateRejectsInvalidInputs(t *testing.T) {
	base := time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC)
	validVersion := secret.RotationVersion{
		Ref:      secret.Ref("api.key").WithVersion("v1"),
		ActiveAt: base,
	}

	tests := []struct {
		name     string
		schedule secret.RotationSchedule
		want     error
	}{
		{
			name: "missing purpose",
			schedule: secret.RotationSchedule{
				Versions: []secret.RotationVersion{validVersion},
			},
			want: secret.ErrRotationPurposeRequired,
		},
		{
			name: "negative overlap",
			schedule: secret.RotationSchedule{
				Purpose:  "api",
				Overlap:  -time.Second,
				Versions: []secret.RotationVersion{validVersion},
			},
			want: secret.ErrRotationOverlapInvalid,
		},
		{
			name: "missing reference name",
			schedule: secret.RotationSchedule{
				Purpose: "api",
				Versions: []secret.RotationVersion{{
					Ref:      secret.SecretRef{Version: "v1"},
					ActiveAt: base,
				}},
			},
			want: secret.ErrInvalidRef,
		},
		{
			name: "missing version label",
			schedule: secret.RotationSchedule{
				Purpose: "api",
				Versions: []secret.RotationVersion{{
					Ref:      secret.Ref("api.key"),
					ActiveAt: base,
				}},
			},
			want: secret.ErrRotationVersionRequired,
		},
		{
			name: "missing activation",
			schedule: secret.RotationSchedule{
				Purpose: "api",
				Versions: []secret.RotationVersion{{
					Ref: secret.Ref("api.key").WithVersion("v1"),
				}},
			},
			want: secret.ErrRotationActivationRequired,
		},
		{
			name: "duplicate version",
			schedule: secret.RotationSchedule{
				Purpose: "api",
				Versions: []secret.RotationVersion{
					validVersion,
					{Ref: secret.Ref("api.key").WithVersion("v1"), ActiveAt: base.Add(time.Hour)},
				},
			},
			want: secret.ErrDuplicateRotationVersion,
		},
		{
			name: "overlapping windows",
			schedule: secret.RotationSchedule{
				Purpose: "api",
				Overlap: 2 * time.Hour,
				Versions: []secret.RotationVersion{
					validVersion,
					{Ref: secret.Ref("api.key").WithVersion("v2"), ActiveAt: base.Add(time.Hour)},
					{Ref: secret.Ref("api.key").WithVersion("v3"), ActiveAt: base.Add(2 * time.Hour)},
				},
			},
			want: secret.ErrRotationOverlapInvalid,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := tt.schedule.Validate(); !errors.Is(err, tt.want) {
				t.Fatalf("Validate() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestRotationPlanLookupPurposeAndValidateDuplicates(t *testing.T) {
	base := time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC)
	signing := secret.RotationSchedule{
		Purpose: " signing ",
		Versions: []secret.RotationVersion{{
			Ref:      secret.Ref("signing.key").WithVersion("v1"),
			ActiveAt: base,
		}},
	}
	encryption := secret.RotationSchedule{
		Purpose: "encryption",
		Versions: []secret.RotationVersion{{
			Ref:      secret.Ref("encryption.key").WithVersion("v1"),
			ActiveAt: base,
		}},
	}
	plan := secret.RotationPlan{Schedules: []secret.RotationSchedule{signing, encryption}}

	got, ok := plan.LookupPurpose("signing")
	if !ok {
		t.Fatal("LookupPurpose() did not find normalized purpose")
	}
	if got.Purpose != "signing" {
		t.Fatalf("LookupPurpose() Purpose = %q, want normalized schedule", got.Purpose)
	}
	assertRotationVersion(t, got.Versions[0], "signing.key", "v1")

	if err := plan.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	plan.Schedules = append(plan.Schedules, secret.RotationSchedule{
		Purpose: "signing",
		Versions: []secret.RotationVersion{{
			Ref:      secret.Ref("other.key").WithVersion("v1"),
			ActiveAt: base,
		}},
	})
	if err := secret.ValidateRotationPlan(plan); !errors.Is(err, secret.ErrDuplicateRotationPurpose) {
		t.Fatalf("ValidateRotationPlan() error = %v, want ErrDuplicateRotationPurpose", err)
	}
}

func assertRotationVersion(t *testing.T, version secret.RotationVersion, name string, label secret.VersionLabel) {
	t.Helper()

	if version.Ref.Name != name {
		t.Fatalf("version name = %q, want %q", version.Ref.Name, name)
	}
	if version.Ref.Version != label {
		t.Fatalf("version label = %q, want %q", version.Ref.Version, label)
	}
}
