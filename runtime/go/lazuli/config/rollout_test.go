package config

import (
	"errors"
	"strings"
	"testing"
)

func TestRolloutBucketIsStable(t *testing.T) {
	bucket, err := RolloutBucket("checkout.new_flow", "user-42")
	if err != nil {
		t.Fatalf("RolloutBucket() error = %v", err)
	}
	if bucket != 77 {
		t.Fatalf("RolloutBucket() = %d, want 77", bucket)
	}

	enabled, err := PercentageRollout("checkout.new_flow", "user-42", 78)
	if err != nil {
		t.Fatalf("PercentageRollout(78) error = %v", err)
	}
	if !enabled {
		t.Fatal("PercentageRollout(78) = false, want true")
	}
	enabled, err = PercentageRollout("checkout.new_flow", "user-42", 77)
	if err != nil {
		t.Fatalf("PercentageRollout(77) error = %v", err)
	}
	if enabled {
		t.Fatal("PercentageRollout(77) = true, want false")
	}
}

func TestRolloutEvaluateHonorsDenyAllowThenPercentage(t *testing.T) {
	rollout := Rollout{
		Feature:    "checkout.new_flow",
		Percentage: 8,
		Key:        RolloutKeyTenant,
		Allow: RolloutTargets{
			Tenants: []string{" tenant-allow "},
			Users:   []string{"user-allow"},
		},
		Deny: RolloutTargets{
			Tenants: []string{"tenant-blocked"},
			Users:   []string{"user-denied"},
		},
	}

	tests := []struct {
		name   string
		target RolloutTarget
		want   bool
	}{
		{
			name:   "deny tenant wins over allowed user",
			target: RolloutTarget{TenantKey: "tenant-blocked", UserKey: "user-allow"},
			want:   false,
		},
		{
			name:   "deny user wins",
			target: RolloutTarget{TenantKey: "tenant-allow", UserKey: "user-denied"},
			want:   false,
		},
		{
			name:   "allow user wins over percentage",
			target: RolloutTarget{TenantKey: "tenant-out", UserKey: "user-allow"},
			want:   true,
		},
		{
			name:   "allow tenant trims list entries",
			target: RolloutTarget{TenantKey: "tenant-allow", UserKey: "user-out"},
			want:   true,
		},
		{
			name:   "percentage enables tenant bucket",
			target: RolloutTarget{TenantKey: "tenant-7", UserKey: "user-out"},
			want:   true,
		},
		{
			name:   "percentage disables tenant bucket",
			target: RolloutTarget{TenantKey: "tenant-out", UserKey: "user-out"},
			want:   false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := rollout.Evaluate(tt.target)
			if err != nil {
				t.Fatalf("Evaluate() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("Evaluate() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestRolloutEvaluateUsesConfiguredKey(t *testing.T) {
	target := RolloutTarget{TenantKey: "tenant-7", UserKey: "user-7"}
	byTenant := Rollout{
		Feature:    "checkout.new_flow",
		Percentage: 8,
		Key:        RolloutKeyTenant,
	}
	byUser := Rollout{
		Feature:    "checkout.new_flow",
		Percentage: 8,
		Key:        RolloutKeyUser,
	}

	enabled, err := byTenant.Evaluate(target)
	if err != nil {
		t.Fatalf("tenant Evaluate() error = %v", err)
	}
	if !enabled {
		t.Fatal("tenant Evaluate() = false, want true")
	}

	enabled, err = byUser.Evaluate(target)
	if err != nil {
		t.Fatalf("user Evaluate() error = %v", err)
	}
	if enabled {
		t.Fatal("user Evaluate() = true, want false")
	}
}

func TestRolloutEvaluateRequiresConfiguredTargetKey(t *testing.T) {
	rollout := Rollout{
		Feature:    "checkout.new_flow",
		Percentage: 50,
		Key:        RolloutKeyUser,
	}

	_, err := rollout.Evaluate(RolloutTarget{TenantKey: "tenant-7"})
	if !errors.Is(err, ErrInvalidRolloutTarget) {
		t.Fatalf("Evaluate() error = %v, want ErrInvalidRolloutTarget", err)
	}
}

func TestRolloutValidateRejectsInvalidConfig(t *testing.T) {
	tests := []struct {
		name      string
		rollout   Rollout
		fragments []string
	}{
		{
			name: "invalid percentage",
			rollout: Rollout{
				Percentage: 101,
			},
			fragments: []string{"percentage"},
		},
		{
			name: "partial missing feature",
			rollout: Rollout{
				Percentage: 50,
				Key:        RolloutKeyTenant,
			},
			fragments: []string{"feature is required"},
		},
		{
			name: "partial missing key",
			rollout: Rollout{
				Feature:    "checkout.new_flow",
				Percentage: 50,
			},
			fragments: []string{"key is required"},
		},
		{
			name: "unknown key",
			rollout: Rollout{
				Key: RolloutKey("account"),
			},
			fragments: []string{"key must be"},
		},
		{
			name: "bad target lists",
			rollout: Rollout{
				Allow: RolloutTargets{
					Tenants: []string{"tenant-1", " tenant-1 "},
				},
				Deny: RolloutTargets{
					Users: []string{" "},
				},
			},
			fragments: []string{"duplicates", "deny.users[0] is empty"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.rollout.Validate()
			if !errors.Is(err, ErrInvalidRollout) {
				t.Fatalf("Validate() error = %v, want ErrInvalidRollout", err)
			}
			for _, fragment := range tt.fragments {
				if !strings.Contains(err.Error(), fragment) {
					t.Fatalf("Validate() error = %v; want fragment %q", err, fragment)
				}
			}
		})
	}
}
