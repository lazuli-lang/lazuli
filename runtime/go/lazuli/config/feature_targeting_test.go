package config

import (
	"errors"
	"testing"
)

func TestRolloutTargetsMatchReportsTenantBeforeUser(t *testing.T) {
	targets := RolloutTargets{
		Tenants: []string{" tenant-1 "},
		Users:   []string{"user-1"},
	}

	match, ok := targets.Match(RolloutTarget{TenantKey: "tenant-1", UserKey: "user-1"})
	if !ok {
		t.Fatal("Match() ok = false, want true")
	}
	if match != (RolloutTargetMatch{Kind: RolloutTargetKindTenant, Key: "tenant-1"}) {
		t.Fatalf("Match() = %#v, want tenant match", match)
	}

	match, ok = targets.Match(RolloutTarget{TenantKey: "tenant-out", UserKey: " user-1 "})
	if !ok {
		t.Fatal("Match() user ok = false, want true")
	}
	if match != (RolloutTargetMatch{Kind: RolloutTargetKindUser, Key: "user-1"}) {
		t.Fatalf("Match() user = %#v, want user match", match)
	}
}

func TestRolloutExplainHonorsDenyAllowThenFullPercentage(t *testing.T) {
	rollout := Rollout{
		Feature:    "checkout.new_flow",
		Percentage: 100,
		Key:        RolloutKeyUser,
		Allow: RolloutTargets{
			Users: []string{" user-allow "},
		},
		Deny: RolloutTargets{
			Tenants: []string{"tenant-denied"},
		},
	}

	tests := []struct {
		name        string
		target      RolloutTarget
		wantEnabled bool
		wantReason  RolloutReason
		wantMatch   RolloutTargetMatch
	}{
		{
			name:        "deny target wins",
			target:      RolloutTarget{TenantKey: " tenant-denied ", UserKey: "user-allow"},
			wantEnabled: false,
			wantReason:  RolloutReasonDenyTarget,
			wantMatch:   RolloutTargetMatch{Kind: RolloutTargetKindTenant, Key: "tenant-denied"},
		},
		{
			name:        "allow target wins over percentage",
			target:      RolloutTarget{TenantKey: "tenant-out", UserKey: " user-allow "},
			wantEnabled: true,
			wantReason:  RolloutReasonAllowTarget,
			wantMatch:   RolloutTargetMatch{Kind: RolloutTargetKindUser, Key: "user-allow"},
		},
		{
			name:        "full percentage enables unmatched target",
			target:      RolloutTarget{TenantKey: "tenant-out", UserKey: "user-out"},
			wantEnabled: true,
			wantReason:  RolloutReasonPercentageFull,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := rollout.Explain(tt.target)
			if err != nil {
				t.Fatalf("Explain() error = %v", err)
			}
			if got.Enabled != tt.wantEnabled || got.Reason != tt.wantReason || got.Match != tt.wantMatch {
				t.Fatalf("Explain() = %#v, want enabled=%v reason=%q match=%#v", got, tt.wantEnabled, tt.wantReason, tt.wantMatch)
			}
			if got.Bucketed {
				t.Fatalf("Explain() Bucketed = true, want false")
			}
		})
	}
}

func TestRolloutExplainIncludesPercentageBucket(t *testing.T) {
	rollout := Rollout{
		Feature:    " checkout.new_flow ",
		Percentage: 78,
		Key:        RolloutKeyUser,
	}

	got, err := rollout.Explain(RolloutTarget{TenantKey: "tenant-42", UserKey: " user-42 "})
	if err != nil {
		t.Fatalf("Explain() error = %v", err)
	}
	if !got.Enabled {
		t.Fatal("Explain() Enabled = false, want true")
	}
	if got.Reason != RolloutReasonPercentage {
		t.Fatalf("Explain() Reason = %q, want %q", got.Reason, RolloutReasonPercentage)
	}
	if got.Feature != "checkout.new_flow" {
		t.Fatalf("Explain() Feature = %q, want trimmed feature", got.Feature)
	}
	if got.Target.UserKey != "user-42" {
		t.Fatalf("Explain() Target.UserKey = %q, want trimmed user", got.Target.UserKey)
	}
	if !got.Bucketed || got.Bucket != 77 {
		t.Fatalf("Explain() bucketed/bucket = %v/%d, want true/77", got.Bucketed, got.Bucket)
	}
	if got.Match != (RolloutTargetMatch{Kind: RolloutTargetKindUser, Key: "user-42"}) {
		t.Fatalf("Explain() Match = %#v, want bucket user match", got.Match)
	}

	enabled, err := rollout.Evaluate(RolloutTarget{TenantKey: "tenant-42", UserKey: " user-42 "})
	if err != nil {
		t.Fatalf("Evaluate() error = %v", err)
	}
	if enabled != got.Enabled {
		t.Fatalf("Evaluate() = %v, Explain().Enabled = %v", enabled, got.Enabled)
	}
}

func TestRolloutExplainRequiresBucketTargetForPartialPercentage(t *testing.T) {
	rollout := Rollout{
		Feature:    "checkout.new_flow",
		Percentage: 50,
		Key:        RolloutKeyTenant,
	}

	_, err := rollout.Explain(RolloutTarget{UserKey: "user-42"})
	if !errors.Is(err, ErrInvalidRolloutTarget) {
		t.Fatalf("Explain() error = %v, want ErrInvalidRolloutTarget", err)
	}
}
