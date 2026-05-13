package auth

import (
	"slices"
	"testing"
	"time"
)

func TestPlanSessionRotationByAbsoluteAge(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 18, 0, 0, 0, time.UTC)
	policy := SessionRotationPolicy{MaxAbsoluteAge: 24 * time.Hour}

	tests := []struct {
		name     string
		issuedAt time.Time
		want     bool
	}{
		{
			name:     "below threshold",
			issuedAt: now.Add(-24*time.Hour + time.Nanosecond),
			want:     false,
		},
		{
			name:     "at threshold",
			issuedAt: now.Add(-24 * time.Hour),
			want:     true,
		},
		{
			name:     "past threshold",
			issuedAt: now.Add(-24*time.Hour - time.Nanosecond),
			want:     true,
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			plan := PlanSessionRotation(
				now,
				SessionRotationSnapshot{IssuedAt: tt.issuedAt, LastSeenAt: now.Add(-time.Minute)},
				SessionRotationEvent{},
				policy,
			)
			if plan.Rotate != tt.want {
				t.Fatalf("Rotate = %v, want %v", plan.Rotate, tt.want)
			}
			wantReasons := []SessionRotationReason(nil)
			if tt.want {
				wantReasons = []SessionRotationReason{SessionRotationReasonAbsoluteAge}
			}
			if !slices.Equal(plan.Reasons, wantReasons) {
				t.Fatalf("Reasons = %#v, want %#v", plan.Reasons, wantReasons)
			}
			if got := ShouldRotateSession(
				now,
				SessionRotationSnapshot{IssuedAt: tt.issuedAt, LastSeenAt: now.Add(-time.Minute)},
				SessionRotationEvent{},
				policy,
			); got != tt.want {
				t.Fatalf("ShouldRotateSession = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestPlanSessionRotationByIdleAge(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 19, 0, 0, 0, time.UTC)
	policy := SessionRotationPolicy{MaxIdleAge: 30 * time.Minute}

	tests := []struct {
		name       string
		session    SessionRotationSnapshot
		wantRotate bool
	}{
		{
			name: "uses LastSeenAt",
			session: SessionRotationSnapshot{
				IssuedAt:   now.Add(-2 * time.Hour),
				LastSeenAt: now.Add(-30 * time.Minute),
			},
			wantRotate: true,
		},
		{
			name: "does not rotate active session",
			session: SessionRotationSnapshot{
				IssuedAt:   now.Add(-2 * time.Hour),
				LastSeenAt: now.Add(-30*time.Minute + time.Nanosecond),
			},
			wantRotate: false,
		},
		{
			name: "falls back to IssuedAt",
			session: SessionRotationSnapshot{
				IssuedAt: now.Add(-30 * time.Minute),
			},
			wantRotate: true,
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			plan := PlanSessionRotation(now, tt.session, SessionRotationEvent{}, policy)
			if plan.Rotate != tt.wantRotate {
				t.Fatalf("Rotate = %v, want %v", plan.Rotate, tt.wantRotate)
			}
			wantReasons := []SessionRotationReason(nil)
			if tt.wantRotate {
				wantReasons = []SessionRotationReason{SessionRotationReasonIdleAge}
			}
			if !slices.Equal(plan.Reasons, wantReasons) {
				t.Fatalf("Reasons = %#v, want %#v", plan.Reasons, wantReasons)
			}
		})
	}
}

func TestPlanSessionRotationCredentialSensitiveAction(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 20, 0, 0, 0, time.UTC)

	plan := PlanSessionRotation(
		now,
		SessionRotationSnapshot{IssuedAt: now, LastSeenAt: now},
		SessionRotationEvent{CredentialSensitiveAction: true},
		SessionRotationPolicy{},
	)
	if !plan.Rotate {
		t.Fatal("Rotate = false, want true for credential-sensitive action")
	}
	if got, want := plan.Reasons, []SessionRotationReason{SessionRotationReasonCredentialSensitiveAction}; !slices.Equal(got, want) {
		t.Fatalf("Reasons = %#v, want %#v", got, want)
	}
}

func TestPlanSessionRotationRotateOnLoginPolicy(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 21, 0, 0, 0, time.UTC)
	session := SessionRotationSnapshot{IssuedAt: now, LastSeenAt: now}
	event := SessionRotationEvent{Login: true}

	disabled := PlanSessionRotation(now, session, event, SessionRotationPolicy{})
	if disabled.Rotate {
		t.Fatalf("Rotate = true, want false when RotateOnLogin is disabled")
	}

	enabled := PlanSessionRotation(now, session, event, SessionRotationPolicy{RotateOnLogin: true})
	if !enabled.Rotate {
		t.Fatal("Rotate = false, want true when RotateOnLogin is enabled")
	}
	if got, want := enabled.Reasons, []SessionRotationReason{SessionRotationReasonLogin}; !slices.Equal(got, want) {
		t.Fatalf("Reasons = %#v, want %#v", got, want)
	}
}

func TestPlanSessionRotationReturnsAllMatchingReasons(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 22, 0, 0, 0, time.UTC)
	plan := PlanSessionRotation(
		now,
		SessionRotationSnapshot{
			IssuedAt:   now.Add(-48 * time.Hour),
			LastSeenAt: now.Add(-2 * time.Hour),
		},
		SessionRotationEvent{
			CredentialSensitiveAction: true,
			Login:                     true,
		},
		SessionRotationPolicy{
			MaxAbsoluteAge: 24 * time.Hour,
			MaxIdleAge:     time.Hour,
			RotateOnLogin:  true,
		},
	)

	want := []SessionRotationReason{
		SessionRotationReasonAbsoluteAge,
		SessionRotationReasonIdleAge,
		SessionRotationReasonCredentialSensitiveAction,
		SessionRotationReasonLogin,
	}
	if !plan.Rotate {
		t.Fatal("Rotate = false, want true")
	}
	if !plan.GeneratedAt.Equal(now) {
		t.Fatalf("GeneratedAt = %s, want %s", plan.GeneratedAt, now)
	}
	if !slices.Equal(plan.Reasons, want) {
		t.Fatalf("Reasons = %#v, want %#v", plan.Reasons, want)
	}
}

func TestPlanSessionRotationIgnoresUnavailableAndDisabledAges(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 23, 0, 0, 0, time.UTC)
	for _, policy := range []SessionRotationPolicy{
		{MaxAbsoluteAge: 0, MaxIdleAge: 0},
		{MaxAbsoluteAge: -time.Hour, MaxIdleAge: -time.Minute},
		{MaxAbsoluteAge: time.Hour, MaxIdleAge: time.Minute},
	} {
		plan := PlanSessionRotation(now, SessionRotationSnapshot{}, SessionRotationEvent{}, policy)
		if plan.Rotate {
			t.Fatalf("Rotate = true, want false for unavailable/disabled ages with policy %#v", policy)
		}
		if len(plan.Reasons) != 0 {
			t.Fatalf("Reasons = %#v, want none", plan.Reasons)
		}
	}
}
